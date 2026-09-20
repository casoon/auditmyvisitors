use crate::config::AppConfig;
use crate::domain::{PageSummary, SearchPerformanceBreakdown, TopPagesReport};
use crate::errors::Result;
use crate::google::api::GoogleApi;
use crate::google::analytics_data::{DateRange, ReportRequest};
use crate::google::search_console::SearchAnalyticsRequest;
use crate::helpers;
use crate::insights::insights_for_top_pages;
use std::collections::HashMap;

pub async fn build(
    config: &AppConfig,
    api: &impl GoogleApi,
    days: u32,
    limit: usize,
    sort_by: &str,
) -> Result<TopPagesReport> {
    let property_id = config.require_ga4_property()?.to_string();
    let sc_url = config.require_search_console_url().ok().map(String::from);
    let property_name = config
        .properties
        .ga4_property_name
        .clone()
        .unwrap_or_else(|| property_id.clone());

    let date_range = DateRange::last_n_days(days);
    let date_label = format!("last {} days", days);

    // ── GA4: sessions + engagement per page ──────────────────────────────────
    let req = ReportRequest {
        property_id: property_id.clone(),
        date_ranges: vec![date_range.clone()],
        dimensions: vec!["pagePath".into(), "sessionDefaultChannelGroup".into()],
        metrics: vec![
            "sessions".into(),
            "engagementRate".into(),
            "averageSessionDuration".into(),
            "bounceRate".into(),
            "newUsers".into(),
            "keyEvents".into(),
        ],
        dimension_filter: None,
        limit: Some(500),
        order_by: Some(vec![serde_json::json!({
            "metric": { "metricName": "sessions" },
            "desc": true
        })]),
    };

    let event_req = ReportRequest {
        property_id: property_id.clone(),
        date_ranges: vec![date_range.clone()],
        dimensions: vec!["pagePath".into(), "eventName".into()],
        metrics: vec!["eventCount".into()],
        dimension_filter: None,
        limit: Some(5000),
        order_by: None,
    };

    let (ga_report, event_report) = tokio::join!(
        api.run_report(req),
        api.run_report(event_req),
    );
    let ga_report = ga_report?;
    let event_report = event_report?;

    // Aggregate per page path
    let mut page_map: HashMap<String, PageSummary> = HashMap::new();

    for row in &ga_report.rows {
        let path = row.dimension_values.first().cloned().unwrap_or_default();
        let channel = row.dimension_values.get(1).map(String::as_str).unwrap_or("");
        let sessions: i64 = row.metric_values.first().and_then(|v| v.parse().ok()).unwrap_or(0);
        let eng: f64 = row.metric_values.get(1).and_then(|v| v.parse().ok()).unwrap_or(0.0);
        let dur: f64 = row.metric_values.get(2).and_then(|v| v.parse().ok()).unwrap_or(0.0);
        let bounce: f64 = row.metric_values.get(3).and_then(|v| v.parse().ok()).unwrap_or(0.0);
        let new_users: i64 = row.metric_values.get(4).and_then(|v| v.parse::<f64>().ok()).map(|v| v as i64).unwrap_or(0);
        let key_events: i64 = row.metric_values.get(5).and_then(|v| v.parse::<f64>().ok()).map(|v| v as i64).unwrap_or(0);

        let entry = page_map.entry(path.clone()).or_insert_with(|| PageSummary {
            url: path.clone(),
            sessions: 0,
            organic_sessions: 0,
            direct_sessions: 0,
            engagement_rate: 0.0,
            bounce_rate: 0.0,
            avg_session_duration_secs: dur,
            new_user_share: 0.0,
            key_events: 0,
            scroll_events: 0,
            internal_link_clicks: 0,
            service_hint_clicks: 0,
            search: SearchPerformanceBreakdown::default(),
        });

        entry.sessions += sessions;
        entry.engagement_rate = eng;
        entry.bounce_rate = bounce;
        entry.key_events += key_events;
        entry.new_user_share += new_users as f64; // accumulate raw new users, convert to share below
        match channel {
            "Organic Search" => entry.organic_sessions += sessions,
            "Direct"         => entry.direct_sessions  += sessions,
            _                => {}
        }
    }

    for row in &event_report.rows {
        let path = row.dimension_values.first().cloned().unwrap_or_default();
        let event_name = row.dimension_values.get(1).cloned().unwrap_or_default().to_lowercase();
        let count: i64 = row.metric_values.first().and_then(|v| v.parse::<f64>().ok()).map(|v| v as i64).unwrap_or(0);
        if count == 0 {
            continue;
        }

        if let Some(entry) = page_map.get_mut(&path) {
            if event_name.starts_with("scroll") {
                entry.scroll_events += count;
            }
            if event_name == "internal_link_click" || event_name == "internal_click" {
                entry.internal_link_clicks += count;
            }
            if event_name == "service_hint_click" || event_name == "servicehint_click" {
                entry.service_hint_clicks += count;
                entry.internal_link_clicks += count;
            }
        }
    }

    // ── Search Console: per page ─────────────────────────────────────────────
    if let Some(sc) = sc_url {
        let sc_req = SearchAnalyticsRequest {
            site_url: sc,
            start_date: helpers::days_ago(days),
            end_date: helpers::yesterday(),
            dimensions: vec!["page".into()],
            page_filter: None,
            row_limit: Some(1000),
        };

        let sc_query_req = SearchAnalyticsRequest {
            site_url: sc_req.site_url.clone(),
            start_date: sc_req.start_date.clone(),
            end_date: sc_req.end_date.clone(),
            dimensions: vec!["page".into(), "query".into()],
            page_filter: None,
            row_limit: Some(2500),
        };

        let (sc_resp, sc_query_resp) = tokio::join!(
            api.search_analytics(sc_req),
            api.search_analytics(sc_query_req),
        );
        let sc_resp = sc_resp?;
        let sc_query_resp = sc_query_resp?;
        helpers::merge_sc_into_page_map(&sc_resp.rows, &mut page_map);
        helpers::merge_sc_queries_into_page_map(&sc_query_resp.rows, &mut page_map);
    }

    // ── Convert raw newUsers count → share ─────────────────────────────────
    for page in page_map.values_mut() {
        if page.sessions > 0 {
            page.new_user_share /= page.sessions as f64;
        }
    }

    // ── Sort and truncate ────────────────────────────────────────────────────
    let mut pages: Vec<PageSummary> = page_map.into_values().collect();

    match sort_by {
        "clicks"      => pages.sort_by(|a, b| b.search.clicks.total_cmp(&a.search.clicks)),
        "impressions" => pages.sort_by(|a, b| b.search.impressions.total_cmp(&a.search.impressions)),
        _             => pages.sort_by_key(|p| std::cmp::Reverse(p.sessions)),
    }

    pages.truncate(limit);

    let mut report = TopPagesReport {
        property_name,
        date_range: date_label,
        pages,
        insights: vec![],
    };

    insights_for_top_pages(&mut report, &config.thresholds);
    Ok(report)
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

    /// GA4 reports paths, Search Console reports absolute URLs, and the two
    /// disagree about the trailing slash. If the reconciliation misses, the
    /// page keeps its sessions and silently loses every click.
    #[tokio::test]
    async fn search_data_attaches_across_the_path_url_gap() {
        let api = FixtureGoogleApi::new()
            .with_report(
                &["pagePath", "sessionDefaultChannelGroup"],
                vec![(
                    vec!["/blog/rust", "Organic Search"],
                    vec!["120", "0.65", "90.0", "0.3", "80", "4"],
                )],
            )
            .with_report(&["pagePath", "eventName"], vec![])
            .with_search(
                &["page"],
                // trailing slash, absolute URL — neither matches the GA4 key verbatim
                vec![(vec!["https://example.com/blog/rust/"], 42.0, 900.0, 0.046, 7.4)],
            )
            .with_search(&["page", "query"], vec![]);

        let report = build(&config(), &api, 28, 20, "sessions").await.unwrap();

        let page = report.pages.iter().find(|p| p.url == "/blog/rust").unwrap();
        assert_eq!(page.sessions, 120);
        assert_eq!(page.search.clicks, 42.0);
        assert_eq!(page.search.impressions, 900.0);
    }

    /// The same gap in the other direction: GA4 keeps the trailing slash and
    /// Search Console does not. Dropping this branch broke no test before it
    /// was written.
    #[tokio::test]
    async fn the_path_url_gap_reconciles_in_both_directions() {
        let api = FixtureGoogleApi::new()
            .with_report(
                &["pagePath", "sessionDefaultChannelGroup"],
                vec![(
                    vec!["/blog/rust/", "Organic Search"],
                    vec!["30", "0.5", "30.0", "0.4", "20", "1"],
                )],
            )
            .with_report(&["pagePath", "eventName"], vec![])
            .with_search(
                &["page"],
                vec![(vec!["https://example.com/blog/rust"], 11.0, 200.0, 0.055, 6.0)],
            )
            .with_search(&["page", "query"], vec![]);

        let report = build(&config(), &api, 28, 20, "sessions").await.unwrap();

        let page = report.pages.iter().find(|p| p.url == "/blog/rust/").unwrap();
        assert_eq!(page.search.clicks, 11.0);
    }

    /// A URL Search Console knows and GA4 does not is dropped. That is the
    /// current contract, and it is the reason a page with impressions but no
    /// sessions never shows up in this report.
    #[tokio::test]
    async fn search_rows_without_a_ga4_page_are_dropped() {
        let api = FixtureGoogleApi::new()
            .with_report(
                &["pagePath", "sessionDefaultChannelGroup"],
                vec![(
                    vec!["/blog/rust", "Organic Search"],
                    vec!["10", "0.5", "30.0", "0.4", "5", "0"],
                )],
            )
            .with_report(&["pagePath", "eventName"], vec![])
            .with_search(
                &["page"],
                vec![
                    (vec!["https://example.com/blog/rust"], 5.0, 100.0, 0.05, 9.0),
                    (vec!["https://example.com/ghost-page"], 99.0, 999.0, 0.1, 2.0),
                ],
            )
            .with_search(&["page", "query"], vec![]);

        let report = build(&config(), &api, 28, 20, "sessions").await.unwrap();

        assert_eq!(report.pages.len(), 1);
        assert_eq!(report.pages[0].search.clicks, 5.0);
    }

    /// Per-page queries are merged, sorted by clicks and capped at five.
    #[tokio::test]
    async fn top_queries_are_ranked_and_capped() {
        let queries: Vec<(Vec<&str>, f64, f64, f64, f64)> = vec![
            (vec!["https://example.com/blog/rust", "rust cli"], 3.0, 50.0, 0.06, 8.0),
            (vec!["https://example.com/blog/rust", "rust reporting"], 9.0, 80.0, 0.11, 4.0),
            (vec!["https://example.com/blog/rust", "rust ga4"], 1.0, 20.0, 0.05, 12.0),
            (vec!["https://example.com/blog/rust", "rust gsc"], 7.0, 60.0, 0.11, 5.0),
            (vec!["https://example.com/blog/rust", "rust audit"], 2.0, 30.0, 0.06, 9.0),
            (vec!["https://example.com/blog/rust", "rust tool"], 5.0, 40.0, 0.12, 6.0),
        ];

        let api = FixtureGoogleApi::new()
            .with_report(
                &["pagePath", "sessionDefaultChannelGroup"],
                vec![(
                    vec!["/blog/rust", "Organic Search"],
                    vec!["10", "0.5", "30.0", "0.4", "5", "0"],
                )],
            )
            .with_report(&["pagePath", "eventName"], vec![])
            .with_search(&["page"], vec![])
            .with_search(&["page", "query"], queries);

        let report = build(&config(), &api, 28, 20, "sessions").await.unwrap();
        let top = &report.pages[0].search.top_queries;

        assert_eq!(top.len(), 5, "capped at five");
        assert_eq!(top[0].query, "rust reporting", "highest clicks first");
        assert_eq!(top[1].query, "rust gsc");
        assert!(
            !top.iter().any(|q| q.query == "rust ga4"),
            "the weakest query is the one dropped"
        );
    }

    /// Sorting by clicks has to run after the Search Console merge, otherwise
    /// it ranks on data that is not there yet.
    #[tokio::test]
    async fn sort_by_clicks_uses_merged_search_data() {
        let api = FixtureGoogleApi::new()
            .with_report(
                &["pagePath", "sessionDefaultChannelGroup"],
                vec![
                    (vec!["/many-sessions", "Direct"], vec!["500", "0.5", "10.0", "0.5", "400", "0"]),
                    (vec!["/many-clicks", "Organic Search"], vec!["20", "0.8", "60.0", "0.2", "15", "2"]),
                ],
            )
            .with_report(&["pagePath", "eventName"], vec![])
            .with_search(
                &["page"],
                vec![
                    (vec!["https://example.com/many-sessions"], 1.0, 10.0, 0.1, 30.0),
                    (vec!["https://example.com/many-clicks"], 250.0, 5000.0, 0.05, 3.0),
                ],
            )
            .with_search(&["page", "query"], vec![]);

        let report = build(&config(), &api, 28, 20, "clicks").await.unwrap();

        assert_eq!(report.pages[0].url, "/many-clicks");
        assert_eq!(report.pages[1].url, "/many-sessions");
    }
}
