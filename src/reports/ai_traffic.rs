use crate::config::AppConfig;
use crate::domain::{AiPageRow, AiTrafficReport, Insight, InsightCategory, InsightSeverity, SourceRow};
use crate::errors::Result;
use crate::google::analytics_data::{DateRange, ReportRequest, run_report};
use crate::helpers;

/// Known AI referrer domains
const AI_DOMAINS: &[&str] = &[
    "chatgpt.com",
    "chat.openai.com",
    "perplexity.ai",
    "claude.ai",
    "gemini.google.com",
    "bard.google.com",
    "copilot.microsoft.com",
    "you.com",
    "phind.com",
    "poe.com",
    "mistral.ai",
    "groq.com",
    "together.ai",
    "character.ai",
    "kagi.com",
    "brave.com",
];

fn is_ai_source(source: &str) -> bool {
    let s = source.to_lowercase();
    AI_DOMAINS.iter().any(|ai| s.contains(ai))
}

pub async fn build(config: &AppConfig, access_token: &str, days: u32) -> Result<AiTrafficReport> {
    let property_id = config.require_ga4_property()?.to_string();
    let property_name = config
        .properties
        .ga4_property_name
        .clone()
        .unwrap_or_else(|| property_id.clone());

    let date_label = format!("last {} days", days);

    // ── GA4: sessions by source (with engagement for comparison) ──────────────
    let source_req = ReportRequest {
        property_id: property_id.clone(),
        date_ranges: vec![DateRange::last_n_days(days)],
        dimensions: vec!["sessionSource".into()],
        metrics: vec!["sessions".into(), "engagementRate".into()],
        dimension_filter: None,
        limit: Some(200),
        order_by: Some(vec![serde_json::json!({
            "metric": { "metricName": "sessions" },
            "desc": true
        })]),
    };

    // ── GA4: AI traffic per page (with engagement) ────────────────────────────
    let ai_values: Vec<serde_json::Value> = AI_DOMAINS.iter()
        .map(|d| serde_json::Value::String(d.to_string()))
        .collect();
    let ai_page_req = ReportRequest {
        property_id: property_id.clone(),
        date_ranges: vec![DateRange::last_n_days(days)],
        dimensions: vec!["pagePath".into()],
        metrics: vec!["sessions".into(), "engagementRate".into()],
        dimension_filter: Some(serde_json::json!({
            "filter": {
                "fieldName": "sessionSource",
                "inListFilter": { "values": ai_values.clone() }
            }
        })),
        limit: Some(50),
        order_by: Some(vec![serde_json::json!({
            "metric": { "metricName": "sessions" },
            "desc": true
        })]),
    };

    // ── GA4: AI sessions in previous period (trend) ─────────────────────────
    let prev_ai_req = ReportRequest {
        property_id: property_id.clone(),
        date_ranges: vec![DateRange::prev_period(days)],
        dimensions: vec!["sessionSource".into()],
        metrics: vec!["sessions".into()],
        dimension_filter: Some(serde_json::json!({
            "filter": {
                "fieldName": "sessionSource",
                "inListFilter": { "values": ai_values }
            }
        })),
        limit: Some(50),
        order_by: None,
    };

    let (source_report, ai_page_report, prev_ai_report) = tokio::join!(
        run_report(access_token, source_req),
        run_report(access_token, ai_page_req),
        run_report(access_token, prev_ai_req),
    );
    let source_report = source_report?;
    let ai_page_report = ai_page_report?;
    let prev_ai_report = prev_ai_report?;

    // ── Parse source data ────────────────────────────────────────────────────
    let mut total_sessions = 0i64;
    let mut ai_sources: Vec<SourceRow> = Vec::new();
    let mut overall_engagement_sum = 0.0f64;
    let mut overall_session_count = 0i64;

    for row in &source_report.rows {
        let src = row.dimension_values.first().cloned().unwrap_or_default();
        let sessions: i64 = row.metric_values.first().and_then(|v| v.parse().ok()).unwrap_or(0);
        let eng: f64 = row.metric_values.get(1).and_then(|v| v.parse().ok()).unwrap_or(0.0);
        total_sessions += sessions;
        overall_session_count += sessions;
        overall_engagement_sum += eng * sessions as f64;

        if is_ai_source(&src) && sessions > 0 {
            ai_sources.push(SourceRow { source: src, sessions });
        }
    }

    ai_sources.sort_by(|a, b| b.sessions.cmp(&a.sessions));
    let ai_sessions: i64 = ai_sources.iter().map(|s| s.sessions).sum();
    let ai_share_pct = if total_sessions > 0 {
        ai_sessions as f64 / total_sessions as f64 * 100.0
    } else {
        0.0
    };

    // ── Previous period AI sessions ─────────────────────────────────────────
    let prev_ai_sessions: i64 = prev_ai_report
        .rows
        .iter()
        .filter_map(|r| r.metric_values.first()?.parse::<i64>().ok())
        .sum();
    let ai_trend_pct = helpers::pct_change(prev_ai_sessions as f64, ai_sessions as f64);

    // ── AI pages with engagement ────────────────────────────────────────────
    let ai_total_page_sessions: i64 = ai_page_report.rows.iter()
        .filter_map(|r| r.metric_values.first()?.parse::<i64>().ok())
        .sum();

    let mut ai_engagement_sum = 0.0f64;
    let mut ai_engagement_count = 0i64;

    let ai_pages: Vec<AiPageRow> = ai_page_report.rows.iter().filter_map(|row| {
        let url = row.dimension_values.first()?.clone();
        let sessions: i64 = row.metric_values.first()?.parse().ok()?;
        let eng: f64 = row.metric_values.get(1).and_then(|v| v.parse().ok()).unwrap_or(0.0);
        if sessions == 0 { return None; }
        ai_engagement_sum += eng * sessions as f64;
        ai_engagement_count += sessions;
        let share = if ai_total_page_sessions > 0 {
            sessions as f64 / ai_total_page_sessions as f64
        } else { 0.0 };
        Some(AiPageRow { url, sessions, share_of_ai: share })
    }).collect();

    let ai_engagement_rate = if ai_engagement_count > 0 {
        ai_engagement_sum / ai_engagement_count as f64
    } else {
        0.0
    };

    // Overall engagement from source report (approximate)
    let overall_engagement_rate = if overall_session_count > 0 {
        overall_engagement_sum / overall_session_count as f64
    } else {
        0.0
    };

    // ── Insights ─────────────────────────────────────────────────────────────
    let mut insights = Vec::new();

    if ai_sessions > 0 {
        insights.push(Insight {
            severity: InsightSeverity::Positive,
            category: InsightCategory::Traffic,
            headline: format!("{} sessions from AI tools ({:.1}%)", ai_sessions, ai_share_pct),
            explanation: format!(
                "Your content is being referenced by AI tools such as {}.",
                ai_sources.iter().take(3).map(|s| s.source.as_str()).collect::<Vec<_>>().join(", ")
            ),
        });
    } else {
        insights.push(Insight {
            severity: InsightSeverity::Info,
            category: InsightCategory::Traffic,
            headline: "No AI referral traffic detected".into(),
            explanation: "No sessions from known AI tools were detected during the analysis period.".into(),
        });
    }

    if ai_share_pct > 5.0 {
        insights.push(Insight {
            severity: InsightSeverity::Info,
            category: InsightCategory::Traffic,
            headline: "Above-average AI traffic share".into(),
            explanation: format!(
                "{:.1}% of traffic comes from AI tools. Check whether the referenced content is current and accurate.",
                ai_share_pct
            ),
        });
    }

    if ai_pages.len() > 1 {
        let top_page = &ai_pages[0];
        insights.push(Insight {
            severity: InsightSeverity::Info,
            category: InsightCategory::Traffic,
            headline: format!("{} pages receive AI traffic", ai_pages.len()),
            explanation: format!(
                "Most visited: {} ({} sessions, {:.0}% of AI traffic)",
                top_page.url, top_page.sessions, top_page.share_of_ai * 100.0
            ),
        });
    }

    // Trend insight
    if prev_ai_sessions > 0 && ai_sessions > 0 {
        if ai_trend_pct > 20.0 {
            insights.push(Insight {
                severity: InsightSeverity::Positive,
                category: InsightCategory::Trend,
                headline: format!("AI traffic growing by {:.0}% compared to previous period", ai_trend_pct),
                explanation: format!(
                    "{} -> {} sessions from AI tools",
                    prev_ai_sessions, ai_sessions
                ),
            });
        } else if ai_trend_pct < -20.0 {
            insights.push(Insight {
                severity: InsightSeverity::Warning,
                category: InsightCategory::Trend,
                headline: format!("AI traffic declining by {:.0}% compared to previous period", ai_trend_pct.abs()),
                explanation: format!(
                    "{} -> {} sessions from AI tools",
                    prev_ai_sessions, ai_sessions
                ),
            });
        }
    }

    // Engagement comparison insight
    if ai_engagement_rate > 0.0 && overall_engagement_rate > 0.0 {
        let diff_pct = (ai_engagement_rate - overall_engagement_rate) / overall_engagement_rate * 100.0;
        if diff_pct > 10.0 {
            insights.push(Insight {
                severity: InsightSeverity::Positive,
                category: InsightCategory::Engagement,
                headline: format!(
                    "AI traffic has {:.0}% higher engagement rate",
                    diff_pct
                ),
                explanation: format!(
                    "AI: {:.0}% vs. overall: {:.0}% — AI visitors engage more strongly with the content.",
                    ai_engagement_rate * 100.0,
                    overall_engagement_rate * 100.0
                ),
            });
        } else if diff_pct < -10.0 {
            insights.push(Insight {
                severity: InsightSeverity::Warning,
                category: InsightCategory::Engagement,
                headline: format!(
                    "AI traffic has {:.0}% lower engagement rate",
                    diff_pct.abs()
                ),
                explanation: format!(
                    "AI: {:.0}% vs. overall: {:.0}% — AI-referred visitors leave the page faster.",
                    ai_engagement_rate * 100.0,
                    overall_engagement_rate * 100.0
                ),
            });
        }
    }

    // ── Pattern analysis: what do AI-referred pages have in common? ────────
    let (content_pattern, recommendations) = analyze_ai_patterns(
        &ai_pages,
        ai_engagement_rate,
        overall_engagement_rate,
        ai_share_pct,
        ai_trend_pct,
    );

    Ok(AiTrafficReport {
        property_name,
        date_range: date_label,
        total_sessions,
        ai_sessions,
        ai_share_pct,
        prev_ai_sessions,
        ai_trend_pct,
        ai_engagement_rate,
        overall_engagement_rate,
        ai_sources,
        ai_pages,
        content_pattern,
        recommendations,
        insights,
    })
}

