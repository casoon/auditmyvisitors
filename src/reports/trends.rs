//! Weekly Trends Report
//!
//! Breaks down performance by week and identifies ranking jumps.

use std::collections::HashMap;

use crate::config::AppConfig;
use crate::domain::{GrowthRow, Insight, InsightCategory, InsightSeverity, TrendsReport, WeekRow};
use crate::errors::Result;
use crate::google::analytics_data::{run_report, DateRange, ReportRequest};
use crate::google::search_console::{query as sc_query, SearchAnalyticsRequest};
use crate::helpers;

pub async fn build(config: &AppConfig, access_token: &str, days: u32) -> Result<TrendsReport> {
    let property_id = config.require_ga4_property()?.to_string();
    let sc_url = config.require_search_console_url()?;
    let property_name = config
        .properties
        .ga4_property_name
        .clone()
        .unwrap_or_else(|| sc_url.to_string());
    let date_label = format!("last {} days (weekly trend)", days);

    // ── Parallel: GA4 daily sessions + SC daily + SC query comparison ────────
    let (ga_daily, sc_daily, sc_queries_recent, sc_queries_prev) = tokio::join!(
        run_report(
            access_token,
            ReportRequest {
                property_id,
                date_ranges: vec![DateRange::last_n_days(days)],
                dimensions: vec!["date".into()],
                metrics: vec!["sessions".into()],
                dimension_filter: None,
                limit: Some(500),
                order_by: None,
            },
        ),
        sc_query(
            access_token,
            SearchAnalyticsRequest {
                site_url: sc_url.to_string(),
                start_date: helpers::days_ago(days),
                end_date: helpers::yesterday(),
                dimensions: vec!["date".into()],
                page_filter: None,
                row_limit: Some(500),
            },
        ),
        // Recent 14 days queries for ranking jump detection
        sc_query(
            access_token,
            SearchAnalyticsRequest {
                site_url: sc_url.to_string(),
                start_date: helpers::days_ago(14),
                end_date: helpers::yesterday(),
                dimensions: vec!["query".into()],
                page_filter: None,
                row_limit: Some(500),
            },
        ),
        // Previous 14 days queries
        sc_query(
            access_token,
            SearchAnalyticsRequest {
                site_url: sc_url.to_string(),
                start_date: helpers::days_ago(28),
                end_date: helpers::days_ago(15),
                dimensions: vec!["query".into()],
                page_filter: None,
                row_limit: Some(500),
            },
        ),
    );

    let ga_daily = ga_daily?;
    let sc_daily = sc_daily?;
    let sc_recent = sc_queries_recent?;
    let sc_prev = sc_queries_prev?;

    // ── Aggregate by ISO week ───────────────────────────────────────────────
    // GA4 dates: YYYYMMDD, SC dates: YYYY-MM-DD
    let mut ga_weeks: HashMap<String, i64> = HashMap::new();
    for row in &ga_daily.rows {
        let date_str = row.dimension_values.first().cloned().unwrap_or_default();
        let sessions: i64 = row.metric_values.first().and_then(|v| v.parse().ok()).unwrap_or(0);
        let week = date_to_week_start(&date_str);
        *ga_weeks.entry(week).or_default() += sessions;
    }

    let mut sc_weeks: HashMap<String, (f64, f64, f64)> = HashMap::new(); // (clicks, impressions, pos_sum)
    let mut sc_week_counts: HashMap<String, f64> = HashMap::new();
    for row in &sc_daily.rows {
        let date_str = row.keys.first().cloned().unwrap_or_default();
        let week = date_to_week_start(&date_str);
        let entry = sc_weeks.entry(week.clone()).or_default();
        entry.0 += row.clicks;
        entry.1 += row.impressions;
        entry.2 += row.position * row.impressions; // weighted position
        *sc_week_counts.entry(week).or_default() += row.impressions;
    }

    let mut weeks: Vec<WeekRow> = ga_weeks
        .iter()
        .map(|(week, &sessions)| {
            let (clicks, impressions, _) = sc_weeks.get(week).copied().unwrap_or_default();
            let weight = sc_week_counts.get(week).copied().unwrap_or(0.0);
            let avg_pos = if weight > 0.0 {
                sc_weeks.get(week).map(|e| e.2 / weight).unwrap_or(0.0)
            } else {
                0.0
            };
            let ctr = if impressions > 0.0 { clicks / impressions } else { 0.0 };
            WeekRow {
                week_start: week.clone(),
                sessions,
                clicks,
                impressions,
                ctr,
                avg_position: avg_pos,
            }
        })
        .collect();
    weeks.sort_by(|a, b| a.week_start.cmp(&b.week_start));

    // ── Ranking jumps ───────────────────────────────────────────────────────
    let mut prev_pos: HashMap<String, f64> = HashMap::new();
    for row in &sc_prev.rows {
        let q = row.keys.first().cloned().unwrap_or_default();
        prev_pos.insert(q, row.position);
    }

    let mut ranking_jumps: Vec<GrowthRow> = sc_recent
        .rows
        .iter()
        .filter_map(|row| {
            let q = row.keys.first().cloned().unwrap_or_default();
            let prev = prev_pos.get(&q)?;
            let jump = prev - row.position; // positive = improved
            if jump.abs() >= 5.0 {
                Some(GrowthRow {
                    label: q,
                    current: row.position,
                    previous: *prev,
                    delta: -jump, // negative delta = position improved
                    delta_pct: helpers::pct_change(*prev, row.position),
                })
            } else {
                None
            }
        })
        .collect();
    ranking_jumps.sort_by(|a, b| a.delta.partial_cmp(&b.delta).unwrap_or(std::cmp::Ordering::Equal));
    ranking_jumps.truncate(15);

    // ── Insights ────────────────────────────────────────────────────────────
    let mut insights = Vec::new();

    // Trend direction
    if weeks.len() >= 2 {
        let last = weeks.last().unwrap();
        let prev = &weeks[weeks.len() - 2];
        let session_trend = helpers::pct_change(prev.sessions as f64, last.sessions as f64);
        if session_trend > 15.0 {
            insights.push(Insight {
                severity: InsightSeverity::Positive,
                category: InsightCategory::Trend,
                headline: format!("Sessions rising: {:+.0}% last week", session_trend),
                explanation: format!(
                    "{} -> {} sessions (week {} vs. {})",
                    prev.sessions, last.sessions, prev.week_start, last.week_start
                ),
            });
        } else if session_trend < -15.0 {
            insights.push(Insight {
                severity: InsightSeverity::Warning,
                category: InsightCategory::Trend,
                headline: format!("Sessions falling: {:.0}% last week", session_trend),
                explanation: format!(
                    "{} -> {} sessions (week {} vs. {})",
                    prev.sessions, last.sessions, prev.week_start, last.week_start
                ),
            });
        }
    }

    let improved = ranking_jumps.iter().filter(|r| r.delta < 0.0).count();
    let declined = ranking_jumps.iter().filter(|r| r.delta > 0.0).count();
    if improved > 0 {
        insights.push(Insight {
            severity: InsightSeverity::Positive,
            category: InsightCategory::Search,
            headline: format!("{} keywords with ranking jump upward", improved),
            explanation: "Position improved by at least 5 positions in the last 2 weeks.".into(),
        });
    }
    if declined > 0 {
        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Search,
            headline: format!("{} keywords with ranking loss", declined),
            explanation: "Position worsened by at least 5 positions in the last 2 weeks.".into(),
        });
    }

    Ok(TrendsReport {
        property_name,
        date_range: date_label,
        weeks,
        ranking_jumps,
        insights,
    })
}

/// Convert a date string (YYYYMMDD or YYYY-MM-DD) to its ISO week start (Monday).
fn date_to_week_start(date_str: &str) -> String {
    let clean = date_str.replace('-', "");
    if let Ok(date) = chrono::NaiveDate::parse_from_str(&clean, "%Y%m%d") {
        use chrono::Datelike;
        let days_since_monday = date.weekday().num_days_from_monday();
        let monday = date - chrono::Duration::days(days_since_monday as i64);
        monday.format("%Y-%m-%d").to_string()
    } else {
        date_str.to_string()
    }
}
