pub mod style;

use comfy_table::{Cell, CellAlignment, Table};
use indicatif::{ProgressBar, ProgressStyle};
use runemark::{DetailLevel, Finding, FindingGroup, Report, Tone, Verdict};
use style::Paint;

use crate::domain::{
    AiTrafficReport, ChannelsReport, ClustersReport, ComparisonReport, CountriesReport,
    DecayReport, DevicesReport, GrowthReport, InsightCategory, InsightSeverity,
    OpportunitiesReport,
    PageDetailReport, QueriesReport, SiteOverviewReport, TopPagesReport, TrendsReport,
};
use crate::page_audit;

// ─── Welcome ────────────────────────────────────────────────────────────────

/// An indeterminate spinner for a single pending fetch.
///
/// runemark owns colour and reports, but its progress sink models countable work
/// (`{pos}/{len}`); these waits are one concurrent batch of API calls with nothing
/// to count. The spinner therefore stays on `indicatif` and only borrows the colour
/// decision, so `NO_COLOR` and redirected output behave like every other line.
pub fn spinner(msg: &str) -> ProgressBar {
    let template = if style::console().color_enabled() {
        "{spinner:.cyan} {msg}"
    } else {
        "{spinner} {msg}"
    };
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template(template)
            .expect("the spinner template is a literal and always valid"),
    );
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb
}

/// One line per set of dimensions, keeping the worst case.
///
/// A single command runs several reports, and they ask overlapping questions —
/// `overview` and `top_pages` both want pages. Repeating the same warning once
/// per request would bury it.
fn worst_per_dimension(
    truncations: &[crate::google::api::Truncation],
) -> Vec<&crate::google::api::Truncation> {
    let mut worst: Vec<&crate::google::api::Truncation> = Vec::new();
    for t in truncations {
        match worst.iter_mut().find(|w| w.what == t.what) {
            Some(existing) if t.total > existing.total => *existing = t,
            Some(_) => {}
            None => worst.push(t),
        }
    }
    worst
}

/// Tell the reader when a report is built on a partial answer.
///
/// A truncated result is worse than a missing one: the numbers look complete
/// and are not. Reported once per run, per set of dimensions, after the tables
/// so it is the last thing on screen.
pub fn print_truncation_warnings(truncations: &[crate::google::api::Truncation]) {
    if truncations.is_empty() {
        return;
    }

    println!("{}", "PARTIAL DATA".heading());
    for t in worst_per_dimension(truncations) {
        match t.total {
            Some(total) => println!(
                "  {} {}: {} of {} rows — the figures above cover the top {}.",
                "⚠".warn(),
                t.what.accent(),
                format_number(t.returned as i64),
                format_number(total),
                format_number(t.returned as i64),
            ),
            None => println!(
                "  {} {}: {} rows, the maximum requested — there may be more.",
                "⚠".warn(),
                t.what.accent(),
                format_number(t.returned as i64),
            ),
        }
    }
    println!();
}

pub fn print_welcome() {
    println!();
    println!("{}", "auditmyvisitors".accent());
    println!("{}", "Google Analytics 4 & Search Console Reporting".muted());
    println!();
}

// ─── Auth ─────────────────────────────────────────────────────────────────────

pub fn print_auth_status(status: &crate::auth::AuthStatus) {
    match status {
        crate::auth::AuthStatus::LoggedIn => {
            println!("{} Logged in — token is valid.", "✓".ok());
        }
        crate::auth::AuthStatus::TokenExpired => {
            println!(
                "{} Token expired — will be refreshed automatically on next API call.",
                "⚠".warn()
            );
        }
        crate::auth::AuthStatus::NotLoggedIn => {
            println!(
                "{} Not logged in. Start with: {}",
                "✗".err(),
                "auditmyvisitors auth login".accent()
            );
        }
    }
}

// ─── Snapshot comparison ────────────────────────────────────────────────────

pub fn print_snapshot_comparison(
    prev: &crate::snapshots::Snapshot,
    report: &SiteOverviewReport,
) {
    println!("{}", "COMPARISON WITH LAST SNAPSHOT".heading());
    println!("Last snapshot: {} ({} days)\n", prev.date.accent(), prev.days);

    let sess_pct = crate::helpers::pct_change(prev.sessions as f64, report.traffic.total_sessions as f64);
    let clicks_pct = crate::helpers::pct_change(prev.clicks, report.search.clicks);
    let impr_pct = crate::helpers::pct_change(prev.impressions, report.search.impressions);

    let sess = fmt_trend_colored(sess_pct, false);
    let clicks = fmt_trend_colored(clicks_pct, false);
    let impr = fmt_trend_colored(impr_pct, false);

    println!("  Sessions {}  |  Clicks {}  |  Impressions {}\n", sess, clicks, impr);
}

// ─── Overview ────────────────────────────────────────────────────────────────

pub fn print_overview(report: &SiteOverviewReport) {
    println!("\n{}", "OVERVIEW".heading());
    println!("Property: {}  |  Period: {}\n", report.property_name.accent(), report.date_range);

    // ── Trend summary (if available) ─────────────────────────────────────────
    if let Some(trend) = &report.trend {
        let sess = fmt_trend_colored(trend.sessions_pct, false);
        let clicks = fmt_trend_colored(trend.clicks_pct, false);
        let impr = fmt_trend_colored(trend.impressions_pct, false);
        println!(
            "{}  Sessions {}  |  Clicks {}  |  Impressions {}",
            "TREND".strong(), sess, clicks, impr
        );
        println!();
    }

    let mut table = traffic_table();
    table.add_row(vec![
        Cell::new("Total sessions"),
        Cell::new(format_number(report.traffic.total_sessions)).set_alignment(CellAlignment::Right),
    ]);
    table.add_row(vec![
        Cell::new("  organic"),
        Cell::new(format!(
            "{} ({:.0}%)",
            format_number(report.traffic.organic_sessions),
            report.traffic.organic_share()
        )).set_alignment(CellAlignment::Right),
    ]);
    table.add_row(vec![
        Cell::new("  direct"),
        Cell::new(format!(
            "{} ({:.0}%)",
            format_number(report.traffic.direct_sessions),
            report.traffic.direct_share()
        )).set_alignment(CellAlignment::Right),
    ]);
    table.add_row(vec![
        Cell::new("Engagement Rate"),
        Cell::new(format!("{:.1}%", report.engagement_rate * 100.0)).set_alignment(CellAlignment::Right),
    ]);
    println!("{table}\n");

    // ── Top sources ──────────────────────────────────────────────────────────
    if !report.top_sources.is_empty() {
        println!("{}", "TOP SOURCES".heading());
        let mut src_table = Table::new();
        src_table.set_header(vec![
            Cell::new("Source"),
            Cell::new("Sessions"),
            Cell::new("Share"),
        ]);
        for src in report.top_sources.iter().take(10) {
            let share = if report.traffic.total_sessions > 0 {
                src.sessions as f64 / report.traffic.total_sessions as f64 * 100.0
            } else { 0.0 };
            src_table.add_row(vec![
                Cell::new(&src.source),
                Cell::new(format_number(src.sessions)).set_alignment(CellAlignment::Right),
                Cell::new(format!("{:.1}%", share)).set_alignment(CellAlignment::Right),
            ]);
        }
        println!("{src_table}\n");
    }

    // ── AI traffic ───────────────────────────────────────────────────────────
    if !report.ai_sources.is_empty() {
        let ai_total: i64 = report.ai_sources.iter().map(|s| s.sessions).sum();
        let ai_pct = if report.traffic.total_sessions > 0 {
            ai_total as f64 / report.traffic.total_sessions as f64 * 100.0
        } else { 0.0 };
        println!(
            "{} {} Sessions ({:.1}%)",
            "AI-TRAFFIC".heading(),
            format_number(ai_total),
            ai_pct
        );
        for src in &report.ai_sources {
            println!("  {} — {} Sessions", src.source, format_number(src.sessions));
        }
        println!();
    }

    println!("{}", "SEARCH CONSOLE".heading());
    let mut sc = traffic_table();
    sc.add_row(vec!["Clicks", &format_f64(report.search.clicks)]);
    sc.add_row(vec!["Impressions", &format_f64(report.search.impressions)]);
    sc.add_row(vec!["CTR", &format!("{:.1}%", report.search.ctr * 100.0)]);
    sc.add_row(vec!["Avg. Position", &format!("{:.1}", report.search.average_position)]);
    println!("{sc}\n");

    // ── Top opportunities (max 5) ────────────────────────────────────────────
    if !report.opportunities.is_empty() {
        println!("{}", "TOP OPPORTUNITIES".heading());
        for (i, opp) in report.opportunities.iter().take(5).enumerate() {
            let kw = opp.keyword.as_deref()
                .or(if opp.url.is_empty() { None } else { Some(opp.url.as_str()) })
                .unwrap_or("-");
            println!(
                "  {}. {} — \"{}\" (+{:.0} Clicks, {})",
                i + 1,
                opp.type_labels.join(" + ").warn(),
                kw,
                opp.estimated_clicks,
                opp.opportunity_type.effort_label()
            );
        }
        let total: f64 = report.opportunities.iter().map(|o| o.estimated_clicks).sum();
        if total > 0.0 {
            println!("  Estimated total potential: {} Clicks/month", format!("+{:.0}", total).ok());
        }
        println!();
    }

    print_insights(&report.insights);
}

