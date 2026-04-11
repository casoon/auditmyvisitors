use chrono::NaiveDate;

use crate::config::AppConfig;
use crate::domain::{
    ComparisonDelta, ComparisonPeriod, ComparisonReport, SearchPerformanceBreakdown,
    TrafficSourceBreakdown,
};
use crate::errors::{AppError, Result};
use crate::google::analytics_data::{DateRange, ReportRequest, run_report};
use crate::google::search_console::{query, SearchAnalyticsRequest};
use crate::insights::insights_for_comparison;
use serde_json::json;

pub async fn build(
    config: &AppConfig,
    access_token: &str,
    url: Option<&str>,
    before_days: u32,
    after_days: u32,
    since: &str,
) -> Result<ComparisonReport> {
    let change_date = NaiveDate::parse_from_str(since, "%Y-%m-%d")
        .map_err(|_| AppError::InvalidDate(since.to_string()))?;

    let before_end = change_date - chrono::Duration::days(1);
    let before_start = before_end - chrono::Duration::days(before_days as i64 - 1);
    let after_start = change_date;
    let after_end = after_start + chrono::Duration::days(after_days as i64 - 1);

    let property_id = config.require_ga4_property()?.to_string();
    let sc_url = config.require_search_console_url().ok().map(String::from);
    let property_name = config
        .properties
        .ga4_property_name
        .clone()
        .unwrap_or_else(|| property_id.clone());

    let fmt = |d: NaiveDate| d.format("%Y-%m-%d").to_string();

    let (before_traffic, before_search) = fetch_period(
        access_token,
        &property_id,
        sc_url.as_deref(),
        url,
        &fmt(before_start),
        &fmt(before_end),
    ).await?;

    let (after_traffic, after_search) = fetch_period(
        access_token,
        &property_id,
        sc_url.as_deref(),
        url,
        &fmt(after_start),
        &fmt(after_end),
    ).await?;

    let delta = compute_delta(&before_traffic, &after_traffic, &before_search, &after_search);
    let summary = generate_summary(&delta);

    let before = ComparisonPeriod {
        start_date: fmt(before_start),
        end_date: fmt(before_end),
        sessions: before_traffic.total_sessions,
        organic_sessions: before_traffic.organic_sessions,
        engagement_rate: 0.0,
        search: before_search,
    };

    let after = ComparisonPeriod {
        start_date: fmt(after_start),
        end_date: fmt(after_end),
        sessions: after_traffic.total_sessions,
        organic_sessions: after_traffic.organic_sessions,
        engagement_rate: 0.0,
        search: after_search,
    };

    let mut report = ComparisonReport {
        url: url.map(String::from),
        property_name,
        change_date: since.to_string(),
        before_days,
        after_days,
        before,
        after,
        delta,
        summary,
        insights: vec![],
    };

    insights_for_comparison(&mut report);
    Ok(report)
}

