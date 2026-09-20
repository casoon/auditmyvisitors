use chrono::NaiveDate;

use crate::config::AppConfig;
use crate::domain::{
    ComparisonDelta, ComparisonPeriod, ComparisonReport, SearchPerformanceBreakdown,
    TrafficSourceBreakdown,
};
use crate::errors::{AppError, Result};
use crate::google::api::GoogleApi;
use crate::google::analytics_data::{DateRange, ReportRequest};
use crate::google::search_console::SearchAnalyticsRequest;
use crate::helpers;
use crate::insights::insights_for_comparison;
use serde_json::json;

pub async fn build(
    config: &AppConfig,
    api: &impl GoogleApi,
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
        api,
        &property_id,
        sc_url.as_deref(),
        url,
        &fmt(before_start),
        &fmt(before_end),
    ).await?;

    let (after_traffic, after_search) = fetch_period(
        api,
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
    api: &impl GoogleApi,
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
        let path = helpers::extract_path(u);
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

    let ga_report = api.run_report(req).await?;

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

        let sc_resp = api.search_analytics(sc_req).await?;

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
    ComparisonDelta {
        sessions_abs: a_t.total_sessions - b_t.total_sessions,
        sessions_pct: helpers::pct_change(b_t.total_sessions as f64, a_t.total_sessions as f64),
        organic_sessions_abs: a_t.organic_sessions - b_t.organic_sessions,
        organic_sessions_pct: helpers::pct_change(b_t.organic_sessions as f64, a_t.organic_sessions as f64),
        engagement_rate_abs: 0.0,
        clicks_abs: a_s.clicks - b_s.clicks,
        clicks_pct: helpers::pct_change(b_s.clicks, a_s.clicks),
        impressions_abs: a_s.impressions - b_s.impressions,
        impressions_pct: helpers::pct_change(b_s.impressions, a_s.impressions),
        ctr_abs: a_s.ctr - b_s.ctr,
        position_abs: a_s.average_position - b_s.average_position,
    }
}

fn generate_summary(delta: &ComparisonDelta) -> String {
    let mut parts = Vec::new();

    if delta.sessions_pct.abs() >= 5.0 {
        parts.push(format!(
            "Sessions {:+.0}% ({:+})",
            delta.sessions_pct, delta.sessions_abs
        ));
    }
    if delta.clicks_pct.abs() >= 5.0 {
        parts.push(format!(
            "Clicks {:+.0}% ({:+.0})",
            delta.clicks_pct, delta.clicks_abs
        ));
    }
    if delta.position_abs.abs() >= 1.0 {
        parts.push(format!("Position {:+.1}", delta.position_abs));
    }

    if parts.is_empty() {
        "No significant changes detected.".to_string()
    } else {
        parts.join("  ·  ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::google::api::FixtureGoogleApi;

    fn config() -> AppConfig {
        let mut c = AppConfig::default();
        c.set_ga4_property("properties/1".into(), "example.com".into());
        c.set_search_console_url("https://example.com/".into());
        c
    }

    /// The change date belongs to the *after* period, and the before period
    /// ends the day prior. An off-by-one here puts the deploy day itself on the
    /// wrong side of the comparison it is supposed to explain.
    #[tokio::test]
    async fn the_change_date_opens_the_after_period() {
        let api = FixtureGoogleApi::new()
            .with_report(&["sessionDefaultChannelGroup"], vec![])
            .with_search(&["date"], vec![]);

        let report = build(&config(), &api, None, 30, 30, "2026-03-01")
            .await
            .unwrap();

        assert_eq!(report.before.start_date, "2026-01-30");
        assert_eq!(report.before.end_date, "2026-02-28");
        assert_eq!(report.after.start_date, "2026-03-01");
        assert_eq!(report.after.end_date, "2026-03-30");
    }

    /// Asymmetric windows are allowed, and each has to span exactly what was
    /// asked for.
    #[tokio::test]
    async fn windows_span_the_requested_number_of_days() {
        let api = FixtureGoogleApi::new()
            .with_report(&["sessionDefaultChannelGroup"], vec![])
            .with_search(&["date"], vec![]);

        let report = build(&config(), &api, None, 7, 14, "2026-03-01")
            .await
            .unwrap();

        // 7 days ending 2026-02-28
        assert_eq!(report.before.start_date, "2026-02-22");
        assert_eq!(report.before.end_date, "2026-02-28");
        // 14 days from 2026-03-01
        assert_eq!(report.after.start_date, "2026-03-01");
        assert_eq!(report.after.end_date, "2026-03-14");
    }

    /// Both periods ask the same question with the same dimensions and differ
    /// only in their dates. Swapping the two answers would invert every delta
    /// and turn a drop into a win.
    #[tokio::test]
    async fn the_two_periods_do_not_get_swapped() {
        let api = FixtureGoogleApi::new()
            .with_report_at(
                &["sessionDefaultChannelGroup"],
                "2026-01-30",
                vec![(vec!["Organic Search"], vec!["1000"])],
            )
            .with_report_at(
                &["sessionDefaultChannelGroup"],
                "2026-03-01",
                vec![(vec!["Organic Search"], vec!["750"])],
            )
            .with_search_at(
                &["date"],
                "2026-01-30",
                vec![(vec!["2026-02-01"], 200.0, 4000.0, 0.05, 5.0)],
            )
            .with_search_at(
                &["date"],
                "2026-03-01",
                vec![(vec!["2026-03-02"], 150.0, 3000.0, 0.05, 6.0)],
            );

        let report = build(&config(), &api, None, 30, 30, "2026-03-01")
            .await
            .unwrap();

        assert_eq!(report.before.sessions, 1000);
        assert_eq!(report.after.sessions, 750);
        assert_eq!(report.delta.sessions_abs, -250);
        assert!((report.delta.sessions_pct - -25.0).abs() < 1e-9);
        assert_eq!(report.delta.clicks_abs, -50.0);
        assert!((report.delta.position_abs - 1.0).abs() < 1e-9, "position got worse by 1");
    }

    /// A malformed `--since` is rejected before any request goes out.
    #[tokio::test]
    async fn an_unparseable_change_date_is_an_error() {
        let api = FixtureGoogleApi::new();

        let err = build(&config(), &api, None, 30, 30, "01.03.2026")
            .await
            .unwrap_err();

        assert!(matches!(err, AppError::InvalidDate(d) if d == "01.03.2026"));
    }
}