// ─── Top Pages ───────────────────────────────────────────────────────────────

pub fn print_top_pages(report: &TopPagesReport) {
    println!("\n{}", "PAGE PERFORMANCE".heading());
    println!("Property: {}  |  Period: {}\n", report.property_name.accent(), report.date_range);

    let tracking_enabled = report.pages.iter().any(|p| p.internal_link_clicks > 0 || p.service_hint_clicks > 0);
    let strengths = page_audit::ranking(&report.pages, 20, |p| page_audit::strength_score(p, tracking_enabled));
    let weaknesses = page_audit::ranking(&report.pages, 20, page_audit::weakness_score);
    let isolated = page_audit::ranking(&report.pages, 10, |p| page_audit::isolated_score(p, tracking_enabled));

    let mut table = Table::new();
    table.set_header(vec![
        Cell::new("#"),
        Cell::new("Page"),
        Cell::new("Sessions"),
        Cell::new("Organic"),
        Cell::new("Engagement"),
        Cell::new("Bounce"),
        Cell::new("New Users"),
        Cell::new("Key Events"),
        Cell::new("Clicks"),
        Cell::new("CTR"),
        Cell::new("Position"),
    ]);

    for (i, page) in strengths.iter().enumerate() {
        let short_url = shorten_url(&page.url, 45);
        table.add_row(vec![
            Cell::new(i + 1).set_alignment(CellAlignment::Right),
            Cell::new(&short_url),
            Cell::new(format_number(page.sessions)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.0}%", if page.sessions > 0 { page.organic_sessions as f64 / page.sessions as f64 * 100.0 } else { 0.0 })).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.0}%", page.engagement_rate * 100.0)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.0}%", page.bounce_rate * 100.0)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.0}%", page.new_user_share * 100.0)).set_alignment(CellAlignment::Right),
            Cell::new(format_number(page.key_events)).set_alignment(CellAlignment::Right),
            Cell::new(format_f64(page.search.clicks)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.1}%", page.search.ctr * 100.0)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.1}", page.search.average_position)).set_alignment(CellAlignment::Right),
        ]);
    }

    println!("{}\n{table}\n", "TOP 20 STRENGTHS".heading());

    print_weakest_pages(&TopPagesReport {
        property_name: report.property_name.clone(),
        date_range: report.date_range.clone(),
        pages: weaknesses,
        insights: vec![],
    });

    if !isolated.is_empty() {
        println!("{}", "TOP 10 ISOLATED ARTICLES".heading());
        let mut isolated_table = Table::new();
        isolated_table.set_header(vec![
            Cell::new("#"),
            Cell::new("Page"),
            Cell::new("Sessions"),
            Cell::new("Clicks"),
            Cell::new("ServiceHint"),
            Cell::new("Internal Clicks"),
        ]);

        for (i, page) in isolated.iter().enumerate() {
            isolated_table.add_row(vec![
                Cell::new(i + 1).set_alignment(CellAlignment::Right),
                Cell::new(shorten_url(&page.url, 45)),
                Cell::new(format_number(page.sessions)).set_alignment(CellAlignment::Right),
                Cell::new(format_f64(page.search.clicks)).set_alignment(CellAlignment::Right),
                Cell::new(format_number(page.service_hint_clicks)).set_alignment(CellAlignment::Right),
                Cell::new(format_number(page.internal_link_clicks)).set_alignment(CellAlignment::Right),
            ]);
        }

        println!("{isolated_table}\n");
    }

    if !strengths.is_empty() {
        println!("{}", "TOP 20 PAGE DIAGNOSES".heading());
        for (i, page) in strengths.iter().enumerate() {
            println!("{}. {}", i + 1, shorten_url(&page.url, 70).strong());
            println!(
                "   Metrics: {} sessions | {:.0} impressions | {:.1}% CTR | pos {:.1} | {:.0}% engagement",
                format_number(page.sessions),
                page.search.impressions,
                page.search.ctr * 100.0,
                page.search.average_position,
                page.engagement_rate * 100.0
            );
            println!(
                "   Queries: {}",
                page_audit::top_query_summary(page).muted()
            );
            println!(
                "   Diagnosis: {}",
                page_audit::issue_label(page, tracking_enabled).warn()
            );
            println!(
                "   Next step: {}",
                page_audit::recommendation(page, tracking_enabled)
            );
            println!();
        }
    }

    print_insights(&report.insights);
}

pub fn print_weakest_pages(report: &TopPagesReport) {
    println!("\n{}", "PAGES WITH THE BIGGEST ISSUES".heading());
    println!("Property: {}  |  Period: {}\n", report.property_name.accent(), report.date_range);

    let mut table = Table::new();
    table.set_header(vec![
        Cell::new("#"),
        Cell::new("Page"),
        Cell::new("Sessions"),
        Cell::new("Organic"),
        Cell::new("Engagement"),
        Cell::new("Bounce"),
        Cell::new("Clicks"),
        Cell::new("Impr."),
        Cell::new("CTR"),
        Cell::new("Pos."),
        Cell::new("Issue"),
    ]);

    for (i, page) in report.pages.iter().enumerate() {
        let short_url = shorten_url(&page.url, 40);
        table.add_row(vec![
            Cell::new(i + 1).set_alignment(CellAlignment::Right),
            Cell::new(&short_url),
            Cell::new(format_number(page.sessions)).set_alignment(CellAlignment::Right),
            Cell::new(format!(
                "{:.0}%",
                if page.sessions > 0 {
                    page.organic_sessions as f64 / page.sessions as f64 * 100.0
                } else {
                    0.0
                }
            ))
            .set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.0}%", page.engagement_rate * 100.0)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.0}%", page.bounce_rate * 100.0)).set_alignment(CellAlignment::Right),
            Cell::new(format_f64(page.search.clicks)).set_alignment(CellAlignment::Right),
            Cell::new(format_f64(page.search.impressions)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.1}%", page.search.ctr * 100.0)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.1}", page.search.average_position)).set_alignment(CellAlignment::Right),
            Cell::new(weak_page_signal(page)),
        ]);
    }

    println!("{table}\n");
}

