//! Narrative Engine
//!
//! Rule-based English text generation for management summaries
//! and table interpretations. Each rule checks a condition against
//! report data and emits an English sentence if triggered.

use crate::domain::{
    AiTrafficReport, ClustersReport, GrowthReport, OpportunitiesReport, SiteOverviewReport,
    TopPagesReport,
};

/// Input for the narrative engine: references to all available report data.
/// All fields are optional so the engine works with partial data.
pub struct NarrativeInput<'a> {
    pub overview: Option<&'a SiteOverviewReport>,
    pub top_pages: Option<&'a TopPagesReport>,
    pub opportunities: Option<&'a OpportunitiesReport>,
    pub growth: Option<&'a GrowthReport>,
    pub ai_traffic: Option<&'a AiTrafficReport>,
    pub clusters: Option<&'a ClustersReport>,
}

/// Generate a management summary as connected story paragraphs.
///
/// Returns 2-4 paragraphs that tell a coherent story:
/// 1. State + Tension: how is traffic developing, what's the main dynamic?
/// 2. Key Finding: what's the most notable specific insight?
/// 3. Emerging Signal: forward-looking observation (AI traffic, growth patterns)
pub fn management_summary(input: &NarrativeInput) -> Vec<String> {
    let mut paragraphs = Vec::new();

    // Paragraph 1: State + Tension
    if let Some(para) = build_state_paragraph(input) {
        paragraphs.push(para);
    }

    // Paragraph 2: Key Finding (opportunities + search gaps)
    if let Some(para) = build_key_finding_paragraph(input) {
        paragraphs.push(para);
    }

    // Paragraph 3: Emerging signal / forward-looking
    if let Some(para) = build_forward_signal_paragraph(input) {
        paragraphs.push(para);
    }

    // Fallback: if we couldn't build any paragraphs, use legacy rules
    if paragraphs.is_empty() {
        let mut sentences = Vec::new();
        if let Some(overview) = input.overview {
            traffic_rules(overview, &mut sentences);
        }
        sentences.truncate(3);
        paragraphs = sentences;
    }

    paragraphs
}

/// Paragraph 1: State + Tension — traffic situation, trend, and the "but" (concentration, gap).
fn build_state_paragraph(input: &NarrativeInput) -> Option<String> {
    let overview = input.overview?;
    let t = &overview.traffic;

    if t.total_sessions == 0 {
        return None;
    }

    let mut parts = Vec::new();

    // Traffic state + trend
    let organic_pct = t.organic_share();
    if let Some(trend) = &overview.trend {
        if trend.sessions_pct > 15.0 {
            parts.push(format!(
                "Traffic is growing strongly ({:+.0}% sessions, {:+.0}% impressions)",
                trend.sessions_pct, trend.impressions_pct
            ));
        } else if trend.sessions_pct < -15.0 {
            parts.push(format!(
                "Traffic has declined significantly ({:.0}% fewer sessions)",
                trend.sessions_pct.abs()
            ));
        } else {
            parts.push(format!(
                "Traffic is stable at {} sessions in the period",
                t.total_sessions
            ));
        }
    } else {
        parts.push(format!(
            "{} sessions in the analysis period",
            t.total_sessions
        ));
    }

    // The "but" — concentration or untapped potential
    let mut tension = None;

    // Check page dependency
    if let Some(top_pages) = input.top_pages {
        if top_pages.pages.len() >= 3 {
            let total: i64 = top_pages.pages.iter().map(|p| p.sessions).sum();
            if total > 0 {
                let top3: i64 = top_pages.pages.iter().take(3).map(|p| p.sessions).sum();
                let top3_pct = top3 as f64 / total as f64 * 100.0;
                if top3_pct > 60.0 {
                    tension = Some(format!(
                        "but is currently carried by a few pages ({:.0}% of traffic comes from just three URLs)",
                        top3_pct
                    ));
                }
            }
        }
    }

    // Check search potential gap
    if tension.is_none() {
        if overview.search.average_position > 0.0
            && overview.search.average_position <= 10.0
            && overview.search.ctr < 0.03
            && overview.search.impressions > 200.0
        {
            tension = Some(
                "but a large part of the search potential remains untapped: \
                 the site ranks well on page 1 yet achieves below-average click rates"
                    .into(),
            );
        }
    }

    // Organic share context
    let organic_context = if organic_pct > 70.0 {
        format!(
            "The SEO foundation is strong ({:.0}% organic traffic)",
            organic_pct
        )
    } else if organic_pct < 20.0 && t.total_sessions > 50 {
        format!(
            "SEO potential is largely untapped — only {:.0}% of sessions come from organic search",
            organic_pct
        )
    } else {
        String::new()
    };

    // Compose the paragraph
    let mut para = parts.join(", ");
    if let Some(t) = tension {
        para = format!("{}, {}.", para, t);
    } else {
        para.push('.');
    }
    if !organic_context.is_empty() {
        para = format!("{} {}", para, organic_context);
        if !para.ends_with('.') {
            para.push('.');
        }
    }

    Some(para)
}

