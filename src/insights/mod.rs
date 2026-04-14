use crate::config::ThresholdsConfig;
use crate::domain::{
    ComparisonReport, Insight, InsightCategory, InsightSeverity, PageDetailReport,
    SearchPerformanceBreakdown, SiteOverviewReport, TopPagesReport, TrafficSourceBreakdown,
};
use crate::opportunities::expected_ctr;

// ─── Public entry points ─────────────────────────────────────────────────────

pub fn insights_for_overview(report: &mut SiteOverviewReport, th: &ThresholdsConfig) {
    let mut insights: Vec<Insight> = generate_traffic_insights(&report.traffic, th)
        .chain(generate_search_insights(&report.search))
        .collect();

    // ── Engagement insight ───────────────────────────────────────────────────
    if report.engagement_rate < th.low_engagement_rate && report.traffic.total_sessions > th.min_sessions {
        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Engagement,
            headline: format!("Low engagement rate ({:.0}%)", report.engagement_rate * 100.0),
            explanation: format!("Less than {:.0}% of sessions are considered engaged. Check landing pages and load times.", th.low_engagement_rate * 100.0),
        });
    } else if report.engagement_rate > th.high_engagement_rate {
        insights.push(Insight {
            severity: InsightSeverity::Positive,
            category: InsightCategory::Engagement,
            headline: format!("Strong engagement rate ({:.0}%)", report.engagement_rate * 100.0),
            explanation: format!("Over {:.0}% of sessions are engaged — good content quality.", th.high_engagement_rate * 100.0),
        });
    }

    // ── AI traffic insight ───────────────────────────────────────────────────
    let ai_total: i64 = report.ai_sources.iter().map(|s| s.sessions).sum();
    if ai_total > 0 {
        let ai_pct = if report.traffic.total_sessions > 0 {
            ai_total as f64 / report.traffic.total_sessions as f64 * 100.0
        } else { 0.0 };
        insights.push(Insight {
            severity: InsightSeverity::Info,
            category: InsightCategory::Traffic,
            headline: format!("AI referral traffic: {} sessions ({:.1}%)", ai_total, ai_pct),
            explanation: format!(
                "Traffic from AI tools such as {}. Content is being referenced in AI responses.",
                report.ai_sources.iter().take(3).map(|s| s.source.as_str()).collect::<Vec<_>>().join(", ")
            ),
        });
    }

    // ── Trend insights ───────────────────────────────────────────────────────
    if let Some(trend) = &report.trend {
        if trend.sessions_pct <= -th.trend_significant_pct {
            insights.push(Insight {
                severity: InsightSeverity::Critical,
                category: InsightCategory::Trend,
                headline: format!("Significant traffic decline ({:+.1}%)", trend.sessions_pct),
                explanation: format!("Sessions have dropped by more than {:.0}% compared to the previous period. Investigate causes.", th.trend_significant_pct),
            });
        } else if trend.sessions_pct >= th.trend_significant_pct {
            insights.push(Insight {
                severity: InsightSeverity::Positive,
                category: InsightCategory::Trend,
                headline: format!("Strong traffic growth ({:+.1}%)", trend.sessions_pct),
                explanation: format!("Sessions have increased by more than {:.0}% compared to the previous period.", th.trend_significant_pct),
            });
        }

        if trend.clicks_pct <= -th.trend_significant_pct {
            insights.push(Insight {
                severity: InsightSeverity::Warning,
                category: InsightCategory::Trend,
                headline: format!("Search clicks declining ({:+.1}%)", trend.clicks_pct),
                explanation: "Clicks from search have dropped significantly — check for ranking changes.".into(),
            });
        }
    }

    report.insights = insights;
}

pub fn insights_for_page(report: &mut PageDetailReport, th: &ThresholdsConfig) {
    report.insights = generate_traffic_insights(&report.traffic, th)
        .chain(generate_search_insights(&report.search))
        .collect();
}

