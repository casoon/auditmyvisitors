use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Authentication error: {0}")]
    Auth(String),

    #[error("Token not found — run `auth login` first")]
    NotAuthenticated,

    #[error("Token refresh failed: {0}")]
    TokenRefresh(String),

    #[error("Google API error ({status}): {message}")]
    GoogleApi { status: u16, message: String },

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Config error: {0}")]
    Config(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("No GA4 property selected — run `properties select` first")]
    NoPropertySelected,

    #[error("No Search Console property selected")]
    NoSearchConsolePropertySelected,

    #[error("Invalid date format '{0}' — expected YYYY-MM-DD")]
    InvalidDate(String),

    #[error("Property not found: {0}")]
    PropertyNotFound(String),
}

pub type Result<T> = std::result::Result<T, AppError>;
