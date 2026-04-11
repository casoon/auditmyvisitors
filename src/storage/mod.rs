use std::path::PathBuf;

use anyhow::Context;
use serde::{Deserialize, Serialize};

/// Stored OAuth tokens
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// Unix timestamp (seconds) when the access token expires
    pub expires_at: Option<i64>,
}

impl StoredTokens {
    pub fn is_expired(&self) -> bool {
        match self.expires_at {
            None => false,
            Some(exp) => {
                let now = chrono::Utc::now().timestamp();
                // Consider expired 60s early to avoid edge cases
                now >= exp - 60
            }
        }
    }
}

fn token_path() -> anyhow::Result<PathBuf> {
    let base = dirs::config_dir().context("Cannot determine config directory")?;
    Ok(base.join("auditmyvisitors").join("tokens.json"))
}

pub fn load_tokens() -> anyhow::Result<Option<StoredTokens>> {
    let path = token_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Cannot read tokens from {}", path.display()))?;
    let tokens = serde_json::from_str(&content)
        .context("Cannot parse stored tokens")?;
    Ok(Some(tokens))
}

pub fn save_tokens(tokens: &StoredTokens) -> anyhow::Result<()> {
    let path = token_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Cannot create directory {}", parent.display()))?;
    }
    let content = serde_json::to_string_pretty(tokens)
        .context("Cannot serialize tokens")?;
    std::fs::write(&path, content)
        .with_context(|| format!("Cannot write tokens to {}", path.display()))?;
    Ok(())
}

pub fn delete_tokens() -> anyhow::Result<()> {
    let path = token_path()?;
    if path.exists() {
        std::fs::remove_file(&path)
            .with_context(|| format!("Cannot delete tokens at {}", path.display()))?;
    }
    Ok(())
}
