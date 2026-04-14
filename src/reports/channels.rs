use crate::config::AppConfig;
use crate::domain::{
    ChannelDetail, ChannelsReport, Insight, InsightCategory, InsightSeverity,
};
use crate::errors::Result;
use crate::google::analytics_data::{DateRange, ReportRequest, run_report};

pub async fn build(config: &AppConfig, access_token: &str, days: u32) -> Result<ChannelsReport> {
    let property_id = config.require_ga4_property()?.to_string();
    let property_name = config
        .properties
        .ga4_property_name
        .clone()
        .unwrap_or_else(|| property_id.clone());

    let date_label = format!("last {} days", days);

    let req = ReportRequest {
        property_id,
        date_ranges: vec![DateRange::last_n_days(days)],
        dimensions: vec!["sessionDefaultChannelGroup".into()],
        metrics: vec![
            "sessions".into(),
            "engagementRate".into(),
            "averageSessionDuration".into(),
        ],
        dimension_filter: None,
        limit: Some(50),
        order_by: Some(vec![serde_json::json!({
            "metric": { "metricName": "sessions" },
            "desc": true
        })]),
    };

    let report = run_report(access_token, req).await?;

    let mut total_sessions = 0i64;
    let mut channels: Vec<ChannelDetail> = Vec::new();

    for row in &report.rows {
        let channel = row.dimension_values.first().cloned().unwrap_or_default();
        let sessions: i64 = row.metric_values.first().and_then(|v| v.parse().ok()).unwrap_or(0);
        let eng: f64 = row.metric_values.get(1).and_then(|v| v.parse().ok()).unwrap_or(0.0);
        let dur: f64 = row.metric_values.get(2).and_then(|v| v.parse().ok()).unwrap_or(0.0);

        total_sessions += sessions;
        channels.push(ChannelDetail {
            channel,
            sessions,
            share_pct: 0.0, // filled below
            engagement_rate: eng,
            avg_session_duration_secs: dur,
        });
    }

    // Fill share
    for ch in &mut channels {
        ch.share_pct = if total_sessions > 0 {
            ch.sessions as f64 / total_sessions as f64 * 100.0
        } else {
            0.0
        };
    }

    // Insights
    let mut insights = Vec::new();

    // Dominant channel
    if let Some(top) = channels.first() {
        if top.share_pct > 60.0 {
            insights.push(Insight {
                severity: InsightSeverity::Info,
                category: InsightCategory::Traffic,
                headline: format!("Dominant channel: {} ({:.0}%)", top.channel, top.share_pct),
                explanation: "A single channel accounts for over 60% of traffic. Diversification can reduce risk.".into(),
            });
        }
    }

    // Low-engagement channels
    let low_eng: Vec<&ChannelDetail> = channels.iter()
        .filter(|c| c.sessions >= 20 && c.engagement_rate < 0.3)
        .collect();
    if !low_eng.is_empty() {
        let names: Vec<&str> = low_eng.iter().map(|c| c.channel.as_str()).collect();
        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Engagement,
            headline: format!("{} channels with weak engagement", low_eng.len()),
            explanation: format!(
                "Below 30% engagement rate: {}. Check landing page quality for these channels.",
                names.join(", ")
            ),
        });
    }

    // High-engagement channels
    let high_eng: Vec<&ChannelDetail> = channels.iter()
        .filter(|c| c.sessions >= 20 && c.engagement_rate > 0.7)
        .collect();
    if !high_eng.is_empty() {
        let names: Vec<&str> = high_eng.iter().map(|c| c.channel.as_str()).collect();
        insights.push(Insight {
            severity: InsightSeverity::Positive,
            category: InsightCategory::Engagement,
            headline: format!("{} channels with strong engagement", high_eng.len()),
            explanation: format!("Over 70% engagement rate: {}.", names.join(", ")),
        });
    }

    // Organic share
    let organic = channels.iter().find(|c| c.channel == "Organic Search");
    if let Some(org) = organic {
        if org.share_pct < 15.0 && total_sessions > 100 {
            insights.push(Insight {
                severity: InsightSeverity::Warning,
                category: InsightCategory::Traffic,
                headline: format!("Organic share only {:.0}%", org.share_pct),
                explanation: "Low organic traffic — SEO measures can strengthen this channel.".into(),
            });
        }
    }

    Ok(ChannelsReport {
        property_name,
        date_range: date_label,
        channels,
        total_sessions,
        insights,
    })
}