/// Paragraph 2: Key Finding — the most impactful specific insight from opportunities/search data.
fn build_key_finding_paragraph(input: &NarrativeInput) -> Option<String> {
    let opportunities = input.opportunities?;

    if opportunities.opportunities.is_empty() {
        return None;
    }

    let mut parts = Vec::new();

    // Total opportunity potential
    if opportunities.total_estimated_clicks > 20.0 {
        parts.push(format!(
            "Across {} identified opportunities, an estimated ~{:.0} additional clicks per period \
             are achievable",
            opportunities.opportunities.len(),
            opportunities.total_estimated_clicks
        ));
    }

    // Highlight top opportunity with interpretation
    if let Some(top) = opportunities.opportunities.first() {
        let target = top
            .keyword
            .as_deref()
            .unwrap_or(top.url.as_str());

        // Determine specific finding based on type
        let finding = match top.opportunity_type {
            crate::domain::OpportunityType::CtrFix | crate::domain::OpportunityType::SnippetProblem => {
                format!(
                    "The highest-impact opportunity: \"{}\" ranks well but the snippet does not match \
                     search intent — rewriting the title and description alone could yield +{:.0} clicks",
                    target, top.estimated_clicks
                )
            }
            crate::domain::OpportunityType::IntentMismatch => {
                format!(
                    "Most notable: {} gets impressions but both CTR and engagement are low — the content \
                     does not answer what users are actually searching for",
                    target
                )
            }
            crate::domain::OpportunityType::RankingPush | crate::domain::OpportunityType::RankingProblem => {
                format!(
                    "The biggest lever: \"{}\" ranks on the edge of page 1 — pushing it into the top 3 \
                     would multiply click volume (+{:.0} clicks estimated)",
                    target, top.estimated_clicks
                )
            }
            crate::domain::OpportunityType::ContentExpansion | crate::domain::OpportunityType::ExpansionPotential => {
                format!(
                    "Largest content gap: \"{}\" generates search demand but lacks a strong dedicated page \
                     (+{:.0} clicks possible with targeted content)",
                    target, top.estimated_clicks
                )
            }
            _ => {
                format!(
                    "Top opportunity: \"{}\" with +{:.0} estimated clicks",
                    target, top.estimated_clicks
                )
            }
        };
        parts.push(finding);
    }

    // Quick wins count
    let quick_wins = opportunities.action_plan.quick_wins.len();
    if quick_wins > 0 {
        parts.push(format!(
            "{} of these are quick wins (low effort, high impact) that can be tackled this week",
            quick_wins
        ));
    }

    if parts.is_empty() {
        return None;
    }

    Some(format!("{}.", parts.join(". ")))
}

/// Paragraph 3: Forward Signal — AI traffic, emerging patterns, or strategic recommendation.
fn build_forward_signal_paragraph(input: &NarrativeInput) -> Option<String> {
    let mut parts = Vec::new();

    // AI traffic signal
    if let Some(ai) = input.ai_traffic {
        if ai.ai_sessions > 0 && ai.ai_share_pct > 1.0 {
            let trend_note = if ai.ai_trend_pct > 20.0 {
                format!(" (growing {:+.0}% vs. previous period)", ai.ai_trend_pct)
            } else {
                String::new()
            };

            parts.push(format!(
                "An emerging signal: {:.1}% of traffic already comes from AI tools{}",
                ai.ai_share_pct, trend_note
            ));

            // Content pattern insight
            if let Some(pattern) = &ai.content_pattern {
                // Take first sentence of pattern
                let first_sentence = pattern.split(". ").next().unwrap_or(pattern);
                parts.push(first_sentence.to_string());
            }

            if ai.ai_engagement_rate > ai.overall_engagement_rate * 1.1 {
                parts.push(
                    "AI-referred visitors show above-average engagement — \
                     this content style can be expanded to more topics"
                        .into(),
                );
            }
        }
    }

    // Growth drivers as forward signal
    if parts.is_empty() {
        if let Some(growth) = input.growth {
            if let Some(top) = growth.top_growing_pages.first() {
                if top.delta > 30.0 {
                    parts.push(format!(
                        "Strongest growth signal: {} with {:+.0} sessions ({:+.0}%)",
                        shorten(&top.label, 40),
                        top.delta,
                        top.delta_pct
                    ));
                }
            }
            if !growth.new_queries.is_empty() {
                let new_clicks: f64 = growth.new_queries.iter().map(|q| q.clicks).sum();
                if new_clicks > 10.0 {
                    parts.push(format!(
                        "{} new search queries have appeared, bringing {:.0} additional clicks — \
                         the content is gaining new topical relevance",
                        growth.new_queries.len(),
                        new_clicks
                    ));
                }
            }
        }
    }

    // Cluster insight
    if let Some(clusters) = input.clusters {
        if let Some(best) = clusters.clusters.iter()
            .filter(|c| c.ctr_potential > 20.0)
            .max_by(|a, b| a.ctr_potential.partial_cmp(&b.ctr_potential).unwrap_or(std::cmp::Ordering::Equal))
        {
            parts.push(format!(
                "The topic cluster \"{}\" has the highest optimization potential \
                 (~{:.0} additional clicks at optimal CTR)",
                best.name, best.ctr_potential
            ));
        }
    }

    if parts.is_empty() {
        return None;
    }

    Some(format!("{}.", parts.join(". ")))
}