pub fn insights_for_top_pages(report: &mut TopPagesReport, th: &ThresholdsConfig) {
    let mut insights = Vec::new();
    let pages = &report.pages;

    if pages.is_empty() {
        report.insights = insights;
        return;
    }

    // ── Dependency: top page concentration ───────────────────────────────────
    let total_sessions: i64 = pages.iter().map(|p| p.sessions).sum();
    if total_sessions > 0 && !pages.is_empty() {
        let top3_sessions: i64 = pages.iter().take(3).map(|p| p.sessions).sum();
        let top3_share = top3_sessions as f64 / total_sessions as f64 * 100.0;
        if top3_share > th.top3_dependency_pct {
            insights.push(Insight {
                severity: InsightSeverity::Warning,
                category: InsightCategory::Traffic,
                headline: format!("High dependency: top 3 pages = {:.0}% of traffic", top3_share),
                explanation: "A few pages carry the majority of traffic. A ranking loss on one of these pages would have a significant impact.".into(),
            });
        }
    }

    // ── Pages with CTR opportunity ───────────────────────────────────────────
    let ctr_opp_pages: Vec<_> = pages.iter()
        .filter(|p| {
            p.search.impressions > 200.0
                && p.search.average_position > 0.0
                && p.search.average_position <= 10.0
                && p.search.ctr < expected_ctr(p.search.average_position) * 0.7
        })
        .collect();
    if !ctr_opp_pages.is_empty() {
        // Distinguish WHY CTR is low
        let low_engagement_and_ctr: Vec<_> = ctr_opp_pages.iter()
            .filter(|p| p.engagement_rate < 0.3 && p.sessions > 5)
            .collect();

        if !low_engagement_and_ctr.is_empty() && low_engagement_and_ctr.len() as f64 / ctr_opp_pages.len() as f64 > 0.5 {
            // Majority have both low CTR AND low engagement → intent mismatch
            insights.push(Insight {
                severity: InsightSeverity::Warning,
                category: InsightCategory::Search,
                headline: format!("{} pages: CTR and engagement both below expectation", low_engagement_and_ctr.len()),
                explanation: format!(
                    "Probable cause: search intent mismatch. These pages rank on page 1 but neither \
                     the snippet nor the content matches what users are looking for. \
                     Action: analyze the top search queries for these pages and restructure content \
                     to answer the actual question. Example: {}",
                    low_engagement_and_ctr.first().map(|p| p.url.as_str()).unwrap_or("-")
                ),
            });
        } else {
            // Low CTR but engagement is ok → snippet problem
            insights.push(Insight {
                severity: InsightSeverity::Warning,
                category: InsightCategory::Search,
                headline: format!("{} pages with CTR below expectation (snippet problem)", ctr_opp_pages.len()),
                explanation: format!(
                    "Probable cause: the search snippet (title + meta description) does not match \
                     user expectations. These pages rank on page 1 and engagement is acceptable when \
                     users do click — the problem is getting the click. \
                     Action: rewrite meta titles to address the specific user question, \
                     not just the topic. Example: {}",
                    ctr_opp_pages.first().map(|p| p.url.as_str()).unwrap_or("-")
                ),
            });
        }
    }

    // ── Pages with high impressions but low clicks ───────────────────────────
    let high_impr_low_click: Vec<_> = pages.iter()
        .filter(|p| p.search.impressions > 500.0 && p.search.clicks < 10.0)
        .collect();
    if !high_impr_low_click.is_empty() {
        let avg_pos: f64 = high_impr_low_click.iter()
            .map(|p| p.search.average_position)
            .sum::<f64>() / high_impr_low_click.len() as f64;

        let cause = if avg_pos > 10.0 {
            "Probable cause: pages rank on page 2+ where organic CTR drops dramatically. \
             Action: improve content depth and internal linking to push into the top 10."
        } else {
            "Probable cause: the search result snippet does not trigger clicks despite good position. \
             Action: test more specific, benefit-driven meta titles and descriptions."
        };

        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Search,
            headline: format!("{} pages: high visibility, few clicks (avg. pos. {:.1})", high_impr_low_click.len(), avg_pos),
            explanation: format!(
                "{} impressions total across these pages but barely any clicks. {}",
                high_impr_low_click.iter().map(|p| p.search.impressions).sum::<f64>() as i64,
                cause
            ),
        });
    }

    // ── Low engagement pages ─────────────────────────────────────────────────
    let low_engagement: Vec<_> = pages.iter()
        .filter(|p| p.sessions >= 20 && p.engagement_rate < 0.3)
        .collect();
    if !low_engagement.is_empty() {
        // Try to distinguish cause: wrong traffic or wrong content?
        let organic_heavy: usize = low_engagement.iter()
            .filter(|p| p.sessions > 0 && p.organic_sessions as f64 / p.sessions as f64 > 0.5)
            .count();

        let cause = if organic_heavy as f64 / low_engagement.len() as f64 > 0.5 {
            "Probable cause: search traffic arrives with expectations that the page content does not meet. \
             The page likely ranks for a topic but answers a different question. \
             Action: check which search queries drive traffic to these pages and align the opening \
             paragraph and structure with the user's actual question."
        } else {
            "Probable cause: traffic from non-search sources (social, referral, direct) may have \
             different expectations than the content delivers. \
             Action: check if landing page experience matches the link context, \
             improve page load speed, and add clear calls to action."
        };

        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Engagement,
            headline: format!("{} pages with low engagement rate (<30%)", low_engagement.len()),
            explanation: cause.into(),
        });
    }

    // ── SEO winners (high organic share) ─────────────────────────────────────
    let seo_winners: Vec<&str> = pages.iter()
        .filter(|p| {
            p.sessions >= 20
                && (p.organic_sessions as f64 / p.sessions as f64) > 0.7
        })
        .map(|p| p.url.as_str())
        .collect();
    if seo_winners.len() >= 3 {
        insights.push(Insight {
            severity: InsightSeverity::Positive,
            category: InsightCategory::Traffic,
            headline: format!("{} pages with strong organic traffic (>70%)", seo_winners.len()),
            explanation: "These pages effectively attract organic traffic. \
                         Use them as internal linking hubs to distribute authority to weaker pages."
                .into(),
        });
    }

    // ── Internal linking opportunities ────────────────────────────────────────
    let link_opps: Vec<_> = pages.iter()
        .filter(|p| {
            p.search.average_position > 0.0
                && p.search.average_position < 10.0
                && p.search.impressions > 50.0
                && p.sessions < 10
        })
        .collect();
    if !link_opps.is_empty() {
        insights.push(Insight {
            severity: InsightSeverity::Info,
            category: InsightCategory::Search,
            headline: format!("{} pages: good search position, but few sessions", link_opps.len()),
            explanation: format!(
                "These pages rank well but are barely visited — likely orphaned in your site structure. \
                 Action: add internal links from your top-traffic pages to these URLs. \
                 Priority candidate: {}",
                link_opps.first().map(|p| p.url.as_str()).unwrap_or("-")
            ),
        });
    }

    report.insights = insights;
}