// ─── Page detail ─────────────────────────────────────────────────────────────

pub fn print_page_detail(report: &PageDetailReport) {
    println!("\n{}", "PAGE DETAIL".heading());
    println!("URL: {}  |  Period: {}\n", report.url.accent(), report.date_range);

    let mut table = traffic_table();
    table.add_row(vec!["Total sessions", &format_number(report.traffic.total_sessions)]);
    table.add_row(vec!["Organic", &format!("{} ({:.0}%)", format_number(report.traffic.organic_sessions), report.traffic.organic_share())]);
    table.add_row(vec!["Direct", &format!("{} ({:.0}%)", format_number(report.traffic.direct_sessions), report.traffic.direct_share())]);
    table.add_row(vec!["Engagement Rate", &format!("{:.1}%", report.engagement_rate * 100.0)]);
    table.add_row(vec!["Bounce Rate", &format!("{:.1}%", report.bounce_rate * 100.0)]);
    table.add_row(vec!["Avg. session duration", &format_duration(report.avg_session_duration_secs)]);
    table.add_row(vec!["New Users", &format!("{:.0}%", report.new_user_share * 100.0)]);
    if report.key_events > 0 {
        table.add_row(vec!["Key Events", &format_number(report.key_events)]);
    }
    println!("{table}\n");

    println!("{}", "SEARCH CONSOLE".heading());
    let mut sc = traffic_table();
    sc.add_row(vec!["Clicks", &format_f64(report.search.clicks)]);
    sc.add_row(vec!["Impressions", &format_f64(report.search.impressions)]);
    sc.add_row(vec!["CTR", &format!("{:.1}%", report.search.ctr * 100.0)]);
    sc.add_row(vec!["Avg. Position", &format!("{:.1}", report.search.average_position)]);
    println!("{sc}\n");

    if !report.search.top_queries.is_empty() {
        println!("{}", "TOP KEYWORDS".heading());
        let mut qt = Table::new();
        qt.set_header(vec!["Query", "Clicks", "Impressions", "CTR", "Position"]);
        for q in report.search.top_queries.iter().take(10) {
            qt.add_row(vec![
                Cell::new(&q.query),
                Cell::new(format_f64(q.clicks)).set_alignment(CellAlignment::Right),
                Cell::new(format_f64(q.impressions)).set_alignment(CellAlignment::Right),
                Cell::new(format!("{:.1}%", q.ctr * 100.0)).set_alignment(CellAlignment::Right),
                Cell::new(format!("{:.1}", q.position)).set_alignment(CellAlignment::Right),
            ]);
        }
        println!("{qt}\n");
    }

    print_insights(&report.insights);
    print_recommendations(&report.recommendations);
}

// ─── Comparison ──────────────────────────────────────────────────────────────

pub fn print_comparison(report: &ComparisonReport) {
    println!("\n{}", "COMPARISON".heading());
    if let Some(url) = &report.url {
        println!("URL: {}", url.accent());
    }
    println!("Change date: {}  |  Before: {} days  |  After: {} days\n",
        report.change_date.warn(), report.before_days, report.after_days);

    // ── Verdict line ─────────────────────────────────────────────────────────
    let d = &report.delta;
    let winners: Vec<&str> = [
        (d.sessions_pct >= 10.0, "Sessions"),
        (d.clicks_pct >= 15.0, "Clicks"),
        (d.impressions_pct >= 20.0, "Impressions"),
        (d.position_abs <= -2.0, "Position"),
    ].iter().filter(|(cond, _)| *cond).map(|(_, l)| *l).collect();

    let losers: Vec<&str> = [
        (d.sessions_pct <= -10.0, "Sessions"),
        (d.clicks_pct <= -15.0, "Clicks"),
        (d.impressions_pct <= -20.0, "Impressions"),
        (d.position_abs >= 2.0, "Position"),
    ].iter().filter(|(cond, _)| *cond).map(|(_, l)| *l).collect();

    if !winners.is_empty() || !losers.is_empty() {
        if !winners.is_empty() {
            println!("{} {}", "Winners:".ok(), winners.join(", "));
        }
        if !losers.is_empty() {
            println!("{} {}", "Losers:".err(), losers.join(", "));
        }
        println!();
    }

    let mut table = Table::new();
    table.set_header(vec![
        Cell::new("Metric"),
        Cell::new(format!("Before\n{}–{}", report.before.start_date, report.before.end_date)),
        Cell::new(format!("After\n{}–{}", report.after.start_date, report.after.end_date)),
        Cell::new("Δ absolute"),
        Cell::new("Δ %"),
    ]);

    add_comparison_row(&mut table, "Sessions",
        report.before.sessions as f64, report.after.sessions as f64,
        report.delta.sessions_abs as f64, report.delta.sessions_pct, false);

    add_comparison_row(&mut table, "Organic",
        report.before.organic_sessions as f64, report.after.organic_sessions as f64,
        report.delta.organic_sessions_abs as f64, report.delta.organic_sessions_pct, false);

    add_comparison_row(&mut table, "Clicks",
        report.before.search.clicks, report.after.search.clicks,
        report.delta.clicks_abs, report.delta.clicks_pct, false);

    add_comparison_row(&mut table, "Impressions",
        report.before.search.impressions, report.after.search.impressions,
        report.delta.impressions_abs, report.delta.impressions_pct, false);

    add_comparison_row_f64(&mut table, "CTR",
        report.before.search.ctr * 100.0, report.after.search.ctr * 100.0,
        report.delta.ctr_abs * 100.0, "%");

    add_comparison_row_f64(&mut table, "Avg. Position",
        report.before.search.average_position, report.after.search.average_position,
        report.delta.position_abs, "");

    println!("{table}\n");

    if !report.summary.is_empty() {
        println!("{}\n{}\n", "SUMMARY".heading(), report.summary);
    }

    print_insights(&report.insights);
}

// ─── Management Summary ─────────────────────────────────────────────────────

pub fn print_management_summary(paragraphs: &[String]) {
    if paragraphs.is_empty() {
        return;
    }
    println!("\n{}", "EXECUTIVE SUMMARY".heading());
    println!();
    for para in paragraphs {
        // Wrap paragraph at ~90 chars for terminal readability
        for line in wrap_text(para, 90) {
            println!("  {}", line);
        }
        println!();
    }
}

pub fn print_growth_highlights(report: &GrowthReport) {
    if report.top_growing_pages.is_empty()
        && report.top_declining_pages.is_empty()
        && report.channel_growth.is_empty()
        && report.new_queries.is_empty()
    {
        return;
    }

    println!("{}", "GROWTH HIGHLIGHTS".heading());

    if let Some(top) = report.top_growing_pages.first() {
        println!(
            "  Strongest page growth: {} ({:+.0} sessions, {:+.0}%)",
            shorten_url(&top.label, 50).ok(),
            top.delta,
            top.delta_pct
        );
    }

    if let Some(drop) = report.top_declining_pages.first() {
        println!(
            "  Biggest page decline: {} ({:+.0} sessions, {:+.0}%)",
            shorten_url(&drop.label, 50).err(),
            drop.delta,
            drop.delta_pct
        );
    }

    if let Some(channel) = report.channel_growth.iter().max_by_key(|c| c.delta) {
        if channel.delta > 0 {
            println!(
                "  Fastest channel growth: {} ({:+} sessions, {:+.0}%)",
                channel.channel.accent(),
                channel.delta,
                channel.delta_pct
            );
        }
    }

    if !report.new_queries.is_empty() {
        let new_clicks: f64 = report.new_queries.iter().map(|q| q.clicks).sum();
        println!(
            "  New search demand: {} new queries generated {:.0} clicks",
            report.new_queries.len(),
            new_clicks
        );
    }

    println!();
}

