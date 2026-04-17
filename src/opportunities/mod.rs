//! Opportunity Engine
//!
//! Score = Impact x Confidence / Effort
//! Impact   = estimated additional clicks
//! Confidence = log10(impressions + 1)
//! Effort   = 1 (quick) / 2 (medium) / 3 (involved)
//!
//! After scoring, opportunities are grouped by keyword so each keyword
//! produces exactly one combined entry in the output.

use std::collections::HashMap;
use crate::domain::{Opportunity, OpportunityType, PageSummary, QueryRow};
use crate::intent::Intent;

// ── Expected CTR curves ─────────────────────────────────────────────────────
//
// Base curve (non-brand, desktop, informational):
const BASE_CTR: &[(f64, f64)] = &[
    (1.0, 0.28), (2.0, 0.15), (3.0, 0.11), (4.0, 0.08),
    (5.0, 0.07), (6.0, 0.06), (7.0, 0.05), (8.0, 0.04),
    (9.0, 0.03), (10.0, 0.03), (15.0, 0.015), (20.0, 0.01),
];

// Segment multipliers applied on top of base curve
const BRAND_MULTIPLIER: f64 = 2.0;
const COMMERCIAL_MULTIPLIER: f64 = 0.8; // more SERP features / ads
const MOBILE_MULTIPLIER: f64 = 0.85;
const DESKTOP_MULTIPLIER: f64 = 1.1;

fn base_ctr(position: f64) -> f64 {
    if position <= 0.0 { return 0.28; }
    if position >= 20.0 { return 0.01; }
    for window in BASE_CTR.windows(2) {
        let (p1, c1) = window[0];
        let (p2, c2) = window[1];
        if position >= p1 && position <= p2 {
            let t = (position - p1) / (p2 - p1);
            return c1 + t * (c2 - c1);
        }
    }
    0.01
}

/// CTR expectation for a given position, unmodified (backwards-compatible).
pub fn expected_ctr(position: f64) -> f64 {
    base_ctr(position)
}

/// Segment-aware CTR expectation.
pub fn expected_ctr_segmented(
    position: f64,
    is_brand: bool,
    intent: Option<Intent>,
    device: Option<&str>,
) -> f64 {
    let mut ctr = base_ctr(position);

    // Brand queries have much higher CTR
    if is_brand {
        ctr *= BRAND_MULTIPLIER;
    } else if let Some(Intent::Commercial) = intent {
        // Commercial queries have lower organic CTR (ads, SERP features)
        ctr *= COMMERCIAL_MULTIPLIER;
    }

    // Device adjustment
    match device {
        Some("MOBILE" | "mobile") => ctr *= MOBILE_MULTIPLIER,
        Some("DESKTOP" | "desktop") => ctr *= DESKTOP_MULTIPLIER,
        _ => {}
    }

    // Cap at 1.0
    ctr.min(1.0)
}

// ── Raw opportunity generation (before grouping) ──────────────────────────────

