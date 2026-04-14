pub mod analytics_admin;
pub mod analytics_data;
pub mod search_console;

use std::time::Duration;

use crate::errors::{AppError, Result};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RETRIES: u32 = 3;

/// Build a shared HTTP client with sensible timeout.
pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .expect("failed to build HTTP client")
}

/// Execute a request builder with automatic retry on 429 and 5xx.
pub async fn send_with_retry(
    builder_fn: impl Fn() -> reqwest::RequestBuilder,
) -> Result<reqwest::Response> {
    let mut last_err = None;

    for attempt in 0..MAX_RETRIES {
        let result = builder_fn().send().await;

        match result {
            Ok(resp) if resp.status() == 429 => {
                let wait = retry_delay(attempt, resp.headers());
                tracing::warn!("Rate limited (429), retrying in {:.1}s…", wait.as_secs_f64());
                tokio::time::sleep(wait).await;
                last_err = Some(AppError::RateLimited);
            }
            Ok(resp) if resp.status().is_server_error() => {
                let status = resp.status().as_u16();
                let wait = Duration::from_millis(500 * 2u64.pow(attempt));
                tracing::warn!("Server error ({}), retrying in {:.1}s…", status, wait.as_secs_f64());
                tokio::time::sleep(wait).await;
                last_err = Some(AppError::GoogleApi {
                    status,
                    message: format!("Server error (attempt {})", attempt + 1),
                });
            }
            Ok(resp) => return Ok(resp),
            Err(e) if e.is_timeout() => {
                if attempt + 1 < MAX_RETRIES {
                    let wait = Duration::from_millis(1000 * 2u64.pow(attempt));
                    tracing::warn!("Request timed out, retrying in {:.1}s…", wait.as_secs_f64());
                    tokio::time::sleep(wait).await;
                    last_err = Some(AppError::Timeout(REQUEST_TIMEOUT.as_secs()));
                } else {
                    return Err(AppError::Timeout(REQUEST_TIMEOUT.as_secs()));
                }
            }
            Err(e) => return Err(AppError::Http(e)),
        }
    }

    Err(last_err.unwrap_or(AppError::RateLimited))
}

/// Parse Retry-After header or use exponential backoff.
fn retry_delay(attempt: u32, headers: &reqwest::header::HeaderMap) -> Duration {
    if let Some(val) = headers.get(reqwest::header::RETRY_AFTER) {
        if let Ok(secs) = val.to_str().unwrap_or("").parse::<u64>() {
            return Duration::from_secs(secs.min(60));
        }
    }
    Duration::from_millis(1000 * 2u64.pow(attempt))
}

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