/// Generate a short interpretation (1-2 sentences) for a specific section.
#[allow(dead_code)]
pub fn section_interpretation(section: &str, input: &NarrativeInput) -> Option<String> {
    match section {
        "overview" => input.overview.and_then(interpret_overview),
        "top_pages" => input.top_pages.and_then(interpret_top_pages),
        "opportunities" => input.opportunities.and_then(interpret_opportunities),
        "growth" => input.growth.and_then(interpret_growth),
        "ai_traffic" => input.ai_traffic.and_then(interpret_ai),
        _ => None,
    }
}

// ─── Traffic rules ─────────────────────────────────────────────────────────

fn traffic_rules(overview: &SiteOverviewReport, out: &mut Vec<String>) {
    let t = &overview.traffic;

    // Organic share assessment
    let organic_pct = t.organic_share();
    if organic_pct > 70.0 {
        out.push(format!(
            "Organic traffic dominates with {:.0}% of sessions — \
            the SEO foundation is strong.",
            organic_pct
        ));
    } else if organic_pct < 20.0 && t.total_sessions > 50 {
        out.push(format!(
            "Only {:.0}% of sessions come from organic search — \
            SEO potential is largely untapped.",
            organic_pct
        ));
    }

    // Trend assessment
    if let Some(trend) = &overview.trend {
        if trend.sessions_pct > 15.0 && trend.impressions_pct > 10.0 {
            out.push(format!(
                "Organic visibility is growing ({:+.0}% impressions), \
                and click yield is keeping pace ({:+.0}% sessions).",
                trend.impressions_pct, trend.sessions_pct
            ));
        } else if trend.sessions_pct > 10.0 && trend.impressions_pct < 0.0 {
            out.push(
                "Sessions are rising despite declining impressions — \
                CTR optimization is working."
                    .into(),
            );
        } else if trend.impressions_pct > 15.0 && trend.sessions_pct < 5.0 {
            out.push(
                "Visibility is growing, but click yield is not keeping pace \
                — several well-ranking pages are clicked too rarely."
                    .into(),
            );
        } else if trend.sessions_pct < -15.0 {
            out.push(format!(
                "Sessions have dropped by {:.0}% — \
                check root cause analysis in Growth Drivers and Content Decay.",
                trend.sessions_pct.abs()
            ));
        }
    }

    // Engagement assessment
    if overview.engagement_rate > 0.7 {
        out.push(format!(
            "The engagement rate of {:.0}% is above average.",
            overview.engagement_rate * 100.0
        ));
    } else if overview.engagement_rate < 0.3 && t.total_sessions > 100 {
        out.push(format!(
            "The engagement rate is only {:.0}% — \
            check content relevance and page speed.",
            overview.engagement_rate * 100.0
        ));
    }
}

// ─── Legacy rule functions (kept for section_interpretation) ──────────────
// These are used by section_interpretation() which is wired for future use.

// ─── Section interpretations ───────────────────────────────────────────────

fn interpret_overview(overview: &SiteOverviewReport) -> Option<String> {
    let t = &overview.traffic;
    if t.total_sessions == 0 {
        return None;
    }

    let organic_pct = t.organic_share();
    let trend_hint = overview
        .trend
        .as_ref()
        .map(|tr| {
            if tr.sessions_pct > 10.0 {
                " with upward trend"
            } else if tr.sessions_pct < -10.0 {
                " with downward trend"
            } else {
                " at a stable level"
            }
        })
        .unwrap_or("");

    Some(format!(
        "{} sessions in the period, {:.0}% of which organic{}.",
        t.total_sessions, organic_pct, trend_hint
    ))
}

