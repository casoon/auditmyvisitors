use crate::config::AppConfig;
use crate::domain::{
    Insight, InsightCategory, InsightSeverity, OpportunitiesReport, OpportunityType,
    PageSummary, QueryRow, SearchPerformanceBreakdown,
};
use crate::errors::Result;
use crate::google::analytics_data::{DateRange, ReportRequest, run_report};
use crate::google::search_console::{query, SearchAnalyticsRequest};
use crate::helpers;
use crate::opportunities::generate_opportunities;
use std::collections::HashMap;

pub async fn build(config: &AppConfig, access_token: &str, days: u32) -> Result<OpportunitiesReport> {
    let property_id = config.require_ga4_property()?.to_string();
    let sc_url = config.require_search_console_url()?;
    let property_name = config
        .properties
        .ga4_property_name
        .clone()
        .unwrap_or_else(|| property_id.clone());

    let date_label = format!("last {} days", days);

    // ── Search Console: top queries ──────────────────────────────────────────
    let queries_req = SearchAnalyticsRequest {
        site_url: sc_url.to_string(),
        start_date: helpers::days_ago(days),
        end_date: helpers::yesterday(),
        dimensions: vec!["query".into()],
        page_filter: None,
        row_limit: Some(200),
    };

    // ── GA4: sessions per page (for internal linking opportunities) ──────────
    let pages_req = ReportRequest {
        property_id: property_id.clone(),
        date_ranges: vec![DateRange::last_n_days(days)],
        dimensions: vec!["pagePath".into(), "sessionDefaultChannelGroup".into()],
        metrics: vec!["sessions".into()],
        dimension_filter: None,
        limit: Some(500),
        order_by: Some(vec![serde_json::json!({
            "metric": { "metricName": "sessions" },
            "desc": true
        })]),
    };

    // ── Search Console: per-page data ────────────────────────────────────────
    let sc_pages_req = SearchAnalyticsRequest {
        site_url: sc_url.to_string(),
        start_date: helpers::days_ago(days),
        end_date: helpers::yesterday(),
        dimensions: vec!["page".into()],
        page_filter: None,
        row_limit: Some(500),
    };

    let (queries_resp, ga_report, sc_pages_resp) = tokio::join!(
        query(access_token, queries_req),
        run_report(access_token, pages_req),
        query(access_token, sc_pages_req),
    );

    let queries_resp = queries_resp?;
    let ga_report = ga_report?;
    let sc_pages_resp = sc_pages_resp?;

    // ── Build query rows ─────────────────────────────────────────────────────
    let query_rows: Vec<QueryRow> = queries_resp
        .rows
        .iter()
        .map(|r| QueryRow {
            query: r.keys.first().cloned().unwrap_or_default(),
            clicks: r.clicks,
            impressions: r.impressions,
            ctr: r.ctr,
            position: r.position,
            intent: None,
        })
        .collect();

    // ── Build page summaries ─────────────────────────────────────────────────
    let mut page_map: HashMap<String, PageSummary> = HashMap::new();
    for row in &ga_report.rows {
        let path = row.dimension_values.first().cloned().unwrap_or_default();
        let channel = row.dimension_values.get(1).map(String::as_str).unwrap_or("");
        let sessions: i64 = row.metric_values.first().and_then(|v| v.parse().ok()).unwrap_or(0);

        let entry = page_map.entry(path.clone()).or_insert_with(|| PageSummary {
            url: path.clone(),
            sessions: 0,
            organic_sessions: 0,
            direct_sessions: 0,
            engagement_rate: 0.0,
            bounce_rate: 0.0,
            avg_session_duration_secs: 0.0,
            new_user_share: 0.0,
            key_events: 0,
            search: SearchPerformanceBreakdown::default(),
        });
        entry.sessions += sessions;
        match channel {
            "Organic Search" => entry.organic_sessions += sessions,
            "Direct" => entry.direct_sessions += sessions,
            _ => {}
        }
    }

    // Merge SC page data
    helpers::merge_sc_into_page_map(&sc_pages_resp.rows, &mut page_map);

    let pages: Vec<PageSummary> = page_map.into_values().collect();

    // ── Generate opportunities ────────────────────────────────────────────────
    let opportunities = generate_opportunities(&query_rows, &pages, days, &config.report.brand_terms);

    let total_estimated_clicks: f64 = opportunities.iter().map(|o| o.estimated_clicks).sum();

    // ── Summary ──────────────────────────────────────────────────────────────
    let summary = build_summary(&opportunities, total_estimated_clicks);

    // ── Insights ─────────────────────────────────────────────────────────────
    let mut insights = Vec::new();

    let snippet_count: usize = opportunities.iter()
        .filter(|o| matches!(o.opportunity_type, OpportunityType::SnippetProblem | OpportunityType::CtrFix))
        .count();
    let ranking_count: usize = opportunities.iter()
        .filter(|o| matches!(o.opportunity_type, OpportunityType::RankingProblem | OpportunityType::RankingPush))
        .count();
    let intent_count: usize = opportunities.iter()
        .filter(|o| o.opportunity_type == OpportunityType::IntentMismatch)
        .count();

    if snippet_count > 3 {
        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Search,
            headline: format!("{} snippet issues detected", snippet_count),
            explanation: "Multiple keywords rank well, but the snippets fail to address search intent. Systematically revise title and description.".into(),
        });
    }

    if ranking_count > 3 {
        insights.push(Insight {
            severity: InsightSeverity::Info,
            category: InsightCategory::Search,
            headline: format!("{} ranking issues (pos. 5-15)", ranking_count),
            explanation: "Content expansion and internal linking can push these keywords to the first page.".into(),
        });
    }

    if intent_count > 0 {
        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Search,
            headline: format!("{} pages with intent mismatch", intent_count),
            explanation: "These pages rank, but low CTR and weak engagement suggest the content does not match the search intent.".into(),
        });
    }

    if total_estimated_clicks > 100.0 {
        insights.push(Insight {
            severity: InsightSeverity::Positive,
            category: InsightCategory::Search,
            headline: format!("Estimated potential: +{:.0} clicks/month", total_estimated_clicks),
            explanation: "Sum of estimated additional clicks across all opportunities.".into(),
        });
    }

    let action_plan = crate::opportunities::build_action_plan(&opportunities);

    Ok(OpportunitiesReport {
        property_name,
        date_range: date_label,
        opportunities,
        total_estimated_clicks,
        summary,
        action_plan,
        insights,
    })
}