/// Analyze what AI-referred pages have in common and derive actionable recommendations.
fn analyze_ai_patterns(
    ai_pages: &[AiPageRow],
    ai_engagement_rate: f64,
    overall_engagement_rate: f64,
    ai_share_pct: f64,
    ai_trend_pct: f64,
) -> (Option<String>, Vec<String>) {
    if ai_pages.is_empty() {
        return (None, vec![]);
    }

    let mut recs = Vec::new();

    // ── URL-structure analysis: what types of content attract AI traffic? ──
    let blog_count = ai_pages.iter().filter(|p| p.url.contains("/blog") || p.url.contains("/artikel")).count();
    let guide_count = ai_pages.iter().filter(|p| {
        p.url.contains("/guide") || p.url.contains("/tutorial") || p.url.contains("/docs")
            || p.url.contains("/anleitung") || p.url.contains("/how-to")
    }).count();
    let total = ai_pages.len();

    // Determine content pattern
    let pattern = if guide_count > 0 && guide_count as f64 / total as f64 > 0.3 {
        Some("AI tools predominantly reference structured, explanatory content (guides, tutorials, documentation). \
              These pages likely share clear headings, concrete information, and directly usable answers."
            .to_string())
    } else if blog_count > 0 && blog_count as f64 / total as f64 > 0.3 {
        Some("AI tools primarily reference blog articles with concrete, topical information. \
              These pages likely provide specific answers and structured explanations that AI can cite."
            .to_string())
    } else if total >= 3 {
        Some("AI referral traffic is distributed across multiple content types. \
              Pages that receive AI traffic likely share clear structure, factual density, \
              and directly answerable content."
            .to_string())
    } else {
        None
    };

    // ── Engagement comparison drives recommendations ─────────────────────
    if ai_engagement_rate > overall_engagement_rate * 1.1 {
        recs.push(
            "AI-referred visitors show higher engagement than average — the content matches \
             their expectations well. Double down on this content style for other topics."
                .into(),
        );
    } else if ai_engagement_rate > 0.0 && ai_engagement_rate < overall_engagement_rate * 0.9 {
        recs.push(
            "AI-referred visitors engage less than average — AI tools may be sending users \
             with slightly different expectations. Add concise summary sections at the top \
             of key pages to immediately address the likely question."
                .into(),
        );
    }

    // ── Growth-based recommendations ─────────────────────────────────────
    if ai_trend_pct > 20.0 {
        recs.push(
            "AI referral traffic is growing. Accelerate this by adding structured data, \
             clear FAQ sections, and quotable summary paragraphs to high-performing pages."
                .into(),
        );
    }

    if ai_share_pct > 3.0 {
        recs.push(
            "AI traffic is a meaningful channel. Ensure referenced content stays accurate \
             and up-to-date — outdated information cited by AI tools can damage credibility."
                .into(),
        );
    }

    // ── Concrete content recommendations based on top pages ──────────────
    if let Some(top) = ai_pages.first() {
        let url_short = if top.url.len() > 50 {
            format!("{}…", &top.url[..49])
        } else {
            top.url.clone()
        };
        recs.push(format!(
            "Your top AI-referred page ({}) serves as a template: create more content \
             with similar structure, depth, and concreteness for adjacent topics.",
            url_short
        ));
    }

    if ai_pages.len() <= 3 && total >= 1 {
        recs.push(
            "AI traffic is concentrated on few pages. Expand by adding structured, \
             answer-oriented content to more topics — especially those already ranking \
             well in search."
                .into(),
        );
    }

    (pattern, recs)
}
