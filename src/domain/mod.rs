use serde::{Deserialize, Serialize};
use crate::intent::Intent;

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
    #[serde(default)]
    pub intent: Option<Intent>,
}

// ─── Reports ─────────────────────────────────────────────────────────────────

/// Single traffic source (domain or medium)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceRow {
    pub source: String,
    pub sessions: i64,
}

/// Site-wide overview for a time period
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteOverviewReport {
    pub property_name: String,
    pub date_range: String,
    pub traffic: TrafficSourceBreakdown,
    pub engagement_rate: f64,
    pub search: SearchPerformanceBreakdown,
    /// Period-over-period trend (None if no prev data available)
    pub trend: Option<PeriodDelta>,
    /// Top traffic sources by session count (sessionSource dimension)
    pub top_sources: Vec<SourceRow>,
    /// Sources identified as AI tools
    pub ai_sources: Vec<SourceRow>,
    /// Prioritised opportunities derived from data
    pub opportunities: Vec<Opportunity>,
    /// Which pages receive AI-tool traffic
    pub ai_pages: Vec<AiPageRow>,
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
    pub bounce_rate: f64,
    pub avg_session_duration_secs: f64,
    pub new_user_share: f64,
    pub key_events: i64,
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
    pub bounce_rate: f64,
    pub avg_session_duration_secs: f64,
    pub new_user_share: f64,
    pub key_events: i64,
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

// ─── Trend ───────────────────────────────────────────────────────────────────

/// Period-over-period delta for the overview
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PeriodDelta {
    pub sessions_pct: f64,
    pub clicks_pct: f64,
    pub impressions_pct: f64,
    pub ctr_abs: f64,      // absolute pp change
    pub position_abs: f64, // negative = improved
}

// ─── Opportunities ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OpportunityType {
    // Legacy types (kept for serialization compatibility)
    CtrFix,
    RankingPush,
    ContentExpansion,
    ContentDecay,
    InternalLinking,
    // Diagnosis classes (concept phase 1.3)
    SnippetProblem,
    RankingProblem,
    IntentMismatch,
    ExpansionPotential,
    DistributionProblem,
}

impl OpportunityType {
    pub fn label(&self) -> &'static str {
        match self {
            Self::CtrFix | Self::SnippetProblem     => "Snippet Issue",
            Self::RankingPush | Self::RankingProblem => "Ranking Issue",
            Self::IntentMismatch                     => "Intent Mismatch",
            Self::ContentExpansion | Self::ExpansionPotential => "Expansion Potential",
            Self::ContentDecay                       => "Content Decay",
            Self::InternalLinking | Self::DistributionProblem => "Link Distribution",
        }
    }

    pub fn effort(&self) -> u8 {
        match self {
            Self::CtrFix | Self::SnippetProblem     => 1,
            Self::InternalLinking | Self::DistributionProblem => 1,
            Self::RankingPush | Self::RankingProblem => 2,
            Self::ContentDecay                       => 2,
            Self::IntentMismatch                     => 2,
            Self::ContentExpansion | Self::ExpansionPotential => 3,
        }
    }

    pub fn effort_label(&self) -> &'static str {
        match self.effort() {
            1 => "Low",
            2 => "Medium",
            _ => "High",
        }
    }

    /// Diagnosis class label for the report narrative.
    pub fn diagnosis(&self) -> &'static str {
        match self {
            Self::CtrFix | Self::SnippetProblem =>
                "Snippet does not match search intent or title is too generic",
            Self::RankingPush | Self::RankingProblem =>
                "Topic is relevant but page does not rank high enough",
            Self::IntentMismatch =>
                "Page ranks but does not answer what is being searched for",
            Self::ContentExpansion | Self::ExpansionPotential =>
                "Topic could be expanded into a cluster or hub",
            Self::ContentDecay =>
                "Page is losing visibility and needs updating",
            Self::InternalLinking | Self::DistributionProblem =>
                "Good content but barely any internal linking",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Opportunity {
    pub opportunity_type: OpportunityType,
    /// URL the opportunity applies to
    pub url: String,
    /// Keyword (if query-level opportunity)
    pub keyword: Option<String>,
    /// Estimated additional clicks per period
    pub estimated_clicks: f64,
    /// Current clicks (for % growth calculation)
    pub current_clicks: f64,
    /// Priority score: impact * confidence / effort (higher = act first)
    pub score: f64,
    /// Human-readable action (may be merged from multiple types)
    pub action: String,
    /// Supporting data for the action
    pub context: String,
    /// All opportunity types merged into this entry (display labels)
    pub type_labels: Vec<String>,
    /// Root cause interpretation — why this is happening, what the data means
    pub interpretation: String,
    /// Concrete action steps the user should take (ordered by priority)
    pub specific_actions: Vec<String>,
}

/// AI-traffic breakdown per page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiPageRow {
    pub url: String,
    pub sessions: i64,
    pub share_of_ai: f64, // 0.0–1.0
}

// ─── Standalone Opportunities Report ─────────────────────────────────────────

/// Standalone opportunities report — query- and page-level opportunities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpportunitiesReport {
    pub property_name: String,
    pub date_range: String,
    pub opportunities: Vec<Opportunity>,
    pub total_estimated_clicks: f64,
    pub summary: String,
    pub action_plan: ActionPlan,
    pub insights: Vec<Insight>,
}

// ─── Queries Report ──────────────────────────────────────────────────────────

/// Query-level analysis with opportunity scoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueriesReport {
    pub property_name: String,
    pub date_range: String,
    pub queries: Vec<QueryRow>,
    pub total_clicks: f64,
    pub total_impressions: f64,
    pub avg_ctr: f64,
    pub avg_position: f64,
    pub brand_clicks: f64,
    pub non_brand_clicks: f64,
    pub intent_distribution: crate::intent::IntentDistribution,
    pub insights: Vec<Insight>,
}

