/// Google Analytics Data API — reporting
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::errors::{AppError, Result};

use super::parse_google_response;

const BASE: &str = "https://analyticsdata.googleapis.com/v1beta";

// ─── Request types ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct DateRange {
    pub start_date: String, // YYYY-MM-DD
    pub end_date: String,
}

impl DateRange {
    pub fn last_n_days(n: u32) -> Self {
        Self {
            start_date: format!("{n}daysAgo"),
            end_date: "yesterday".into(),
        }
    }

    /// The previous period of the same length (for trend comparison)
    pub fn prev_period(n: u32) -> Self {
        Self {
            start_date: format!("{}daysAgo", n * 2),
            end_date: format!("{}daysAgo", n + 1),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReportRequest {
    pub property_id: String,
    pub date_ranges: Vec<DateRange>,
    pub dimensions: Vec<String>,
    pub metrics: Vec<String>,
    pub dimension_filter: Option<Value>,
    pub limit: Option<i64>,
    pub order_by: Option<Vec<Value>>,
}

// ─── Response types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportRow {
    pub dimension_values: Vec<String>,
    pub metric_values: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunReportResponse {
    pub dimension_headers: Vec<String>,
    pub metric_headers: Vec<String>,
    pub rows: Vec<ReportRow>,
    pub row_count: i64,
}

// ─── API call ────────────────────────────────────────────────────────────────

pub async fn run_report(
    access_token: &str,
    request: ReportRequest,
) -> Result<RunReportResponse> {
    let client = reqwest::Client::new();
    let url = format!("{BASE}/{}:runReport", request.property_id);

    let body = build_report_body(&request);

    let response = client
        .post(&url)
        .bearer_auth(access_token)
        .json(&body)
        .send()
        .await
        .map_err(AppError::Http)?;

    let json = parse_google_response(response).await?;
    parse_run_report_response(json)
}

fn build_report_body(req: &ReportRequest) -> Value {
    let date_ranges: Vec<Value> = req
        .date_ranges
        .iter()
        .map(|dr| json!({ "startDate": dr.start_date, "endDate": dr.end_date }))
        .collect();

    let dimensions: Vec<Value> = req
        .dimensions
        .iter()
        .map(|d| json!({ "name": d }))
        .collect();

    let metrics: Vec<Value> = req
        .metrics
        .iter()
        .map(|m| json!({ "name": m }))
        .collect();

    let mut body = json!({
        "dateRanges": date_ranges,
        "dimensions": dimensions,
        "metrics": metrics,
    });

    if let Some(filter) = &req.dimension_filter {
        body["dimensionFilter"] = filter.clone();
    }

    if let Some(limit) = req.limit {
        body["limit"] = json!(limit);
    }

    if let Some(order_by) = &req.order_by {
        body["orderBys"] = json!(order_by);
    }

    body
}

fn parse_run_report_response(json: Value) -> Result<RunReportResponse> {
    let dimension_headers = json["dimensionHeaders"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|h| h["name"].as_str().map(String::from))
        .collect();

    let metric_headers = json["metricHeaders"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|h| h["name"].as_str().map(String::from))
        .collect();

    let rows = json["rows"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .map(|row| {
            let dimension_values = row["dimensionValues"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .filter_map(|v| v["value"].as_str().map(String::from))
                .collect();

            let metric_values = row["metricValues"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .filter_map(|v| v["value"].as_str().map(String::from))
                .collect();

            ReportRow {
                dimension_values,
                metric_values,
            }
        })
        .collect();

    let row_count = json["rowCount"].as_i64().unwrap_or(0);

    Ok(RunReportResponse {
        dimension_headers,
        metric_headers,
        rows,
        row_count,
    })
}
