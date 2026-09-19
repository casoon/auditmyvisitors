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
    restrict_to_owner(&path)?;
    Ok(())
}

/// Restrict a file to owner read/write.
///
/// The tokens grant read access to the user's Analytics and Search Console data,
/// so they must not be readable by other accounts on a shared machine. On Windows
/// the per-user profile directory already provides that, and there is no direct
/// equivalent to a Unix mode.
#[cfg(unix)]
fn restrict_to_owner(path: &std::path::Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .with_context(|| format!("Cannot restrict permissions on {}", path.display()))
}

#[cfg(not(unix))]
fn restrict_to_owner(_path: &std::path::Path) -> anyhow::Result<()> {
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
