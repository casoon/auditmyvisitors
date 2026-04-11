use chrono::Utc;

use crate::errors::{AppError, Result};
use crate::storage::{load_tokens, save_tokens, StoredTokens};

use super::credentials;

const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

pub async fn refresh_access_token(tokens: &StoredTokens) -> Result<StoredTokens> {
    let refresh_token = tokens
        .refresh_token
        .as_deref()
        .ok_or_else(|| AppError::TokenRefresh("Kein Refresh-Token vorhanden — bitte erneut einloggen".into()))?;

    let client = reqwest::Client::new();

    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", credentials::CLIENT_ID),
        ("client_secret", credentials::CLIENT_SECRET),
    ];

    let response = client
        .post(TOKEN_URL)
        .form(&params)
        .send()
        .await
        .map_err(AppError::Http)?;

    let status = response.status();
    let body: serde_json::Value = response.json().await.map_err(AppError::Http)?;

    if !status.is_success() {
        let msg = body
            .get("error_description")
            .or_else(|| body.get("error"))
            .and_then(|v| v.as_str())
            .unwrap_or("unbekannter Fehler")
            .to_string();
        return Err(AppError::TokenRefresh(msg));
    }

    let access_token = body["access_token"]
        .as_str()
        .ok_or_else(|| AppError::TokenRefresh("Kein access_token in der Refresh-Antwort".into()))?
        .to_string();

    let expires_at = body["expires_in"]
        .as_i64()
        .map(|secs| Utc::now().timestamp() + secs);

    let new_refresh_token = body["refresh_token"]
        .as_str()
        .map(String::from)
        .or_else(|| tokens.refresh_token.clone());

    let refreshed = StoredTokens {
        access_token,
        refresh_token: new_refresh_token,
        expires_at,
    };

    save_tokens(&refreshed)
        .map_err(|e| AppError::Auth(format!("Tokens konnten nicht gespeichert werden: {e}")))?;

    Ok(refreshed)
}

pub async fn ensure_valid_token() -> Result<String> {
    let tokens = load_tokens()
        .map_err(|e| AppError::Auth(e.to_string()))?
        .ok_or(AppError::NotAuthenticated)?;

    if !tokens.is_expired() {
        return Ok(tokens.access_token);
    }

    let refreshed = refresh_access_token(&tokens).await?;
    Ok(refreshed.access_token)
}