pub fn insights_for_comparison(report: &mut ComparisonReport) {
    let mut insights = Vec::new();
    let d = &report.delta;

    // ── Sessions ─────────────────────────────────────────────────────────────
    if d.sessions_pct >= 10.0 {
        insights.push(Insight {
            severity: InsightSeverity::Positive,
            category: InsightCategory::Traffic,
            headline: format!("Sessions increased ({:+.1}%)", d.sessions_pct),
            explanation: format!(
                "In the period after {}, sessions increased by {:.1}% ({:+} absolute).",
                report.change_date, d.sessions_pct, d.sessions_abs
            ),
        });
    } else if d.sessions_pct <= -10.0 {
        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Traffic,
            headline: format!("Sessions decreased ({:+.1}%)", d.sessions_pct),
            explanation: format!(
                "In the period after {}, sessions decreased by {:.1}% ({:+} absolute).",
                report.change_date, d.sessions_pct, d.sessions_abs
            ),
        });
    }

    // ── Organic sessions ─────────────────────────────────────────────────────
    if d.organic_sessions_pct >= 15.0 {
        insights.push(Insight {
            severity: InsightSeverity::Positive,
            category: InsightCategory::Traffic,
            headline: format!("Organic traffic increased ({:+.1}%)", d.organic_sessions_pct),
            explanation: "Organic sessions have developed positively.".into(),
        });
    } else if d.organic_sessions_pct <= -15.0 {
        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Traffic,
            headline: format!("Organic traffic decreased ({:+.1}%)", d.organic_sessions_pct),
            explanation: "Decline in organic traffic — check whether ranking changes have occurred.".into(),
        });
    }

    // ── Search clicks ────────────────────────────────────────────────────────
    if d.clicks_pct >= 15.0 {
        insights.push(Insight {
            severity: InsightSeverity::Positive,
            category: InsightCategory::Search,
            headline: format!("Search clicks increased ({:+.1}%)", d.clicks_pct),
            explanation: "Change correlates with a positive search trend.".into(),
        });
    } else if d.clicks_pct <= -15.0 {
        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Search,
            headline: format!("Search clicks decreased ({:+.1}%)", d.clicks_pct),
            explanation: "Search visibility decreased after the change.".into(),
        });
    }

    // ── Impressions ──────────────────────────────────────────────────────────
    if d.impressions_pct >= 20.0 {
        insights.push(Insight {
            severity: InsightSeverity::Positive,
            category: InsightCategory::Search,
            headline: format!("Impressions increased ({:+.1}%)", d.impressions_pct),
            explanation: "Higher search visibility after the change.".into(),
        });
    } else if d.impressions_pct <= -20.0 {
        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Search,
            headline: format!("Impressions decreased ({:+.1}%)", d.impressions_pct),
            explanation: "Significantly fewer search impressions — possible ranking loss.".into(),
        });
    }

    // ── CTR change ───────────────────────────────────────────────────────────
    let ctr_pp = d.ctr_abs * 100.0; // percentage points
    if ctr_pp >= 1.0 {
        insights.push(Insight {
            severity: InsightSeverity::Positive,
            category: InsightCategory::Search,
            headline: format!("CTR improved ({:+.1} percentage points)", ctr_pp),
            explanation: "Snippets are getting more clicks — title/description changes are working.".into(),
        });
    } else if ctr_pp <= -1.0 {
        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Search,
            headline: format!("CTR declined ({:+.1} percentage points)", ctr_pp),
            explanation: "Click-through rate has dropped — check whether snippets or search result format have changed.".into(),
        });
    }

    // ── Position change ──────────────────────────────────────────────────────
    if d.position_abs <= -2.0 {
        insights.push(Insight {
            severity: InsightSeverity::Positive,
            category: InsightCategory::Search,
            headline: format!("Ranking improved ({:+.1} positions)", d.position_abs),
            explanation: "Average position has improved significantly.".into(),
        });
    } else if d.position_abs >= 2.0 {
        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Search,
            headline: format!("Ranking declined ({:+.1} positions)", d.position_abs),
            explanation: "Average position has dropped — investigate the cause.".into(),
        });
    }

    // ── Overall verdict ──────────────────────────────────────────────────────
    let positive_count = insights.iter()
        .filter(|i| i.severity == InsightSeverity::Positive).count();
    let warning_count = insights.iter()
        .filter(|i| i.severity == InsightSeverity::Warning).count();

    if positive_count > 0 && warning_count == 0 {
        insights.insert(0, Insight {
            severity: InsightSeverity::Positive,
            category: InsightCategory::Trend,
            headline: "Positive development after change date".into(),
            explanation: format!(
                "All key metrics have developed positively after {}.",
                report.change_date
            ),
        });
    } else if warning_count > 0 && positive_count == 0 {
        insights.insert(0, Insight {
            severity: InsightSeverity::Critical,
            category: InsightCategory::Trend,
            headline: "Negative development after change date".into(),
            explanation: format!(
                "Multiple metrics show deterioration after {}. Review changes.",
                report.change_date
            ),
        });
    } else if positive_count > 0 && warning_count > 0 {
        insights.insert(0, Insight {
            severity: InsightSeverity::Info,
            category: InsightCategory::Trend,
            headline: "Mixed development after change date".into(),
            explanation: format!(
                "Some metrics improved after {}, while others deteriorated.",
                report.change_date
            ),
        });
    }

    report.insights = insights;
}