fn raw_opportunities(queries: &[QueryRow], pages: &[PageSummary], brand_terms: &[String]) -> Vec<Opportunity> {
    let mut ops: Vec<Opportunity> = Vec::new();

    for q in queries {
        if q.impressions < 30.0 { continue; }
        let is_brand = crate::helpers::is_brand_query(&q.query, brand_terms);
        let exp_ctr = expected_ctr_segmented(q.position, is_brand, q.intent, None);

        // ── Snippet Problem (Position 1-10, CTR far below expected) ─────
        if q.position >= 1.0 && q.position <= 10.0 && q.ctr < exp_ctr * 0.7 {
            let impact = q.impressions * (exp_ctr - q.ctr);
            let conf   = (q.impressions + 1.0).log10();
            let otype = OpportunityType::SnippetProblem;
            let ctr_gap = exp_ctr * 100.0 - q.ctr * 100.0;

            let interpretation = format!(
                "\"{}\" ranks at position {:.1} — the topic is clearly relevant. \
                 However, only {:.1}% of users click through, while {:.1}% would be expected \
                 at this position (gap: {:.1} percentage points). This strong gap indicates \
                 the search snippet (title + meta description) does not match what users \
                 expect when searching for this term.",
                q.query, q.position, q.ctr * 100.0, exp_ctr * 100.0, ctr_gap
            );

            let specific_actions = vec![
                format!(
                    "Rewrite the meta title to address the user's specific question — \
                     not \"What is {}\" but \"How to use {} effectively\"",
                    q.query, q.query
                ),
                "Add a compelling meta description that promises concrete value \
                 (examples, steps, comparisons) rather than generic information"
                    .into(),
                "Lead with the most actionable content in the first paragraph — \
                 search engines often pull this as the snippet"
                    .into(),
            ];

            ops.push(Opportunity {
                opportunity_type: otype.clone(),
                url: String::new(),
                keyword: Some(q.query.clone()),
                estimated_clicks: impact,
                current_clicks: q.clicks,
                score: impact * conf / otype.effort() as f64,
                action: format!(
                    "Rewrite snippet for \"{}\" — align title and description with search intent",
                    q.query
                ),
                context: format!(
                    "Pos {:.1} | CTR {:.1}% (expected {:.1}%) | {} impressions",
                    q.position, q.ctr * 100.0, exp_ctr * 100.0, q.impressions as i64
                ),
                type_labels: vec![otype.label().into()],
                interpretation,
                specific_actions,
            });
        }

        // ── Ranking Problem (Position 5-15, enough impressions, not high enough) ─
        if q.position >= 5.0 && q.position <= 15.0 && q.impressions >= 50.0 {
            let target_ctr = expected_ctr_segmented(3.0, is_brand, q.intent, None);
            let impact = q.impressions * (target_ctr - q.ctr).max(0.0);
            let conf   = (q.impressions + 1.0).log10() * 0.7;
            let otype = OpportunityType::RankingProblem;

            let interpretation = format!(
                "\"{}\" has {} impressions at position {:.1} — the topic generates search demand, \
                 but the page does not rank high enough to capture significant traffic. \
                 Moving from position {:.0} to the top 3 would multiply click volume.",
                q.query, q.impressions as i64, q.position, q.position
            );

            let specific_actions = vec![
                format!(
                    "Deepen the content for \"{}\" — cover subtopics, add examples, \
                     and address related questions",
                    q.query
                ),
                "Strengthen internal linking: add links from topically related, \
                 high-authority pages on your site"
                    .into(),
                "Review competitor pages ranking in the top 3 — identify what they \
                 cover that your page does not"
                    .into(),
            ];

            ops.push(Opportunity {
                opportunity_type: otype.clone(),
                url: String::new(),
                keyword: Some(q.query.clone()),
                estimated_clicks: impact,
                current_clicks: q.clicks,
                score: impact * conf / otype.effort() as f64,
                action: format!(
                    "Strengthen content and authority for \"{}\" — on-page depth + internal links",
                    q.query
                ),
                context: format!(
                    "Pos {:.1} | {} impressions | {} clicks",
                    q.position, q.impressions as i64, q.clicks as i64
                ),
                type_labels: vec![otype.label().into()],
                interpretation,
                specific_actions,
            });
        }

        // ── Expansion Potential (high impressions, very few clicks) ──────
        if q.impressions >= 100.0 && q.clicks < 5.0 {
            let impact = q.impressions * expected_ctr_segmented(q.position.min(10.0), is_brand, q.intent, None);
            let conf   = (q.impressions + 1.0).log10() * 0.5;
            let otype = OpportunityType::ExpansionPotential;

            let interpretation = format!(
                "\"{}\" generates {} impressions but only {:.0} clicks — your site appears \
                 in search results for this term but lacks a strong, dedicated page. \
                 This is a content gap: users are searching, but your content doesn't \
                 capture the demand.",
                q.query, q.impressions as i64, q.clicks
            );

            let specific_actions = vec![
                format!(
                    "Create a dedicated page or content hub for \"{}\" that directly \
                     addresses the search intent",
                    q.query
                ),
                "Structure the new content around specific user questions — use clear \
                 headings, concrete examples, and a direct answer in the first paragraph"
                    .into(),
                "Link the new page from existing related content to establish topical \
                 authority"
                    .into(),
            ];

            ops.push(Opportunity {
                opportunity_type: otype.clone(),
                url: String::new(),
                keyword: Some(q.query.clone()),
                estimated_clicks: impact,
                current_clicks: q.clicks,
                score: impact * conf / otype.effort() as f64,
                action: format!(
                    "Create dedicated content for \"{}\" to capture untapped search demand",
                    q.query
                ),
                context: format!(
                    "{} impressions | only {:.0} clicks | position {:.1}",
                    q.impressions as i64, q.clicks, q.position
                ),
                type_labels: vec![otype.label().into()],
                interpretation,
                specific_actions,
            });
        }
    }

    // ── Page-level opportunities ─────────────────────────────────────────────
    for page in pages {
        let avg_pos = page.search.average_position;

        // Distribution Problem: good position but very few sessions
        if avg_pos > 0.0 && avg_pos < 10.0 && page.sessions < 10 && page.search.impressions > 50.0 {
            let exp  = expected_ctr(avg_pos) * page.search.impressions;
            let gap  = (exp - page.sessions as f64).max(0.0);
            let conf = (page.search.impressions + 1.0).log10() * 0.6;
            let otype = OpportunityType::DistributionProblem;

            let interpretation = format!(
                "{} ranks at position {:.1} with {} impressions, but generates only {} sessions. \
                 The page is visible in search but users rarely reach it through internal navigation. \
                 This disconnect suggests weak internal linking — the page is orphaned or buried.",
                page.url, avg_pos, page.search.impressions as i64, page.sessions
            );

            let specific_actions = vec![
                format!(
                    "Add prominent internal links to {} from your 3-5 highest-traffic \
                     pages on related topics",
                    page.url
                ),
                "Consider adding this page to your main navigation or sidebar if topically important"
                    .into(),
                "Create a topical hub page that links to this and related content"
                    .into(),
            ];

            ops.push(Opportunity {
                opportunity_type: otype.clone(),
                url: page.url.clone(),
                keyword: None,
                estimated_clicks: gap,
                current_clicks: page.sessions as f64,
                score: gap * conf / otype.effort() as f64,
                action: format!("Build internal links to {} — connect it to related high-traffic pages", page.url),
                context: format!(
                    "Pos {:.1} | {} impressions | only {} sessions",
                    avg_pos, page.search.impressions as i64, page.sessions
                ),
                type_labels: vec![otype.label().into()],
                interpretation,
                specific_actions,
            });
        }

        // Intent Mismatch: high impressions + low CTR + low engagement
        if page.search.impressions > 100.0
            && page.search.ctr < 0.02
            && page.engagement_rate < 0.3
            && page.sessions > 5
        {
            let exp_ctr_val = expected_ctr(avg_pos);
            let impact = page.search.impressions * (exp_ctr_val - page.search.ctr).max(0.0);
            let conf = (page.search.impressions + 1.0).log10() * 0.8;
            let otype = OpportunityType::IntentMismatch;

            let interpretation = format!(
                "{} gets {} impressions but only {:.1}% CTR, and visitors who do land \
                 show low engagement ({:.0}%). This double signal — low CTR plus low engagement — \
                 indicates the page content does not match what users are actually searching for. \
                 The page ranks for the topic, but answers a different question than users have.",
                page.url, page.search.impressions as i64, page.search.ctr * 100.0,
                page.engagement_rate * 100.0
            );

            let specific_actions = vec![
                "Analyze the top search queries driving impressions to this page — \
                 what specific question are users asking?"
                    .into(),
                "Restructure the content to lead with a direct answer to the most \
                 common query, not a general introduction"
                    .into(),
                "Add concrete examples, step-by-step instructions, or comparison tables \
                 that match the likely search intent"
                    .into(),
                "Consider splitting the page if it tries to serve multiple intents at once"
                    .into(),
            ];

            ops.push(Opportunity {
                opportunity_type: otype.clone(),
                url: page.url.clone(),
                keyword: None,
                estimated_clicks: impact,
                current_clicks: page.search.clicks,
                score: impact * conf / otype.effort() as f64,
                action: "Realign page content with actual search intent — what users search for vs. what the page delivers".into(),
                context: format!(
                    "CTR {:.1}% | engagement {:.0}% | {} impressions",
                    page.search.ctr * 100.0, page.engagement_rate * 100.0,
                    page.search.impressions as i64
                ),
                type_labels: vec![otype.label().into()],
                interpretation,
                specific_actions,
            });
        }
    }

    ops
}

