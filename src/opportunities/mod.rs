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

// ── Expected CTR curve ────────────────────────────────────────────────────────
const EXPECTED_CTR: &[(f64, f64)] = &[
    (1.0, 0.28), (2.0, 0.15), (3.0, 0.11), (4.0, 0.08),
    (5.0, 0.07), (6.0, 0.06), (7.0, 0.05), (8.0, 0.04),
    (9.0, 0.03), (10.0, 0.03), (15.0, 0.015), (20.0, 0.01),
];

pub fn expected_ctr(position: f64) -> f64 {
    if position <= 0.0 { return 0.28; }
    if position >= 20.0 { return 0.01; }
    for window in EXPECTED_CTR.windows(2) {
        let (p1, c1) = window[0];
        let (p2, c2) = window[1];
        if position >= p1 && position <= p2 {
            let t = (position - p1) / (p2 - p1);
            return c1 + t * (c2 - c1);
        }
    }
    0.01
}

// ── Raw opportunity generation (before grouping) ──────────────────────────────

fn raw_opportunities(queries: &[QueryRow], pages: &[PageSummary]) -> Vec<Opportunity> {
    let mut ops: Vec<Opportunity> = Vec::new();

    for q in queries {
        if q.impressions < 30.0 { continue; }
        let exp_ctr = expected_ctr(q.position);

        // CTR Fix
        if q.position >= 1.0 && q.position <= 10.0 && q.ctr < exp_ctr * 0.7 {
            let impact = q.impressions * (exp_ctr - q.ctr);
            let conf   = (q.impressions + 1.0).log10();
            ops.push(Opportunity {
                opportunity_type: OpportunityType::CtrFix,
                url: String::new(),
                keyword: Some(q.query.clone()),
                estimated_clicks: impact,
                current_clicks: q.clicks,
                score: impact * conf / 1.0,
                action: format!("Title & Description fuer \"{}\" ueberarbeiten", q.query),
                context: format!(
                    "Pos {:.1} - CTR {:.1}% (erwartet {:.1}%) - {} Impressionen",
                    q.position, q.ctr * 100.0, exp_ctr * 100.0, q.impressions as i64
                ),
                type_labels: vec![OpportunityType::CtrFix.label().into()],
            });
        }

        // Ranking Push (pos 5-15)
        if q.position >= 5.0 && q.position <= 15.0 && q.impressions >= 50.0 {
            let impact = q.impressions * (expected_ctr(3.0) - q.ctr).max(0.0);
            let conf   = (q.impressions + 1.0).log10() * 0.7;
            ops.push(Opportunity {
                opportunity_type: OpportunityType::RankingPush,
                url: String::new(),
                keyword: Some(q.query.clone()),
                estimated_clicks: impact,
                current_clicks: q.clicks,
                score: impact * conf / 2.0,
                action: format!("Content fuer \"{}\" ausbauen, interne Verlinkung staerken", q.query),
                context: format!(
                    "Pos {:.1} - {} Impressionen - {} Klicks aktuell",
                    q.position, q.impressions as i64, q.clicks as i64
                ),
                type_labels: vec![OpportunityType::RankingPush.label().into()],
            });
        }

        // Content Expansion
        if q.impressions >= 100.0 && q.clicks < 5.0 {
            let impact = q.impressions * expected_ctr(q.position.min(10.0));
            let conf   = (q.impressions + 1.0).log10() * 0.5;
            ops.push(Opportunity {
                opportunity_type: OpportunityType::ContentExpansion,
                url: String::new(),
                keyword: Some(q.query.clone()),
                estimated_clicks: impact,
                current_clicks: q.clicks,
                score: impact * conf / 3.0,
                action: format!("Content fuer \"{}\" stark ausbauen oder neue Seite erstellen", q.query),
                context: format!(
                    "{} Impressionen, nur {:.0} Klicks - Suchintention nicht erfuellt",
                    q.impressions as i64, q.clicks
                ),
                type_labels: vec![OpportunityType::ContentExpansion.label().into()],
            });
        }
    }

    // Internal Linking (page-level)
    for page in pages {
        let avg_pos = page.search.average_position;
        if avg_pos > 0.0 && avg_pos < 10.0 && page.sessions < 10 && page.search.impressions > 50.0 {
            let exp  = expected_ctr(avg_pos) * page.search.impressions;
            let gap  = (exp - page.sessions as f64).max(0.0);
            let conf = (page.search.impressions + 1.0).log10() * 0.6;
            ops.push(Opportunity {
                opportunity_type: OpportunityType::InternalLinking,
                url: page.url.clone(),
                keyword: None,
                estimated_clicks: gap,
                current_clicks: page.sessions as f64,
                score: gap * conf / 1.0,
                action: "Interne Links zu dieser Seite erhoehen".into(),
                context: format!(
                    "Pos {:.1} - {} Impressionen - nur {} Sessions",
                    avg_pos, page.search.impressions as i64, page.sessions
                ),
                type_labels: vec![OpportunityType::InternalLinking.label().into()],
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
) -> Vec<Opportunity> {
    let raw = raw_opportunities(queries, pages);
    group_by_keyword(raw)
}

pub fn opportunities_from_overview(
    queries: &[QueryRow],
    pages: &[PageSummary],
    days: u32,
) -> Vec<Opportunity> {
    generate_opportunities(queries, pages, days)
}
