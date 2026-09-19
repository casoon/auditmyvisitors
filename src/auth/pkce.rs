/// OAuth2 Authorization Code Flow with PKCE for CLI applications.
use std::collections::HashMap;

use anyhow::{bail, Context};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::Utc;
use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::storage::{save_tokens, StoredTokens};

use super::credentials;

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

/// Redirect port for the local OAuth receiver
const REDIRECT_PORT: u16 = 9004;

/// Required Google OAuth scopes (minimal)
const SCOPES: &[&str] = &[
    "https://www.googleapis.com/auth/analytics.readonly",
    "https://www.googleapis.com/auth/webmasters.readonly",
];

pub async fn run_oauth_login() -> anyhow::Result<()> {
    if credentials::CLIENT_ID == "GOOGLE_CLIENT_ID_NOT_SET" {
        bail!(
            "This binary was built without Google OAuth credentials.\n\
            Use an official release, or build it yourself:\n\n\
            GOOGLE_CLIENT_ID=xxx GOOGLE_CLIENT_SECRET=xxx cargo build --release\n"
        );
    }

    let redirect_uri = format!("http://127.0.0.1:{REDIRECT_PORT}");

    // Generate PKCE verifier + challenge
    let code_verifier = generate_code_verifier();
    let code_challenge = generate_code_challenge(&code_verifier);
    let state = generate_state();

    let scope = SCOPES.join(" ");
    let auth_url = format!(
        "{AUTH_URL}?response_type=code\
        &client_id={client_id}\
        &redirect_uri={redirect_uri}\
        &scope={scope}\
        &state={state}\
        &code_challenge={code_challenge}\
        &code_challenge_method=S256\
        &access_type=offline\
        &prompt=consent",
        client_id = urlencoding::encode(credentials::CLIENT_ID),
        redirect_uri = urlencoding::encode(&redirect_uri),
        scope = urlencoding::encode(&scope),
        state = urlencoding::encode(&state),
        code_challenge = urlencoding::encode(&code_challenge),
    );

    println!("Opening your browser for the Google login…");
    println!("If it does not open, visit:\n{auth_url}\n");

    webbrowser::open(&auth_url).context("Cannot open a browser")?;

    println!("Waiting for Google to redirect back…");

    let query_string = super::server::wait_for_redirect(REDIRECT_PORT)
        .context("The local OAuth redirect listener failed")?;

    let params: HashMap<String, String> = url::form_urlencoded::parse(query_string.as_bytes())
        .into_owned()
        .collect();

    let returned_state = params.get("state").map(String::as_str).unwrap_or("");
    if returned_state != state {
        bail!("OAuth state mismatch — possible CSRF attempt. Aborted.");
    }

    if let Some(error) = params.get("error") {
        bail!("Google returned an error: {error}");
    }

    let code = params
        .get("code")
        .context("No authorization code in the redirect")?
        .clone();

    let tokens = exchange_code_for_tokens(&code, &code_verifier, &redirect_uri).await?;
    save_tokens(&tokens).context("Cannot store the tokens")?;

    println!("Login successful — tokens stored.");
    Ok(())
}

async fn exchange_code_for_tokens(
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> anyhow::Result<StoredTokens> {
    let client = reqwest::Client::new();

    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", credentials::CLIENT_ID),
        ("client_secret", credentials::CLIENT_SECRET),
        ("code_verifier", code_verifier),
    ];

    let response = client
        .post(TOKEN_URL)
        .form(&params)
        .send()
        .await
        .context("Token exchange failed")?;

    let status = response.status();
    let body: serde_json::Value = response
        .json()
        .await
        .context("Cannot parse the token response")?;

    if !status.is_success() {
        bail!(
            "Token exchange failed ({}): {}",
            status,
            body.get("error_description")
                .or_else(|| body.get("error"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error")
        );
    }

    let access_token = body["access_token"]
        .as_str()
        .context("No access_token in the response")?
        .to_string();

    let refresh_token = body["refresh_token"].as_str().map(String::from);

    let expires_at = body["expires_in"]
        .as_i64()
        .map(|secs| Utc::now().timestamp() + secs);

    Ok(StoredTokens {
        access_token,
        refresh_token,
        expires_at,
    })
}

fn generate_code_verifier() -> String {
    let mut bytes = [0u8; 64];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn generate_code_challenge(verifier: &str) -> String {
    let hash = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hash)
}

fn generate_state() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}
