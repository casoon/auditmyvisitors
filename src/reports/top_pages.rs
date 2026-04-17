use crate::config::AppConfig;
use crate::domain::{PageSummary, SearchPerformanceBreakdown, TopPagesReport};
use crate::errors::Result;
use crate::google::analytics_data::{DateRange, ReportRequest, run_report};
use crate::google::search_console::{query, SearchAnalyticsRequest};
use crate::helpers;
use crate::insights::insights_for_top_pages;
use std::collections::HashMap;

pub async fn build(
    config: &AppConfig,
    access_token: &str,
    days: u32,
    limit: usize,
    sort_by: &str,
) -> Result<TopPagesReport> {
    let property_id = config.require_ga4_property()?.to_string();
    let sc_url = config.require_search_console_url().ok().map(String::from);
    let property_name = config
        .properties
        .ga4_property_name
        .clone()
        .unwrap_or_else(|| property_id.clone());

    let date_range = DateRange::last_n_days(days);
    let date_label = format!("last {} days", days);

    // ── GA4: sessions + engagement per page ──────────────────────────────────
    let req = ReportRequest {
        property_id: property_id.clone(),
        date_ranges: vec![date_range.clone()],
        dimensions: vec!["pagePath".into(), "sessionDefaultChannelGroup".into()],
        metrics: vec![
            "sessions".into(),
            "engagementRate".into(),
            "averageSessionDuration".into(),
            "bounceRate".into(),
            "newUsers".into(),
            "keyEvents".into(),
        ],
        dimension_filter: None,
        limit: Some(500),
        order_by: Some(vec![serde_json::json!({
            "metric": { "metricName": "sessions" },
            "desc": true
        })]),
    };

    let event_req = ReportRequest {
        property_id: property_id.clone(),
        date_ranges: vec![date_range.clone()],
        dimensions: vec!["pagePath".into(), "eventName".into()],
        metrics: vec!["eventCount".into()],
        dimension_filter: None,
        limit: Some(5000),
        order_by: None,
    };

    let (ga_report, event_report) = tokio::join!(
        run_report(access_token, req),
        run_report(access_token, event_req),
    );
    let ga_report = ga_report?;
    let event_report = event_report?;

    // Aggregate per page path
    let mut page_map: HashMap<String, PageSummary> = HashMap::new();

    for row in &ga_report.rows {
        let path = row.dimension_values.first().cloned().unwrap_or_default();
        let channel = row.dimension_values.get(1).map(String::as_str).unwrap_or("");
        let sessions: i64 = row.metric_values.first().and_then(|v| v.parse().ok()).unwrap_or(0);
        let eng: f64 = row.metric_values.get(1).and_then(|v| v.parse().ok()).unwrap_or(0.0);
        let dur: f64 = row.metric_values.get(2).and_then(|v| v.parse().ok()).unwrap_or(0.0);
        let bounce: f64 = row.metric_values.get(3).and_then(|v| v.parse().ok()).unwrap_or(0.0);
        let new_users: i64 = row.metric_values.get(4).and_then(|v| v.parse::<f64>().ok()).map(|v| v as i64).unwrap_or(0);
        let key_events: i64 = row.metric_values.get(5).and_then(|v| v.parse::<f64>().ok()).map(|v| v as i64).unwrap_or(0);

        let entry = page_map.entry(path.clone()).or_insert_with(|| PageSummary {
            url: path.clone(),
            sessions: 0,
            organic_sessions: 0,
            direct_sessions: 0,
            engagement_rate: 0.0,
            bounce_rate: 0.0,
            avg_session_duration_secs: dur,
            new_user_share: 0.0,
            key_events: 0,
            scroll_events: 0,
            internal_link_clicks: 0,
            service_hint_clicks: 0,
            search: SearchPerformanceBreakdown::default(),
        });

        entry.sessions += sessions;
        entry.engagement_rate = eng;
        entry.bounce_rate = bounce;
        entry.key_events += key_events;
        entry.new_user_share += new_users as f64; // accumulate raw new users, convert to share below
        match channel {
            "Organic Search" => entry.organic_sessions += sessions,
            "Direct"         => entry.direct_sessions  += sessions,
            _                => {}
        }
    }

    for row in &event_report.rows {
        let path = row.dimension_values.first().cloned().unwrap_or_default();
        let event_name = row.dimension_values.get(1).cloned().unwrap_or_default().to_lowercase();
        let count: i64 = row.metric_values.first().and_then(|v| v.parse::<f64>().ok()).map(|v| v as i64).unwrap_or(0);
        if count == 0 {
            continue;
        }

        if let Some(entry) = page_map.get_mut(&path) {
            if event_name.starts_with("scroll") {
                entry.scroll_events += count;
            }
            if event_name == "internal_link_click" || event_name == "internal_click" {
                entry.internal_link_clicks += count;
            }
            if event_name == "service_hint_click" || event_name == "servicehint_click" {
                entry.service_hint_clicks += count;
                entry.internal_link_clicks += count;
            }
        }
    }

    // ── Search Console: per page ─────────────────────────────────────────────
    if let Some(sc) = sc_url {
        let sc_req = SearchAnalyticsRequest {
            site_url: sc,
            start_date: helpers::days_ago(days),
            end_date: helpers::yesterday(),
            dimensions: vec!["page".into()],
            page_filter: None,
            row_limit: Some(1000),
        };

        let sc_query_req = SearchAnalyticsRequest {
            site_url: sc_req.site_url.clone(),
            start_date: sc_req.start_date.clone(),
            end_date: sc_req.end_date.clone(),
            dimensions: vec!["page".into(), "query".into()],
            page_filter: None,
            row_limit: Some(2500),
        };

        let (sc_resp, sc_query_resp) = tokio::join!(
            query(access_token, sc_req),
            query(access_token, sc_query_req),
        );
        let sc_resp = sc_resp?;
        let sc_query_resp = sc_query_resp?;
        helpers::merge_sc_into_page_map(&sc_resp.rows, &mut page_map);
        helpers::merge_sc_queries_into_page_map(&sc_query_resp.rows, &mut page_map);
    }

    // ── Convert raw newUsers count → share ─────────────────────────────────
    for page in page_map.values_mut() {
        if page.sessions > 0 {
            page.new_user_share = page.new_user_share / page.sessions as f64;
        }
    }

    // ── Sort and truncate ────────────────────────────────────────────────────
    let mut pages: Vec<PageSummary> = page_map.into_values().collect();

    match sort_by {
        "clicks"      => pages.sort_by(|a, b| b.search.clicks.partial_cmp(&a.search.clicks).unwrap()),
        "impressions" => pages.sort_by(|a, b| b.search.impressions.partial_cmp(&a.search.impressions).unwrap()),
        _             => pages.sort_by(|a, b| b.sessions.cmp(&a.sessions)),
    }

    pages.truncate(limit);

    let mut report = TopPagesReport {
        property_name,
        date_range: date_label,
        pages,
        insights: vec![],
    };

    insights_for_top_pages(&mut report, &config.thresholds);
    Ok(report)
}
