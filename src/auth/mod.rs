mod credentials;
mod pkce;
mod refresh;
mod server;

pub use pkce::run_oauth_login;
pub use refresh::ensure_valid_token;

use crate::storage::load_tokens;

pub fn auth_status() -> anyhow::Result<AuthStatus> {
    match load_tokens()? {
        None => Ok(AuthStatus::NotLoggedIn),
        Some(t) if t.is_expired() => Ok(AuthStatus::TokenExpired),
        Some(_) => Ok(AuthStatus::LoggedIn),
    }
}

#[derive(Debug)]
pub enum AuthStatus {
    LoggedIn,
    TokenExpired,
    NotLoggedIn,
}