// ── Group by keyword ──────────────────────────────────────────────────────────

/// Merge all opportunities for the same keyword into a single entry.
/// The merged entry gets:
///   - highest score of the group
///   - sum of distinct impacts (capped to avoid double counting)
///   - combined type labels
///   - combined action (joined with " + ")
fn group_by_keyword(mut ops: Vec<Opportunity>) -> Vec<Opportunity> {
    // Sort by score desc first so the "best" opportunity ends up as base
    ops.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    let mut groups: HashMap<String, Vec<Opportunity>> = HashMap::new();

    for op in ops {
        let key = op.keyword.clone()
            .filter(|k| !k.is_empty())
            .unwrap_or_else(|| op.url.clone());
        groups.entry(key).or_default().push(op);
    }

    let mut merged: Vec<Opportunity> = groups
        .into_values()
        .map(|group| {
            // Base = highest scored entry
            let base = &group[0];

            // Collect unique type labels (preserve insertion order)
            let mut seen = std::collections::HashSet::new();
            let type_labels: Vec<String> = group.iter()
                .flat_map(|o| o.type_labels.iter().cloned())
                .filter(|l| seen.insert(l.clone()))
                .collect();

            // Merge actions (unique, join with " | ")
            let mut seen_actions = std::collections::HashSet::new();
            let combined_action: String = group.iter()
                .map(|o| o.action.as_str())
                .filter(|a| seen_actions.insert(*a))
                .collect::<Vec<_>>()
                .join(" | ");

            // Impact: use best single impact (different types measure different things,
            // summing would double-count the same clicks)
            let best_impact = group.iter()
                .map(|o| o.estimated_clicks)
                .fold(0.0f64, f64::max);

            // Merge interpretations (join unique)
            let mut seen_interp = std::collections::HashSet::new();
            let combined_interpretation: String = group.iter()
                .map(|o| o.interpretation.as_str())
                .filter(|i| !i.is_empty() && seen_interp.insert(*i))
                .collect::<Vec<_>>()
                .join(" ");

            // Merge specific actions (unique, preserve order)
            let mut seen_sa = std::collections::HashSet::new();
            let combined_specific_actions: Vec<String> = group.iter()
                .flat_map(|o| o.specific_actions.iter().cloned())
                .filter(|a| seen_sa.insert(a.clone()))
                .collect();

            Opportunity {
                opportunity_type: base.opportunity_type.clone(),
                url: base.url.clone(),
                keyword: base.keyword.clone(),
                estimated_clicks: best_impact,
                current_clicks: base.current_clicks,
                score: base.score,
                action: combined_action,
                context: base.context.clone(),
                type_labels,
                interpretation: combined_interpretation,
                specific_actions: combined_specific_actions,
            }
        })
        .collect();

    merged.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    merged.truncate(15);
    merged
}

