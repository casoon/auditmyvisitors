use std::path::PathBuf;

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::errors::Result;
use crate::errors::AppError;

/// Top-level config stored in ~/.config/auditmyvisitors/config.toml
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub properties: PropertiesConfig,

    #[serde(default)]
    pub report: ReportConfig,

    #[serde(default)]
    pub thresholds: ThresholdsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PropertiesConfig {
    /// Selected GA4 property ID, e.g. "properties/123456789"
    pub ga4_property_id: Option<String>,
    /// Human-readable name for display
    pub ga4_property_name: Option<String>,
    /// Selected Search Console property URL
    pub search_console_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportConfig {
    /// Default number of days to look back
    pub default_days: u32,
    /// Default number of top pages to show
    pub top_pages_limit: usize,
    /// Brand terms for query classification (e.g. ["mycompany", "my company"])
    #[serde(default)]
    pub brand_terms: Vec<String>,
}

/// Configurable thresholds for insight generation.
/// All percentage values are absolute (e.g. 20.0 means 20%).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdsConfig {
    /// Organic share below this triggers a warning (default: 20%)
    pub low_organic_pct: f64,
    /// Direct share above this triggers an info (default: 60%)
    pub high_direct_pct: f64,
    /// Organic share above this triggers a positive insight (default: 70%)
    pub high_organic_pct: f64,
    /// Engagement rate below this triggers a warning (default: 0.3 = 30%)
    pub low_engagement_rate: f64,
    /// Engagement rate above this triggers a positive insight (default: 0.7 = 70%)
    pub high_engagement_rate: f64,
    /// Trend change beyond this % triggers a significant insight (default: 20%)
    pub trend_significant_pct: f64,
    /// Top 3 pages above this share triggers dependency warning (default: 60%)
    pub top3_dependency_pct: f64,
    /// Minimum sessions before generating session-based insights (default: 100)
    pub min_sessions: i64,
}

impl Default for ThresholdsConfig {
    fn default() -> Self {
        Self {
            low_organic_pct: 20.0,
            high_direct_pct: 60.0,
            high_organic_pct: 70.0,
            low_engagement_rate: 0.3,
            high_engagement_rate: 0.7,
            trend_significant_pct: 20.0,
            top3_dependency_pct: 60.0,
            min_sessions: 100,
        }
    }
}

impl Default for ReportConfig {
    fn default() -> Self {
        Self {
            default_days: 28,
            top_pages_limit: 20,
            brand_terms: Vec::new(),
        }
    }
}

impl AppConfig {
    pub fn config_dir() -> anyhow::Result<PathBuf> {
        let base = dirs::config_dir()
            .context("Cannot determine config directory")?;
        Ok(base.join("auditmyvisitors"))
    }

    pub fn config_path() -> anyhow::Result<PathBuf> {
        Ok(Self::config_dir()?.join("config.toml"))
    }

    pub fn load() -> anyhow::Result<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Cannot read config at {}", path.display()))?;
        toml::from_str(&content)
            .with_context(|| format!("Cannot parse config at {}", path.display()))
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let dir = Self::config_dir()?;
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("Cannot create config dir {}", dir.display()))?;
        let path = Self::config_path()?;
        let content = toml::to_string_pretty(self)
            .context("Cannot serialize config")?;
        std::fs::write(&path, content)
            .with_context(|| format!("Cannot write config to {}", path.display()))?;
        Ok(())
    }

    pub fn set_ga4_property(&mut self, id: String, name: String) {
        self.properties.ga4_property_id = Some(id);
        self.properties.ga4_property_name = Some(name);
    }

    pub fn set_search_console_url(&mut self, url: String) {
        self.properties.search_console_url = Some(url);
    }

    pub fn require_ga4_property(&self) -> Result<&str> {
        self.properties.ga4_property_id
            .as_deref()
            .ok_or(AppError::NoPropertySelected)
    }

    pub fn require_search_console_url(&self) -> Result<&str> {
        self.properties.search_console_url
            .as_deref()
            .ok_or(AppError::NoSearchConsolePropertySelected)
    }
}
