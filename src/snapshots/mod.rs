//! Local snapshots — store key metrics on disk for trend tracking.
//!
//! Snapshots live in ~/.config/auditmyvisitors/snapshots/<property_slug>/
//! One JSON file per run, named by date: 2026-04-14.json

use std::path::PathBuf;

use anyhow::Context;
use serde::{Deserialize, Serialize};

/// Key metrics captured in a snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// ISO date when the snapshot was taken
    pub date: String,
    /// Number of days the report covered
    pub days: u32,
    pub sessions: i64,
    pub organic_sessions: i64,
    pub engagement_rate: f64,
    pub clicks: f64,
    pub impressions: f64,
    pub ctr: f64,
    pub avg_position: f64,
}

fn snapshots_dir(property_slug: &str) -> anyhow::Result<PathBuf> {
    let base = dirs::config_dir().context("Cannot determine config directory")?;
    Ok(base.join("auditmyvisitors").join("snapshots").join(property_slug))
}

fn slug(property_name: &str) -> String {
    property_name
        .to_lowercase()
        .replace(|c: char| !c.is_alphanumeric() && c != '-', "-")
        .trim_matches('-')
        .to_string()
}

/// Save a snapshot for the given property.
pub fn save(property_name: &str, snapshot: &Snapshot) -> anyhow::Result<PathBuf> {
    let dir = snapshots_dir(&slug(property_name))?;
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Cannot create snapshot dir {}", dir.display()))?;

    let path = dir.join(format!("{}.json", snapshot.date));
    let json = serde_json::to_string_pretty(snapshot)?;
    std::fs::write(&path, json)?;
    Ok(path)
}

/// Load the most recent snapshot before the given date.
pub fn load_previous(property_name: &str, before_date: &str) -> anyhow::Result<Option<Snapshot>> {
    let dir = snapshots_dir(&slug(property_name))?;
    if !dir.exists() {
        return Ok(None);
    }

    let mut snapshots: Vec<String> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if name.ends_with(".json") {
                Some(name.trim_end_matches(".json").to_string())
            } else {
                None
            }
        })
        .filter(|date| date.as_str() < before_date)
        .collect();

    snapshots.sort();

    if let Some(latest) = snapshots.last() {
        let path = dir.join(format!("{}.json", latest));
        let content = std::fs::read_to_string(&path)?;
        let snap: Snapshot = serde_json::from_str(&content)?;
        Ok(Some(snap))
    } else {
        Ok(None)
    }
}

/// List all snapshots for a property, most recent first.
pub fn list(property_name: &str) -> anyhow::Result<Vec<Snapshot>> {
    let dir = snapshots_dir(&slug(property_name))?;
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "json"))
        .collect();

    files.sort();
    files.reverse();

    let mut snapshots = Vec::new();
    for path in files {
        let content = std::fs::read_to_string(&path)?;
        if let Ok(snap) = serde_json::from_str::<Snapshot>(&content) {
            snapshots.push(snap);
        }
    }

    Ok(snapshots)
}