pub fn print_trend_highlights(report: &TrendsReport) {
    if report.weeks.is_empty() && report.ranking_jumps.is_empty() {
        return;
    }

    println!("{}", "TREND HIGHLIGHTS".heading());

    if report.weeks.len() >= 2 {
        let prev = &report.weeks[report.weeks.len() - 2];
        let last = report.weeks.last().unwrap();
        let sessions_pct = crate::helpers::pct_change(prev.sessions as f64, last.sessions as f64);
        let clicks_pct = crate::helpers::pct_change(prev.clicks, last.clicks);
        println!(
            "  Last week vs previous: sessions {:+.0}%, clicks {:+.0}%",
            sessions_pct,
            clicks_pct
        );
    }

    if let Some(best) = report.ranking_jumps.iter().filter(|r| r.delta < 0.0).min_by(|a, b| {
        a.delta.partial_cmp(&b.delta).unwrap_or(std::cmp::Ordering::Equal)
    }) {
        println!(
            "  Biggest ranking jump: {} ({:.1} -> {:.1})",
            shorten_url(&best.label, 45).ok(),
            best.previous,
            best.current
        );
    }

    if let Some(worst) = report.ranking_jumps.iter().filter(|r| r.delta > 0.0).max_by(|a, b| {
        a.delta.partial_cmp(&b.delta).unwrap_or(std::cmp::Ordering::Equal)
    }) {
        println!(
            "  Biggest ranking loss: {} ({:.1} -> {:.1})",
            shorten_url(&worst.label, 45).err(),
            worst.previous,
            worst.current
        );
    }

    println!();
}

pub fn print_cluster_highlights(report: &ClustersReport) {
    if report.clusters.is_empty() {
        return;
    }

    println!("{}", "CLUSTER HIGHLIGHTS".heading());

    if let Some(top) = report.clusters.first() {
        println!(
            "  Strongest topic: {} ({} sessions across {} pages)",
            top.name.accent(),
            format_number(top.sessions),
            top.pages
        );
    }

    if let Some(best) = report
        .clusters
        .iter()
        .filter(|c| c.ctr_potential > 0.0)
        .max_by(|a, b| a.ctr_potential.partial_cmp(&b.ctr_potential).unwrap_or(std::cmp::Ordering::Equal))
    {
        println!(
            "  Best optimization cluster: {} (~{:.0} additional clicks at optimal CTR)",
            best.name.warn(),
            best.ctr_potential
        );
    }

    if let Some(hub) = report.clusters.iter().find(|c| c.pages <= 1 && c.queries >= 5) {
        println!(
            "  Expansion candidate: {} ({} queries, {} page)",
            hub.name.accent(),
            hub.queries,
            hub.pages
        );
    }

    println!();
}

// ─── Insights / Recommendations ──────────────────────────────────────────────

fn print_insights(insights: &[crate::domain::Insight]) {
    if insights.is_empty() {
        return;
    }
    print!("{}", build_insight_report(insights).render(style::console()));
    println!();
}

/// Turn insights into a runemark report: one group per category, the explanation
/// as the remedy, and the worst severity as the verdict.
///
/// `Finding` offers `remedy` as its only secondary line, so explanatory insights
/// ("Organic sessions have developed positively.") are labelled `Remedy:` as well.
/// The alternative — folding the explanation into the message — would need a
/// terminal width for wrapping and buys less than the grouping costs.
fn build_insight_report(insights: &[crate::domain::Insight]) -> Report {
    let mut report = Report::new("Insights", overall_verdict(insights))
        .with_detail_level(DetailLevel::Detailed);

    // Group by category, in the order the categories first appear.
    let mut order: Vec<&InsightCategory> = Vec::new();
    for insight in insights {
        if !order.contains(&&insight.category) {
            order.push(&insight.category);
        }
    }

    for category in order {
        let mut group = FindingGroup::new(category_label(category));
        for insight in insights.iter().filter(|i| &i.category == category) {
            group = group.add_finding(
                Finding::new(severity_tone(&insight.severity), &insight.headline)
                    .with_remedy(&insight.explanation),
            );
        }
        report = report.add_group(group);
    }

    report
}

/// The worst severity present decides how the whole insight block reads.
fn overall_verdict(insights: &[crate::domain::Insight]) -> Verdict {
    if insights.iter().any(|i| i.severity == InsightSeverity::Critical) {
        Verdict::Failed
    } else if insights.iter().any(|i| i.severity == InsightSeverity::Warning) {
        Verdict::Warning
    } else if insights.iter().any(|i| i.severity == InsightSeverity::Positive) {
        Verdict::Passed
    } else {
        Verdict::Info
    }
}

fn severity_tone(severity: &InsightSeverity) -> Tone {
    match severity {
        InsightSeverity::Positive => Tone::Success,
        InsightSeverity::Info => Tone::Info,
        InsightSeverity::Warning => Tone::Warning,
        InsightSeverity::Critical => Tone::Error,
    }
}

fn category_label(category: &InsightCategory) -> &'static str {
    match category {
        InsightCategory::Traffic => "Traffic",
        InsightCategory::Search => "Search",
        InsightCategory::Engagement => "Engagement",
        InsightCategory::Conversion => "Conversion",
        InsightCategory::Trend => "Trend",
    }
}

