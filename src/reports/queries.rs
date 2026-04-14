use crate::config::AppConfig;
use crate::domain::{Insight, InsightCategory, InsightSeverity, QueriesReport, QueryRow};
use crate::errors::Result;
use crate::google::search_console::{query, SearchAnalyticsRequest};
use crate::helpers;
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

    let date_label = format!("letzte {} Tage", days);

    let req = SearchAnalyticsRequest {
        site_url: sc_url.to_string(),
        start_date: helpers::days_ago(days),
        end_date: helpers::yesterday(),
        dimensions: vec!["query".into()],
        page_filter: None,
        row_limit: Some(500),
    };

    let resp = query(access_token, req).await?;

    let mut queries: Vec<QueryRow> = resp
        .rows
        .iter()
        .map(|r| QueryRow {
            query: r.keys.first().cloned().unwrap_or_default(),
            clicks: r.clicks,
            impressions: r.impressions,
            ctr: r.ctr,
            position: r.position,
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
    let brand_terms = &config.report.brand_terms;
    let brand_clicks: f64 = queries.iter()
        .filter(|q| helpers::is_brand_query(&q.query, brand_terms))
        .map(|q| q.clicks)
        .sum();

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
            headline: format!("{} Keywords mit CTR unter Erwartung", ctr_opps.len()),
            explanation: format!(
                "Diese Keywords ranken auf Seite 1, aber die CTR ist deutlich niedriger als erwartet. Bestes Potenzial: \"{}\"",
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
            headline: format!("{} Keywords auf Seite 2 mit hohem Volumen", page2_queries.len()),
            explanation: "Diese Keywords haben hohe Impressionen, ranken aber auf Seite 2. Content-Ausbau kann den Sprung auf Seite 1 ermoeglichen.".into(),
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
            headline: format!("{} Stark-performende Keywords", top_performers.len()),
            explanation: format!(
                "Keywords mit >5% CTR und >20 Klicks. Staerkstes: \"{}\" ({:.1}% CTR)",
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
                headline: format!("Brand-lastig: {:.0}% der Klicks sind Brand-Queries", brand_pct),
                explanation: "Hoher Brand-Anteil kann ein Zeichen guter Markenbekanntheit sein, verdeckt aber organisches Wachstumspotenzial bei generischen Begriffen.".into(),
            });
        } else if brand_pct < 20.0 {
            insights.push(Insight {
                severity: InsightSeverity::Positive,
                category: InsightCategory::Search,
                headline: format!("Starker Non-Brand-Traffic: {:.0}% der Klicks sind generisch", 100.0 - brand_pct),
                explanation: "Der Grossteil des Traffics kommt ueber generische Keywords — gute organische Reichweite.".into(),
            });
        }
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
        insights,
    })
}

