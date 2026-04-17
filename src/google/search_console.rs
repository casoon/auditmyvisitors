/// Google Search Console API — search analytics + sitemaps + URL inspection
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::errors::Result;

use super::parse_google_response;

const BASE: &str = "https://www.googleapis.com/webmasters/v3";
const INSPECTION_BASE: &str = "https://searchconsole.googleapis.com/v1";

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
    let client = super::http_client();
    let url = format!("{BASE}/sites");

    let response = super::send_with_retry(|| {
        client.get(&url).bearer_auth(access_token)
    }).await?;

    let body = parse_google_response(response).await?;

    let sites = body["siteEntry"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|s| s["siteUrl"].as_str().map(String::from))
        .collect();

    Ok(sites)
}

// ─── Sitemaps ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct SitemapInfo {
    pub path: String,
    pub submitted: i64,
    pub indexed: i64,
    pub warnings: i64,
    pub errors: i64,
    pub last_submitted: Option<String>,
    pub is_pending: bool,
}

pub async fn list_sitemaps(access_token: &str, site_url: &str) -> Result<Vec<SitemapInfo>> {
    let client = super::http_client();
    let encoded = urlencoding::encode(site_url);
    let url = format!("{BASE}/sites/{encoded}/sitemaps");

    let response = super::send_with_retry(|| {
        client.get(&url).bearer_auth(access_token)
    }).await?;

    let body = parse_google_response(response).await?;

    let sitemaps = body["sitemap"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .map(|s| {
            let mut submitted = 0i64;
            let mut indexed = 0i64;
            if let Some(contents) = s["contents"].as_array() {
                for c in contents {
                    submitted += c["submitted"].as_str().and_then(|v| v.parse().ok()).unwrap_or(0);
                    indexed  += c["indexed"].as_str().and_then(|v| v.parse().ok()).unwrap_or(0);
                }
            }
            let last_submitted = s["lastSubmitted"].as_str()
                .map(|d| d.get(..10).unwrap_or(d).to_string());
            SitemapInfo {
                path: s["path"].as_str().unwrap_or("").to_string(),
                submitted,
                indexed,
                warnings: s["warnings"].as_str().and_then(|v| v.parse().ok()).unwrap_or(0),
                errors:   s["errors"].as_str().and_then(|v| v.parse().ok()).unwrap_or(0),
                last_submitted,
                is_pending: s["isPending"].as_bool().unwrap_or(false),
            }
        })
        .collect();

    Ok(sitemaps)
}

// ─── URL Inspection ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct UrlInspectionData {
    pub url: String,
    pub verdict: String,        // "PASS", "FAIL", "PARTIAL", "VERDICT_UNSPECIFIED"
    pub coverage_state: String, // "Submitted and indexed", "Crawled - currently not indexed", etc.
    pub robots_allowed: bool,
    pub indexing_allowed: bool,
    pub last_crawl: Option<String>,
    pub mobile_verdict: String, // "PASS", "FAIL", "VERDICT_UNSPECIFIED"
    pub canonical_ok: bool,     // user canonical == google canonical
}

pub async fn inspect_url(
    access_token: &str,
    site_url: &str,
    inspection_url: &str,
) -> Result<UrlInspectionData> {
    let client = super::http_client();
    let url = format!("{INSPECTION_BASE}/urlInspection/index:inspect");

    let body = json!({
        "inspectionUrl": inspection_url,
        "siteUrl": site_url,
    });

    let response = super::send_with_retry(|| {
        client.post(&url).bearer_auth(access_token).json(&body)
    }).await?;

    let json = parse_google_response(response).await?;

    let index  = &json["inspectionResult"]["indexStatusResult"];
    let mobile = &json["inspectionResult"]["mobileUsabilityResult"];

    let user_canonical   = index["userCanonical"].as_str().unwrap_or("");
    let google_canonical = index["googleCanonical"].as_str().unwrap_or("");
    let canonical_ok = !user_canonical.is_empty() && user_canonical == google_canonical;

    let last_crawl = index["lastCrawlTime"].as_str()
        .map(|d| d.get(..10).unwrap_or(d).to_string());

    Ok(UrlInspectionData {
        url: inspection_url.to_string(),
        verdict:          index["verdict"].as_str().unwrap_or("VERDICT_UNSPECIFIED").to_string(),
        coverage_state:   index["coverageState"].as_str().unwrap_or("Unknown").to_string(),
        robots_allowed:   index["robotsTxtState"].as_str().unwrap_or("") == "ALLOWED",
        indexing_allowed: index["indexingState"].as_str().unwrap_or("") == "INDEXING_ALLOWED",
        last_crawl,
        mobile_verdict:   mobile["verdict"].as_str().unwrap_or("VERDICT_UNSPECIFIED").to_string(),
        canonical_ok,
    })
}

// ─── Search Analytics ─────────────────────────────────────────────────────────

pub async fn query(
    access_token: &str,
    req: SearchAnalyticsRequest,
) -> Result<SearchAnalyticsResponse> {
    let client = super::http_client();
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

    let response = super::send_with_retry(|| {
        client.post(&url).bearer_auth(access_token).json(&body)
    }).await?;

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
