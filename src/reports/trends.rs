//! Weekly Trends Report
//!
//! Breaks down performance by week and identifies ranking jumps.

use std::collections::HashMap;

use crate::config::AppConfig;
use crate::domain::{GrowthRow, Insight, InsightCategory, InsightSeverity, TrendsReport, WeekRow};
use crate::errors::Result;
use crate::google::api::GoogleApi;
use crate::google::analytics_data::{DateRange, ReportRequest};
use crate::google::search_console::SearchAnalyticsRequest;
use crate::helpers;

pub async fn build(config: &AppConfig, api: &impl GoogleApi, days: u32) -> Result<TrendsReport> {
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
        api.run_report(
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
        api.search_analytics(
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
        api.search_analytics(
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
        api.search_analytics(
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

    fn api(
        ga_daily: Vec<(Vec<&str>, Vec<&str>)>,
        sc_daily: Vec<(Vec<&str>, f64, f64, f64, f64)>,
        recent_queries: Vec<(Vec<&str>, f64, f64, f64, f64)>,
        prev_queries: Vec<(Vec<&str>, f64, f64, f64, f64)>,
    ) -> FixtureGoogleApi {
        FixtureGoogleApi::new()
            .with_report(&["date"], ga_daily)
            .with_search(&["date"], sc_daily)
            .with_search_at(&["query"], &helpers::days_ago(14), recent_queries)
            .with_search_at(&["query"], &helpers::days_ago(28), prev_queries)
    }

    /// GA4 dates arrive as `YYYYMMDD` and Search Console dates as `YYYY-MM-DD`.
    /// If the two formats do not bucket to the same week, every week ends up
    /// with sessions or with clicks but never both.
    #[tokio::test]
    async fn both_date_formats_land_in_the_same_week() {
        let report = build(
            &config(),
            &api(
                vec![(vec!["20260304"], vec!["300"])],
                vec![(vec!["2026-03-04"], 50.0, 1000.0, 0.05, 6.0)],
                vec![],
                vec![],
            ),
            28,
        )
        .await
        .unwrap();

        assert_eq!(report.weeks.len(), 1);
        let week = &report.weeks[0];
        assert_eq!(week.week_start, "2026-03-02", "the Monday of that week");
        assert_eq!(week.sessions, 300);
        assert_eq!(week.clicks, 50.0, "search data joined onto the same week");
    }

    /// Days collapse into their ISO week, and weeks come out in order.
    #[tokio::test]
    async fn days_aggregate_into_ordered_weeks() {
        let report = build(
            &config(),
            &api(
                vec![
                    (vec!["20260309"], vec!["10"]),
                    (vec!["20260302"], vec!["100"]),
                    (vec!["20260308"], vec!["200"]),
                ],
                vec![],
                vec![],
                vec![],
            ),
            28,
        )
        .await
        .unwrap();

        let starts: Vec<&str> = report.weeks.iter().map(|w| w.week_start.as_str()).collect();
        assert_eq!(starts, vec!["2026-03-02", "2026-03-09"], "ascending");
        assert_eq!(report.weeks[0].sessions, 300, "Monday plus the Sunday after it");
        assert_eq!(report.weeks[1].sessions, 10);
    }

    /// The weekly position is weighted by impressions, so a single quiet day at
    /// a poor rank does not drag the week down.
    #[tokio::test]
    async fn weekly_position_is_impression_weighted() {
        let report = build(
            &config(),
            &api(
                vec![(vec!["20260302"], vec!["10"])],
                vec![
                    (vec!["2026-03-02"], 0.0, 9000.0, 0.0, 2.0),
                    (vec!["2026-03-03"], 0.0, 1000.0, 0.0, 22.0),
                ],
                vec![],
                vec![],
            ),
            28,
        )
        .await
        .unwrap();

        // (2 * 9000 + 22 * 1000) / 10000 = 4.0, not the mean of 12
        assert!((report.weeks[0].avg_position - 4.0).abs() < 1e-9, "{}", report.weeks[0].avg_position);
    }

    /// Only moves of five places or more are reported, and an improvement is a
    /// negative delta because a lower position number is better.
    #[tokio::test]
    async fn ranking_jumps_need_five_places() {
        let report = build(
            &config(),
            &api(
                vec![(vec!["20260302"], vec!["1"])],
                vec![],
                vec![
                    (vec!["climber"], 0.0, 0.0, 0.0, 3.0),
                    (vec!["faller"], 0.0, 0.0, 0.0, 18.0),
                    (vec!["steady"], 0.0, 0.0, 0.0, 9.0),
                ],
                vec![
                    (vec!["climber"], 0.0, 0.0, 0.0, 14.0),
                    (vec!["faller"], 0.0, 0.0, 0.0, 7.0),
                    (vec!["steady"], 0.0, 0.0, 0.0, 11.0),
                ],
            ),
            28,
        )
        .await
        .unwrap();

        let labels: Vec<&str> = report.ranking_jumps.iter().map(|r| r.label.as_str()).collect();
        assert!(!labels.contains(&"steady"), "two places is not a jump");
        assert_eq!(labels.len(), 2);

        let climber = report.ranking_jumps.iter().find(|r| r.label == "climber").unwrap();
        assert_eq!(climber.previous, 14.0);
        assert_eq!(climber.current, 3.0);
        assert!(climber.delta < 0.0, "an improvement reads as a negative delta");

        // best improvement first
        assert_eq!(report.ranking_jumps[0].label, "climber");
    }

    /// A query that only appears in the recent period has nothing to compare
    /// against and is not a jump.
    #[tokio::test]
    async fn a_query_without_a_previous_position_is_not_a_jump() {
        let report = build(
            &config(),
            &api(
                vec![(vec!["20260302"], vec!["1"])],
                vec![],
                vec![(vec!["brand new"], 0.0, 0.0, 0.0, 2.0)],
                vec![],
            ),
            28,
        )
        .await
        .unwrap();

        assert!(report.ranking_jumps.is_empty());
    }
}
