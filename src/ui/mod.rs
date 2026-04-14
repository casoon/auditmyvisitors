use colored::Colorize;
use comfy_table::{Cell, CellAlignment, Table};

use crate::domain::{
    AiTrafficReport, ChannelsReport, ComparisonReport, CountriesReport, DecayReport,
    DevicesReport, InsightSeverity, OpportunitiesReport, PageDetailReport, QueriesReport,
    SiteOverviewReport, TopPagesReport,
};

// ─── Welcome ────────────────────────────────────────────────────────────────

pub fn print_welcome() {
    println!();
    println!("{}", "auditmyvisitors".bold().cyan());
    println!("{}", "Google Analytics 4 & Search Console Reporting".dimmed());
    println!();
}

// ─── Auth ─────────────────────────────────────────────────────────────────────

pub fn print_auth_status(status: &crate::auth::AuthStatus) {
    match status {
        crate::auth::AuthStatus::LoggedIn => {
            println!("{} Eingeloggt — Token ist gültig.", "✓".green().bold());
        }
        crate::auth::AuthStatus::TokenExpired => {
            println!(
                "{} Token abgelaufen — wird beim nächsten API-Aufruf automatisch erneuert.",
                "⚠".yellow().bold()
            );
        }
        crate::auth::AuthStatus::NotLoggedIn => {
            println!(
                "{} Nicht eingeloggt. Starte mit: {}",
                "✗".red().bold(),
                "auditmyvisitors auth login".cyan()
            );
        }
    }
}

// ─── Snapshot comparison ────────────────────────────────────────────────────

pub fn print_snapshot_comparison(
    prev: &crate::snapshots::Snapshot,
    report: &SiteOverviewReport,
) {
    println!("{}", "VERGLEICH MIT LETZTEM SNAPSHOT".bold().underline());
    println!("Letzter Snapshot: {} ({} Tage)\n", prev.date.cyan(), prev.days);

    let sess_pct = crate::helpers::pct_change(prev.sessions as f64, report.traffic.total_sessions as f64);
    let clicks_pct = crate::helpers::pct_change(prev.clicks, report.search.clicks);
    let impr_pct = crate::helpers::pct_change(prev.impressions, report.search.impressions);

    let sess = fmt_trend_colored(sess_pct, false);
    let clicks = fmt_trend_colored(clicks_pct, false);
    let impr = fmt_trend_colored(impr_pct, false);

    println!("  Sessions {}  |  Klicks {}  |  Impressionen {}\n", sess, clicks, impr);
}

// ─── Overview ────────────────────────────────────────────────────────────────

