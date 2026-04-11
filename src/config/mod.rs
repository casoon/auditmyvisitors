use std::path::PathBuf;

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::errors::Result;
use crate::errors::AppError;

/// Top-level config stored in ~/.config/audit-my-visitors/config.toml
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub properties: PropertiesConfig,

    #[serde(default)]
    pub report: ReportConfig,
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
}

impl Default for ReportConfig {
    fn default() -> Self {
        Self {
            default_days: 28,
            top_pages_limit: 20,
        }
    }
}

impl AppConfig {
    pub fn config_dir() -> anyhow::Result<PathBuf> {
        let base = dirs::config_dir()
            .context("Cannot determine config directory")?;
        Ok(base.join("audit-my-visitors"))
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
