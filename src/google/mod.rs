pub mod analytics_admin;
pub mod analytics_data;
pub mod search_console;

use crate::errors::{AppError, Result};

/// Shared helper: check a Google API response for errors and extract JSON.
pub async fn parse_google_response(response: reqwest::Response) -> Result<serde_json::Value> {
    let status = response.status();
    let body: serde_json::Value = response.json().await.map_err(AppError::Http)?;

    if !status.is_success() {
        let message = body
            .pointer("/error/message")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown Google API error")
            .to_string();
        return Err(AppError::GoogleApi {
            status: status.as_u16(),
            message,
        });
    }

    Ok(body)
}