// ─── Rule implementations ────────────────────────────────────────────────────

fn generate_traffic_insights(
    traffic: &TrafficSourceBreakdown,
    th: &ThresholdsConfig,
) -> impl Iterator<Item = Insight> {
    let mut insights = Vec::new();

    let organic_share = traffic.organic_share();
    let direct_share = traffic.direct_share();

    if organic_share < th.low_organic_pct && traffic.total_sessions > th.min_sessions {
        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Traffic,
            headline: "Low organic traffic".into(),
            explanation: format!(
                "Only {:.0}% of sessions come from organic search. \
                This indicates SEO potential.",
                organic_share
            ),
        });
    }

    if direct_share > th.high_direct_pct {
        insights.push(Insight {
            severity: InsightSeverity::Info,
            category: InsightCategory::Traffic,
            headline: "Above-average direct traffic".into(),
            explanation: format!(
                "{:.0}% of sessions come directly. \
                This may indicate strong brand awareness or missing UTM tracking parameters.",
                direct_share
            ),
        });
    }

    if organic_share > th.high_organic_pct {
        insights.push(Insight {
            severity: InsightSeverity::Positive,
            category: InsightCategory::Traffic,
            headline: "Strong organic traffic".into(),
            explanation: format!(
                "{:.0}% of traffic comes from organic search — good SEO foundation.",
                organic_share
            ),
        });
    }

    insights.into_iter()
}