fn print_recommendations(recs: &[crate::domain::Recommendation]) {
    if recs.is_empty() {
        return;
    }
    println!("{}", "RECOMMENDATIONS".heading());
    for rec in recs {
        println!("{}. {}", rec.priority, rec.headline.strong());
        println!("   {}", rec.action);
    }
    println!();
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn traffic_table() -> Table {
    let mut t = Table::new();
    t.set_header(vec![
        Cell::new("Metric"),
        Cell::new("Value").set_alignment(CellAlignment::Right),
    ]);
    t
}

fn format_number(n: i64) -> String {
    // Simple thousands separator
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push('.');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

fn format_f64(n: f64) -> String {
    format_number(n.round() as i64)
}

fn format_duration(secs: f64) -> String {
    let mins = (secs / 60.0) as u64;
    let s = (secs as u64) % 60;
    format!("{mins}:{s:02} min")
}

fn shorten_url(url: &str, max: usize) -> String {
    if url.len() <= max {
        return url.to_string();
    }
    format!("{}…", &url[..max - 1])
}

fn add_comparison_row(
    table: &mut Table,
    label: &str,
    before: f64,
    after: f64,
    delta_abs: f64,
    delta_pct: f64,
    lower_is_better: bool,
) {
    let pct_str = if delta_pct == 0.0 {
        "—".to_string()
    } else if (delta_pct > 0.0) != lower_is_better {
        format!("{:+.1}%", delta_pct).ok()
    } else {
        format!("{:+.1}%", delta_pct).err()
    };

    table.add_row(vec![
        Cell::new(label),
        Cell::new(format_f64(before)).set_alignment(CellAlignment::Right),
        Cell::new(format_f64(after)).set_alignment(CellAlignment::Right),
        Cell::new(format!("{:+.0}", delta_abs)).set_alignment(CellAlignment::Right),
        Cell::new(pct_str).set_alignment(CellAlignment::Right),
    ]);
}

fn add_comparison_row_f64(
    table: &mut Table,
    label: &str,
    before: f64,
    after: f64,
    delta: f64,
    unit: &str,
) {
    table.add_row(vec![
        Cell::new(label),
        Cell::new(format!("{:.2}{unit}", before)).set_alignment(CellAlignment::Right),
        Cell::new(format!("{:.2}{unit}", after)).set_alignment(CellAlignment::Right),
        Cell::new(format!("{:+.2}{unit}", delta)).set_alignment(CellAlignment::Right),
        Cell::new("—").set_alignment(CellAlignment::Right),
    ]);
}

fn fmt_trend_colored(pct: f64, lower_is_better: bool) -> String {
    let s = format!("{:+.1}%", pct);
    if pct == 0.0 {
        "—".to_string()
    } else if (pct > 0.0) != lower_is_better {
        s.ok()
    } else {
        s.err()
    }
}

// ─── Opportunities report ────────────────────────────────────────────────────

pub fn print_opportunities(report: &OpportunitiesReport) {
    println!("\n{}", "OPPORTUNITIES".heading());
    println!("Property: {}  |  Period: {}\n", report.property_name.accent(), report.date_range);

    if report.opportunities.is_empty() {
        println!("No significant opportunities found.\n");
        return;
    }

    println!("{}\n", report.summary);

    let mut table = Table::new();
    table.set_header(vec![
        Cell::new("#"),
        Cell::new("Score"),
        Cell::new("Typ"),
        Cell::new("Keyword / URL"),
        Cell::new("+ Clicks"),
        Cell::new("Effort"),
    ]);

    for (i, opp) in report.opportunities.iter().enumerate() {
        let kw = opp.keyword.as_deref()
            .or(if opp.url.is_empty() { None } else { Some(opp.url.as_str()) })
            .unwrap_or("-");
        let kw_short = if kw.len() > 45 { format!("{}...", &kw[..42]) } else { kw.to_string() };

        table.add_row(vec![
            Cell::new(i + 1).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.1}", opp.score)).set_alignment(CellAlignment::Right),
            Cell::new(opp.type_labels.join(" + ")),
            Cell::new(&kw_short),
            Cell::new(format!("+{:.0}", opp.estimated_clicks)).set_alignment(CellAlignment::Right),
            Cell::new(opp.opportunity_type.effort_label()),
        ]);
    }
    println!("{table}\n");

    // Diagnostic cards for top 5
    println!("{}", "DIAGNOSIS".heading());
    for (i, opp) in report.opportunities.iter().take(5).enumerate() {
        let kw = opp.keyword.as_deref()
            .or(if opp.url.is_empty() { None } else { Some(opp.url.as_str()) })
            .unwrap_or("-");
        println!("{}. {} — \"{}\"", i + 1, opp.type_labels.join(" + ").warn(), kw);
        println!("   {}", opp.context.muted());

        // Interpretation (root cause analysis)
        if !opp.interpretation.is_empty() {
            println!();
            println!("   {}", "Interpretation:".strong());
            for line in wrap_text(&opp.interpretation, 80) {
                println!("   {}", line);
            }
        }

        // Specific actions
        if !opp.specific_actions.is_empty() {
            println!();
            println!("   {}", "Actions:".strong());
            for (j, action) in opp.specific_actions.iter().enumerate() {
                println!("   {}. {}", j + 1, action);
            }
        }

        println!();
        println!("   {}", format!("Impact: +{:.0} clicks | Effort: {}", opp.estimated_clicks, opp.opportunity_type.effort_label()).muted());
        println!();
    }

    // Action Plan
    print_action_plan(&report.action_plan);

    print_insights(&report.insights);
}

pub fn print_action_plan(plan: &crate::domain::ActionPlan) {
    if plan.quick_wins.is_empty() && plan.strategic.is_empty() && plan.monitoring.is_empty() {
        return;
    }

    println!("{}\n", "ACTION PLAN".heading());

    if !plan.quick_wins.is_empty() {
        println!("{}", "Quick Wins (this week)".ok());
        for (i, a) in plan.quick_wins.iter().enumerate() {
            println!("  {}. {} [{}]", i + 1, a.action, a.diagnosis.warn());
            println!("     {}", a.reason.muted());
        }
        println!();
    }

    if !plan.strategic.is_empty() {
        println!("{}", "Strategic (this month)".accent());
        for (i, a) in plan.strategic.iter().enumerate() {
            println!("  {}. {} [{}]", i + 1, a.action, a.diagnosis.warn());
            println!("     {}", a.reason.muted());
        }
        println!();
    }

    if !plan.monitoring.is_empty() {
        println!("{}", "Monitoring (in 2-4 weeks)".muted());
        for (i, a) in plan.monitoring.iter().enumerate() {
            println!("  {}. {} [{}]", i + 1, a.action, a.diagnosis.warn());
            println!("     {}", a.reason.muted());
        }
        println!();
    }
}

// ─── Queries report ──────────────────────────────────────────────────────────

pub fn print_queries(report: &QueriesReport) {
    println!("\n{}", "SEARCH QUERIES".heading());
    println!("Property: {}  |  Period: {}\n", report.property_name.accent(), report.date_range);

    let mut summary = traffic_table();
    summary.add_row(vec!["Total clicks", &format_f64(report.total_clicks)]);
    summary.add_row(vec!["Total impressions", &format_f64(report.total_impressions)]);
    summary.add_row(vec!["Avg. CTR", &format!("{:.1}%", report.avg_ctr * 100.0)]);
    summary.add_row(vec!["Avg. Position (weighted)", &format!("{:.1}", report.avg_position)]);
    if report.brand_clicks > 0.0 || report.non_brand_clicks > 0.0 {
        let brand_pct = if report.total_clicks > 0.0 {
            report.brand_clicks / report.total_clicks * 100.0
        } else { 0.0 };
        summary.add_row(vec![
            "Brand / Non-Brand",
            &format!("{:.0} ({:.0}%) / {:.0} ({:.0}%)",
                report.brand_clicks, brand_pct,
                report.non_brand_clicks, 100.0 - brand_pct)
        ]);
    }
    println!("{summary}\n");

    let mut table = Table::new();
    table.set_header(vec![
        Cell::new("#"),
        Cell::new("Query"),
        Cell::new("Clicks"),
        Cell::new("Impressions"),
        Cell::new("CTR"),
        Cell::new("Position"),
        Cell::new("Signal"),
    ]);

    for (i, q) in report.queries.iter().enumerate() {
        let signal = query_signal(q);
        table.add_row(vec![
            Cell::new(i + 1).set_alignment(CellAlignment::Right),
            Cell::new(shorten_url(&q.query, 40)),
            Cell::new(format_f64(q.clicks)).set_alignment(CellAlignment::Right),
            Cell::new(format_f64(q.impressions)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.1}%", q.ctr * 100.0)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.1}", q.position)).set_alignment(CellAlignment::Right),
            Cell::new(signal),
        ]);
    }
    println!("{table}\n");

    print_insights(&report.insights);
}

fn query_signal(q: &crate::domain::QueryRow) -> String {
    use crate::opportunities::expected_ctr;

    if q.position <= 10.0 && q.impressions >= 50.0 && q.ctr < expected_ctr(q.position) * 0.7 {
        "CTR-Fix".warn()
    } else if q.position > 10.0 && q.position <= 20.0 && q.impressions >= 100.0 {
        "Push".accent()
    } else if q.ctr > 0.05 && q.clicks > 20.0 {
        "Strong".ok()
    } else {
        String::new()
    }
}