fn build_summary(opportunities: &[crate::domain::Opportunity], total: f64) -> String {
    if opportunities.is_empty() {
        return "No significant opportunities found.".into();
    }

    let snippet_count = opportunities.iter()
        .filter(|o| matches!(o.opportunity_type, OpportunityType::SnippetProblem | OpportunityType::CtrFix)).count();
    let ranking_count = opportunities.iter()
        .filter(|o| matches!(o.opportunity_type, OpportunityType::RankingProblem | OpportunityType::RankingPush)).count();
    let expansion_count = opportunities.iter()
        .filter(|o| matches!(o.opportunity_type, OpportunityType::ExpansionPotential | OpportunityType::ContentExpansion)).count();
    let distribution_count = opportunities.iter()
        .filter(|o| matches!(o.opportunity_type, OpportunityType::DistributionProblem | OpportunityType::InternalLinking)).count();
    let intent_count = opportunities.iter()
        .filter(|o| o.opportunity_type == OpportunityType::IntentMismatch).count();

    let mut parts = Vec::new();
    if snippet_count > 0 { parts.push(format!("{} snippet issues", snippet_count)); }
    if ranking_count > 0 { parts.push(format!("{} ranking issues", ranking_count)); }
    if intent_count > 0 { parts.push(format!("{} intent mismatches", intent_count)); }
    if expansion_count > 0 { parts.push(format!("{} expansion potentials", expansion_count)); }
    if distribution_count > 0 { parts.push(format!("{} distribution issues", distribution_count)); }

    format!(
        "{} opportunities found ({}) — estimated potential: +{:.0} clicks/month",
        opportunities.len(),
        parts.join(", "),
        total
    )
}

