use crate::config::AppConfig;
use crate::domain::{SearchPerformanceBreakdown, SiteOverviewReport, TrafficSourceBreakdown};
use crate::errors::Result;
use crate::google::analytics_data::{DateRange, ReportRequest, run_report};
use crate::google::search_console::{query, SearchAnalyticsRequest};
use crate::insights::insights_for_overview;

pub async fn build(config: &AppConfig, access_token: &str, days: u32) -> Result<SiteOverviewReport> {
    let property_id = config.require_ga4_property()?.to_string();
    let sc_url = config.require_search_console_url().ok().map(String::from);
    let property_name = config
        .properties
        .ga4_property_name
        .clone()
        .unwrap_or_else(|| property_id.clone());

    let date_range = DateRange::last_n_days(days);
    let date_label = format!("letzte {} Tage", days);

    // ── GA4: traffic by session default channel group ──────────────────────
    let channel_req = ReportRequest {
        property_id: property_id.clone(),
        date_ranges: vec![date_range.clone()],
        dimensions: vec!["sessionDefaultChannelGroup".into()],
        metrics: vec![
            "sessions".into(),
            "engagementRate".into(),
        ],
        dimension_filter: None,
        limit: Some(100),
        order_by: None,
    };

    let channel_report = run_report(access_token, channel_req).await?;

    let mut traffic = TrafficSourceBreakdown::default();
    let mut engagement_sum = 0.0f64;
    let mut engagement_count = 0;

    for row in &channel_report.rows {
        let channel = row.dimension_values.first().map(String::as_str).unwrap_or("");
        let sessions: i64 = row.metric_values.first().and_then(|v| v.parse().ok()).unwrap_or(0);
        let eng: f64 = row.metric_values.get(1).and_then(|v| v.parse().ok()).unwrap_or(0.0);

        traffic.total_sessions += sessions;
        engagement_sum += eng * sessions as f64;
        engagement_count += sessions;

        match channel {
            "Organic Search" => traffic.organic_sessions += sessions,
            "Direct"         => traffic.direct_sessions  += sessions,
            "Referral"       => traffic.referral_sessions += sessions,
            _                => traffic.other_sessions    += sessions,
        }
    }

    let engagement_rate = if engagement_count > 0 {
        engagement_sum / engagement_count as f64
    } else {
        0.0
    };

    // ── Search Console ──────────────────────────────────────────────────────
    let search = if let Some(sc) = sc_url {
        let sc_req = SearchAnalyticsRequest {
            site_url: sc,
            start_date: chrono_days_ago(days),
            end_date: chrono_yesterday(),
            dimensions: vec!["date".into()],
            page_filter: None,
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
            let total_pos: f64 = sc_resp.rows.iter().map(|r| r.position * r.impressions).sum();
            let total_imp: f64 = sc_resp.rows.iter().map(|r| r.impressions).sum();
            if total_imp > 0.0 { total_pos / total_imp } else { 0.0 }
        };

        SearchPerformanceBreakdown {
            clicks,
            impressions,
            ctr,
            average_position: avg_pos,
            top_queries: vec![],
        }
    } else {
        SearchPerformanceBreakdown::default()
    };

    let mut report = SiteOverviewReport {
        property_name,
        date_range: date_label,
        traffic,
        engagement_rate,
        search,
        insights: vec![],
    };

    insights_for_overview(&mut report);
    Ok(report)
}

// ─── Date helpers ─────────────────────────────────────────────────────────────

fn chrono_days_ago(days: u32) -> String {
    let date = chrono::Utc::now() - chrono::Duration::days(days as i64);
    date.format("%Y-%m-%d").to_string()
}

fn chrono_yesterday() -> String {
    let date = chrono::Utc::now() - chrono::Duration::days(1);
    date.format("%Y-%m-%d").to_string()
}
