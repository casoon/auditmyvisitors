use crate::config::AppConfig;
use crate::domain::{
    PageDetailReport, QueryRow, Recommendation, SearchPerformanceBreakdown, TrafficSourceBreakdown,
};
use crate::errors::Result;
use crate::google::analytics_data::{DateRange, ReportRequest, run_report};
use crate::google::search_console::{query, SearchAnalyticsRequest};
use crate::insights::insights_for_page;
use serde_json::json;

pub async fn build(
    config: &AppConfig,
    access_token: &str,
    url: &str,
    days: u32,
) -> Result<PageDetailReport> {
    let property_id = config.require_ga4_property()?.to_string();
    let sc_url = config.require_search_console_url().ok().map(String::from);
    let property_name = config
        .properties
        .ga4_property_name
        .clone()
        .unwrap_or_else(|| property_id.clone());

    // Determine the path part for GA4 filter
    let path = extract_path(url);
    let date_range = DateRange::last_n_days(days);
    let date_label = format!("letzte {} Tage", days);

    // ── GA4: channel breakdown for this page ──────────────────────────────────
    let req = ReportRequest {
        property_id: property_id.clone(),
        date_ranges: vec![date_range.clone()],
        dimensions: vec!["sessionDefaultChannelGroup".into()],
        metrics: vec![
            "sessions".into(),
            "engagementRate".into(),
            "averageSessionDuration".into(),
        ],
        dimension_filter: Some(json!({
            "filter": {
                "fieldName": "pagePath",
                "stringFilter": { "matchType": "EXACT", "value": path }
            }
        })),
        limit: Some(50),
        order_by: None,
    };

    let ga_report = run_report(access_token, req).await?;

    let mut traffic = TrafficSourceBreakdown::default();
    let mut eng_sum = 0.0f64;
    let mut dur_sum = 0.0f64;
    let mut session_count = 0i64;

    for row in &ga_report.rows {
        let channel = row.dimension_values.first().map(String::as_str).unwrap_or("");
        let sessions: i64 = row.metric_values.first().and_then(|v| v.parse().ok()).unwrap_or(0);
        let eng: f64 = row.metric_values.get(1).and_then(|v| v.parse().ok()).unwrap_or(0.0);
        let dur: f64 = row.metric_values.get(2).and_then(|v| v.parse().ok()).unwrap_or(0.0);

        traffic.total_sessions += sessions;
        eng_sum += eng * sessions as f64;
        dur_sum += dur * sessions as f64;
        session_count += sessions;

        match channel {
            "Organic Search" => traffic.organic_sessions += sessions,
            "Direct"         => traffic.direct_sessions  += sessions,
            "Referral"       => traffic.referral_sessions += sessions,
            _                => traffic.other_sessions    += sessions,
        }
    }

    let engagement_rate = if session_count > 0 { eng_sum / session_count as f64 } else { 0.0 };
    let avg_session_duration = if session_count > 0 { dur_sum / session_count as f64 } else { 0.0 };

    // ── Search Console: this page + queries ──────────────────────────────────
    let search = if let Some(sc) = sc_url {
        let query_req = SearchAnalyticsRequest {
            site_url: sc.clone(),
            start_date: chrono_days_ago(days),
            end_date: chrono_yesterday(),
            dimensions: vec!["query".into()],
            page_filter: Some(url.to_string()),
            row_limit: Some(20),
        };

        let page_req = SearchAnalyticsRequest {
            site_url: sc,
            start_date: chrono_days_ago(days),
            end_date: chrono_yesterday(),
            dimensions: vec!["page".into()],
            page_filter: Some(url.to_string()),
            row_limit: Some(1),
        };

        let (query_resp, page_resp) = tokio::join!(
            query(access_token, query_req),
            query(access_token, page_req),
        );

        let top_queries: Vec<QueryRow> = query_resp
            .unwrap_or_default()
            .rows
            .into_iter()
            .map(|r| QueryRow {
                query: r.keys.into_iter().next().unwrap_or_default(),
                clicks: r.clicks,
                impressions: r.impressions,
                ctr: r.ctr,
                position: r.position,
            })
            .collect();

        let (clicks, impressions, ctr, avg_pos) = page_resp
            .unwrap_or_default()
            .rows
            .into_iter()
            .next()
            .map(|r| (r.clicks, r.impressions, r.ctr, r.position))
            .unwrap_or((0.0, 0.0, 0.0, 0.0));

        SearchPerformanceBreakdown {
            clicks,
            impressions,
            ctr,
            average_position: avg_pos,
            top_queries,
        }
    } else {
        SearchPerformanceBreakdown::default()
    };

    let recommendations = build_recommendations(&traffic, &search);

    let mut report = PageDetailReport {
        url: url.to_string(),
        property_name,
        date_range: date_label,
        traffic,
        engagement_rate,
        avg_session_duration_secs: avg_session_duration,
        search,
        insights: vec![],
        recommendations,
    };

    insights_for_page(&mut report);
    Ok(report)
}

fn build_recommendations(
    traffic: &TrafficSourceBreakdown,
    search: &SearchPerformanceBreakdown,
) -> Vec<Recommendation> {
    let mut recs = Vec::new();
    let mut prio = 1u8;

    if search.impressions > 200.0 && search.ctr < 0.02 {
        recs.push(Recommendation {
            priority: prio,
            headline: "Meta-Titel und Description optimieren".into(),
            action: "Klick-treibende Formulierungen testen; Suchintent klarer adressieren.".into(),
        });
        prio += 1;
    }

    if traffic.organic_share() < 20.0 && traffic.total_sessions > 50 {
        recs.push(Recommendation {
            priority: prio,
            headline: "SEO-Potenzial erschließen".into(),
            action: "Interne Verlinkung stärken und Seite für relevante Keywords optimieren.".into(),
        });
    }

    recs
}

fn extract_path(url: &str) -> String {
    url::Url::parse(url)
        .map(|u| u.path().to_string())
        .unwrap_or_else(|_| url.to_string())
}

fn chrono_days_ago(days: u32) -> String {
    let date = chrono::Utc::now() - chrono::Duration::days(days as i64);
    date.format("%Y-%m-%d").to_string()
}

fn chrono_yesterday() -> String {
    let date = chrono::Utc::now() - chrono::Duration::days(1);
    date.format("%Y-%m-%d").to_string()
}
