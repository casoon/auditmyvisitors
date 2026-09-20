use crate::config::AppConfig;
use crate::domain::{DecayPage, DecayReport, Insight, InsightCategory, InsightSeverity};
use crate::errors::Result;
use crate::google::{SC_MAX_ROWS};
use crate::google::api::GoogleApi;
use crate::google::search_console::SearchAnalyticsRequest;
use crate::helpers;
use std::collections::HashMap;

pub async fn build(config: &AppConfig, api: &impl GoogleApi, days: u32) -> Result<DecayReport> {
    let sc_url = config.require_search_console_url()?;
    let property_name = config
        .properties
        .ga4_property_name
        .clone()
        .unwrap_or_else(|| sc_url.to_string());

    let date_label = format!("last {} vs. previous {} days", days, days);

    // Current period: last N days
    let current_req = SearchAnalyticsRequest {
        site_url: sc_url.to_string(),
        start_date: helpers::days_ago(days),
        end_date: helpers::yesterday(),
        dimensions: vec!["page".into()],
        page_filter: None,
        row_limit: Some(SC_MAX_ROWS),
    };

    // Previous period: N*2..N+1 days ago
    let prev_req = SearchAnalyticsRequest {
        site_url: sc_url.to_string(),
        start_date: helpers::days_ago(days * 2),
        end_date: helpers::days_ago(days + 1),
        dimensions: vec!["page".into()],
        page_filter: None,
        row_limit: Some(SC_MAX_ROWS),
    };

    let (current_resp, prev_resp) = tokio::join!(
        api.search_analytics(current_req),
        api.search_analytics(prev_req),
    );
    let current_resp = current_resp?;
    let prev_resp = prev_resp?;

    // Index previous period by page URL
    let mut prev_map: HashMap<String, (f64, f64, f64)> = HashMap::new(); // clicks, impr, pos
    for row in &prev_resp.rows {
        let url = row.keys.first().cloned().unwrap_or_default();
        prev_map.insert(url, (row.clicks, row.impressions, row.position));
    }

    // Compare and find declining pages
    let mut declining: Vec<DecayPage> = Vec::new();

    for row in &current_resp.rows {
        let url = row.keys.first().cloned().unwrap_or_default();
        if let Some(&(prev_clicks, prev_impr, prev_pos)) = prev_map.get(&url) {
            // Only consider pages with meaningful previous traffic
            if prev_clicks < 5.0 && prev_impr < 50.0 {
                continue;
            }

            let clicks_pct = helpers::pct_change(prev_clicks, row.clicks);
            let impr_pct = helpers::pct_change(prev_impr, row.impressions);
            let pos_delta = row.position - prev_pos;

            // Decay: significant decline in clicks OR impressions
            let is_declining = clicks_pct <= -20.0 || impr_pct <= -20.0 || pos_delta >= 3.0;

            if is_declining {
                declining.push(DecayPage {
                    url,
                    clicks_before: prev_clicks,
                    clicks_after: row.clicks,
                    clicks_pct,
                    impressions_before: prev_impr,
                    impressions_after: row.impressions,
                    impressions_pct: impr_pct,
                    position_before: prev_pos,
                    position_after: row.position,
                    position_delta: pos_delta,
                });
            }
        }
    }

    // Also check pages that disappeared entirely (in prev but not in current)
    let current_urls: std::collections::HashSet<String> = current_resp.rows.iter()
        .filter_map(|r| r.keys.first().cloned())
        .collect();

    for (url, (prev_clicks, prev_impr, prev_pos)) in &prev_map {
        if !current_urls.contains(url) && (*prev_clicks >= 5.0 || *prev_impr >= 50.0) {
            declining.push(DecayPage {
                url: url.clone(),
                clicks_before: *prev_clicks,
                clicks_after: 0.0,
                clicks_pct: -100.0,
                impressions_before: *prev_impr,
                impressions_after: 0.0,
                impressions_pct: -100.0,
                position_before: *prev_pos,
                position_after: 0.0,
                position_delta: 0.0,
            });
        }
    }

    // Sort by absolute click loss (most impactful first)
    declining.sort_by(|a, b| {
        let loss_a = a.clicks_before - a.clicks_after;
        let loss_b = b.clicks_before - b.clicks_after;
        loss_b.partial_cmp(&loss_a).unwrap_or(std::cmp::Ordering::Equal)
    });

    declining.truncate(30);

    // Insights
    let mut insights = Vec::new();

    if declining.is_empty() {
        insights.push(Insight {
            severity: InsightSeverity::Positive,
            category: InsightCategory::Trend,
            headline: "No significant content decay detected".into(),
            explanation: "No page shows a significant decline in clicks or impressions.".into(),
        });
    } else {
        let total_click_loss: f64 = declining.iter()
            .map(|p| (p.clicks_before - p.clicks_after).max(0.0))
            .sum();

        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Trend,
            headline: format!("{} pages with decay detected", declining.len()),
            explanation: format!(
                "Estimated click loss: {:.0} clicks compared to the previous period.",
                total_click_loss
            ),
        });

        let disappeared: Vec<&DecayPage> = declining.iter()
            .filter(|p| p.clicks_pct <= -99.0)
            .collect();
        if !disappeared.is_empty() {
            insights.push(Insight {
                severity: InsightSeverity::Critical,
                category: InsightCategory::Search,
                headline: format!("{} pages completely disappeared from search", disappeared.len()),
                explanation: "These pages previously had traffic but no longer appear in search results.".into(),
            });
        }

        let pos_drops: Vec<&DecayPage> = declining.iter()
            .filter(|p| p.position_delta >= 5.0)
            .collect();
        if !pos_drops.is_empty() {
            insights.push(Insight {
                severity: InsightSeverity::Warning,
                category: InsightCategory::Search,
                headline: format!("{} pages with severe ranking loss (>5 positions)", pos_drops.len()),
                explanation: "Significant ranking deterioration — check content quality and competitive analysis.".into(),
            });
        }
    }

    Ok(DecayReport {
        property_name,
        date_range: date_label,
        days,
        declining_pages: declining,
        insights,
    })
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
        current: Vec<(Vec<&str>, f64, f64, f64, f64)>,
        previous: Vec<(Vec<&str>, f64, f64, f64, f64)>,
    ) -> FixtureGoogleApi {
        FixtureGoogleApi::new()
            .with_search_at(&["page"], &helpers::days_ago(28), current)
            .with_search_at(&["page"], &helpers::days_ago(56), previous)
    }

    /// Both periods ask for the same dimension and differ only in their dates.
    /// Reading them the wrong way round would report every recovering page as
    /// decaying.
    #[tokio::test]
    async fn the_periods_are_not_swapped() {
        let report = build(
            &config(),
            &api(
                vec![(vec!["https://example.com/a"], 40.0, 1000.0, 0.04, 5.0)],
                vec![(vec!["https://example.com/a"], 100.0, 2000.0, 0.05, 4.0)],
            ),
            28,
        )
        .await
        .unwrap();

        let page = &report.declining_pages[0];
        assert_eq!(page.clicks_before, 100.0);
        assert_eq!(page.clicks_after, 40.0);
        assert!((page.clicks_pct - -60.0).abs() < 1e-9);
    }

    /// The floor keeps noise out: a page that never had traffic worth the name
    /// is not a decay story, however badly it fell.
    #[tokio::test]
    async fn pages_below_the_traffic_floor_are_ignored() {
        let report = build(
            &config(),
            &api(
                vec![(vec!["https://example.com/tiny"], 0.0, 2.0, 0.0, 40.0)],
                // 4 clicks and 49 impressions — under both halves of the floor
                vec![(vec!["https://example.com/tiny"], 4.0, 49.0, 0.0816, 10.0)],
            ),
            28,
        )
        .await
        .unwrap();

        assert!(report.declining_pages.is_empty(), "{:?}", report.declining_pages);
    }

    /// One side of the floor is enough to qualify.
    #[tokio::test]
    async fn meeting_either_half_of_the_floor_qualifies() {
        let report = build(
            &config(),
            &api(
                vec![(vec!["https://example.com/impressions-only"], 0.0, 10.0, 0.0, 30.0)],
                // no clicks, but impressions over the threshold
                vec![(vec!["https://example.com/impressions-only"], 0.0, 500.0, 0.0, 12.0)],
            ),
            28,
        )
        .await
        .unwrap();

        assert_eq!(report.declining_pages.len(), 1);
        assert!((report.declining_pages[0].impressions_pct - -98.0).abs() < 1e-9);
    }

    /// Losing rank counts as decay even when the click count holds up — that is
    /// the early warning the report exists for.
    #[tokio::test]
    async fn a_position_drop_alone_counts_as_decay() {
        let report = build(
            &config(),
            &api(
                vec![(vec!["https://example.com/slipping"], 50.0, 1000.0, 0.05, 11.0)],
                vec![(vec!["https://example.com/slipping"], 50.0, 1000.0, 0.05, 8.0)],
            ),
            28,
        )
        .await
        .unwrap();

        assert_eq!(report.declining_pages.len(), 1, "clicks flat, position three places worse");
        assert!((report.declining_pages[0].position_delta - 3.0).abs() < 1e-9);
    }

    /// A page that vanished from the current period is the worst case and has
    /// to survive the join, not fall out of it.
    #[tokio::test]
    async fn a_page_that_disappeared_is_reported_as_a_total_loss() {
        let report = build(
            &config(),
            &api(
                vec![(vec!["https://example.com/still-here"], 10.0, 500.0, 0.02, 6.0)],
                vec![
                    (vec!["https://example.com/still-here"], 10.0, 500.0, 0.02, 6.0),
                    (vec!["https://example.com/gone"], 80.0, 1600.0, 0.05, 3.0),
                ],
            ),
            28,
        )
        .await
        .unwrap();

        let gone = report
            .declining_pages
            .iter()
            .find(|p| p.url.ends_with("/gone"))
            .expect("the vanished page is reported");
        assert_eq!(gone.clicks_after, 0.0);
        assert!((gone.clicks_pct - -100.0).abs() < 1e-9);
    }

    /// Ranking is by absolute clicks lost, so the biggest real loss leads —
    /// not the steepest percentage on a small page.
    #[tokio::test]
    async fn ranking_follows_absolute_click_loss() {
        let report = build(
            &config(),
            &api(
                vec![
                    (vec!["https://example.com/small"], 0.0, 100.0, 0.0, 9.0),
                    (vec!["https://example.com/big"], 600.0, 20000.0, 0.03, 4.0),
                ],
                vec![
                    // -100%, but only 20 clicks
                    (vec!["https://example.com/small"], 20.0, 400.0, 0.05, 7.0),
                    // -25%, but 200 clicks gone
                    (vec!["https://example.com/big"], 800.0, 25000.0, 0.032, 3.0),
                ],
            ),
            28,
        )
        .await
        .unwrap();

        assert_eq!(report.declining_pages[0].url, "https://example.com/big");
        assert_eq!(report.declining_pages[1].url, "https://example.com/small");
    }
}
