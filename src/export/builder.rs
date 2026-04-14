//! Transforms domain reports into a flat ViewModel for PDF rendering.

use crate::domain::{InsightSeverity, Opportunity, SiteOverviewReport, TopPagesReport};

pub struct ReportViewModel {
    pub property_name: String,
    pub date_range: String,
    pub created_at: String,

    // Cover KPIs
    pub exec_sessions: String,
    pub exec_sessions_trend: Option<TrendValue>,
    pub exec_organic_pct: String,
    pub exec_clicks: String,
    pub exec_clicks_trend: Option<TrendValue>,
    pub exec_avg_position: String,

    // Top 3 To-Dos for very top of report
    pub top3_todos: Vec<String>,

    // Traffic
    pub channel_rows: Vec<ChannelRow>,
    pub engagement_rate: String,
    pub top_sources: Vec<SourceRow>,
    pub ai_sessions_total: i64,
    pub ai_source_rows: Vec<SourceRow>,
    pub ai_page_rows: Vec<AiPagePdfRow>,

    // Search Console
    pub search_impressions: String,
    pub search_ctr: String,
    pub top_queries: Vec<QueryRow>,
    pub opportunity_queries: Vec<QueryRow>,

    // Top pages
    pub top_pages: Vec<PageRow>,

    // Opportunities (grouped, scored)
    pub opportunities: Vec<OpportunityRow>,

    // Insights
    pub insights: Vec<InsightRow>,
}

impl ReportViewModel {
    pub fn total_sessions_raw(&self) -> i64 {
        self.exec_sessions.replace('.', "").parse().unwrap_or(0)
    }
}

pub struct TrendValue {
    pub label: String,
    pub is_positive: bool,
}

pub struct ChannelRow { pub channel: String, pub sessions: String, pub share: String }
pub struct SourceRow  { pub source: String,  pub sessions: String, pub share: String }
pub struct AiPagePdfRow { pub url: String, pub sessions: String, pub share_pct: String }
pub struct QueryRow   {
    pub query: String, pub clicks: String, pub impressions: String,
    pub ctr: String,   pub position: String,
}
pub struct PageRow {
    pub url: String,  pub sessions: String, pub organic: String,
    pub clicks: String, pub position: String,
}
pub struct OpportunityRow {
    pub score: String,
    pub type_label: String,
    pub keyword_or_url: String,
    pub estimated_clicks: String,
    pub pct_potential: String,
    pub effort: String,
    pub action: String,
    pub context: String,
}
pub struct InsightRow {
    pub severity: InsightSeverity,
    pub headline: String,
    pub explanation: String,
}