fn interpret_top_pages(top_pages: &TopPagesReport) -> Option<String> {
    if top_pages.pages.is_empty() {
        return None;
    }

    let total: i64 = top_pages.pages.iter().map(|p| p.sessions).sum();
    let top = &top_pages.pages[0];
    let top_pct = if total > 0 {
        top.sessions as f64 / total as f64 * 100.0
    } else {
        0.0
    };

    Some(format!(
        "The top page ({}) accounts for {:.0}% of visible sessions.",
        shorten(&top.url, 40),
        top_pct
    ))
}

fn interpret_opportunities(opportunities: &OpportunitiesReport) -> Option<String> {
    if opportunities.opportunities.is_empty() {
        return Some("No relevant opportunities identified.".into());
    }

    Some(format!(
        "{} opportunities with approximately ~{:.0} estimated additional clicks.",
        opportunities.opportunities.len(),
        opportunities.total_estimated_clicks
    ))
}

fn interpret_growth(growth: &GrowthReport) -> Option<String> {
    let growing = growth.top_growing_pages.len();
    let declining = growth.top_declining_pages.len();

    if growing == 0 && declining == 0 {
        return Some("No significant changes compared to the previous period.".into());
    }

    Some(format!(
        "{} pages growing, {} pages losing sessions compared to the previous period.",
        growing, declining
    ))
}

fn interpret_ai(ai: &AiTrafficReport) -> Option<String> {
    if ai.ai_sessions == 0 {
        return Some("No measurable AI referral traffic in the period.".into());
    }

    Some(format!(
        "{} sessions ({:.1}%) come from AI tools across {} sources.",
        ai.ai_sessions,
        ai.ai_share_pct,
        ai.ai_sources.len()
    ))
}

