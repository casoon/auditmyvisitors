//! Growth Drivers Report
//!
//! Identifies what's driving traffic growth or loss by comparing
//! current period vs. previous period across pages, queries, and channels.

use std::collections::HashMap;

use crate::config::AppConfig;
use crate::domain::{
    ChannelGrowthRow, GrowthReport, GrowthRow, Insight, InsightCategory, InsightSeverity, QueryRow,
};
use crate::errors::Result;
use crate::google::analytics_data::{run_report, DateRange, ReportRequest};
use crate::google::search_console::{query as sc_query, SearchAnalyticsRequest};
use crate::helpers;

pub async fn build(config: &AppConfig, access_token: &str, days: u32) -> Result<GrowthReport> {
    let property_id = config.require_ga4_property()?.to_string();
    let sc_url = config.require_search_console_url()?;
    let property_name = config
        .properties
        .ga4_property_name
        .clone()
        .unwrap_or_else(|| sc_url.to_string());
    let date_label = format!("last {} days vs. previous period", days);

    // ── Parallel API calls ──────────────────────────────────────────────────
    let (ga_pages, ga_channels, sc_queries_cur, sc_queries_prev) = tokio::join!(
        // GA4: sessions per page (current + previous)
        run_report(
            access_token,
            ReportRequest {
                property_id: property_id.clone(),
                date_ranges: vec![DateRange::last_n_days(days), DateRange::prev_period(days)],
                dimensions: vec!["pagePath".into()],
                metrics: vec!["sessions".into()],
                dimension_filter: None,
                limit: Some(200),
                order_by: None,
            },
        ),
        // GA4: sessions per channel (current + previous)
        run_report(
            access_token,
            ReportRequest {
                property_id: property_id.clone(),
                date_ranges: vec![DateRange::last_n_days(days), DateRange::prev_period(days)],
                dimensions: vec!["sessionDefaultChannelGroup".into()],
                metrics: vec!["sessions".into()],
                dimension_filter: None,
                limit: Some(20),
                order_by: None,
            },
        ),
        // SC: queries current period
        sc_query(
            access_token,
            SearchAnalyticsRequest {
                site_url: sc_url.to_string(),
                start_date: helpers::days_ago(days),
                end_date: helpers::yesterday(),
                dimensions: vec!["query".into()],
                page_filter: None,
                row_limit: Some(500),
            },
        ),
        // SC: queries previous period
        sc_query(
            access_token,
            SearchAnalyticsRequest {
                site_url: sc_url.to_string(),
                start_date: helpers::days_ago(days * 2),
                end_date: helpers::days_ago(days + 1),
                dimensions: vec!["query".into()],
                page_filter: None,
                row_limit: Some(500),
            },
        ),
    );

    let ga_pages = ga_pages?;
    let ga_channels = ga_channels?;
    let sc_cur = sc_queries_cur?;
    let sc_prev = sc_queries_prev?;

    // ── Page growth (GA4) ───────────────────────────────────────────────────
    let mut page_current: HashMap<String, f64> = HashMap::new();
    let mut page_previous: HashMap<String, f64> = HashMap::new();

    for row in &ga_pages.rows {
        let path = row.dimension_values.first().cloned().unwrap_or_default();
        let range_idx = row.dimension_values.last().cloned().unwrap_or_default();
        let sessions: f64 = row.metric_values.first().and_then(|v| v.parse().ok()).unwrap_or(0.0);

        if range_idx == "date_range_0" || row.metric_values.len() == 1 {
            *page_current.entry(path.clone()).or_default() += sessions;
        }
        if range_idx == "date_range_1" {
            *page_previous.entry(path).or_default() += sessions;
        }
    }

    // For multi-date-range responses, GA4 returns rows with dateRange dimension
    // If not present, we use the first value as current
    let mut page_growth: Vec<GrowthRow> = page_current
        .iter()
        .map(|(path, &cur)| {
            let prev = page_previous.get(path).copied().unwrap_or(0.0);
            GrowthRow {
                label: path.clone(),
                current: cur,
                previous: prev,
                delta: cur - prev,
                delta_pct: helpers::pct_change(prev, cur),
            }
        })
        .collect();

    page_growth.sort_by(|a, b| b.delta.partial_cmp(&a.delta).unwrap_or(std::cmp::Ordering::Equal));
    let top_growing_pages: Vec<GrowthRow> = page_growth.iter().filter(|r| r.delta > 0.0).take(10).cloned().collect();

    page_growth.sort_by(|a, b| a.delta.partial_cmp(&b.delta).unwrap_or(std::cmp::Ordering::Equal));
    let top_declining_pages: Vec<GrowthRow> = page_growth.iter().filter(|r| r.delta < 0.0).take(10).cloned().collect();

    // ── Channel growth (GA4) ────────────────────────────────────────────────
    let mut ch_current: HashMap<String, i64> = HashMap::new();
    let mut ch_previous: HashMap<String, i64> = HashMap::new();

    for row in &ga_channels.rows {
        let channel = row.dimension_values.first().cloned().unwrap_or_default();
        let range_idx = row.dimension_values.last().cloned().unwrap_or_default();
        let sessions: i64 = row.metric_values.first().and_then(|v| v.parse().ok()).unwrap_or(0);

        if range_idx == "date_range_0" || row.metric_values.len() == 1 {
            *ch_current.entry(channel.clone()).or_default() += sessions;
        }
        if range_idx == "date_range_1" {
            *ch_previous.entry(channel).or_default() += sessions;
        }
    }

    let mut channel_growth: Vec<ChannelGrowthRow> = ch_current
        .iter()
        .map(|(ch, &cur)| {
            let prev = ch_previous.get(ch).copied().unwrap_or(0);
            ChannelGrowthRow {
                channel: ch.clone(),
                current_sessions: cur,
                previous_sessions: prev,
                delta: cur - prev,
                delta_pct: helpers::pct_change(prev as f64, cur as f64),
            }
        })
        .collect();
    channel_growth.sort_by(|a, b| b.delta.cmp(&a.delta));

    // ── Query growth (SC) ───────────────────────────────────────────────────
    let mut q_cur_map: HashMap<String, f64> = HashMap::new();
    let mut q_prev_map: HashMap<String, f64> = HashMap::new();

    for row in &sc_cur.rows {
        let q = row.keys.first().cloned().unwrap_or_default();
        q_cur_map.insert(q, row.clicks);
    }
    for row in &sc_prev.rows {
        let q = row.keys.first().cloned().unwrap_or_default();
        q_prev_map.insert(q, row.clicks);
    }

    let mut query_growth: Vec<GrowthRow> = q_cur_map
        .iter()
        .map(|(q, &cur)| {
            let prev = q_prev_map.get(q).copied().unwrap_or(0.0);
            GrowthRow {
                label: q.clone(),
                current: cur,
                previous: prev,
                delta: cur - prev,
                delta_pct: helpers::pct_change(prev, cur),
            }
        })
        .collect();
    query_growth.sort_by(|a, b| b.delta.partial_cmp(&a.delta).unwrap_or(std::cmp::Ordering::Equal));
    let top_growing_queries: Vec<GrowthRow> = query_growth.iter().filter(|r| r.delta > 0.0).take(10).cloned().collect();

    // New queries: in current period but not in previous
    let new_queries: Vec<QueryRow> = sc_cur
        .rows
        .iter()
        .filter(|r| {
            let q = r.keys.first().cloned().unwrap_or_default();
            !q_prev_map.contains_key(&q) && r.clicks >= 1.0
        })
        .take(20)
        .map(|r| QueryRow {
            query: r.keys.first().cloned().unwrap_or_default(),
            clicks: r.clicks,
            impressions: r.impressions,
            ctr: r.ctr,
            position: r.position,
            intent: None,
        })
        .collect();

    // ── Insights ────────────────────────────────────────────────────────────
    let mut insights = Vec::new();

    if let Some(top) = top_growing_pages.first() {
        insights.push(Insight {
            severity: InsightSeverity::Positive,
            category: InsightCategory::Traffic,
            headline: format!("Strongest growth driver: {}", top.label),
            explanation: format!(
                "+{:.0} sessions compared to previous period ({:+.0}%)",
                top.delta, top.delta_pct
            ),
        });
    }

    if !top_declining_pages.is_empty() {
        let total_loss: f64 = top_declining_pages.iter().map(|r| r.delta).sum();
        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Traffic,
            headline: format!("{} pages with decline", top_declining_pages.len()),
            explanation: format!("{:.0} fewer sessions compared to previous period", total_loss.abs()),
        });
    }

    if !new_queries.is_empty() {
        let new_clicks: f64 = new_queries.iter().map(|q| q.clicks).sum();
        insights.push(Insight {
            severity: InsightSeverity::Info,
            category: InsightCategory::Search,
            headline: format!("{} new search queries discovered", new_queries.len()),
            explanation: format!("{:.0} clicks from queries not present in the previous period", new_clicks),
        });
    }

    // Channel shift insight
    if let Some(fastest) = channel_growth.iter().filter(|c| c.delta > 0).max_by_key(|c| c.delta) {
        insights.push(Insight {
            severity: InsightSeverity::Info,
            category: InsightCategory::Traffic,
            headline: format!("Strongest channel growth: {}", fastest.channel),
            explanation: format!("+{} Sessions ({:+.0}%)", fastest.delta, fastest.delta_pct),
        });
    }

    Ok(GrowthReport {
        property_name,
        date_range: date_label,
        top_growing_pages,
        top_declining_pages,
        top_growing_queries,
        new_queries,
        channel_growth,
        insights,
    })
}