pub fn build_view_model(
    overview: &SiteOverviewReport,
    top_pages: &TopPagesReport,
) -> ReportViewModel {
    let t = &overview.traffic;
    let s = &overview.search;

    // ── Trend ────────────────────────────────────────────────────────────────
    let sessions_trend = overview.trend.as_ref().map(|d| TrendValue {
        label: fmt_trend(d.sessions_pct),
        is_positive: d.sessions_pct >= 0.0,
    });
    let clicks_trend = overview.trend.as_ref().map(|d| TrendValue {
        label: fmt_trend(d.clicks_pct),
        is_positive: d.clicks_pct >= 0.0,
    });

    // ── Opportunities ────────────────────────────────────────────────────────
    let opportunities: Vec<OpportunityRow> = overview.opportunities.iter()
        .map(opp_to_row)
        .collect();

    // ── Top 3 To-Dos ─────────────────────────────────────────────────────────
    let top3_todos: Vec<String> = overview.opportunities.iter().take(3).map(|o| {
        let kw = o.keyword.as_deref()
            .or(if o.url.is_empty() { None } else { Some(o.url.as_str()) })
            .unwrap_or("?");
        let labels = o.type_labels.join(" + ");
        format!(
            "{}: \"{}\" - +{:.0} clicks possible ({})",
            labels, kw, o.estimated_clicks, o.opportunity_type.effort_label()
        )
    }).collect();

    // ── Channel rows ─────────────────────────────────────────────────────────
    let mut channels: Vec<(&str, i64)> = vec![
        ("Organic Search",   t.organic_sessions),
        ("Direct",           t.direct_sessions),
        ("Referral",         t.referral_sessions),
        ("Other",            t.other_sessions),
    ];
    channels.retain(|(_, n)| *n > 0);
    channels.sort_by(|a, b| b.1.cmp(&a.1));
    let channel_rows = channels.iter().map(|(name, n)| ChannelRow {
        channel: name.to_string(),
        sessions: fmt_num(*n),
        share: pct(*n, t.total_sessions),
    }).collect();

    // ── Sources ───────────────────────────────────────────────────────────────
    let top_sources = overview.top_sources.iter().map(|s| SourceRow {
        source: s.source.clone(), sessions: fmt_num(s.sessions), share: pct(s.sessions, t.total_sessions),
    }).collect();

    let ai_sessions_total: i64 = overview.ai_sources.iter().map(|s| s.sessions).sum();
    let ai_source_rows = overview.ai_sources.iter().map(|s| SourceRow {
        source: s.source.clone(), sessions: fmt_num(s.sessions), share: pct(s.sessions, t.total_sessions),
    }).collect();

    // ── AI pages ─────────────────────────────────────────────────────────────
    let ai_page_rows = overview.ai_pages.iter().map(|p| AiPagePdfRow {
        url:        shorten_url(&p.url),
        sessions:   fmt_num(p.sessions),
        share_pct:  format!("{:.0}%", p.share_of_ai * 100.0),
    }).collect();

    // ── Queries ───────────────────────────────────────────────────────────────
    let top_queries = s.top_queries.iter().take(15).map(|q| QueryRow {
        query: q.query.clone(), clicks: format!("{:.0}", q.clicks),
        impressions: format!("{:.0}", q.impressions),
        ctr: format!("{:.1}%", q.ctr * 100.0), position: format!("{:.1}", q.position),
    }).collect();

    let opportunity_queries = s.top_queries.iter()
        .filter(|q| q.position >= 4.0 && q.position <= 20.0 && q.ctr < 0.03 && q.impressions > 50.0)
        .take(10)
        .map(|q| QueryRow {
            query: q.query.clone(), clicks: format!("{:.0}", q.clicks),
            impressions: format!("{:.0}", q.impressions),
            ctr: format!("{:.1}%", q.ctr * 100.0), position: format!("{:.1}", q.position),
        }).collect();

    // ── Insights (collect before top_pages is shadowed) ─────────────────────
    let combined: Vec<&crate::domain::Insight> = overview.insights.iter()
        .chain(top_pages.insights.iter())
        .collect();

    // ── Pages ─────────────────────────────────────────────────────────────────
    let top_pages: Vec<PageRow> = top_pages.pages.iter().take(15).map(|p| PageRow {
        url: shorten_url(&p.url), sessions: fmt_num(p.sessions),
        organic: fmt_num(p.organic_sessions), clicks: format!("{:.0}", p.search.clicks),
        position: if p.search.average_position > 0.0 { format!("{:.1}", p.search.average_position) } else { "-".into() },
    }).collect();
    let mut all_insights: Vec<InsightRow> = combined.into_iter().map(|i| InsightRow {
        severity: i.severity.clone(), headline: i.headline.clone(), explanation: i.explanation.clone(),
    }).collect();
    all_insights.sort_by_key(|i| match i.severity {
        InsightSeverity::Critical => 0, InsightSeverity::Warning => 1,
        InsightSeverity::Positive => 2, InsightSeverity::Info => 3,
    });

    ReportViewModel {
        property_name:       overview.property_name.clone(),
        date_range:          overview.date_range.clone(),
        created_at:          chrono::Utc::now().format("%d.%m.%Y").to_string(),
        exec_sessions:       fmt_num(t.total_sessions),
        exec_sessions_trend: sessions_trend,
        exec_organic_pct:    format!("{:.0}%", t.organic_share()),
        exec_clicks:         format!("{:.0}", s.clicks),
        exec_clicks_trend:   clicks_trend,
        exec_avg_position:   if s.average_position > 0.0 { format!("{:.1}", s.average_position) } else { "-".into() },
        top3_todos,
        channel_rows,
        engagement_rate: format!("{:.1}%", overview.engagement_rate * 100.0),
        top_sources,
        ai_sessions_total,
        ai_source_rows,
        ai_page_rows,
        search_impressions: format!("{:.0}", s.impressions),
        search_ctr:         format!("{:.1}%", s.ctr * 100.0),
        top_queries,
        opportunity_queries,
        top_pages,
        opportunities,
        insights: all_insights,
    }
}

fn opp_to_row(o: &Opportunity) -> OpportunityRow {
    let kw = o.keyword.as_deref()
        .or(if o.url.is_empty() { None } else { Some(o.url.as_str()) })
        .unwrap_or("-");
    let kw_short = if kw.len() > 45 { format!("{}...", &kw[..42]) } else { kw.to_string() };

    let pct = if o.current_clicks > 0.5 {
        format!("+{:.0}%", o.estimated_clicks / o.current_clicks * 100.0)
    } else {
        "new".into()
    };

    OpportunityRow {
        score:            format!("{:.1}", o.score),
        type_label:       o.type_labels.join(" + "),
        keyword_or_url:   kw_short,
        estimated_clicks: format!("+{:.0}", o.estimated_clicks),
        pct_potential:    pct,
        effort:           o.opportunity_type.effort_label().to_string(),
        action:           o.action.clone(),
        context:          o.context.clone(),
    }
}

fn fmt_trend(pct: f64) -> String {
    if pct >= 0.0 { format!("+{:.1}%", pct) } else { format!("{:.1}%", pct) }
}

fn pct(n: i64, total: i64) -> String {
    if total == 0 { return "0%".into(); }
    format!("{:.0}%", n as f64 / total as f64 * 100.0)
}

pub fn fmt_num(n: i64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 { result.push('.'); }
        result.push(ch);
    }
    result.chars().rev().collect()
}

fn shorten_url(url: &str) -> String {
    let u = url.trim_start_matches("https://").trim_start_matches("http://");
    if u.len() > 58 { format!("{}...", &u[..55]) } else { u.to_string() }
}