fn weak_page_signal(page: &crate::domain::PageSummary) -> String {
    use crate::opportunities::expected_ctr;

    if page.search.impressions > 200.0
        && page.search.average_position > 0.0
        && page.search.average_position <= 10.0
        && page.search.ctr < expected_ctr(page.search.average_position) * 0.7
        && page.engagement_rate < 0.3
    {
        "Intent mismatch".warn()
    } else if page.search.impressions > 200.0
        && page.search.average_position > 0.0
        && page.search.average_position <= 10.0
        && page.search.ctr < expected_ctr(page.search.average_position) * 0.7
    {
        "Snippet issue".warn()
    } else if page.search.average_position > 10.0
        && page.search.average_position <= 20.0
        && page.search.impressions > 200.0
    {
        "Ranking issue".accent()
    } else if page.engagement_rate < 0.3 && page.sessions >= 20 {
        "Weak engagement".err()
    } else if page.bounce_rate > 0.7 && page.sessions >= 20 {
        "High bounce".err()
    } else {
        "Needs review".muted()
    }
}

// ─── AI Traffic report ───────────────────────────────────────────────────────

pub fn print_ai_traffic(report: &AiTrafficReport) {
    println!("\n{}", "AI TRAFFIC ANALYSIS".heading());
    println!("Property: {}  |  Period: {}\n", report.property_name.accent(), report.date_range);

    let mut summary = traffic_table();
    summary.add_row(vec!["Total sessions", &format_number(report.total_sessions)]);
    summary.add_row(vec!["AI Sessions", &format_number(report.ai_sessions)]);
    summary.add_row(vec!["AI Share", &format!("{:.2}%", report.ai_share_pct)]);
    if report.prev_ai_sessions > 0 {
        let trend_str = if report.ai_trend_pct > 0.0 {
            format!("{:+.0}% (from {})", report.ai_trend_pct, report.prev_ai_sessions).ok()
        } else if report.ai_trend_pct < 0.0 {
            format!("{:+.0}% (from {})", report.ai_trend_pct, report.prev_ai_sessions).err()
        } else {
            format!("±0% ({})", report.prev_ai_sessions)
        };
        summary.add_row(vec!["Trend vs. previous period", &trend_str]);
    }
    if report.ai_engagement_rate > 0.0 {
        summary.add_row(vec![
            "Engagement (AI)",
            &format!("{:.0}%", report.ai_engagement_rate * 100.0),
        ]);
        summary.add_row(vec![
            "Engagement (Overall)",
            &format!("{:.0}%", report.overall_engagement_rate * 100.0),
        ]);
    }
    println!("{summary}\n");

    if !report.ai_sources.is_empty() {
        println!("{}", "AI SOURCES".heading());
        let mut table = Table::new();
        table.set_header(vec![
            Cell::new("Source"),
            Cell::new("Sessions"),
            Cell::new("Share of AI"),
        ]);
        let ai_total = report.ai_sessions;
        for src in &report.ai_sources {
            let share = if ai_total > 0 { src.sessions as f64 / ai_total as f64 * 100.0 } else { 0.0 };
            table.add_row(vec![
                Cell::new(&src.source),
                Cell::new(format_number(src.sessions)).set_alignment(CellAlignment::Right),
                Cell::new(format!("{:.1}%", share)).set_alignment(CellAlignment::Right),
            ]);
        }
        println!("{table}\n");
    }

    if !report.ai_pages.is_empty() {
        println!("{}", "PAGES WITH AI TRAFFIC".heading());
        let mut table = Table::new();
        table.set_header(vec![
            Cell::new("#"),
            Cell::new("Page"),
            Cell::new("Sessions"),
            Cell::new("Share"),
        ]);
        for (i, page) in report.ai_pages.iter().enumerate() {
            table.add_row(vec![
                Cell::new(i + 1).set_alignment(CellAlignment::Right),
                Cell::new(shorten_url(&page.url, 55)),
                Cell::new(format_number(page.sessions)).set_alignment(CellAlignment::Right),
                Cell::new(format!("{:.0}%", page.share_of_ai * 100.0)).set_alignment(CellAlignment::Right),
            ]);
        }
        println!("{table}\n");
    }

    // Content pattern analysis
    if let Some(pattern) = &report.content_pattern {
        println!("{}", "CONTENT PATTERN".heading());
        for line in wrap_text(pattern, 90) {
            println!("  {}", line);
        }
        println!();
    }

    // Actionable recommendations
    if !report.recommendations.is_empty() {
        println!("{}", "RECOMMENDATIONS".heading());
        for (i, rec) in report.recommendations.iter().enumerate() {
            println!("  {}. {}", i + 1, rec);
        }
        println!();
    }

    print_insights(&report.insights);
}

// ─── Channels report ─────────────────────────────────────────────────────────

pub fn print_channels(report: &ChannelsReport) {
    println!("\n{}", "CHANNEL ANALYSIS".heading());
    println!(
        "Property: {}  |  Period: {}  |  Sessions: {}\n",
        report.property_name.accent(),
        report.date_range,
        format_number(report.total_sessions)
    );

    let mut table = Table::new();
    table.set_header(vec![
        Cell::new("Channel"),
        Cell::new("Sessions"),
        Cell::new("Share"),
        Cell::new("Engagement"),
        Cell::new("Avg. Duration"),
    ]);

    for ch in &report.channels {
        table.add_row(vec![
            Cell::new(&ch.channel),
            Cell::new(format_number(ch.sessions)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.1}%", ch.share_pct)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.0}%", ch.engagement_rate * 100.0)).set_alignment(CellAlignment::Right),
            Cell::new(format_duration(ch.avg_session_duration_secs)).set_alignment(CellAlignment::Right),
        ]);
    }
    println!("{table}\n");

    print_insights(&report.insights);
}

// ─── Clusters report ────────────────────────────────────────────────────────

pub fn print_clusters(report: &ClustersReport) {
    println!("\n{}", "TOPIC CLUSTERS".heading());
    println!(
        "Property: {}  |  Period: {}\n",
        report.property_name.accent(),
        report.date_range
    );

    if report.clusters.is_empty() {
        println!("{}\n", "No topic clusters detected.".muted());
    } else {
        let mut table = Table::new();
        table.set_header(vec![
            Cell::new("Cluster"),
            Cell::new("Pages"),
            Cell::new("Queries"),
            Cell::new("Sessions"),
            Cell::new("Clicks"),
            Cell::new("Impressions"),
            Cell::new("CTR"),
            Cell::new("Avg. Position"),
            Cell::new("CTR Potential"),
        ]);

        for c in &report.clusters {
            table.add_row(vec![
                Cell::new(&c.name),
                Cell::new(c.pages).set_alignment(CellAlignment::Right),
                Cell::new(c.queries).set_alignment(CellAlignment::Right),
                Cell::new(format_number(c.sessions)).set_alignment(CellAlignment::Right),
                Cell::new(format_f64(c.clicks)).set_alignment(CellAlignment::Right),
                Cell::new(format_f64(c.impressions)).set_alignment(CellAlignment::Right),
                Cell::new(format!("{:.1}%", c.ctr * 100.0)).set_alignment(CellAlignment::Right),
                Cell::new(if c.avg_position > 0.0 {
                    format!("{:.1}", c.avg_position)
                } else {
                    "-".into()
                })
                .set_alignment(CellAlignment::Right),
                Cell::new(if c.ctr_potential > 0.0 {
                    format!("+{:.0}", c.ctr_potential)
                } else {
                    "-".into()
                })
                .set_alignment(CellAlignment::Right),
            ]);
        }

        println!("{table}\n");

        // Strategic interpretation per top cluster
        println!("{}", "CLUSTER ANALYSIS".heading());
        for c in report.clusters.iter().take(5) {
            println!("  {} {}", "▸".accent(), c.name.strong());

            // Role assessment
            let role = if c.sessions > 0 && c.queries >= 5 {
                "Established topic with search demand and traffic"
            } else if c.queries >= 5 && c.sessions == 0 {
                "Search demand exists but traffic has not materialized"
            } else if c.pages >= 3 {
                "Content hub with multiple pages"
            } else {
                "Emerging topic"
            };
            println!("    Role: {}", role);

            // Strength / weakness
            if c.avg_position > 0.0 && c.avg_position <= 10.0 && c.ctr > 0.03 {
                println!("    {} Good search position and CTR — this cluster performs well", "✓".ok());
            } else if c.avg_position > 0.0 && c.avg_position <= 10.0 && c.ctr < 0.03 && c.impressions > 100.0 {
                println!("    {} Ranks on page 1 but CTR is below average — snippet optimization opportunity", "⚠".warn());
            } else if c.avg_position > 10.0 && c.impressions > 200.0 {
                println!("    {} High visibility on page 2+ — push into top 10 with deeper content", "⚠".warn());
            }

            // Hub opportunity
            if c.pages <= 1 && c.queries >= 5 {
                println!("    {} {} queries but only {} page — content hub expansion recommended", "→".accent(), c.queries, c.pages);
            }

            // CTR potential
            if c.ctr_potential > 10.0 {
                println!(
                    "    {} ~{:.0} additional clicks possible at optimal CTR",
                    "→".accent(), c.ctr_potential
                );
            }

            println!();
        }
    }

    print_insights(&report.insights);
}