pub fn print_overview(report: &SiteOverviewReport) {
    println!("\n{}", "ÜBERSICHT".bold().underline());
    println!("Property: {}  |  Zeitraum: {}\n", report.property_name.cyan(), report.date_range);

    // ── Trend summary (if available) ─────────────────────────────────────────
    if let Some(trend) = &report.trend {
        let sess = fmt_trend_colored(trend.sessions_pct, false);
        let clicks = fmt_trend_colored(trend.clicks_pct, false);
        let impr = fmt_trend_colored(trend.impressions_pct, false);
        println!(
            "{}  Sessions {}  |  Klicks {}  |  Impressionen {}",
            "TREND".bold(), sess, clicks, impr
        );
        println!();
    }

    let mut table = traffic_table();
    table.add_row(vec![
        Cell::new("Sitzungen gesamt"),
        Cell::new(format_number(report.traffic.total_sessions)).set_alignment(CellAlignment::Right),
    ]);
    table.add_row(vec![
        Cell::new("  davon organisch"),
        Cell::new(format!(
            "{} ({:.0}%)",
            format_number(report.traffic.organic_sessions),
            report.traffic.organic_share()
        )).set_alignment(CellAlignment::Right),
    ]);
    table.add_row(vec![
        Cell::new("  davon direkt"),
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
        println!("{}", "TOP QUELLEN".bold().underline());
        let mut src_table = Table::new();
        src_table.set_header(vec![
            Cell::new("Quelle"),
            Cell::new("Sessions"),
            Cell::new("Anteil"),
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
            "AI-TRAFFIC".bold().underline(),
            format_number(ai_total),
            ai_pct
        );
        for src in &report.ai_sources {
            println!("  {} — {} Sessions", src.source, format_number(src.sessions));
        }
        println!();
    }

    println!("{}", "SEARCH CONSOLE".bold().underline());
    let mut sc = traffic_table();
    sc.add_row(vec!["Klicks", &format_f64(report.search.clicks)]);
    sc.add_row(vec!["Impressionen", &format_f64(report.search.impressions)]);
    sc.add_row(vec!["CTR", &format!("{:.1}%", report.search.ctr * 100.0)]);
    sc.add_row(vec!["Ø Position", &format!("{:.1}", report.search.average_position)]);
    println!("{sc}\n");

    // ── Top opportunities (max 5) ────────────────────────────────────────────
    if !report.opportunities.is_empty() {
        println!("{}", "TOP OPPORTUNITIES".bold().underline());
        for (i, opp) in report.opportunities.iter().take(5).enumerate() {
            let kw = opp.keyword.as_deref()
                .or(if opp.url.is_empty() { None } else { Some(opp.url.as_str()) })
                .unwrap_or("-");
            println!(
                "  {}. {} — \"{}\" (+{:.0} Klicks, {})",
                i + 1,
                opp.type_labels.join(" + ").yellow(),
                kw,
                opp.estimated_clicks,
                opp.opportunity_type.effort_label()
            );
        }
        let total: f64 = report.opportunities.iter().map(|o| o.estimated_clicks).sum();
        if total > 0.0 {
            println!("  Geschaetztes Gesamt-Potenzial: {} Klicks/Monat", format!("+{:.0}", total).green());
        }
        println!();
    }

    print_insights(&report.insights);
}

// ─── Top Pages ───────────────────────────────────────────────────────────────

pub fn print_top_pages(report: &TopPagesReport) {
    println!("\n{}", "TOP PAGES".bold().underline());
    println!("Property: {}  |  Zeitraum: {}\n", report.property_name.cyan(), report.date_range);

    let mut table = Table::new();
    table.set_header(vec![
        Cell::new("#"),
        Cell::new("Seite"),
        Cell::new("Sessions"),
        Cell::new("Organisch"),
        Cell::new("Direkt"),
        Cell::new("Engagement"),
        Cell::new("Klicks"),
        Cell::new("Impressionen"),
        Cell::new("CTR"),
        Cell::new("Position"),
    ]);

    for (i, page) in report.pages.iter().enumerate() {
        let short_url = shorten_url(&page.url, 50);
        table.add_row(vec![
            Cell::new(i + 1).set_alignment(CellAlignment::Right),
            Cell::new(&short_url),
            Cell::new(format_number(page.sessions)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.0}%", if page.sessions > 0 { page.organic_sessions as f64 / page.sessions as f64 * 100.0 } else { 0.0 })).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.0}%", if page.sessions > 0 { page.direct_sessions as f64 / page.sessions as f64 * 100.0 } else { 0.0 })).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.0}%", page.engagement_rate * 100.0)).set_alignment(CellAlignment::Right),
            Cell::new(format_f64(page.search.clicks)).set_alignment(CellAlignment::Right),
            Cell::new(format_f64(page.search.impressions)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.1}%", page.search.ctr * 100.0)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.1}", page.search.average_position)).set_alignment(CellAlignment::Right),
        ]);
    }

    println!("{table}\n");
    print_insights(&report.insights);
}

// ─── Page detail ─────────────────────────────────────────────────────────────

