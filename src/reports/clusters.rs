//! Topic Cluster Report
//!
//! Groups pages and queries into topic clusters and aggregates metrics.

use std::collections::HashMap;

use crate::clusters;
use crate::config::AppConfig;
use crate::domain::{ClustersReport, Insight, InsightCategory, InsightSeverity, TopicCluster};
use crate::errors::Result;
use crate::google::analytics_data::{run_report, DateRange, ReportRequest};
use crate::google::search_console::{query as sc_query, SearchAnalyticsRequest};
use crate::helpers;
use crate::opportunities::expected_ctr;

pub async fn build(config: &AppConfig, access_token: &str, days: u32) -> Result<ClustersReport> {
    let property_id = config.require_ga4_property()?.to_string();
    let sc_url = config.require_search_console_url()?;
    let property_name = config
        .properties
        .ga4_property_name
        .clone()
        .unwrap_or_else(|| sc_url.to_string());
    let date_label = format!("last {} days (topic clusters)", days);

    // ── Parallel: GA4 sessions per page + SC queries + SC pages ────────────
    let (ga_pages, sc_queries, sc_pages) = tokio::join!(
        run_report(
            access_token,
            ReportRequest {
                property_id,
                date_ranges: vec![DateRange::last_n_days(days)],
                dimensions: vec!["pagePath".into()],
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
                dimensions: vec!["query".into()],
                page_filter: None,
                row_limit: Some(500),
            },
        ),
        sc_query(
            access_token,
            SearchAnalyticsRequest {
                site_url: sc_url.to_string(),
                start_date: helpers::days_ago(days),
                end_date: helpers::yesterday(),
                dimensions: vec!["page".into()],
                page_filter: None,
                row_limit: Some(500),
            },
        ),
    );

    let ga_pages = ga_pages?;
    let sc_queries = sc_queries?;
    let sc_pages = sc_pages?;

    // ── Collect page URLs and sessions ─────────────────────────────────────
    let mut page_sessions: HashMap<String, i64> = HashMap::new();
    for row in &ga_pages.rows {
        let path = row.dimension_values.first().cloned().unwrap_or_default();
        let sessions: i64 = row
            .metric_values
            .first()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        *page_sessions.entry(path).or_default() += sessions;
    }

    // SC page-level data
    let mut page_clicks: HashMap<String, f64> = HashMap::new();
    let mut page_impressions: HashMap<String, f64> = HashMap::new();
    let mut page_position_sum: HashMap<String, f64> = HashMap::new();
    let mut page_position_weight: HashMap<String, f64> = HashMap::new();

    for row in &sc_pages.rows {
        let url = row.keys.first().cloned().unwrap_or_default();
        let path = helpers::extract_path(&url);
        *page_clicks.entry(path.clone()).or_default() += row.clicks;
        *page_impressions.entry(path.clone()).or_default() += row.impressions;
        *page_position_sum.entry(path.clone()).or_default() += row.position * row.impressions;
        *page_position_weight.entry(path.clone()).or_default() += row.impressions;
    }

    // Queries with impressions for clustering
    let queries_with_impressions: Vec<(String, f64)> = sc_queries
        .rows
        .iter()
        .map(|r| {
            let q = r.keys.first().cloned().unwrap_or_default();
            (q, r.impressions)
        })
        .collect();

    // Query-level SC data
    let mut query_clicks: HashMap<String, f64> = HashMap::new();
    let mut query_impressions: HashMap<String, f64> = HashMap::new();
    let mut query_position: HashMap<String, f64> = HashMap::new();

    for row in &sc_queries.rows {
        let q = row.keys.first().cloned().unwrap_or_default();
        query_clicks.insert(q.clone(), row.clicks);
        query_impressions.insert(q.clone(), row.impressions);
        query_position.insert(q.clone(), row.position);
    }

    // ── Assign clusters ────────────────────────────────────────────────────
    let page_urls: Vec<String> = page_sessions.keys().cloned().collect();
    let (page_cluster_map, query_cluster_map) =
        clusters::assign_clusters(&page_urls, &queries_with_impressions, &config.clusters);

    // ── Aggregate per cluster ──────────────────────────────────────────────
    struct ClusterAcc {
        pages: std::collections::HashSet<String>,
        queries: std::collections::HashSet<String>,
        sessions: i64,
        clicks: f64,
        impressions: f64,
        position_sum: f64,
        position_weight: f64,
        ctr_potential: f64,
    }

    let mut acc: HashMap<String, ClusterAcc> = HashMap::new();

    // Add page data to clusters
    for (path, cluster) in &page_cluster_map {
        let entry = acc.entry(cluster.clone()).or_insert_with(|| ClusterAcc {
            pages: Default::default(),
            queries: Default::default(),
            sessions: 0,
            clicks: 0.0,
            impressions: 0.0,
            position_sum: 0.0,
            position_weight: 0.0,
            ctr_potential: 0.0,
        });
        entry.pages.insert(path.clone());
        entry.sessions += page_sessions.get(path).copied().unwrap_or(0);
        entry.clicks += page_clicks.get(path).copied().unwrap_or(0.0);
        let imps = page_impressions.get(path).copied().unwrap_or(0.0);
        entry.impressions += imps;
        let weight = page_position_weight.get(path).copied().unwrap_or(0.0);
        entry.position_sum += page_position_sum.get(path).copied().unwrap_or(0.0);
        entry.position_weight += weight;
    }

    // Add query data to clusters
    for (query, cluster) in &query_cluster_map {
        let entry = acc.entry(cluster.clone()).or_insert_with(|| ClusterAcc {
            pages: Default::default(),
            queries: Default::default(),
            sessions: 0,
            clicks: 0.0,
            impressions: 0.0,
            position_sum: 0.0,
            position_weight: 0.0,
            ctr_potential: 0.0,
        });
        entry.queries.insert(query.clone());

        // CTR potential: expected - actual for this query
        let pos = query_position.get(query).copied().unwrap_or(20.0);
        let imps = query_impressions.get(query).copied().unwrap_or(0.0);
        let clicks = query_clicks.get(query).copied().unwrap_or(0.0);
        let actual_ctr = if imps > 0.0 { clicks / imps } else { 0.0 };
        let expected = expected_ctr(pos);
        if expected > actual_ctr {
            entry.ctr_potential += (expected - actual_ctr) * imps;
        }
    }

    // ── Build cluster structs ──────────────────────────────────────────────
    let mut cluster_list: Vec<TopicCluster> = acc
        .into_iter()
        .filter(|(_, a)| a.pages.len() + a.queries.len() >= 2) // Skip trivial clusters
        .map(|(name, a)| {
            let avg_pos = if a.position_weight > 0.0 {
                a.position_sum / a.position_weight
            } else {
                0.0
            };
            let ctr = if a.impressions > 0.0 {
                a.clicks / a.impressions
            } else {
                0.0
            };
            TopicCluster {
                name,
                pages: a.pages.len(),
                queries: a.queries.len(),
                sessions: a.sessions,
                clicks: a.clicks,
                impressions: a.impressions,
                ctr,
                avg_position: avg_pos,
                ctr_potential: a.ctr_potential,
            }
        })
        .collect();

    cluster_list.sort_by(|a, b| b.sessions.cmp(&a.sessions));
    cluster_list.truncate(20);

    // ── Insights ───────────────────────────────────────────────────────────
    let mut insights = Vec::new();

    if let Some(top) = cluster_list.first() {
        insights.push(Insight {
            severity: InsightSeverity::Info,
            category: InsightCategory::Traffic,
            headline: format!("Strongest topic cluster: \"{}\"", top.name),
            explanation: format!(
                "{} sessions, {} pages, {} queries",
                top.sessions, top.pages, top.queries
            ),
        });
    }

    // Cluster with highest CTR potential
    if let Some(best_potential) = cluster_list
        .iter()
        .filter(|c| c.ctr_potential > 0.0)
        .max_by(|a, b| a.ctr_potential.partial_cmp(&b.ctr_potential).unwrap_or(std::cmp::Ordering::Equal))
    {
        insights.push(Insight {
            severity: InsightSeverity::Positive,
            category: InsightCategory::Search,
            headline: format!(
                "Greatest CTR potential: \"{}\"",
                best_potential.name
            ),
            explanation: format!(
                "~{:.0} additional clicks possible at optimal CTR",
                best_potential.ctr_potential
            ),
        });
    }

    // Single-page clusters with many queries → content hub opportunity
    let hub_candidates: Vec<&TopicCluster> = cluster_list
        .iter()
        .filter(|c| c.pages <= 1 && c.queries >= 5)
        .collect();
    if !hub_candidates.is_empty() {
        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Search,
            headline: format!(
                "{} topics with only 1 page but many queries",
                hub_candidates.len()
            ),
            explanation: "Check content hub expansion: multiple search queries suggest subtopics that deserve their own pages.".into(),
        });
    }

    Ok(ClustersReport {
        property_name,
        date_range: date_label,
        clusters: cluster_list,
        insights,
    })
}