// ─── AI Traffic Report ───────────────────────────────────────────────────────

/// AI referral traffic breakdown
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiTrafficReport {
    pub property_name: String,
    pub date_range: String,
    pub total_sessions: i64,
    pub ai_sessions: i64,
    pub ai_share_pct: f64,
    /// AI sessions in previous period (for trend)
    pub prev_ai_sessions: i64,
    /// Period-over-period change in AI sessions (%)
    pub ai_trend_pct: f64,
    /// Average engagement rate for AI-referred sessions
    pub ai_engagement_rate: f64,
    /// Average engagement rate for all sessions (comparison)
    pub overall_engagement_rate: f64,
    pub ai_sources: Vec<SourceRow>,
    pub ai_pages: Vec<AiPageRow>,
    /// Pattern detected across AI-referred pages (e.g. "structured, explanatory content")
    pub content_pattern: Option<String>,
    /// Actionable recommendations to grow AI referral traffic
    pub recommendations: Vec<String>,
    pub insights: Vec<Insight>,
}

// ─── Channels Report ─────────────────────────────────────────────────────────

/// Channel breakdown report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelsReport {
    pub property_name: String,
    pub date_range: String,
    pub channels: Vec<ChannelDetail>,
    pub total_sessions: i64,
    pub insights: Vec<Insight>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelDetail {
    pub channel: String,
    pub sessions: i64,
    pub share_pct: f64,
    pub engagement_rate: f64,
    pub avg_session_duration_secs: f64,
}

// ─── Decay Report ────────────────────────────────────────────────────────────

/// Content decay report — pages losing search performance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecayReport {
    pub property_name: String,
    pub date_range: String,
    pub days: u32,
    pub declining_pages: Vec<DecayPage>,
    pub insights: Vec<Insight>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecayPage {
    pub url: String,
    pub clicks_before: f64,
    pub clicks_after: f64,
    pub clicks_pct: f64,
    pub impressions_before: f64,
    pub impressions_after: f64,
    pub impressions_pct: f64,
    pub position_before: f64,
    pub position_after: f64,
    pub position_delta: f64,
}

// ─── Devices Report ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevicesReport {
    pub property_name: String,
    pub date_range: String,
    pub devices: Vec<DeviceDetail>,
    pub total_sessions: i64,
    pub insights: Vec<Insight>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceDetail {
    pub device: String,
    pub sessions: i64,
    pub share_pct: f64,
    pub engagement_rate: f64,
    pub avg_session_duration_secs: f64,
}

// ─── Countries Report ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CountriesReport {
    pub property_name: String,
    pub date_range: String,
    pub countries: Vec<CountryDetail>,
    pub total_sessions: i64,
    pub insights: Vec<Insight>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CountryDetail {
    pub country: String,
    pub sessions: i64,
    pub share_pct: f64,
    pub engagement_rate: f64,
}

// ─── Growth Drivers ─────────────────────────────────────────────────────────

/// Growth Drivers report — what's driving or losing traffic
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrowthReport {
    pub property_name: String,
    pub date_range: String,
    /// Pages with the biggest absolute session gain
    pub top_growing_pages: Vec<GrowthRow>,
    /// Pages with the biggest absolute session loss
    pub top_declining_pages: Vec<GrowthRow>,
    /// Queries with the biggest absolute click gain
    pub top_growing_queries: Vec<GrowthRow>,
    /// Queries that appeared only in the current period
    pub new_queries: Vec<QueryRow>,
    /// Channel-level growth breakdown
    pub channel_growth: Vec<ChannelGrowthRow>,
    pub insights: Vec<Insight>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrowthRow {
    pub label: String,
    pub current: f64,
    pub previous: f64,
    pub delta: f64,
    pub delta_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelGrowthRow {
    pub channel: String,
    pub current_sessions: i64,
    pub previous_sessions: i64,
    pub delta: i64,
    pub delta_pct: f64,
}

// ─── Weekly Trends ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendsReport {
    pub property_name: String,
    pub date_range: String,
    pub weeks: Vec<WeekRow>,
    /// Queries with significant position jumps
    pub ranking_jumps: Vec<GrowthRow>,
    pub insights: Vec<Insight>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeekRow {
    pub week_start: String,
    pub sessions: i64,
    pub clicks: f64,
    pub impressions: f64,
    pub ctr: f64,
    pub avg_position: f64,
}

// ─── Action Plan ────────────────────────────────────────────────────────────

/// Three-tier action plan derived from opportunities.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ActionPlan {
    /// Low effort, high score — do this week
    pub quick_wins: Vec<Action>,
    /// Higher effort or cluster-level — do this month
    pub strategic: Vec<Action>,
    /// Decay items, early signals — revisit in 2-4 weeks
    pub monitoring: Vec<Action>,
}

/// A single action item with context sentence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub url: String,
    pub keyword: Option<String>,
    pub diagnosis: String,
    pub action: String,
    pub impact_label: String,
    pub effort_label: String,
    pub reason: String,
}

// ─── Topic Clusters ────────────────────────────────────────────────────────

/// Topic cluster report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClustersReport {
    pub property_name: String,
    pub date_range: String,
    pub clusters: Vec<TopicCluster>,
    pub insights: Vec<Insight>,
}

/// A single topic cluster aggregating pages and queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicCluster {
    pub name: String,
    pub pages: usize,
    pub queries: usize,
    pub sessions: i64,
    pub clicks: f64,
    pub impressions: f64,
    pub ctr: f64,
    pub avg_position: f64,
    /// Unused CTR potential (sum of expected - actual CTR across queries)
    pub ctr_potential: f64,
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
