use crate::config::AppConfig;
use crate::domain::{
    AiPageRow, PeriodDelta, QueryRow, SearchPerformanceBreakdown, SiteOverviewReport,
    SourceRow, TrafficSourceBreakdown,
};
use crate::errors::Result;
use crate::google::analytics_data::{DateRange, ReportRequest, run_report};
use crate::google::search_console::{query, SearchAnalyticsRequest};
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

pub async fn build(config: &AppConfig, access_token: &str, days: u32) -> Result<SiteOverviewReport> {
    let property_id = config.require_ga4_property()?.to_string();
    let sc_url = config.require_search_console_url().ok().map(String::from);
    let property_name = config
        .properties
        .ga4_property_name
        .clone()
        .unwrap_or_else(|| property_id.clone());

    let date_label = format!("letzte {} Tage", days);

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
        run_report(access_token, channel_req),
        run_report(access_token, source_req),
        run_report(access_token, ai_page_req),
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

    all_sources.sort_by(|a, b| b.sessions.cmp(&a.sessions));

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
            query(access_token, totals_req),
            query(access_token, queries_req),
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
    let opportunities = opportunities_from_overview(&search.top_queries, &[], days);

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