// ── Public API ────────────────────────────────────────────────────────────────

pub fn generate_opportunities(
    queries: &[QueryRow],
    pages: &[PageSummary],
    _days: u32,
    brand_terms: &[String],
) -> Vec<Opportunity> {
    let raw = raw_opportunities(queries, pages, brand_terms);
    group_by_keyword(raw)
}

pub fn opportunities_from_overview(
    queries: &[QueryRow],
    pages: &[PageSummary],
    days: u32,
    brand_terms: &[String],
) -> Vec<Opportunity> {
    generate_opportunities(queries, pages, days, brand_terms)
}

// ── Action Plan builder ──────────────────────────────────────────────────────

use crate::domain::{Action, ActionPlan};

/// Build a three-tier action plan from scored opportunities.
pub fn build_action_plan(opportunities: &[Opportunity]) -> ActionPlan {
    let mut plan = ActionPlan::default();

    for op in opportunities {
        let effort = op.opportunity_type.effort();
        let impact = if op.estimated_clicks > 50.0 { "High" }
                     else if op.estimated_clicks > 15.0 { "Medium" }
                     else { "Low" };

        let target = op.keyword.as_deref()
            .unwrap_or_else(|| op.url.as_str());

        let reason = format!(
            "\"{}\" — {} (score {:.0}, +{:.0} clicks possible)",
            target,
            op.opportunity_type.diagnosis(),
            op.score,
            op.estimated_clicks
        );

        let action = Action {
            url: op.url.clone(),
            keyword: op.keyword.clone(),
            diagnosis: op.opportunity_type.label().to_string(),
            action: op.action.clone(),
            impact_label: impact.to_string(),
            effort_label: op.opportunity_type.effort_label().to_string(),
            reason,
        };

        // Monitoring: decay items or early signals (low impressions, position > 15)
        let is_monitoring = matches!(op.opportunity_type,
            OpportunityType::ContentDecay)
            || op.estimated_clicks < 5.0;

        if is_monitoring {
            plan.monitoring.push(action);
        } else if effort <= 1 {
            plan.quick_wins.push(action);
        } else {
            plan.strategic.push(action);
        }
    }

    // Cap each tier
    plan.quick_wins.truncate(5);
    plan.strategic.truncate(5);
    plan.monitoring.truncate(5);

    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::SearchPerformanceBreakdown;

    #[test]
    fn expected_ctr_position_1() {
        assert!((expected_ctr(1.0) - 0.28).abs() < 0.001);
    }

    #[test]
    fn expected_ctr_position_10() {
        assert!((expected_ctr(10.0) - 0.03).abs() < 0.001);
    }

    #[test]
    fn expected_ctr_position_20() {
        assert!((expected_ctr(20.0) - 0.01).abs() < 0.001);
    }

    #[test]
    fn expected_ctr_interpolates() {
        let ctr_5 = expected_ctr(5.0);
        assert!((ctr_5 - 0.07).abs() < 0.001);
        // Position 2.5 should be between 0.28 and 0.11
        let ctr_2_5 = expected_ctr(2.5);
        assert!(ctr_2_5 > 0.11 && ctr_2_5 < 0.15);
    }

    #[test]
    fn expected_ctr_extreme_values() {
        assert!((expected_ctr(0.0) - 0.28).abs() < 0.001);
        assert!((expected_ctr(-1.0) - 0.28).abs() < 0.001);
        assert!((expected_ctr(50.0) - 0.01).abs() < 0.001);
    }

    #[test]
    fn ctr_fix_opportunity_generated() {
        let queries = vec![QueryRow {
            query: "test keyword".into(),
            clicks: 5.0,
            impressions: 500.0,
            ctr: 0.01,     // 1% actual, position 3 expects 11%
            position: 3.0,
            intent: None,
        }];
        let ops = generate_opportunities(&queries, &[], 28, &[]);
        assert!(!ops.is_empty());
        let snippet = ops.iter().find(|o| o.opportunity_type == OpportunityType::SnippetProblem);
        assert!(snippet.is_some(), "Expected a SnippetProblem opportunity");
    }

    #[test]
    fn ranking_push_opportunity() {
        let queries = vec![QueryRow {
            query: "push keyword".into(),
            clicks: 10.0,
            impressions: 200.0,
            ctr: 0.05,
            position: 8.0,
            intent: None,
        }];
        let ops = generate_opportunities(&queries, &[], 28, &[]);
        let ranking = ops.iter().find(|o| o.opportunity_type == OpportunityType::RankingProblem);
        assert!(ranking.is_some(), "Expected a RankingProblem opportunity");
    }

    #[test]
    fn content_expansion_opportunity() {
        let queries = vec![QueryRow {
            query: "expand keyword".into(),
            clicks: 2.0,
            impressions: 300.0,
            ctr: 0.007,
            position: 12.0,
            intent: None,
        }];
        let ops = generate_opportunities(&queries, &[], 28, &[]);
        // After grouping, the merged entry should include ContentExpansion in its type labels
        assert!(!ops.is_empty());
        let has_expansion_label = ops.iter().any(|o|
            o.type_labels.iter().any(|l| l.contains("Expansion"))
        );
        assert!(has_expansion_label, "Expected Expansion Potential in type labels");
    }

    #[test]
    fn internal_linking_opportunity() {
        let pages = vec![PageSummary {
            url: "/test-page".into(),
            sessions: 5,
            organic_sessions: 3,
            direct_sessions: 2,
            engagement_rate: 0.5,
            bounce_rate: 0.5,
            avg_session_duration_secs: 30.0,
            new_user_share: 0.0,
            key_events: 0,
            scroll_events: 0,
            internal_link_clicks: 0,
            service_hint_clicks: 0,
            search: SearchPerformanceBreakdown {
                clicks: 10.0,
                impressions: 200.0,
                ctr: 0.05,
                average_position: 5.0,
                top_queries: vec![],
            },
        }];
        let ops = generate_opportunities(&[], &pages, 28, &[]);
        let dist = ops.iter().find(|o| o.opportunity_type == OpportunityType::DistributionProblem);
        assert!(dist.is_some(), "Expected a DistributionProblem opportunity");
    }

    #[test]
    fn low_impression_queries_skipped() {
        let queries = vec![QueryRow {
            query: "tiny".into(),
            clicks: 1.0,
            impressions: 10.0, // below 30 threshold
            ctr: 0.01,
            position: 3.0,
            intent: None,
        }];
        let ops = generate_opportunities(&queries, &[], 28, &[]);
        assert!(ops.is_empty(), "Low-impression queries should be skipped");
    }

    #[test]
    fn grouping_merges_same_keyword() {
        let queries = vec![QueryRow {
            query: "same keyword".into(),
            clicks: 5.0,
            impressions: 500.0,
            ctr: 0.01,
            position: 7.0, // qualifies for both CTR fix (pos 1-10) and ranking push (pos 5-15)
            intent: None,
        }];
        let ops = generate_opportunities(&queries, &[], 28, &[]);
        // Should be grouped into a single entry
        let keyword_ops: Vec<_> = ops.iter()
            .filter(|o| o.keyword.as_deref() == Some("same keyword"))
            .collect();
        assert_eq!(keyword_ops.len(), 1, "Same keyword should be grouped");
        assert!(keyword_ops[0].type_labels.len() >= 2, "Should have multiple type labels");
    }

    #[test]
    fn score_formula_correct() {
        // Score = impact * confidence / effort
        let queries = vec![QueryRow {
            query: "score test".into(),
            clicks: 10.0,
            impressions: 1000.0,
            ctr: 0.01,     // 1%, position 1 expects 28%
            position: 1.0,
            intent: None,
        }];
        let ops = raw_opportunities(&queries, &[], &[]);
        let snippet = ops.iter().find(|o| o.opportunity_type == OpportunityType::SnippetProblem).unwrap();
        let impact = 1000.0 * (0.28 - 0.01);
        let confidence = (1001.0_f64).log10();
        let expected_score = impact * confidence / 1.0; // effort = 1 for SnippetProblem
        assert!((snippet.score - expected_score).abs() < 0.1);
    }

    #[test]
    fn max_15_opportunities() {
        // Create many qualifying queries
        let queries: Vec<QueryRow> = (0..50).map(|i| QueryRow {
            query: format!("keyword_{}", i),
            clicks: 5.0,
            impressions: 500.0,
            ctr: 0.01,
            position: 3.0,
            intent: None,
        }).collect();
        let ops = generate_opportunities(&queries, &[], 28, &[]);
        assert!(ops.len() <= 15, "Should cap at 15 opportunities, got {}", ops.len());
    }

    #[test]
    fn segmented_ctr_brand_higher() {
        let base = expected_ctr(3.0);
        let brand = expected_ctr_segmented(3.0, true, None, None);
        assert!(brand > base, "Brand CTR should be higher than base");
    }

    #[test]
    fn segmented_ctr_commercial_lower() {
        let base = expected_ctr(3.0);
        let commercial = expected_ctr_segmented(3.0, false, Some(Intent::Commercial), None);
        assert!(commercial < base, "Commercial CTR should be lower than base");
    }

    #[test]
    fn segmented_ctr_mobile_lower() {
        let base = expected_ctr(3.0);
        let mobile = expected_ctr_segmented(3.0, false, None, Some("MOBILE"));
        assert!(mobile < base, "Mobile CTR should be lower than base");
    }

    #[test]
    fn segmented_ctr_capped_at_one() {
        let ctr = expected_ctr_segmented(1.0, true, None, Some("DESKTOP"));
        assert!(ctr <= 1.0, "CTR should never exceed 1.0");
    }
}