// ─── Decay report ────────────────────────────────────────────────────────────

pub fn print_decay(report: &DecayReport) {
    println!("\n{}", "CONTENT DECAY".heading());
    println!("Property: {}  |  Period: {}\n", report.property_name.accent(), report.date_range);

    if report.declining_pages.is_empty() {
        println!("No significant content decay detected.\n");
        print_insights(&report.insights);
        return;
    }

    let mut table = Table::new();
    table.set_header(vec![
        Cell::new("#"),
        Cell::new("Page"),
        Cell::new("Clicks\nbefore"),
        Cell::new("Clicks\nafter"),
        Cell::new("Δ Clicks"),
        Cell::new("Impr.\nbefore"),
        Cell::new("Impr.\nafter"),
        Cell::new("Δ Impr."),
        Cell::new("Pos.\nbefore"),
        Cell::new("Pos.\nafter"),
        Cell::new("Δ Pos."),
    ]);

    for (i, page) in report.declining_pages.iter().enumerate() {
        let clicks_delta = format!("{:+.1}%", page.clicks_pct);
        let impr_delta = format!("{:+.1}%", page.impressions_pct);
        let pos_delta = if page.position_delta == 0.0 && page.position_after == 0.0 {
            "—".to_string()
        } else {
            format!("{:+.1}", page.position_delta)
        };

        table.add_row(vec![
            Cell::new(i + 1).set_alignment(CellAlignment::Right),
            Cell::new(shorten_url(&page.url, 45)),
            Cell::new(format!("{:.0}", page.clicks_before)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.0}", page.clicks_after)).set_alignment(CellAlignment::Right),
            Cell::new(clicks_delta.err()).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.0}", page.impressions_before)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.0}", page.impressions_after)).set_alignment(CellAlignment::Right),
            Cell::new(impr_delta.err()).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.1}", page.position_before)).set_alignment(CellAlignment::Right),
            Cell::new(if page.position_after > 0.0 { format!("{:.1}", page.position_after) } else { "—".into() }).set_alignment(CellAlignment::Right),
            Cell::new(pos_delta).set_alignment(CellAlignment::Right),
        ]);
    }
    println!("{table}\n");

    print_insights(&report.insights);
}

// ─── Devices report ─────────────────────────────────────────────────────────

pub fn print_devices(report: &DevicesReport) {
    println!("\n{}", "DEVICES".heading());
    println!(
        "Property: {}  |  Period: {}  |  Sessions: {}\n",
        report.property_name.accent(),
        report.date_range,
        format_number(report.total_sessions)
    );

    let mut table = Table::new();
    table.set_header(vec![
        Cell::new("Device"),
        Cell::new("Sessions"),
        Cell::new("Share"),
        Cell::new("Engagement"),
        Cell::new("Avg. Duration"),
    ]);

    for d in &report.devices {
        table.add_row(vec![
            Cell::new(&d.device),
            Cell::new(format_number(d.sessions)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.1}%", d.share_pct)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.0}%", d.engagement_rate * 100.0)).set_alignment(CellAlignment::Right),
            Cell::new(format_duration(d.avg_session_duration_secs)).set_alignment(CellAlignment::Right),
        ]);
    }
    println!("{table}\n");

    print_insights(&report.insights);
}

// ─── Countries report ───────────────────────────────────────────────────────

pub fn print_countries(report: &CountriesReport) {
    println!("\n{}", "COUNTRIES".heading());
    println!(
        "Property: {}  |  Period: {}  |  Sessions: {}\n",
        report.property_name.accent(),
        report.date_range,
        format_number(report.total_sessions)
    );

    let mut table = Table::new();
    table.set_header(vec![
        Cell::new("#"),
        Cell::new("Country"),
        Cell::new("Sessions"),
        Cell::new("Share"),
        Cell::new("Engagement"),
    ]);

    for (i, c) in report.countries.iter().enumerate() {
        table.add_row(vec![
            Cell::new(i + 1).set_alignment(CellAlignment::Right),
            Cell::new(&c.country),
            Cell::new(format_number(c.sessions)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.1}%", c.share_pct)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.0}%", c.engagement_rate * 100.0)).set_alignment(CellAlignment::Right),
        ]);
    }
    println!("{table}\n");

    print_insights(&report.insights);
}

// ─── Growth Drivers report ─────────────────────────────────────────────────

pub fn print_growth(report: &GrowthReport) {
    println!("\n{}", "GROWTH DRIVERS".heading());
    println!("Property: {}  |  Period: {}\n", report.property_name.accent(), report.date_range);

    if !report.top_growing_pages.is_empty() {
        println!("{}", "Top growing pages".ok());
        let mut table = traffic_table();
        table.set_header(vec![
            Cell::new("Page"), Cell::new("Current"), Cell::new("Previous"), Cell::new("Delta"),
        ]);
        for r in &report.top_growing_pages {
            table.add_row(vec![
                Cell::new(shorten_url(&r.label, 50)),
                Cell::new(format_f64(r.current)).set_alignment(CellAlignment::Right),
                Cell::new(format_f64(r.previous)).set_alignment(CellAlignment::Right),
                Cell::new(format!("{:+.0} ({:+.0}%)", r.delta, r.delta_pct))
                    .set_alignment(CellAlignment::Right),
            ]);
        }
        println!("{table}\n");
    }

    if !report.top_declining_pages.is_empty() {
        println!("{}", "Top declining pages".err());
        let mut table = traffic_table();
        table.set_header(vec![
            Cell::new("Page"), Cell::new("Current"), Cell::new("Previous"), Cell::new("Delta"),
        ]);
        for r in &report.top_declining_pages {
            table.add_row(vec![
                Cell::new(shorten_url(&r.label, 50)),
                Cell::new(format_f64(r.current)).set_alignment(CellAlignment::Right),
                Cell::new(format_f64(r.previous)).set_alignment(CellAlignment::Right),
                Cell::new(format!("{:+.0} ({:+.0}%)", r.delta, r.delta_pct))
                    .set_alignment(CellAlignment::Right),
            ]);
        }
        println!("{table}\n");
    }

    if !report.top_growing_queries.is_empty() {
        println!("{}", "Top growing search queries".ok());
        let mut table = traffic_table();
        table.set_header(vec![
            Cell::new("Query"), Cell::new("Clicks current"), Cell::new("Clicks previous"), Cell::new("Delta"),
        ]);
        for r in report.top_growing_queries.iter().take(10) {
            table.add_row(vec![
                Cell::new(shorten_url(&r.label, 40)),
                Cell::new(format_f64(r.current)).set_alignment(CellAlignment::Right),
                Cell::new(format_f64(r.previous)).set_alignment(CellAlignment::Right),
                Cell::new(format!("{:+.0}", r.delta)).set_alignment(CellAlignment::Right),
            ]);
        }
        println!("{table}\n");
    }

    if !report.new_queries.is_empty() {
        println!("{} {} new queries discovered\n", "i".accent(), report.new_queries.len());
    }

    if !report.channel_growth.is_empty() {
        println!("{}", "Channel growth".strong());
        let mut table = traffic_table();
        table.set_header(vec![
            Cell::new("Channel"), Cell::new("Current"), Cell::new("Previous"), Cell::new("Delta"),
        ]);
        for c in &report.channel_growth {
            table.add_row(vec![
                Cell::new(&c.channel),
                Cell::new(format_number(c.current_sessions)).set_alignment(CellAlignment::Right),
                Cell::new(format_number(c.previous_sessions)).set_alignment(CellAlignment::Right),
                Cell::new(format!("{:+} ({:+.0}%)", c.delta, c.delta_pct))
                    .set_alignment(CellAlignment::Right),
            ]);
        }
        println!("{table}\n");
    }

    print_insights(&report.insights);
}

