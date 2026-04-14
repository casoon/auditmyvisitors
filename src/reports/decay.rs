use crate::config::AppConfig;
use crate::domain::{DecayPage, DecayReport, Insight, InsightCategory, InsightSeverity};
use crate::errors::Result;
use crate::google::search_console::{query, SearchAnalyticsRequest};
use crate::helpers;
use std::collections::HashMap;

pub async fn build(config: &AppConfig, access_token: &str, days: u32) -> Result<DecayReport> {
    let sc_url = config.require_search_console_url()?;
    let property_name = config
        .properties
        .ga4_property_name
        .clone()
        .unwrap_or_else(|| sc_url.to_string());

    let date_label = format!("letzte {} vs. vorherige {} Tage", days, days);

    // Current period: last N days
    let current_req = SearchAnalyticsRequest {
        site_url: sc_url.to_string(),
        start_date: helpers::days_ago(days),
        end_date: helpers::yesterday(),
        dimensions: vec!["page".into()],
        page_filter: None,
        row_limit: Some(500),
    };

    // Previous period: N*2..N+1 days ago
    let prev_req = SearchAnalyticsRequest {
        site_url: sc_url.to_string(),
        start_date: helpers::days_ago(days * 2),
        end_date: helpers::days_ago(days + 1),
        dimensions: vec!["page".into()],
        page_filter: None,
        row_limit: Some(500),
    };

    let (current_resp, prev_resp) = tokio::join!(
        query(access_token, current_req),
        query(access_token, prev_req),
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
            headline: "Kein signifikanter Content Decay erkannt".into(),
            explanation: "Keine Seite zeigt einen deutlichen Rueckgang in Klicks oder Impressionen.".into(),
        });
    } else {
        let total_click_loss: f64 = declining.iter()
            .map(|p| (p.clicks_before - p.clicks_after).max(0.0))
            .sum();

        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Trend,
            headline: format!("{} Seiten mit Decay erkannt", declining.len()),
            explanation: format!(
                "Geschaetzter Klick-Verlust: {:.0} Klicks im Vergleich zum Vorzeitraum.",
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
                headline: format!("{} Seiten komplett aus der Suche verschwunden", disappeared.len()),
                explanation: "Diese Seiten hatten vorher Traffic, erscheinen jetzt aber nicht mehr in den Suchergebnissen.".into(),
            });
        }

        let pos_drops: Vec<&DecayPage> = declining.iter()
            .filter(|p| p.position_delta >= 5.0)
            .collect();
        if !pos_drops.is_empty() {
            insights.push(Insight {
                severity: InsightSeverity::Warning,
                category: InsightCategory::Search,
                headline: format!("{} Seiten mit starkem Ranking-Verlust (>5 Positionen)", pos_drops.len()),
                explanation: "Deutliche Ranking-Verschlechterung — Content-Qualitaet und Wettbewerbsanalyse pruefen.".into(),
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
