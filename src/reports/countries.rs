use crate::config::AppConfig;
use crate::domain::{CountriesReport, CountryDetail, Insight, InsightCategory, InsightSeverity};
use crate::errors::Result;
use crate::google::analytics_data::{DateRange, ReportRequest, run_report};

pub async fn build(
    config: &AppConfig,
    access_token: &str,
    days: u32,
    limit: usize,
) -> Result<CountriesReport> {
    let property_id = config.require_ga4_property()?.to_string();
    let property_name = config
        .properties
        .ga4_property_name
        .clone()
        .unwrap_or_else(|| property_id.clone());

    let date_label = format!("last {} days", days);

    let req = ReportRequest {
        property_id,
        date_ranges: vec![DateRange::last_n_days(days)],
        dimensions: vec!["country".into()],
        metrics: vec![
            "sessions".into(),
            "engagementRate".into(),
        ],
        dimension_filter: None,
        limit: Some(200),
        order_by: Some(vec![serde_json::json!({
            "metric": { "metricName": "sessions" },
            "desc": true
        })]),
    };

    let report = run_report(access_token, req).await?;

    let mut total_sessions = 0i64;
    let mut countries: Vec<CountryDetail> = Vec::new();

    for row in &report.rows {
        let country = row.dimension_values.first().cloned().unwrap_or_default();
        let sessions: i64 = row.metric_values.first().and_then(|v| v.parse().ok()).unwrap_or(0);
        let eng: f64 = row.metric_values.get(1).and_then(|v| v.parse().ok()).unwrap_or(0.0);

        total_sessions += sessions;
        countries.push(CountryDetail {
            country,
            sessions,
            share_pct: 0.0,
            engagement_rate: eng,
        });
    }

    for c in &mut countries {
        c.share_pct = if total_sessions > 0 {
            c.sessions as f64 / total_sessions as f64 * 100.0
        } else {
            0.0
        };
    }

    countries.truncate(limit);

    // Insights
    let mut insights = Vec::new();

    if let Some(top) = countries.first() {
        if top.share_pct > 80.0 {
            insights.push(Insight {
                severity: InsightSeverity::Info,
                category: InsightCategory::Traffic,
                headline: format!("Heavily concentrated on {} ({:.0}%)", top.country, top.share_pct),
                explanation: "Over 80% of traffic from one country. Internationalization could offer growth potential.".into(),
            });
        }
    }

    // Low-engagement countries with significant traffic
    let low_eng: Vec<&CountryDetail> = countries.iter()
        .filter(|c| c.sessions >= 20 && c.engagement_rate < 0.25)
        .collect();
    if !low_eng.is_empty() {
        let names: Vec<&str> = low_eng.iter().take(3).map(|c| c.country.as_str()).collect();
        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Engagement,
            headline: format!("{} countries with weak engagement", low_eng.len()),
            explanation: format!(
                "Below 25% engagement: {}. Check language or content adaptation.",
                names.join(", ")
            ),
        });
    }

    let country_count = countries.len();
    if country_count >= 10 {
        let top3_share: f64 = countries.iter().take(3).map(|c| c.share_pct).sum();
        if top3_share < 50.0 {
            insights.push(Insight {
                severity: InsightSeverity::Positive,
                category: InsightCategory::Traffic,
                headline: "Broad international distribution".into(),
                explanation: format!(
                    "Top 3 countries account for only {:.0}% — traffic is well diversified.",
                    top3_share
                ),
            });
        }
    }

    Ok(CountriesReport {
        property_name,
        date_range: date_label,
        countries,
        total_sessions,
        insights,
    })
}