fn shorten(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max - 1])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::*;

    fn mock_overview(organic: i64, total: i64, engagement: f64, trend: Option<PeriodDelta>) -> SiteOverviewReport {
        SiteOverviewReport {
            property_name: "test".into(),
            date_range: "test".into(),
            traffic: TrafficSourceBreakdown {
                organic_sessions: organic,
                direct_sessions: total - organic,
                referral_sessions: 0,
                other_sessions: 0,
                total_sessions: total,
            },
            engagement_rate: engagement,
            search: SearchPerformanceBreakdown {
                clicks: 100.0,
                impressions: 1000.0,
                ctr: 0.1,
                average_position: 8.0,
                top_queries: vec![],
            },
            trend,
            top_sources: vec![],
            ai_sources: vec![],
            opportunities: vec![],
            ai_pages: vec![],
            insights: vec![],
        }
    }

    #[test]
    fn high_organic_produces_sentence() {
        let overview = mock_overview(800, 1000, 0.5, None);
        let input = NarrativeInput {
            overview: Some(&overview),
            top_pages: None,
            opportunities: None,
            growth: None,
            ai_traffic: None,
            clusters: None,
        };
        let summary = management_summary(&input);
        assert!(summary.iter().any(|s| s.contains("SEO foundation is strong")));
    }

    #[test]
    fn low_organic_produces_warning() {
        let overview = mock_overview(10, 200, 0.5, None);
        let input = NarrativeInput {
            overview: Some(&overview),
            top_pages: None,
            opportunities: None,
            growth: None,
            ai_traffic: None,
            clusters: None,
        };
        let summary = management_summary(&input);
        assert!(summary.iter().any(|s| s.contains("untapped")));
    }

    #[test]
    fn growing_trend_produces_sentence() {
        let trend = PeriodDelta {
            sessions_pct: 25.0,
            impressions_pct: 20.0,
            clicks_pct: 15.0,
            ctr_abs: 0.0,
            position_abs: 0.0,
        };
        let overview = mock_overview(500, 1000, 0.5, Some(trend));
        let input = NarrativeInput {
            overview: Some(&overview),
            top_pages: None,
            opportunities: None,
            growth: None,
            ai_traffic: None,
            clusters: None,
        };
        let summary = management_summary(&input);
        assert!(summary.iter().any(|s| s.contains("growing")));
    }

    #[test]
    fn declining_trend_produces_warning() {
        let trend = PeriodDelta {
            sessions_pct: -20.0,
            impressions_pct: -10.0,
            clicks_pct: -15.0,
            ctr_abs: 0.0,
            position_abs: 0.0,
        };
        let overview = mock_overview(500, 1000, 0.5, Some(trend));
        let input = NarrativeInput {
            overview: Some(&overview),
            top_pages: None,
            opportunities: None,
            growth: None,
            ai_traffic: None,
            clusters: None,
        };
        let summary = management_summary(&input);
        assert!(summary.iter().any(|s| s.contains("declined")));
    }

    #[test]
    fn dependency_woven_into_state_paragraph() {
        let overview = mock_overview(500, 1000, 0.5, Some(PeriodDelta {
            sessions_pct: 25.0, impressions_pct: 20.0, clicks_pct: 15.0,
            ctr_abs: 0.0, position_abs: 0.0,
        }));
        let pages = vec![
            PageSummary {
                url: "/a".into(), sessions: 400, organic_sessions: 300,
                direct_sessions: 100, engagement_rate: 0.5, bounce_rate: 0.5,
                avg_session_duration_secs: 60.0, new_user_share: 0.0, key_events: 0,
                scroll_events: 0, internal_link_clicks: 0, service_hint_clicks: 0,
                search: SearchPerformanceBreakdown::default(),
            },
            PageSummary {
                url: "/b".into(), sessions: 200, organic_sessions: 100,
                direct_sessions: 100, engagement_rate: 0.5, bounce_rate: 0.5,
                avg_session_duration_secs: 60.0, new_user_share: 0.0, key_events: 0,
                scroll_events: 0, internal_link_clicks: 0, service_hint_clicks: 0,
                search: SearchPerformanceBreakdown::default(),
            },
            PageSummary {
                url: "/c".into(), sessions: 100, organic_sessions: 50,
                direct_sessions: 50, engagement_rate: 0.5, bounce_rate: 0.5,
                avg_session_duration_secs: 60.0, new_user_share: 0.0, key_events: 0,
                scroll_events: 0, internal_link_clicks: 0, service_hint_clicks: 0,
                search: SearchPerformanceBreakdown::default(),
            },
            PageSummary {
                url: "/d".into(), sessions: 50, organic_sessions: 25,
                direct_sessions: 25, engagement_rate: 0.5, bounce_rate: 0.5,
                avg_session_duration_secs: 60.0, new_user_share: 0.0, key_events: 0,
                scroll_events: 0, internal_link_clicks: 0, service_hint_clicks: 0,
                search: SearchPerformanceBreakdown::default(),
            },
        ];
        let top_pages = TopPagesReport {
            property_name: "test".into(),
            date_range: "test".into(),
            pages,
            insights: vec![],
        };
        let input = NarrativeInput {
            overview: Some(&overview),
            top_pages: Some(&top_pages),
            opportunities: None,
            growth: None,
            ai_traffic: None,
            clusters: None,
        };
        let summary = management_summary(&input);
        // State paragraph should mention both growth and concentration
        assert!(summary.iter().any(|s| s.contains("growing") && s.contains("few pages")));
    }

    #[test]
    fn management_summary_returns_paragraphs() {
        let overview = mock_overview(800, 1000, 0.8, Some(PeriodDelta {
            sessions_pct: 25.0, impressions_pct: 20.0, clicks_pct: 15.0,
            ctr_abs: 0.0, position_abs: -3.0,
        }));
        let input = NarrativeInput {
            overview: Some(&overview),
            top_pages: None,
            opportunities: None,
            growth: None,
            ai_traffic: None,
            clusters: None,
        };
        let summary = management_summary(&input);
        assert!(!summary.is_empty());
        // Each entry should be a paragraph (multi-word, ends with period)
        for para in &summary {
            assert!(para.len() > 10, "Paragraph too short: {}", para);
        }
    }

    #[test]
    fn ai_share_above_threshold_in_forward_signal() {
        let overview = mock_overview(800, 1000, 0.5, None);
        let ai = AiTrafficReport {
            property_name: "test".into(),
            date_range: "test".into(),
            total_sessions: 1000,
            ai_sessions: 80,
            ai_share_pct: 8.0,
            prev_ai_sessions: 50,
            ai_trend_pct: 60.0,
            ai_engagement_rate: 0.6,
            overall_engagement_rate: 0.5,
            ai_sources: vec![],
            ai_pages: vec![],
            content_pattern: None,
            recommendations: vec![],
            insights: vec![],
        };
        let input = NarrativeInput {
            overview: Some(&overview),
            top_pages: None,
            opportunities: None,
            growth: None,
            ai_traffic: Some(&ai),
            clusters: None,
        };
        let summary = management_summary(&input);
        assert!(summary.iter().any(|s| s.contains("AI tools")));
    }
}
