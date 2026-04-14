use crate::config::AppConfig;
use crate::domain::{DeviceDetail, DevicesReport, Insight, InsightCategory, InsightSeverity};
use crate::errors::Result;
use crate::google::analytics_data::{DateRange, ReportRequest, run_report};

pub async fn build(config: &AppConfig, access_token: &str, days: u32) -> Result<DevicesReport> {
    let property_id = config.require_ga4_property()?.to_string();
    let property_name = config
        .properties
        .ga4_property_name
        .clone()
        .unwrap_or_else(|| property_id.clone());

    let date_label = format!("letzte {} Tage", days);

    let req = ReportRequest {
        property_id,
        date_ranges: vec![DateRange::last_n_days(days)],
        dimensions: vec!["deviceCategory".into()],
        metrics: vec![
            "sessions".into(),
            "engagementRate".into(),
            "averageSessionDuration".into(),
        ],
        dimension_filter: None,
        limit: Some(10),
        order_by: Some(vec![serde_json::json!({
            "metric": { "metricName": "sessions" },
            "desc": true
        })]),
    };

    let report = run_report(access_token, req).await?;

    let mut total_sessions = 0i64;
    let mut devices: Vec<DeviceDetail> = Vec::new();

    for row in &report.rows {
        let device = row.dimension_values.first().cloned().unwrap_or_default();
        let sessions: i64 = row.metric_values.first().and_then(|v| v.parse().ok()).unwrap_or(0);
        let eng: f64 = row.metric_values.get(1).and_then(|v| v.parse().ok()).unwrap_or(0.0);
        let dur: f64 = row.metric_values.get(2).and_then(|v| v.parse().ok()).unwrap_or(0.0);

        total_sessions += sessions;
        devices.push(DeviceDetail {
            device,
            sessions,
            share_pct: 0.0,
            engagement_rate: eng,
            avg_session_duration_secs: dur,
        });
    }

    for d in &mut devices {
        d.share_pct = if total_sessions > 0 {
            d.sessions as f64 / total_sessions as f64 * 100.0
        } else {
            0.0
        };
    }

    // Insights
    let mut insights = Vec::new();

    let mobile = devices.iter().find(|d| d.device == "mobile");
    let desktop = devices.iter().find(|d| d.device == "desktop");

    if let Some(m) = mobile {
        if m.share_pct > 60.0 {
            insights.push(Insight {
                severity: InsightSeverity::Info,
                category: InsightCategory::Traffic,
                headline: format!("Mobile-dominiert: {:.0}% des Traffics", m.share_pct),
                explanation: "Ueber 60% der Sessions kommen von Mobilgeraeten. Mobile UX und Ladezeit priorisieren.".into(),
            });
        }

        if let Some(d) = desktop {
            if m.engagement_rate < d.engagement_rate * 0.7 && m.sessions >= 20 {
                insights.push(Insight {
                    severity: InsightSeverity::Warning,
                    category: InsightCategory::Engagement,
                    headline: "Mobile Engagement deutlich schwaecher als Desktop".into(),
                    explanation: format!(
                        "Mobile: {:.0}% vs Desktop: {:.0}% Engagement Rate. Mobile UX pruefen.",
                        m.engagement_rate * 100.0, d.engagement_rate * 100.0
                    ),
                });
            }
        }
    }

    let tablet = devices.iter().find(|d| d.device == "tablet");
    if let Some(t) = tablet {
        if t.sessions >= 20 && t.engagement_rate < 0.3 {
            insights.push(Insight {
                severity: InsightSeverity::Info,
                category: InsightCategory::Engagement,
                headline: "Tablet-Engagement niedrig".into(),
                explanation: format!(
                    "Nur {:.0}% Engagement Rate auf Tablets ({} Sessions). Responsive Layout pruefen.",
                    t.engagement_rate * 100.0, t.sessions
                ),
            });
        }
    }

    Ok(DevicesReport {
        property_name,
        date_range: date_label,
        devices,
        total_sessions,
        insights,
    })
}
