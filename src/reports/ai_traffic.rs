use crate::config::AppConfig;
use crate::domain::{AiPageRow, AiTrafficReport, Insight, InsightCategory, InsightSeverity, SourceRow};
use crate::errors::Result;
use crate::google::analytics_data::{DateRange, ReportRequest, run_report};

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

    let date_label = format!("letzte {} Tage", days);

    // ── GA4: sessions by source ──────────────────────────────────────────────
    let source_req = ReportRequest {
        property_id: property_id.clone(),
        date_ranges: vec![DateRange::last_n_days(days)],
        dimensions: vec!["sessionSource".into()],
        metrics: vec!["sessions".into()],
        dimension_filter: None,
        limit: Some(200),
        order_by: Some(vec![serde_json::json!({
            "metric": { "metricName": "sessions" },
            "desc": true
        })]),
    };

    // ── GA4: AI traffic per page ─────────────────────────────────────────────
    let ai_values: Vec<serde_json::Value> = AI_DOMAINS.iter()
        .map(|d| serde_json::Value::String(d.to_string()))
        .collect();
    let ai_page_req = ReportRequest {
        property_id: property_id.clone(),
        date_ranges: vec![DateRange::last_n_days(days)],
        dimensions: vec!["pagePath".into()],
        metrics: vec!["sessions".into()],
        dimension_filter: Some(serde_json::json!({
            "filter": {
                "fieldName": "sessionSource",
                "inListFilter": { "values": ai_values }
            }
        })),
        limit: Some(50),
        order_by: Some(vec![serde_json::json!({
            "metric": { "metricName": "sessions" },
            "desc": true
        })]),
    };

    let (source_report, ai_page_report) = tokio::join!(
        run_report(access_token, source_req),
        run_report(access_token, ai_page_req),
    );
    let source_report = source_report?;
    let ai_page_report = ai_page_report?;

    // ── Parse source data ────────────────────────────────────────────────────
    let mut total_sessions = 0i64;
    let mut ai_sources: Vec<SourceRow> = Vec::new();

    for row in &source_report.rows {
        let src = row.dimension_values.first().cloned().unwrap_or_default();
        let sessions: i64 = row.metric_values.first().and_then(|v| v.parse().ok()).unwrap_or(0);
        total_sessions += sessions;

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

    // ── AI pages ─────────────────────────────────────────────────────────────
    let ai_total_page_sessions: i64 = ai_page_report.rows.iter()
        .filter_map(|r| r.metric_values.first()?.parse::<i64>().ok())
        .sum();

    let ai_pages: Vec<AiPageRow> = ai_page_report.rows.iter().filter_map(|row| {
        let url = row.dimension_values.first()?.clone();
        let sessions: i64 = row.metric_values.first()?.parse().ok()?;
        if sessions == 0 { return None; }
        let share = if ai_total_page_sessions > 0 {
            sessions as f64 / ai_total_page_sessions as f64
        } else { 0.0 };
        Some(AiPageRow { url, sessions, share_of_ai: share })
    }).collect();

    // ── Insights ─────────────────────────────────────────────────────────────
    let mut insights = Vec::new();

    if ai_sessions > 0 {
        insights.push(Insight {
            severity: InsightSeverity::Positive,
            category: InsightCategory::Traffic,
            headline: format!("{} Sessions von AI-Tools ({:.1}%)", ai_sessions, ai_share_pct),
            explanation: format!(
                "Deine Inhalte werden von AI-Tools wie {} referenziert.",
                ai_sources.iter().take(3).map(|s| s.source.as_str()).collect::<Vec<_>>().join(", ")
            ),
        });
    } else {
        insights.push(Insight {
            severity: InsightSeverity::Info,
            category: InsightCategory::Traffic,
            headline: "Kein AI-Referral-Traffic erkannt".into(),
            explanation: "Im Analysezeitraum wurden keine Sessions von bekannten AI-Tools erkannt.".into(),
        });
    }

    if ai_share_pct > 5.0 {
        insights.push(Insight {
            severity: InsightSeverity::Info,
            category: InsightCategory::Traffic,
            headline: "Ueberdurchschnittlicher AI-Traffic-Anteil".into(),
            explanation: format!(
                "{:.1}% des Traffics kommt von AI-Tools. Pruefen, ob die zitierten Inhalte aktuell und korrekt sind.",
                ai_share_pct
            ),
        });
    }

    if ai_pages.len() > 1 {
        let top_page = &ai_pages[0];
        insights.push(Insight {
            severity: InsightSeverity::Info,
            category: InsightCategory::Traffic,
            headline: format!("{} Seiten erhalten AI-Traffic", ai_pages.len()),
            explanation: format!(
                "Meistbesucht: {} ({} Sessions, {:.0}% des AI-Traffics)",
                top_page.url, top_page.sessions, top_page.share_of_ai * 100.0
            ),
        });
    }

    Ok(AiTrafficReport {
        property_name,
        date_range: date_label,
        total_sessions,
        ai_sessions,
        ai_share_pct,
        ai_sources,
        ai_pages,
        insights,
    })
}
