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
    let date_label = format!("last {} days", days);

    // ── GA4: channel breakdown for this page ──────────────────────────────────
    let req = ReportRequest {
        property_id: property_id.clone(),
        date_ranges: vec![date_range.clone()],
        dimensions: vec!["sessionDefaultChannelGroup".into()],
        metrics: vec![
            "sessions".into(),
            "engagementRate".into(),
            "averageSessionDuration".into(),
            "bounceRate".into(),
            "newUsers".into(),
            "keyEvents".into(),
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
    let mut bounce_sum = 0.0f64;
    let mut new_user_count = 0i64;
    let mut key_event_count = 0i64;
    let mut session_count = 0i64;

    for row in &ga_report.rows {
        let channel = row.dimension_values.first().map(String::as_str).unwrap_or("");
        let sessions: i64 = row.metric_values.first().and_then(|v| v.parse().ok()).unwrap_or(0);
        let eng: f64 = row.metric_values.get(1).and_then(|v| v.parse().ok()).unwrap_or(0.0);
        let dur: f64 = row.metric_values.get(2).and_then(|v| v.parse().ok()).unwrap_or(0.0);
        let bounce: f64 = row.metric_values.get(3).and_then(|v| v.parse().ok()).unwrap_or(0.0);
        let new_users: i64 = row.metric_values.get(4).and_then(|v| v.parse::<f64>().ok()).map(|v| v as i64).unwrap_or(0);
        let key_evts: i64 = row.metric_values.get(5).and_then(|v| v.parse::<f64>().ok()).map(|v| v as i64).unwrap_or(0);

        traffic.total_sessions += sessions;
        eng_sum += eng * sessions as f64;
        dur_sum += dur * sessions as f64;
        bounce_sum += bounce * sessions as f64;
        new_user_count += new_users;
        key_event_count += key_evts;
        session_count += sessions;

        match channel {
            "Organic Search" => traffic.organic_sessions += sessions,
            "Direct"         => traffic.direct_sessions  += sessions,
            "Referral"       => traffic.referral_sessions += sessions,
            _                => traffic.other_sessions    += sessions,
        }
    }

    let engagement_rate = if session_count > 0 { eng_sum / session_count as f64 } else { 0.0 };
    let bounce_rate = if session_count > 0 { bounce_sum / session_count as f64 } else { 0.0 };
    let avg_session_duration = if session_count > 0 { dur_sum / session_count as f64 } else { 0.0 };
    let new_user_share = if session_count > 0 { new_user_count as f64 / session_count as f64 } else { 0.0 };
    let key_events = key_event_count;

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
                intent: None,
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
        bounce_rate,
        avg_session_duration_secs: avg_session_duration,
        new_user_share,
        key_events,
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

    let exp_ctr = crate::opportunities::expected_ctr(search.average_position);

    // CTR fix: good position but low CTR — with root cause distinction
    if search.average_position > 0.0 && search.average_position <= 10.0
        && search.impressions > 100.0
        && search.ctr < exp_ctr * 0.7
    {
        let ctr_gap = exp_ctr * 100.0 - search.ctr * 100.0;

        if engagement_rate < 0.3 && traffic.total_sessions > 10 {
            // Low CTR + low engagement = intent mismatch
            recs.push(Recommendation {
                priority: prio,
                headline: "Realign content with search intent".into(),
                action: format!(
                    "Position {:.1} but only {:.1}% CTR (expected {:.1}%, gap: {:.1}pp) AND low engagement ({:.0}%). \
                     Double signal: neither the snippet nor the content matches what users search for. \
                     Analyze the top queries driving impressions. Restructure the opening paragraph to \
                     directly answer the most common question. Move concrete examples and actionable \
                     information to the top of the page.",
                    search.average_position, search.ctr * 100.0, exp_ctr * 100.0,
                    ctr_gap, engagement_rate * 100.0
                ),
            });
        } else {
            // Low CTR but ok engagement = snippet problem
            recs.push(Recommendation {
                priority: prio,
                headline: "Rewrite search snippet — title and description miss the mark".into(),
                action: format!(
                    "Position {:.1} with {:.0} impressions, but only {:.1}% CTR (expected {:.1}%, gap: {:.1}pp). \
                     When users do click, engagement is acceptable — the problem is getting the click. \
                     Rewrite the meta title to address the user's specific question, not just name the topic. \
                     Add a meta description that promises concrete value (examples, steps, comparisons).",
                    search.average_position, search.impressions, search.ctr * 100.0,
                    exp_ctr * 100.0, ctr_gap
                ),
            });
        }
        prio += 1;
    }

    // High impressions, very low CTR, no snippet fix above
    if search.impressions > 200.0 && search.ctr < 0.02 && prio == 1 {
        let cause = if search.average_position > 10.0 {
            format!(
                "Position {:.1} puts this page on page 2+ where organic CTR drops sharply. \
                 Deepen content depth and strengthen internal linking to push into the top 10. \
                 Review what top-ranking competitors cover that this page does not.",
                search.average_position
            )
        } else {
            "The snippet does not convert despite decent position. \
             Test a more specific, benefit-driven meta title that directly addresses \
             the user's likely question."
                .into()
        };
        recs.push(Recommendation {
            priority: prio,
            headline: "Address CTR gap — users see this page but don't click".into(),
            action: cause,
        });
        prio += 1;
    }

    // Low organic share
    if traffic.organic_share() < 20.0 && traffic.total_sessions > 50 {
        recs.push(Recommendation {
            priority: prio,
            headline: "Unlock organic search potential".into(),
            action: format!(
                "Only {:.0}% of traffic comes from organic search — most visitors arrive through other channels. \
                 This page likely has untapped keyword potential. Add internal links from high-authority pages, \
                 ensure the page targets a clear primary keyword, and structure content with clear headings \
                 that match how users search for this topic.",
                traffic.organic_share()
            ),
        });
        prio += 1;
    }

    // Good search position but few sessions → orphaned page
    if search.average_position > 0.0 && search.average_position < 10.0
        && search.impressions > 50.0
        && traffic.total_sessions < 10
    {
        recs.push(Recommendation {
            priority: prio,
            headline: "Connect this page — it ranks well but is orphaned".into(),
            action: format!(
                "Position {:.1} with {:.0} impressions, but only {} sessions. \
                 The page is visible in search but isolated in your site structure. \
                 Add internal links from your 3-5 highest-traffic related pages. \
                 Consider featuring it in navigation or sidebar if topically important.",
                search.average_position, search.impressions, traffic.total_sessions
            ),
        });
        prio += 1;
    }

    // Weak engagement — with cause distinction
    if engagement_rate < 0.3 && traffic.total_sessions > 20 {
        let organic_pct = traffic.organic_share();
        let cause = if organic_pct > 50.0 {
            format!(
                "Engagement rate only {:.0}% with {:.0}% organic traffic — search visitors likely \
                 arrive with expectations the content doesn't meet. Check which queries drive traffic \
                 and whether the opening section addresses those questions. Consider adding a direct \
                 answer or key takeaway in the first paragraph.",
                engagement_rate * 100.0, organic_pct
            )
        } else {
            format!(
                "Engagement rate only {:.0}%. Most traffic comes from non-search sources — check if \
                 the landing experience matches the link context (social posts, referral pages). \
                 Improve page load speed and add clear visual hierarchy.",
                engagement_rate * 100.0
            )
        };
        recs.push(Recommendation {
            priority: prio,
            headline: "Low engagement — visitors leave too quickly".into(),
            action: cause,
        });
        prio += 1;
    }

    // Very short session duration
    if avg_session_duration < 15.0 && avg_session_duration > 0.0 && traffic.total_sessions > 20 {
        recs.push(Recommendation {
            priority: prio,
            headline: "Extremely short visits — content doesn't hold attention".into(),
            action: format!(
                "Average session duration only {:.0}s — visitors barely read anything. \
                 Probable causes: slow load time, content doesn't match expectations, \
                 or the page answers the question too superficially. \
                 Add depth: concrete examples, step-by-step instructions, visual elements. \
                 Link to related content to encourage further exploration.",
                avg_session_duration
            ),
        });
        prio += 1;
    }

    // Content expansion opportunity
    if search.impressions > 200.0 && search.clicks < 5.0 && search.average_position > 10.0 {
        recs.push(Recommendation {
            priority: prio,
            headline: "Content gap — search demand exists but page doesn't capture it".into(),
            action: format!(
                "{:.0} impressions at position {:.1}, but only {:.0} clicks. \
                 The topic generates search demand but this page doesn't rank high enough to capture it. \
                 Expand the content to cover subtopics and related questions. \
                 Add structured sections with clear headings that match user search patterns.",
                search.impressions, search.average_position, search.clicks
            ),
        });
    }

    recs
}

