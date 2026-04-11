/// Google Search Console API — search analytics
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::errors::{AppError, Result};

use super::parse_google_response;

const BASE: &str = "https://www.googleapis.com/webmasters/v3";

// ─── Request ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SearchAnalyticsRequest {
    pub site_url: String,
    pub start_date: String, // YYYY-MM-DD
    pub end_date: String,
    pub dimensions: Vec<String>, // "page", "query", "country", "device", "date"
    pub page_filter: Option<String>,
    pub row_limit: Option<i32>,
}

// ─── Response ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchAnalyticsRow {
    pub keys: Vec<String>,
    pub clicks: f64,
    pub impressions: f64,
    pub ctr: f64,
    pub position: f64,
}

#[derive(Debug, Clone, Default)]
pub struct SearchAnalyticsResponse {
    pub rows: Vec<SearchAnalyticsRow>,
}

/// List all Search Console properties accessible with the given token.
pub async fn list_sites(access_token: &str) -> Result<Vec<String>> {
    let client = reqwest::Client::new();
    let url = format!("{BASE}/sites");

    let response = client
        .get(&url)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(AppError::Http)?;

    let body = parse_google_response(response).await?;

    let sites = body["siteEntry"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|s| s["siteUrl"].as_str().map(String::from))
        .collect();

    Ok(sites)
}

pub async fn query(
    access_token: &str,
    req: SearchAnalyticsRequest,
) -> Result<SearchAnalyticsResponse> {
    let client = reqwest::Client::new();
    let encoded_site = urlencoding::encode(&req.site_url);
    let url = format!("{BASE}/sites/{encoded_site}/searchAnalytics/query");

    let mut body = json!({
        "startDate": req.start_date,
        "endDate": req.end_date,
        "dimensions": req.dimensions,
    });

    if let Some(limit) = req.row_limit {
        body["rowLimit"] = json!(limit);
    }

    if let Some(page) = &req.page_filter {
        body["dimensionFilterGroups"] = json!([{
            "filters": [{
                "dimension": "page",
                "operator": "equals",
                "expression": page
            }]
        }]);
    }

    let response = client
        .post(&url)
        .bearer_auth(access_token)
        .json(&body)
        .send()
        .await
        .map_err(AppError::Http)?;

    let json = parse_google_response(response).await?;

    let rows = json["rows"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .map(|row| {
            let keys = row["keys"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .filter_map(|k| k.as_str().map(String::from))
                .collect();

            SearchAnalyticsRow {
                keys,
                clicks: row["clicks"].as_f64().unwrap_or(0.0),
                impressions: row["impressions"].as_f64().unwrap_or(0.0),
                ctr: row["ctr"].as_f64().unwrap_or(0.0),
                position: row["position"].as_f64().unwrap_or(0.0),
            }
        })
        .collect();

    Ok(SearchAnalyticsResponse { rows })
}
