use serde::{Deserialize, Serialize};

// ─── Shared primitives ───────────────────────────────────────────────────────

/// Traffic broken down by source / medium
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrafficSourceBreakdown {
    pub organic_sessions: i64,
    pub direct_sessions: i64,
    pub referral_sessions: i64,
    pub other_sessions: i64,
    pub total_sessions: i64,
}

impl TrafficSourceBreakdown {
    pub fn organic_share(&self) -> f64 {
        if self.total_sessions == 0 {
            return 0.0;
        }
        self.organic_sessions as f64 / self.total_sessions as f64 * 100.0
    }

    pub fn direct_share(&self) -> f64 {
        if self.total_sessions == 0 {
            return 0.0;
        }
        self.direct_sessions as f64 / self.total_sessions as f64 * 100.0
    }
}

/// Search Console metrics for a page or site
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchPerformanceBreakdown {
    pub clicks: f64,
    pub impressions: f64,
    pub ctr: f64,
    pub average_position: f64,
    pub top_queries: Vec<QueryRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryRow {
    pub query: String,
    pub clicks: f64,
    pub impressions: f64,
    pub ctr: f64,
    pub position: f64,
}

// ─── Reports ─────────────────────────────────────────────────────────────────

/// Site-wide overview for a time period
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteOverviewReport {
    pub property_name: String,
    pub date_range: String,
    pub traffic: TrafficSourceBreakdown,
    pub engagement_rate: f64,
    pub search: SearchPerformanceBreakdown,
    pub insights: Vec<Insight>,
}

/// A ranked list of pages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopPagesReport {
    pub property_name: String,
    pub date_range: String,
    pub pages: Vec<PageSummary>,
    pub insights: Vec<Insight>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageSummary {
    pub url: String,
    pub sessions: i64,
    pub organic_sessions: i64,
    pub direct_sessions: i64,
    pub engagement_rate: f64,
    pub avg_session_duration_secs: f64,
    pub search: SearchPerformanceBreakdown,
}

/// Deep-dive for a single page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageDetailReport {
    pub url: String,
    pub property_name: String,
    pub date_range: String,
    pub traffic: TrafficSourceBreakdown,
    pub engagement_rate: f64,
    pub avg_session_duration_secs: f64,
    pub search: SearchPerformanceBreakdown,
    pub insights: Vec<Insight>,
    pub recommendations: Vec<Recommendation>,
}

/// Before / after comparison around a change date
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonReport {
    pub url: Option<String>,
    pub property_name: String,
    pub change_date: String,
    pub before_days: u32,
    pub after_days: u32,
    pub before: ComparisonPeriod,
    pub after: ComparisonPeriod,
    pub delta: ComparisonDelta,
    pub summary: String,
    pub insights: Vec<Insight>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ComparisonPeriod {
    pub start_date: String,
    pub end_date: String,
    pub sessions: i64,
    pub organic_sessions: i64,
    pub engagement_rate: f64,
    pub search: SearchPerformanceBreakdown,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ComparisonDelta {
    pub sessions_abs: i64,
    pub sessions_pct: f64,
    pub organic_sessions_abs: i64,
    pub organic_sessions_pct: f64,
    pub engagement_rate_abs: f64,
    pub clicks_abs: f64,
    pub clicks_pct: f64,
    pub impressions_abs: f64,
    pub impressions_pct: f64,
    pub ctr_abs: f64,
    pub position_abs: f64,
}

// ─── Insights & Recommendations ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum InsightSeverity {
    Info,
    Warning,
    Critical,
    Positive,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InsightCategory {
    Traffic,
    Search,
    Engagement,
    Conversion,
    Trend,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Insight {
    pub severity: InsightSeverity,
    pub category: InsightCategory,
    pub headline: String,
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    pub priority: u8, // 1 = highest
    pub headline: String,
    pub action: String,
}
