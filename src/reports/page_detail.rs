use crate::config::AppConfig;
use crate::domain::{
    PageDetailReport, QueryRow, Recommendation, SearchPerformanceBreakdown, TrafficSourceBreakdown,
};
use crate::errors::Result;
use crate::google::analytics_data::{DateRange, ReportRequest, run_report};
use crate::google::search_console::{query, SearchAnalyticsRequest};
use crate::helpers;
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
    let path = helpers::extract_path(url);
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
            start_date: helpers::days_ago(days),
            end_date: helpers::yesterday(),
            dimensions: vec!["query".into()],
            page_filter: Some(url.to_string()),
            row_limit: Some(20),
        };

        let page_req = SearchAnalyticsRequest {
            site_url: sc,
            start_date: helpers::days_ago(days),
            end_date: helpers::yesterday(),
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

    let recommendations = build_recommendations(&traffic, &search, engagement_rate, avg_session_duration);

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

    insights_for_page(&mut report, &config.thresholds);
    Ok(report)
}

fn build_recommendations(
    traffic: &TrafficSourceBreakdown,
    search: &SearchPerformanceBreakdown,
    engagement_rate: f64,
    avg_session_duration: f64,
) -> Vec<Recommendation> {
    let mut recs = Vec::new();
    let mut prio = 1u8;

    // CTR fix: good position but low CTR
    if search.average_position > 0.0 && search.average_position <= 10.0
        && search.impressions > 100.0
        && search.ctr < crate::opportunities::expected_ctr(search.average_position) * 0.7
    {
        recs.push(Recommendation {
            priority: prio,
            headline: "Snippet optimieren (Title & Description)".into(),
            action: format!(
                "Position {:.1} bei {:.0} Impressionen, aber nur {:.1}% CTR. \
                Title und Meta Description auf Suchintent und Klick-Anreiz pruefen.",
                search.average_position, search.impressions, search.ctr * 100.0
            ),
        });
        prio += 1;
    }

    // High impressions, very low CTR (broader threshold)
    if search.impressions > 200.0 && search.ctr < 0.02 && prio == 1 {
        recs.push(Recommendation {
            priority: prio,
            headline: "Meta-Titel und Description ueberarbeiten".into(),
            action: "Klick-treibende Formulierungen testen; Suchintent klarer adressieren.".into(),
        });
        prio += 1;
    }

    // Low organic share
    if traffic.organic_share() < 20.0 && traffic.total_sessions > 50 {
        recs.push(Recommendation {
            priority: prio,
            headline: "SEO-Potenzial erschliessen".into(),
            action: "Interne Verlinkung staerken und Seite fuer relevante Keywords optimieren.".into(),
        });
        prio += 1;
    }

    // Good search position but few sessions → internal linking
    if search.average_position > 0.0 && search.average_position < 10.0
        && search.impressions > 50.0
        && traffic.total_sessions < 10
    {
        recs.push(Recommendation {
            priority: prio,
            headline: "Interne Verlinkung zu dieser Seite erhoehen".into(),
            action: format!(
                "Position {:.1} bei {:.0} Impressionen, aber nur {} Sessions. \
                Interne Links von thematisch verwandten Seiten setzen.",
                search.average_position, search.impressions, traffic.total_sessions
            ),
        });
        prio += 1;
    }

    // Weak engagement
    if engagement_rate < 0.3 && traffic.total_sessions > 20 {
        recs.push(Recommendation {
            priority: prio,
            headline: "Engagement verbessern".into(),
            action: format!(
                "Engagement Rate nur {:.0}%. Ladezeit pruefen, Content-Relevanz sicherstellen, \
                Call-to-Actions und interne Navigation optimieren.",
                engagement_rate * 100.0
            ),
        });
        prio += 1;
    }

    // Very short session duration
    if avg_session_duration < 15.0 && avg_session_duration > 0.0 && traffic.total_sessions > 20 {
        recs.push(Recommendation {
            priority: prio,
            headline: "Verweildauer erhoehen".into(),
            action: format!(
                "Durchschnittliche Sitzungsdauer nur {:.0}s. Inhalte vertiefen, \
                verwandte Inhalte verlinken, Lesbarkeit verbessern.",
                avg_session_duration
            ),
        });
        prio += 1;
    }

    // Content expansion opportunity (many impressions, few clicks, position not great)
    if search.impressions > 200.0 && search.clicks < 5.0 && search.average_position > 10.0 {
        recs.push(Recommendation {
            priority: prio,
            headline: "Content ausbauen".into(),
            action: format!(
                "{:.0} Impressionen bei Position {:.1}, aber nur {:.0} Klicks. \
                Inhalt erweitern, Suchintention besser adressieren.",
                search.impressions, search.average_position, search.clicks
            ),
        });
    }

    recs
}

