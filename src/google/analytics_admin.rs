/// Google Analytics Admin API — property discovery
use serde::{Deserialize, Serialize};

use crate::errors::Result;

use super::parse_google_response;

const BASE: &str = "https://analyticsadmin.googleapis.com/v1beta";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ga4Property {
    /// Full resource name, e.g. "properties/123456789"
    pub name: String,
    /// Human-readable display name
    pub display_name: String,
    /// Parent account resource name
    pub account: String,
}

/// Fetches all GA4 properties accessible with the given access token.
pub async fn list_properties(access_token: &str) -> Result<Vec<Ga4Property>> {
    let client = super::http_client();
    let url = format!("{BASE}/accountSummaries");

    let response = super::send_with_retry(|| {
        client.get(&url).bearer_auth(access_token)
    }).await?;

    let body = parse_google_response(response).await?;

    let mut properties = Vec::new();

    if let Some(summaries) = body["accountSummaries"].as_array() {
        for account in summaries {
            let account_name = account["name"].as_str().unwrap_or("").to_string();

            if let Some(props) = account["propertySummaries"].as_array() {
                for prop in props {
                    let name = prop["property"].as_str().unwrap_or("").to_string();
                    let display_name = prop["displayName"].as_str().unwrap_or(&name).to_string();
                    if !name.is_empty() {
                        properties.push(Ga4Property {
                            name,
                            display_name,
                            account: account_name.clone(),
                        });
                    }
                }
            }
        }
    }

    Ok(properties)
}