// ─── Trends report ─────────────────────────────────────────────────────────

pub fn print_trends(report: &TrendsReport) {
    println!("\n{}", "WEEKLY TRENDS".heading());
    println!("Property: {}  |  Period: {}\n", report.property_name.accent(), report.date_range);

    if !report.weeks.is_empty() {
        let mut table = traffic_table();
        table.set_header(vec![
            Cell::new("Week"), Cell::new("Sessions"), Cell::new("Clicks"),
            Cell::new("Impressions"), Cell::new("CTR"), Cell::new("Position"),
        ]);
        for w in &report.weeks {
            table.add_row(vec![
                Cell::new(&w.week_start),
                Cell::new(format_number(w.sessions)).set_alignment(CellAlignment::Right),
                Cell::new(format_f64(w.clicks)).set_alignment(CellAlignment::Right),
                Cell::new(format_f64(w.impressions)).set_alignment(CellAlignment::Right),
                Cell::new(format!("{:.1}%", w.ctr * 100.0)).set_alignment(CellAlignment::Right),
                Cell::new(format!("{:.1}", w.avg_position)).set_alignment(CellAlignment::Right),
            ]);
        }
        println!("{table}\n");
    }

    if !report.ranking_jumps.is_empty() {
        println!("{}", "Ranking jumps (>5 positions)".strong());
        let mut table = traffic_table();
        table.set_header(vec![
            Cell::new("Query"), Cell::new("Current"), Cell::new("Previous"), Cell::new("Delta"),
        ]);
        for r in &report.ranking_jumps {
            let delta_str = if r.delta < 0.0 {
                format!("{:+.1}", r.delta).ok()
            } else {
                format!("{:+.1}", r.delta).err()
            };
            table.add_row(vec![
                Cell::new(shorten_url(&r.label, 40)),
                Cell::new(format!("{:.1}", r.current)).set_alignment(CellAlignment::Right),
                Cell::new(format!("{:.1}", r.previous)).set_alignment(CellAlignment::Right),
                Cell::new(delta_str).set_alignment(CellAlignment::Right),
            ]);
        }
        println!("{table}\n");
    }

    print_insights(&report.insights);
}

// ─── Text wrapping ─────────────────────────────────────────────────────────

fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();

    for word in text.split_whitespace() {
        if current_line.is_empty() {
            current_line = word.to_string();
        } else if current_line.len() + 1 + word.len() > max_width {
            lines.push(current_line);
            current_line = word.to_string();
        } else {
            current_line.push(' ');
            current_line.push_str(word);
        }
    }
    if !current_line.is_empty() {
        lines.push(current_line);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Insight;
    use runemark::{ColorMode, Console};

    fn truncation(what: &str, returned: usize, total: Option<i64>) -> crate::google::api::Truncation {
        crate::google::api::Truncation {
            what: what.into(),
            returned,
            total,
        }
    }

    /// One command runs several reports that ask overlapping questions, so the
    /// same dimensions show up more than once. The reader should be told once,
    /// about the worst case.
    #[test]
    fn repeated_dimensions_collapse_to_the_worst_case() {
        let all = vec![
            truncation("pagePath", 200, Some(4_000)),
            truncation("pagePath", 500, Some(18_400)),
            truncation("page+query", 2_500, None),
        ];

        let worst = worst_per_dimension(&all);

        assert_eq!(worst.len(), 2);
        let pages = worst.iter().find(|t| t.what == "pagePath").unwrap();
        assert_eq!(pages.total, Some(18_400));
        assert_eq!(pages.returned, 500);
    }

    /// Search Console reports no total, so those entries have nothing to
    /// compare — the first one stands rather than being dropped.
    #[test]
    fn entries_without_a_total_survive() {
        let all = vec![truncation("query", 500, None), truncation("query", 500, None)];

        let worst = worst_per_dimension(&all);

        assert_eq!(worst.len(), 1);
        assert_eq!(worst[0].total, None);
    }

    fn insight(severity: InsightSeverity, category: InsightCategory, headline: &str) -> Insight {
        Insight {
            severity,
            category,
            headline: headline.into(),
            explanation: format!("why {headline}"),
        }
    }

    #[test]
    fn worst_severity_decides_the_verdict() {
        let mixed = vec![
            insight(InsightSeverity::Positive, InsightCategory::Traffic, "a"),
            insight(InsightSeverity::Warning, InsightCategory::Search, "b"),
            insight(InsightSeverity::Critical, InsightCategory::Search, "c"),
        ];
        assert_eq!(overall_verdict(&mixed), Verdict::Failed);

        assert_eq!(overall_verdict(&mixed[..2]), Verdict::Warning);
        assert_eq!(overall_verdict(&mixed[..1]), Verdict::Passed);
        assert_eq!(
            overall_verdict(&[insight(
                InsightSeverity::Info,
                InsightCategory::Trend,
                "d"
            )]),
            Verdict::Info
        );
        assert_eq!(overall_verdict(&[]), Verdict::Info);
    }

    /// The rendered block must group by category and keep every explanation —
    /// the explanation is the part a reader acts on.
    #[test]
    fn insights_render_grouped_with_explanations() {
        let insights = vec![
            insight(InsightSeverity::Warning, InsightCategory::Search, "low ctr"),
            insight(InsightSeverity::Info, InsightCategory::Traffic, "direct share"),
            insight(InsightSeverity::Warning, InsightCategory::Search, "thin snippets"),
        ];

        let report = build_insight_report(&insights);
        let plain = report.render(Console::new(ColorMode::Never, false));

        assert!(plain.contains("Search"), "{plain}");
        assert!(plain.contains("Traffic"), "{plain}");
        assert!(plain.contains("low ctr"), "{plain}");
        assert!(plain.contains("why low ctr"), "{plain}");
        // Search appears first and carries both of its findings.
        assert!(plain.find("Search") < plain.find("Traffic"), "{plain}");
        assert!(plain.contains("thin snippets"), "{plain}");
    }
}
