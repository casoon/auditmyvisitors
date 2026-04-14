use crate::config::AppConfig;
use crate::domain::{Insight, InsightCategory, InsightSeverity, QueriesReport, QueryRow};
use crate::errors::Result;
use crate::google::search_console::{query, SearchAnalyticsRequest};
use crate::helpers;
use crate::intent;
use crate::opportunities::expected_ctr;

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

    let resp = query(access_token, req).await?;

    let brand_terms = &config.report.brand_terms;

    let mut queries: Vec<QueryRow> = resp
        .rows
        .iter()
        .map(|r| {
            let q = r.keys.first().cloned().unwrap_or_default();
            let classified = intent::classify(&q, brand_terms);
            QueryRow {
                query: q,
                clicks: r.clicks,
                impressions: r.impressions,
                ctr: r.ctr,
                position: r.position,
                intent: Some(classified),
            }
        })
        .collect();

    // Sort
    match sort_by {
        "impressions" => queries.sort_by(|a, b| b.impressions.partial_cmp(&a.impressions).unwrap()),
        "ctr" => queries.sort_by(|a, b| b.ctr.partial_cmp(&a.ctr).unwrap()),
        "position" => queries.sort_by(|a, b| a.position.partial_cmp(&b.position).unwrap()),
        _ => queries.sort_by(|a, b| b.clicks.partial_cmp(&a.clicks).unwrap()),
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