async fn fetch_period(
    access_token: &str,
    property_id: &str,
    sc_url: Option<&str>,
    page_url: Option<&str>,
    start: &str,
    end: &str,
) -> Result<(TrafficSourceBreakdown, SearchPerformanceBreakdown)> {
    let date_range = DateRange {
        start_date: start.to_string(),
        end_date: end.to_string(),
    };

    let filter = page_url.map(|u| {
        let path = url::Url::parse(u)
            .map(|p| p.path().to_string())
            .unwrap_or_else(|_| u.to_string());
        json!({
            "filter": {
                "fieldName": "pagePath",
                "stringFilter": { "matchType": "EXACT", "value": path }
            }
        })
    });

    let req = ReportRequest {
        property_id: property_id.to_string(),
        date_ranges: vec![date_range],
        dimensions: vec!["sessionDefaultChannelGroup".into()],
        metrics: vec!["sessions".into()],
        dimension_filter: filter,
        limit: Some(50),
        order_by: None,
    };

    let ga_report = run_report(access_token, req).await?;

    let mut traffic = TrafficSourceBreakdown::default();
    for row in &ga_report.rows {
        let channel = row.dimension_values.first().map(String::as_str).unwrap_or("");
        let sessions: i64 = row.metric_values.first().and_then(|v| v.parse().ok()).unwrap_or(0);
        traffic.total_sessions += sessions;
        match channel {
            "Organic Search" => traffic.organic_sessions += sessions,
            "Direct"         => traffic.direct_sessions  += sessions,
            "Referral"       => traffic.referral_sessions += sessions,
            _                => traffic.other_sessions    += sessions,
        }
    }

    let search = if let Some(sc) = sc_url {
        let sc_req = SearchAnalyticsRequest {
            site_url: sc.to_string(),
            start_date: start.to_string(),
            end_date: end.to_string(),
            dimensions: vec!["date".into()],
            page_filter: page_url.map(String::from),
            row_limit: Some(500),
        };

        let sc_resp = query(access_token, sc_req).await?;

        let (clicks, impressions) = sc_resp
            .rows
            .iter()
            .fold((0.0f64, 0.0f64), |(c, i), r| (c + r.clicks, i + r.impressions));
        let ctr = if impressions > 0.0 { clicks / impressions } else { 0.0 };
        let avg_pos = if sc_resp.rows.is_empty() {
            0.0
        } else {
            let num: f64 = sc_resp.rows.iter().map(|r| r.position * r.impressions).sum();
            let den: f64 = sc_resp.rows.iter().map(|r| r.impressions).sum();
            if den > 0.0 { num / den } else { 0.0 }
        };

        SearchPerformanceBreakdown {
            clicks, impressions, ctr, average_position: avg_pos,
            top_queries: vec![],
        }
    } else {
        SearchPerformanceBreakdown::default()
    };

    Ok((traffic, search))
}

fn compute_delta(
    b_t: &TrafficSourceBreakdown,
    a_t: &TrafficSourceBreakdown,
    b_s: &SearchPerformanceBreakdown,
    a_s: &SearchPerformanceBreakdown,
) -> ComparisonDelta {
    let pct = |before: f64, after: f64| -> f64 {
        if before == 0.0 { return 0.0; }
        (after - before) / before * 100.0
    };

    ComparisonDelta {
        sessions_abs: a_t.total_sessions - b_t.total_sessions,
        sessions_pct: pct(b_t.total_sessions as f64, a_t.total_sessions as f64),
        organic_sessions_abs: a_t.organic_sessions - b_t.organic_sessions,
        organic_sessions_pct: pct(b_t.organic_sessions as f64, a_t.organic_sessions as f64),
        engagement_rate_abs: 0.0,
        clicks_abs: a_s.clicks - b_s.clicks,
        clicks_pct: pct(b_s.clicks, a_s.clicks),
        impressions_abs: a_s.impressions - b_s.impressions,
        impressions_pct: pct(b_s.impressions, a_s.impressions),
        ctr_abs: a_s.ctr - b_s.ctr,
        position_abs: a_s.average_position - b_s.average_position,
    }
}

fn generate_summary(delta: &ComparisonDelta) -> String {
    let mut parts = Vec::new();

    if delta.sessions_pct.abs() >= 5.0 {
        parts.push(format!(
            "Sitzungen {:+.0}% ({:+})",
            delta.sessions_pct, delta.sessions_abs
        ));
    }
    if delta.clicks_pct.abs() >= 5.0 {
        parts.push(format!(
            "Klicks {:+.0}% ({:+.0})",
            delta.clicks_pct, delta.clicks_abs
        ));
    }
    if delta.position_abs.abs() >= 1.0 {
        parts.push(format!("Position {:+.1}", delta.position_abs));
    }

    if parts.is_empty() {
        "Keine signifikanten Veränderungen festgestellt.".to_string()
    } else {
        parts.join("  ·  ")
    }
}
