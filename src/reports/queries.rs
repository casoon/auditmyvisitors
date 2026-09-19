use std::collections::HashMap;

use crate::config::AppConfig;
use crate::domain::{Insight, InsightCategory, InsightSeverity, QueriesReport, QueryRow};
use crate::errors::Result;
use crate::google::search_console::{query, SearchAnalyticsRequest, SearchAnalyticsRow};
use crate::helpers;
use crate::intent;
use crate::opportunities::expected_ctr;

/// Map each query to the page it ranks best on (lowest average position wins).
fn best_page_per_query(rows: &[SearchAnalyticsRow]) -> HashMap<String, String> {
    let mut best: HashMap<String, (String, f64)> = HashMap::new();
    for row in rows {
        let query = row.keys.first().cloned().unwrap_or_default();
        let page = row.keys.get(1).cloned().unwrap_or_default();
        let entry = best.entry(query).or_insert((page.clone(), row.position));
        if row.position < entry.1 {
            *entry = (page, row.position);
        }
    }
    best.into_iter().map(|(q, (page, _))| (q, page)).collect()
}

pub async fn build(
    config: &AppConfig,
    access_token: &str,
    days: u32,
    limit: usize,
    sort_by: &str,
) -> Result<QueriesReport> {
    let sc_url = config.require_search_console_url()?;
    let property_name = config
        .properties
        .ga4_property_name
        .clone()
        .unwrap_or_else(|| sc_url.to_string());

    let date_label = format!("last {} days", days);

    let req = SearchAnalyticsRequest {
        site_url: sc_url.to_string(),
        start_date: helpers::days_ago(days),
        end_date: helpers::yesterday(),
        dimensions: vec!["query".into()],
        page_filter: None,
        row_limit: Some(500),
    };

    let req_pages = SearchAnalyticsRequest {
        site_url: sc_url.to_string(),
        start_date: helpers::days_ago(days),
        end_date: helpers::yesterday(),
        dimensions: vec!["query".into(), "page".into()],
        page_filter: None,
        row_limit: Some(5000),
    };

    let (resp, resp_pages) = tokio::join!(
        query(access_token, req),
        query(access_token, req_pages),
    );
    let resp = resp?;
    let resp_pages = resp_pages?;

    let query_top_page = best_page_per_query(&resp_pages.rows);

    let brand_terms = &config.report.brand_terms;

    let mut queries: Vec<QueryRow> = resp
        .rows
        .iter()
        .map(|r| {
            let q = r.keys.first().cloned().unwrap_or_default();
            let classified = intent::classify(&q, brand_terms);
            let top_page = query_top_page.get(&q).cloned();
            QueryRow {
                query: q,
                clicks: r.clicks,
                impressions: r.impressions,
                ctr: r.ctr,
                position: r.position,
                intent: Some(classified),
                top_page,
            }
        })
        .collect();

    // Sort
    match sort_by {
        "impressions" => queries.sort_by(|a, b| b.impressions.total_cmp(&a.impressions)),
        "ctr" => queries.sort_by(|a, b| b.ctr.total_cmp(&a.ctr)),
        "position" => queries.sort_by(|a, b| a.position.total_cmp(&b.position)),
        _ => queries.sort_by(|a, b| b.clicks.total_cmp(&a.clicks)),
    }

    queries.truncate(limit);

    // Brand / Non-Brand split
    let brand_clicks: f64 = queries.iter()
        .filter(|q| helpers::is_brand_query(&q.query, brand_terms))
        .map(|q| q.clicks)
        .sum();

    // Intent distribution
    let intents: Vec<intent::Intent> = queries.iter()
        .filter_map(|q| q.intent)
        .collect();
    let intent_dist = intent::distribution(&intents);

    // Aggregates
    let total_clicks: f64 = queries.iter().map(|q| q.clicks).sum();
    let non_brand_clicks = total_clicks - brand_clicks;
    let total_impressions: f64 = queries.iter().map(|q| q.impressions).sum();
    let avg_ctr = if total_impressions > 0.0 { total_clicks / total_impressions } else { 0.0 };
    let avg_position = if !queries.is_empty() {
        let weighted_sum: f64 = queries.iter().map(|q| q.position * q.impressions).sum();
        let weight: f64 = queries.iter().map(|q| q.impressions).sum();
        if weight > 0.0 { weighted_sum / weight } else { 0.0 }
    } else {
        0.0
    };

    // Insights
    let mut insights = Vec::new();

    // CTR opportunity queries
    let ctr_opps: Vec<&QueryRow> = queries.iter()
        .filter(|q| q.position <= 10.0 && q.impressions >= 50.0 && q.ctr < expected_ctr(q.position) * 0.7)
        .collect();
    if !ctr_opps.is_empty() {
        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Search,
            headline: format!("{} keywords with CTR below expectation", ctr_opps.len()),
            explanation: format!(
                "These keywords rank on page 1, but CTR is significantly lower than expected. Best potential: \"{}\"",
                ctr_opps.first().map(|q| q.query.as_str()).unwrap_or("-")
            ),
        });
    }

    // Page-2 queries with high impressions
    let page2_queries: Vec<&QueryRow> = queries.iter()
        .filter(|q| q.position > 10.0 && q.position <= 20.0 && q.impressions >= 100.0)
        .collect();
    if !page2_queries.is_empty() {
        insights.push(Insight {
            severity: InsightSeverity::Info,
            category: InsightCategory::Search,
            headline: format!("{} keywords on page 2 with high volume", page2_queries.len()),
            explanation: "These keywords have high impressions but rank on page 2. Content expansion can enable the jump to page 1.".into(),
        });
    }

    // Top performers
    let top_performers: Vec<&QueryRow> = queries.iter()
        .filter(|q| q.ctr > 0.05 && q.clicks > 20.0)
        .collect();
    if !top_performers.is_empty() {
        insights.push(Insight {
            severity: InsightSeverity::Positive,
            category: InsightCategory::Search,
            headline: format!("{} high-performing keywords", top_performers.len()),
            explanation: format!(
                "Keywords with >5% CTR and >20 clicks. Strongest: \"{}\" ({:.1}% CTR)",
                top_performers.first().map(|q| q.query.as_str()).unwrap_or("-"),
                top_performers.first().map(|q| q.ctr * 100.0).unwrap_or(0.0)
            ),
        });
    }

    // Brand insight
    if !brand_terms.is_empty() && total_clicks > 0.0 {
        let brand_pct = brand_clicks / total_clicks * 100.0;
        if brand_pct > 60.0 {
            insights.push(Insight {
                severity: InsightSeverity::Info,
                category: InsightCategory::Search,
                headline: format!("Brand-heavy: {:.0}% of clicks are brand queries", brand_pct),
                explanation: "A high brand share can indicate good brand awareness but obscures organic growth potential for generic terms.".into(),
            });
        } else if brand_pct < 20.0 {
            insights.push(Insight {
                severity: InsightSeverity::Positive,
                category: InsightCategory::Search,
                headline: format!("Strong non-brand traffic: {:.0}% of clicks are generic", 100.0 - brand_pct),
                explanation: "The majority of traffic comes from generic keywords — good organic reach.".into(),
            });
        }
    }

    // Intent insights
    if intent_dist.commercial_pct > 20.0 {
        insights.push(Insight {
            severity: InsightSeverity::Info,
            category: InsightCategory::Search,
            headline: format!("{:.0}% of queries have commercial intent", intent_dist.commercial_pct),
            explanation: "A significant share of search queries indicates comparison or purchase intent. Check whether corresponding landing pages exist.".into(),
        });
    }
    if intent_dist.transactional_pct > 10.0 {
        insights.push(Insight {
            severity: InsightSeverity::Positive,
            category: InsightCategory::Search,
            headline: format!("{:.0}% of queries are transactional", intent_dist.transactional_pct),
            explanation: "Transactional search queries indicate conversion-ready traffic — high value for the website.".into(),
        });
    }

    Ok(QueriesReport {
        property_name,
        date_range: date_label,
        queries,
        total_clicks,
        total_impressions,
        avg_ctr,
        avg_position,
        brand_clicks,
        non_brand_clicks,
        intent_distribution: intent_dist,
        insights,
    })
}