pub fn print_page_detail(report: &PageDetailReport) {
    println!("\n{}", "SEITEN-DETAIL".bold().underline());
    println!("URL: {}  |  Zeitraum: {}\n", report.url.cyan(), report.date_range);

    let mut table = traffic_table();
    table.add_row(vec!["Sitzungen gesamt", &format_number(report.traffic.total_sessions)]);
    table.add_row(vec!["Organisch", &format!("{} ({:.0}%)", format_number(report.traffic.organic_sessions), report.traffic.organic_share())]);
    table.add_row(vec!["Direkt", &format!("{} ({:.0}%)", format_number(report.traffic.direct_sessions), report.traffic.direct_share())]);
    table.add_row(vec!["Engagement Rate", &format!("{:.1}%", report.engagement_rate * 100.0)]);
    table.add_row(vec!["Ø Sitzungsdauer", &format_duration(report.avg_session_duration_secs)]);
    println!("{table}\n");

    println!("{}", "SEARCH CONSOLE".bold().underline());
    let mut sc = traffic_table();
    sc.add_row(vec!["Klicks", &format_f64(report.search.clicks)]);
    sc.add_row(vec!["Impressionen", &format_f64(report.search.impressions)]);
    sc.add_row(vec!["CTR", &format!("{:.1}%", report.search.ctr * 100.0)]);
    sc.add_row(vec!["Ø Position", &format!("{:.1}", report.search.average_position)]);
    println!("{sc}\n");

    if !report.search.top_queries.is_empty() {
        println!("{}", "TOP KEYWORDS".bold().underline());
        let mut qt = Table::new();
        qt.set_header(vec!["Query", "Klicks", "Impressionen", "CTR", "Position"]);
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
    println!("\n{}", "VERGLEICH".bold().underline());
    if let Some(url) = &report.url {
        println!("URL: {}", url.cyan());
    }
    println!("Stichtag: {}  |  Vorher: {} Tage  |  Nachher: {} Tage\n",
        report.change_date.yellow(), report.before_days, report.after_days);

    // ── Verdict line ─────────────────────────────────────────────────────────
    let d = &report.delta;
    let winners: Vec<&str> = [
        (d.sessions_pct >= 10.0, "Sessions"),
        (d.clicks_pct >= 15.0, "Klicks"),
        (d.impressions_pct >= 20.0, "Impressionen"),
        (d.position_abs <= -2.0, "Position"),
    ].iter().filter(|(cond, _)| *cond).map(|(_, l)| *l).collect();

    let losers: Vec<&str> = [
        (d.sessions_pct <= -10.0, "Sessions"),
        (d.clicks_pct <= -15.0, "Klicks"),
        (d.impressions_pct <= -20.0, "Impressionen"),
        (d.position_abs >= 2.0, "Position"),
    ].iter().filter(|(cond, _)| *cond).map(|(_, l)| *l).collect();

    if !winners.is_empty() || !losers.is_empty() {
        if !winners.is_empty() {
            println!("{} {}", "Gewinner:".green().bold(), winners.join(", "));
        }
        if !losers.is_empty() {
            println!("{} {}", "Verlierer:".red().bold(), losers.join(", "));
        }
        println!();
    }

    let mut table = Table::new();
    table.set_header(vec![
        Cell::new("Kennzahl"),
        Cell::new(format!("Vorher\n{}–{}", report.before.start_date, report.before.end_date)),
        Cell::new(format!("Nachher\n{}–{}", report.after.start_date, report.after.end_date)),
        Cell::new("Δ absolut"),
        Cell::new("Δ %"),
    ]);

    add_comparison_row(&mut table, "Sitzungen",
        report.before.sessions as f64, report.after.sessions as f64,
        report.delta.sessions_abs as f64, report.delta.sessions_pct, false);

    add_comparison_row(&mut table, "Organisch",
        report.before.organic_sessions as f64, report.after.organic_sessions as f64,
        report.delta.organic_sessions_abs as f64, report.delta.organic_sessions_pct, false);

    add_comparison_row(&mut table, "Klicks",
        report.before.search.clicks, report.after.search.clicks,
        report.delta.clicks_abs, report.delta.clicks_pct, false);

    add_comparison_row(&mut table, "Impressionen",
        report.before.search.impressions, report.after.search.impressions,
        report.delta.impressions_abs, report.delta.impressions_pct, false);

    add_comparison_row_f64(&mut table, "CTR",
        report.before.search.ctr * 100.0, report.after.search.ctr * 100.0,
        report.delta.ctr_abs * 100.0, "%");

    add_comparison_row_f64(&mut table, "Ø Position",
        report.before.search.average_position, report.after.search.average_position,
        report.delta.position_abs, "");

    println!("{table}\n");

    if !report.summary.is_empty() {
        println!("{}\n{}\n", "ZUSAMMENFASSUNG".bold().underline(), report.summary);
    }

    print_insights(&report.insights);
}

// ─── Insights / Recommendations ──────────────────────────────────────────────

fn print_insights(insights: &[crate::domain::Insight]) {
    if insights.is_empty() {
        return;
    }
    println!("{}", "INSIGHTS".bold().underline());
    for insight in insights {
        let prefix = match insight.severity {
            InsightSeverity::Positive => "✓".green().bold().to_string(),
            InsightSeverity::Info    => "ℹ".blue().bold().to_string(),
            InsightSeverity::Warning => "⚠".yellow().bold().to_string(),
            InsightSeverity::Critical=> "✗".red().bold().to_string(),
        };
        println!("{prefix} {}", insight.headline.bold());
        println!("   {}", insight.explanation);
    }
    println!();
}

fn print_recommendations(recs: &[crate::domain::Recommendation]) {
    if recs.is_empty() {
        return;
    }
    println!("{}", "EMPFEHLUNGEN".bold().underline());
    for rec in recs {
        println!("{}. {}", rec.priority, rec.headline.bold());
        println!("   {}", rec.action);
    }
    println!();
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn traffic_table() -> Table {
    let mut t = Table::new();
    t.set_header(vec![
        Cell::new("Kennzahl"),
        Cell::new("Wert").set_alignment(CellAlignment::Right),
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
        format!("{:+.1}%", delta_pct).green().to_string()
    } else {
        format!("{:+.1}%", delta_pct).red().to_string()
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
        s.green().to_string()
    } else {
        s.red().to_string()
    }
}

// ─── Opportunities report ────────────────────────────────────────────────────

pub fn print_opportunities(report: &OpportunitiesReport) {
    println!("\n{}", "OPPORTUNITIES".bold().underline());
    println!("Property: {}  |  Zeitraum: {}\n", report.property_name.cyan(), report.date_range);

    if report.opportunities.is_empty() {
        println!("Keine signifikanten Opportunities gefunden.\n");
        return;
    }

    println!("{}\n", report.summary);

    let mut table = Table::new();
    table.set_header(vec![
        Cell::new("#"),
        Cell::new("Score"),
        Cell::new("Typ"),
        Cell::new("Keyword / URL"),
        Cell::new("+ Klicks"),
        Cell::new("Aufwand"),
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

    // Detail cards for top 5
    println!("{}", "DETAIL".bold().underline());
    for (i, opp) in report.opportunities.iter().take(5).enumerate() {
        let kw = opp.keyword.as_deref()
            .or(if opp.url.is_empty() { None } else { Some(opp.url.as_str()) })
            .unwrap_or("-");
        println!("{}. {} — \"{}\"", i + 1, opp.type_labels.join(" + ").yellow(), kw);
        println!("   {}", opp.context);
        println!("   {}", opp.action.bold());
        println!();
    }

    print_insights(&report.insights);
}

// ─── Queries report ──────────────────────────────────────────────────────────

pub fn print_queries(report: &QueriesReport) {
    println!("\n{}", "QUERY-ANALYSE".bold().underline());
    println!("Property: {}  |  Zeitraum: {}\n", report.property_name.cyan(), report.date_range);

    let mut summary = traffic_table();
    summary.add_row(vec!["Klicks gesamt", &format_f64(report.total_clicks)]);
    summary.add_row(vec!["Impressionen gesamt", &format_f64(report.total_impressions)]);
    summary.add_row(vec!["Ø CTR", &format!("{:.1}%", report.avg_ctr * 100.0)]);
    summary.add_row(vec!["Ø Position (gewichtet)", &format!("{:.1}", report.avg_position)]);
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
        Cell::new("Klicks"),
        Cell::new("Impressionen"),
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
        "CTR-Fix".yellow().to_string()
    } else if q.position > 10.0 && q.position <= 20.0 && q.impressions >= 100.0 {
        "Push".blue().to_string()
    } else if q.ctr > 0.05 && q.clicks > 20.0 {
        "Stark".green().to_string()
    } else {
        String::new()
    }
}

// ─── AI Traffic report ───────────────────────────────────────────────────────

pub fn print_ai_traffic(report: &AiTrafficReport) {
    println!("\n{}", "AI-TRAFFIC ANALYSE".bold().underline());
    println!("Property: {}  |  Zeitraum: {}\n", report.property_name.cyan(), report.date_range);

    let mut summary = traffic_table();
    summary.add_row(vec!["Sessions gesamt", &format_number(report.total_sessions)]);
    summary.add_row(vec!["AI-Sessions", &format_number(report.ai_sessions)]);
    summary.add_row(vec!["AI-Anteil", &format!("{:.2}%", report.ai_share_pct)]);
    println!("{summary}\n");

    if !report.ai_sources.is_empty() {
        println!("{}", "AI-QUELLEN".bold().underline());
        let mut table = Table::new();
        table.set_header(vec![
            Cell::new("Quelle"),
            Cell::new("Sessions"),
            Cell::new("Anteil an AI"),
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
        println!("{}", "SEITEN MIT AI-TRAFFIC".bold().underline());
        let mut table = Table::new();
        table.set_header(vec![
            Cell::new("#"),
            Cell::new("Seite"),
            Cell::new("Sessions"),
            Cell::new("Anteil"),
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

    print_insights(&report.insights);
}

// ─── Channels report ─────────────────────────────────────────────────────────

pub fn print_channels(report: &ChannelsReport) {
    println!("\n{}", "KANAL-ANALYSE".bold().underline());
    println!(
        "Property: {}  |  Zeitraum: {}  |  Sessions: {}\n",
        report.property_name.cyan(),
        report.date_range,
        format_number(report.total_sessions)
    );

    let mut table = Table::new();
    table.set_header(vec![
        Cell::new("Kanal"),
        Cell::new("Sessions"),
        Cell::new("Anteil"),
        Cell::new("Engagement"),
        Cell::new("Ø Dauer"),
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

// ─── Decay report ────────────────────────────────────────────────────────────

pub fn print_decay(report: &DecayReport) {
    println!("\n{}", "CONTENT DECAY".bold().underline());
    println!("Property: {}  |  Zeitraum: {}\n", report.property_name.cyan(), report.date_range);

    if report.declining_pages.is_empty() {
        println!("Kein signifikanter Content Decay erkannt.\n");
        print_insights(&report.insights);
        return;
    }

    let mut table = Table::new();
    table.set_header(vec![
        Cell::new("#"),
        Cell::new("Seite"),
        Cell::new("Klicks\nvorher"),
        Cell::new("Klicks\nnachher"),
        Cell::new("Δ Klicks"),
        Cell::new("Impr.\nvorher"),
        Cell::new("Impr.\nnachher"),
        Cell::new("Δ Impr."),
        Cell::new("Pos.\nvorher"),
        Cell::new("Pos.\nnachher"),
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
            Cell::new(clicks_delta.red().to_string()).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.0}", page.impressions_before)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.0}", page.impressions_after)).set_alignment(CellAlignment::Right),
            Cell::new(impr_delta.red().to_string()).set_alignment(CellAlignment::Right),
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
    println!("\n{}", "GERÄTE-ANALYSE".bold().underline());
    println!(
        "Property: {}  |  Zeitraum: {}  |  Sessions: {}\n",
        report.property_name.cyan(),
        report.date_range,
        format_number(report.total_sessions)
    );

    let mut table = Table::new();
    table.set_header(vec![
        Cell::new("Gerät"),
        Cell::new("Sessions"),
        Cell::new("Anteil"),
        Cell::new("Engagement"),
        Cell::new("Ø Dauer"),
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
    println!("\n{}", "LÄNDER-ANALYSE".bold().underline());
    println!(
        "Property: {}  |  Zeitraum: {}  |  Sessions: {}\n",
        report.property_name.cyan(),
        report.date_range,
        format_number(report.total_sessions)
    );

    let mut table = Table::new();
    table.set_header(vec![
        Cell::new("#"),
        Cell::new("Land"),
        Cell::new("Sessions"),
        Cell::new("Anteil"),
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