fn generate_search_insights(
    search: &SearchPerformanceBreakdown,
) -> impl Iterator<Item = Insight> + '_ {
    let mut insights = Vec::new();

    // High impressions, low CTR — with cause analysis
    if search.impressions > 500.0 && search.ctr < 0.02 {
        let exp = expected_ctr(search.average_position);
        let cause = if search.average_position <= 10.0 {
            format!(
                "Probable cause: the search snippet does not match user expectations. \
                 At position {:.1}, a CTR of {:.1}% would be expected, but actual is {:.1}%. \
                 Action: rewrite the meta title to address the specific user question — \
                 not just name the topic, but promise a concrete answer or benefit.",
                search.average_position, exp * 100.0, search.ctr * 100.0
            )
        } else {
            format!(
                "Probable cause: pages rank on page 2+ (avg. position {:.1}) where organic \
                 CTR drops dramatically. Action: deepen content, strengthen internal linking, \
                 and address subtopics that competitors in the top 10 cover.",
                search.average_position
            )
        };

        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Search,
            headline: format!("High impressions ({:.0}), but low CTR ({:.1}%)", search.impressions, search.ctr * 100.0),
            explanation: cause,
        });
    }

    // Good position but few clicks
    if search.average_position < 5.0 && search.clicks < 10.0 && search.impressions > 100.0 {
        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Search,
            headline: format!("Position {:.1} but only {:.0} clicks", search.average_position, search.clicks),
            explanation: format!(
                "{:.0} impressions at a strong position, but the snippet fails to convert. \
                 Probable cause: the title is too generic or doesn't match the dominant search intent. \
                 Action: check the top queries driving impressions and rewrite the title \
                 to directly answer the most common query.",
                search.impressions
            ),
        });
    }

    // Solid search performance
    if search.ctr > 0.05 && search.clicks > 100.0 {
        insights.push(Insight {
            severity: InsightSeverity::Positive,
            category: InsightCategory::Search,
            headline: "Strong search performance".into(),
            explanation: format!(
                "{:.1}% CTR with {:.0} clicks — the snippet effectively matches search intent. \
                 Use this page's title/description pattern as a template for similar content.",
                search.ctr * 100.0, search.clicks
            ),
        });
    }

    insights.into_iter()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::*;

    fn make_traffic(organic: i64, direct: i64, referral: i64) -> TrafficSourceBreakdown {
        TrafficSourceBreakdown {
            organic_sessions: organic,
            direct_sessions: direct,
            referral_sessions: referral,
            other_sessions: 0,
            total_sessions: organic + direct + referral,
        }
    }

    fn make_search(clicks: f64, impressions: f64, ctr: f64, position: f64) -> SearchPerformanceBreakdown {
        SearchPerformanceBreakdown {
            clicks, impressions, ctr, average_position: position,
            top_queries: vec![],
        }
    }

    #[test]
    fn low_organic_triggers_warning() {
        let traffic = make_traffic(10, 80, 20);
        let th = ThresholdsConfig::default();
        let insights: Vec<_> = generate_traffic_insights(&traffic, &th).collect();
        assert!(insights.iter().any(|i| i.severity == InsightSeverity::Warning && i.headline.contains("organic")));
    }

    #[test]
    fn high_organic_triggers_positive() {
        let traffic = make_traffic(80, 10, 10);
        let th = ThresholdsConfig::default();
        let insights: Vec<_> = generate_traffic_insights(&traffic, &th).collect();
        assert!(insights.iter().any(|i| i.severity == InsightSeverity::Positive));
    }

    #[test]
    fn high_direct_triggers_info() {
        let traffic = make_traffic(10, 70, 5);
        let th = ThresholdsConfig::default();
        let insights: Vec<_> = generate_traffic_insights(&traffic, &th).collect();
        assert!(insights.iter().any(|i| i.severity == InsightSeverity::Info && i.headline.contains("direct")));
    }

    #[test]
    fn high_impressions_low_ctr_warning() {
        let search = make_search(5.0, 1000.0, 0.005, 8.0);
        let insights: Vec<_> = generate_search_insights(&search).collect();
        assert!(insights.iter().any(|i| i.severity == InsightSeverity::Warning && i.headline.contains("CTR")));
    }

    #[test]
    fn good_search_performance_positive() {
        let search = make_search(200.0, 2000.0, 0.10, 3.0);
        let insights: Vec<_> = generate_search_insights(&search).collect();
        assert!(insights.iter().any(|i| i.severity == InsightSeverity::Positive));
    }

    #[test]
    fn comparison_positive_verdict() {
        let mut report = ComparisonReport {
            url: None,
            property_name: "test".into(),
            change_date: "2026-01-01".into(),
            before_days: 30,
            after_days: 30,
            before: ComparisonPeriod {
                start_date: "2025-12-01".into(),
                end_date: "2025-12-31".into(),
                sessions: 100,
                organic_sessions: 50,
                engagement_rate: 0.5,
                search: make_search(50.0, 1000.0, 0.05, 5.0),
            },
            after: ComparisonPeriod {
                start_date: "2026-01-01".into(),
                end_date: "2026-01-30".into(),
                sessions: 150,
                organic_sessions: 80,
                engagement_rate: 0.6,
                search: make_search(80.0, 1500.0, 0.053, 4.0),
            },
            delta: ComparisonDelta {
                sessions_abs: 50,
                sessions_pct: 50.0,
                organic_sessions_abs: 30,
                organic_sessions_pct: 60.0,
                engagement_rate_abs: 0.1,
                clicks_abs: 30.0,
                clicks_pct: 60.0,
                impressions_abs: 500.0,
                impressions_pct: 50.0,
                ctr_abs: 0.003,
                position_abs: -1.0,
            },
            summary: String::new(),
            insights: vec![],
        };

        insights_for_comparison(&mut report);
        assert!(!report.insights.is_empty());
        assert_eq!(report.insights[0].severity, InsightSeverity::Positive);
        assert!(report.insights[0].headline.contains("Positive"));
    }

    #[test]
    fn comparison_negative_verdict() {
        let mut report = ComparisonReport {
            url: None,
            property_name: "test".into(),
            change_date: "2026-01-01".into(),
            before_days: 30,
            after_days: 30,
            before: ComparisonPeriod::default(),
            after: ComparisonPeriod::default(),
            delta: ComparisonDelta {
                sessions_abs: -50,
                sessions_pct: -30.0,
                organic_sessions_abs: -20,
                organic_sessions_pct: -25.0,
                clicks_abs: -30.0,
                clicks_pct: -40.0,
                impressions_abs: -500.0,
                impressions_pct: -30.0,
                ctr_abs: -0.02,
                position_abs: 3.0,
                ..Default::default()
            },
            summary: String::new(),
            insights: vec![],
        };

        insights_for_comparison(&mut report);
        assert!(!report.insights.is_empty());
        assert_eq!(report.insights[0].severity, InsightSeverity::Critical);
        assert!(report.insights[0].headline.contains("Negative"));
    }

    #[test]
    fn top_pages_dependency_warning() {
        let mut report = TopPagesReport {
            property_name: "test".into(),
            date_range: "test".into(),
            pages: vec![
                PageSummary {
                    url: "/page1".into(), sessions: 700, organic_sessions: 500,
                    direct_sessions: 200, engagement_rate: 0.5, bounce_rate: 0.5,
                    avg_session_duration_secs: 60.0, new_user_share: 0.0, key_events: 0,
                    search: SearchPerformanceBreakdown::default(),
                },
                PageSummary {
                    url: "/page2".into(), sessions: 200, organic_sessions: 100,
                    direct_sessions: 100, engagement_rate: 0.5, bounce_rate: 0.5,
                    avg_session_duration_secs: 60.0, new_user_share: 0.0, key_events: 0,
                    search: SearchPerformanceBreakdown::default(),
                },
                PageSummary {
                    url: "/page3".into(), sessions: 50, organic_sessions: 25,
                    direct_sessions: 25, engagement_rate: 0.5, bounce_rate: 0.5,
                    avg_session_duration_secs: 60.0, new_user_share: 0.0, key_events: 0,
                    search: SearchPerformanceBreakdown::default(),
                },
                PageSummary {
                    url: "/page4".into(), sessions: 50, organic_sessions: 25,
                    direct_sessions: 25, engagement_rate: 0.5, bounce_rate: 0.5,
                    avg_session_duration_secs: 60.0, new_user_share: 0.0, key_events: 0,
                    search: SearchPerformanceBreakdown::default(),
                },
            ],
            insights: vec![],
        };

        let th = ThresholdsConfig::default();
        insights_for_top_pages(&mut report, &th);
        assert!(report.insights.iter().any(|i|
            i.severity == InsightSeverity::Warning && i.headline.contains("dependency")
        ));
    }
}