#[cfg(test)]
mod tests {
    use super::*;

    fn row(query: &str, page: &str, position: f64) -> SearchAnalyticsRow {
        SearchAnalyticsRow {
            keys: vec![query.into(), page.into()],
            clicks: 0.0,
            impressions: 0.0,
            ctr: 0.0,
            position,
        }
    }

    #[test]
    fn best_page_wins_on_lowest_position() {
        let rows = vec![
            row("rust cli", "https://example.com/blog/cli", 8.4),
            row("rust cli", "https://example.com/docs/cli", 3.1),
            row("rust cli", "https://example.com/", 19.0),
        ];
        let map = best_page_per_query(&rows);
        assert_eq!(map.get("rust cli").map(String::as_str), Some("https://example.com/docs/cli"));
    }

    #[test]
    fn queries_are_kept_apart() {
        let rows = vec![
            row("alpha", "https://example.com/a", 5.0),
            row("beta", "https://example.com/b", 2.0),
        ];
        let map = best_page_per_query(&rows);
        assert_eq!(map.len(), 2);
        assert_eq!(map.get("beta").map(String::as_str), Some("https://example.com/b"));
    }

    #[test]
    fn row_without_page_dimension_yields_empty_page() {
        let rows = vec![SearchAnalyticsRow {
            keys: vec!["lonely".into()],
            clicks: 0.0,
            impressions: 0.0,
            ctr: 0.0,
            position: 4.0,
        }];
        let map = best_page_per_query(&rows);
        assert_eq!(map.get("lonely").map(String::as_str), Some(""));
    }
}
