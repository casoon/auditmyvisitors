use crate::config::AppConfig;
use crate::domain::{
    AiPageRow, PeriodDelta, QueryRow, SearchPerformanceBreakdown, SiteOverviewReport,
    SourceRow, TrafficSourceBreakdown,
};
use crate::errors::Result;
use crate::google::api::GoogleApi;
use crate::google::analytics_data::{DateRange, ReportRequest};
use crate::google::search_console::SearchAnalyticsRequest;
use crate::helpers;
use crate::insights::insights_for_overview;
use crate::opportunities::opportunities_from_overview;

/// Known AI referrer domains — classify AI traffic
const AI_DOMAINS: &[&str] = &[
    "chatgpt.com",
    "chat.openai.com",
    "perplexity.ai",
    "claude.ai",
    "gemini.google.com",
    "bard.google.com",
    "copilot.microsoft.com",
    "you.com",
    "phind.com",
    "poe.com",
    "mistral.ai",
    "groq.com",
    "together.ai",
    "character.ai",
    "kagi.com",
    "brave.com",
];

fn is_ai_source(source: &str) -> bool {
    let s = source.to_lowercase();
    AI_DOMAINS.iter().any(|ai| s.contains(ai))
}

pub async fn build(config: &AppConfig, api: &impl GoogleApi, days: u32) -> Result<SiteOverviewReport> {
    let property_id = config.require_ga4_property()?.to_string();
    let sc_url = config.require_search_console_url().ok().map(String::from);
    let property_name = config
        .properties
        .ga4_property_name
        .clone()
        .unwrap_or_else(|| property_id.clone());

    let date_label = format!("last {} days", days);

    // ── GA4: channel breakdown (current + previous period) ─────────────────
    let channel_req = ReportRequest {
        property_id: property_id.clone(),
        date_ranges: vec![
            DateRange::last_n_days(days),
            DateRange::prev_period(days),
        ],
        dimensions: vec!["sessionDefaultChannelGroup".into()],
        metrics: vec!["sessions".into(), "engagementRate".into()],
        dimension_filter: None,
        limit: Some(100),
        order_by: None,
    };

    // ── GA4: traffic by source domain ────────────────────────────────────────
    let source_req = ReportRequest {
        property_id: property_id.clone(),
        date_ranges: vec![DateRange::last_n_days(days)],
        dimensions: vec!["sessionSource".into()],
        metrics: vec!["sessions".into()],
        dimension_filter: None,
        limit: Some(100),
        order_by: Some(vec![serde_json::json!({
            "metric": { "metricName": "sessions" },
            "desc": true
        })]),
    };

    // ── GA4: AI traffic per page ──────────────────────────────────────────────
    // Filter sessionSource IN_LIST of known AI domains, group by pagePath
    let ai_values: Vec<serde_json::Value> = AI_DOMAINS.iter()
        .map(|d| serde_json::Value::String(d.to_string()))
        .collect();
    let ai_page_req = ReportRequest {
        property_id: property_id.clone(),
        date_ranges: vec![DateRange::last_n_days(days)],
        dimensions: vec!["pagePath".into()],
        metrics: vec!["sessions".into()],
        dimension_filter: Some(serde_json::json!({
            "filter": {
                "fieldName": "sessionSource",
                "inListFilter": { "values": ai_values }
            }
        })),
        limit: Some(20),
        order_by: Some(vec![serde_json::json!({
            "metric": { "metricName": "sessions" },
            "desc": true
        })]),
    };

    let (channel_report, source_report, ai_page_report) = tokio::join!(
        api.run_report(channel_req),
        api.run_report(source_req),
        api.run_report(ai_page_req),
    );
    let channel_report  = channel_report?;
    let source_report   = source_report?;
    let ai_page_report  = ai_page_report?;

    // ── Channel breakdown (current period = dateRange0) ──────────────────────
    let mut traffic = TrafficSourceBreakdown::default();
    let mut prev_sessions = 0i64;
    let mut engagement_sum = 0.0f64;
    let mut engagement_count = 0i64;

    // GA4 multi-dateRange returns rows with dimension "dateRange" injected
    // The dateRange dimension is appended automatically — index depends on request
    // We identify periods by checking if a "dateRange0" / "dateRange1" dim exists.
    // In practice with 2 date ranges + 1 user dim, each row has 2 dims:
    //   [0] = sessionDefaultChannelGroup, [1] = dateRange (date_range_0 or date_range_1)

    for row in &channel_report.rows {
        let channel   = row.dimension_values.first().map(String::as_str).unwrap_or("");
        let range_tag = row.dimension_values.get(1).map(String::as_str).unwrap_or("date_range_0");
        let sessions: i64 = row.metric_values.first().and_then(|v| v.parse().ok()).unwrap_or(0);
        let eng: f64      = row.metric_values.get(1).and_then(|v| v.parse().ok()).unwrap_or(0.0);

        if range_tag == "date_range_1" {
            // Previous period
            prev_sessions += sessions;
            continue;
        }

        // Current period
        traffic.total_sessions += sessions;
        engagement_sum  += eng * sessions as f64;
        engagement_count += sessions;

        match channel {
            "Organic Search" => traffic.organic_sessions  += sessions,
            "Direct"         => traffic.direct_sessions   += sessions,
            "Referral"       => traffic.referral_sessions += sessions,
            _                => traffic.other_sessions    += sessions,
        }
    }

    let engagement_rate = if engagement_count > 0 {
        engagement_sum / engagement_count as f64
    } else {
        0.0
    };

    // ── Source breakdown ──────────────────────────────────────────────────────
    let mut all_sources: Vec<SourceRow> = source_report
        .rows
        .iter()
        .filter_map(|row| {
            let src      = row.dimension_values.first()?.clone();
            let sessions: i64 = row.metric_values.first()?.parse().ok()?;
            if sessions == 0 { return None; }
            Some(SourceRow { source: src, sessions })
        })
        .collect();

    all_sources.sort_by_key(|s| std::cmp::Reverse(s.sessions));

    let ai_sources: Vec<SourceRow> = all_sources
        .iter()
        .filter(|s| is_ai_source(&s.source))
        .cloned()
        .collect();

    let top_sources: Vec<SourceRow> = all_sources
        .into_iter()
        .filter(|s| {
            let src = s.source.to_lowercase();
            src != "(direct)" && src != "(not set)" && src != "direct"
        })
        .take(15)
        .collect();

    // ── Search Console ──────────────────────────────────────────────────────
    let (search, prev_clicks, prev_impressions) = if let Some(sc) = sc_url {
        // Fetch totals (by date) for current + prev period in one request
        let totals_req = SearchAnalyticsRequest {
            site_url: sc.clone(),
            start_date: helpers::days_ago(days * 2),
            end_date: helpers::yesterday(),
            dimensions: vec!["date".into()],
            page_filter: None,
            row_limit: Some(1000),
        };

        let queries_req = SearchAnalyticsRequest {
            site_url: sc,
            start_date: helpers::days_ago(days),
            end_date: helpers::yesterday(),
            dimensions: vec!["query".into()],
            page_filter: None,
            row_limit: Some(50),
        };

        let (totals_resp, queries_resp) = tokio::join!(
            api.search_analytics(totals_req),
            api.search_analytics(queries_req),
        );
        let totals_resp = totals_resp?;
        let queries_resp = queries_resp?;

        // Split rows into current vs previous by date cutoff
        let cutoff = helpers::days_ago(days);
        let cutoff_date = chrono::NaiveDate::parse_from_str(&cutoff, "%Y-%m-%d")
            .unwrap_or_default();

        let mut cur_clicks = 0.0f64;
        let mut cur_impr   = 0.0f64;
        let mut prev_clicks = 0.0f64;
        let mut prev_impr  = 0.0f64;
        let mut cur_pos_sum = 0.0f64;
        let mut cur_pos_weight = 0.0f64;

        for row in &totals_resp.rows {
            let date_str = row.keys.first().map(String::as_str).unwrap_or("");
            let row_date = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
                .unwrap_or_default();

            if row_date >= cutoff_date {
                cur_clicks += row.clicks;
                cur_impr   += row.impressions;
                cur_pos_sum += row.position * row.impressions;
                cur_pos_weight += row.impressions;
            } else {
                prev_clicks += row.clicks;
                prev_impr   += row.impressions;
            }
        }

        let ctr     = if cur_impr > 0.0 { cur_clicks / cur_impr } else { 0.0 };
        let avg_pos = if cur_pos_weight > 0.0 { cur_pos_sum / cur_pos_weight } else { 0.0 };

        let top_queries: Vec<QueryRow> = queries_resp
            .rows
            .iter()
            .map(|r| QueryRow {
                query:       r.keys.first().cloned().unwrap_or_default(),
                clicks:      r.clicks,
                impressions: r.impressions,
                ctr:         r.ctr,
                position:    r.position,
                intent:      None,
                top_page:    None,
            })
            .collect();

        let search = SearchPerformanceBreakdown {
            clicks: cur_clicks,
            impressions: cur_impr,
            ctr,
            average_position: avg_pos,
            top_queries,
        };

        (search, prev_clicks, prev_impr)
    } else {
        (SearchPerformanceBreakdown::default(), 0.0, 0.0)
    };

    // ── Period-over-period delta ───────────────────────────────────────────────
    let trend = if prev_sessions > 0 || prev_clicks > 0.0 {
        let sessions_pct = helpers::pct_change(prev_sessions as f64, traffic.total_sessions as f64);
        let clicks_pct   = helpers::pct_change(prev_clicks, search.clicks);
        let impr_pct     = helpers::pct_change(prev_impressions, search.impressions);
        Some(PeriodDelta {
            sessions_pct,
            clicks_pct,
            impressions_pct: impr_pct,
            ctr_abs: 0.0,
            position_abs: 0.0,
        })
    } else {
        None
    };

    // ── AI traffic per page ───────────────────────────────────────────────────
    let ai_total_page_sessions: i64 = ai_page_report.rows.iter()
        .filter_map(|r| r.metric_values.first()?.parse::<i64>().ok())
        .sum();

    let ai_pages: Vec<AiPageRow> = ai_page_report.rows.iter().filter_map(|row| {
        let url      = row.dimension_values.first()?.clone();
        let sessions: i64 = row.metric_values.first()?.parse().ok()?;
        if sessions == 0 { return None; }
        let share = if ai_total_page_sessions > 0 {
            sessions as f64 / ai_total_page_sessions as f64
        } else { 0.0 };
        Some(AiPageRow { url, sessions, share_of_ai: share })
    }).collect();

    // ── Opportunities ─────────────────────────────────────────────────────────
    let opportunities = opportunities_from_overview(&search.top_queries, &[], days, &config.report.brand_terms);

    let mut report = SiteOverviewReport {
        property_name,
        date_range: date_label,
        traffic,
        engagement_rate,
        search,
        trend,
        top_sources,
        ai_sources,
        opportunities,
        ai_pages,
        insights: vec![],
    };

    insights_for_overview(&mut report, &config.thresholds);
    Ok(report)
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

    /// A request with two date ranges comes back as one row set with a
    /// `date_range_N` tag appended to the dimensions. Counting the previous
    /// period into the current one would roughly double every figure and make
    /// the trend read as flat.
    #[tokio::test]
    async fn the_previous_period_stays_out_of_the_current_totals() {
        let api = FixtureGoogleApi::new()
            .with_report(
                &["sessionDefaultChannelGroup"],
                vec![
                    (vec!["Organic Search", "date_range_0"], vec!["600", "0.5"]),
                    (vec!["Direct", "date_range_0"], vec!["400", "0.5"]),
                    (vec!["Organic Search", "date_range_1"], vec!["500", "0.5"]),
                    (vec!["Direct", "date_range_1"], vec!["300", "0.5"]),
                ],
            )
            .with_report(&["sessionSource"], vec![])
            .with_report(&["pagePath"], vec![])
            .with_search(&["date"], vec![])
            .with_search(&["query"], vec![]);

        let report = build(&config(), &api, 28).await.unwrap();

        assert_eq!(report.traffic.total_sessions, 1000, "current period only");
        assert_eq!(report.traffic.organic_sessions, 600);
        // 1000 against 800 in the previous period
        let trend = report.trend.expect("a previous period was returned");
        assert!((trend.sessions_pct - 25.0).abs() < 1e-9, "{}", trend.sessions_pct);
    }

    /// The engagement rate is weighted by sessions, and the previous period
    /// must not pull on that weight either.
    #[tokio::test]
    async fn engagement_is_session_weighted_over_the_current_period() {
        let api = FixtureGoogleApi::new()
            .with_report(
                &["sessionDefaultChannelGroup"],
                vec![
                    (vec!["Organic Search", "date_range_0"], vec!["100", "0.9"]),
                    (vec!["Direct", "date_range_0"], vec!["900", "0.1"]),
                    (vec!["Direct", "date_range_1"], vec!["900", "1.0"]),
                ],
            )
            .with_report(&["sessionSource"], vec![])
            .with_report(&["pagePath"], vec![])
            .with_search(&["date"], vec![])
            .with_search(&["query"], vec![]);

        let report = build(&config(), &api, 28).await.unwrap();

        // (0.9 * 100 + 0.1 * 900) / 1000
        assert!((report.engagement_rate - 0.18).abs() < 1e-9, "{}", report.engagement_rate);
    }

    /// Direct traffic is already its own line in the channel table, so it is
    /// filtered out of the source list — while AI referrers are picked out of
    /// the same list by domain.
    #[tokio::test]
    async fn sources_drop_direct_and_pick_out_ai_referrers() {
        let api = FixtureGoogleApi::new()
            .with_report(
                &["sessionDefaultChannelGroup"],
                vec![(vec!["Direct", "date_range_0"], vec!["10", "0.5"])],
            )
            .with_report(
                &["sessionSource"],
                vec![
                    (vec!["google"], vec!["500"]),
                    (vec!["(direct)"], vec!["400"]),
                    (vec!["chatgpt.com"], vec!["120"]),
                    (vec!["(not set)"], vec!["50"]),
                    (vec!["perplexity.ai"], vec!["30"]),
                    (vec!["ghost-referrer"], vec!["0"]),
                ],
            )
            .with_report(&["pagePath"], vec![])
            .with_search(&["date"], vec![])
            .with_search(&["query"], vec![]);

        let report = build(&config(), &api, 28).await.unwrap();

        let names: Vec<&str> = report.top_sources.iter().map(|s| s.source.as_str()).collect();
        assert_eq!(names, vec!["google", "chatgpt.com", "perplexity.ai"]);
        assert!(!names.contains(&"ghost-referrer"), "zero-session rows are dropped");

        let ai: Vec<&str> = report.ai_sources.iter().map(|s| s.source.as_str()).collect();
        assert_eq!(ai, vec!["chatgpt.com", "perplexity.ai"]);
    }

    /// Search Console returns both periods in one response, split by date. The
    /// average position is weighted by impressions, so a single low-traffic day
    /// at position 40 must not drag the average with the same force as a day
    /// carrying most of the impressions.
    #[tokio::test]
    async fn search_totals_split_on_the_date_cutoff() {
        let inside = helpers::days_ago(28);
        let outside = helpers::days_ago(40);

        let api = FixtureGoogleApi::new()
            .with_report(
                &["sessionDefaultChannelGroup"],
                vec![(vec!["Direct", "date_range_0"], vec!["10", "0.5"])],
            )
            .with_report(&["sessionSource"], vec![])
            .with_report(&["pagePath"], vec![])
            .with_search(
                &["date"],
                vec![
                    (vec![inside.as_str()], 90.0, 900.0, 0.1, 3.0),
                    (vec![outside.as_str()], 40.0, 400.0, 0.1, 9.0),
                ],
            )
            .with_search(&["query"], vec![]);

        let report = build(&config(), &api, 28).await.unwrap();

        assert_eq!(report.search.clicks, 90.0, "the older day belongs to the previous period");
        assert_eq!(report.search.impressions, 900.0);
        assert!((report.search.average_position - 3.0).abs() < 1e-9);

        let trend = report.trend.expect("previous clicks were returned");
        // 90 against 40
        assert!((trend.clicks_pct - 125.0).abs() < 1e-9, "{}", trend.clicks_pct);
    }
}
