use colored::Colorize;
use comfy_table::{Cell, CellAlignment, Table};

use crate::domain::{
    ComparisonReport, InsightSeverity, PageDetailReport, SiteOverviewReport, TopPagesReport,
};

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
                "audit-my-visitors auth login".cyan()
            );
        }
    }
}

// ─── Overview ────────────────────────────────────────────────────────────────

pub fn print_overview(report: &SiteOverviewReport) {
    println!("\n{}", "ÜBERSICHT".bold().underline());
    println!("Property: {}  |  Zeitraum: {}\n", report.property_name.cyan(), report.date_range);

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

    println!("{}", "SEARCH CONSOLE".bold().underline());
    let mut sc = traffic_table();
    sc.add_row(vec!["Klicks", &format_f64(report.search.clicks)]);
    sc.add_row(vec!["Impressionen", &format_f64(report.search.impressions)]);
    sc.add_row(vec!["CTR", &format!("{:.1}%", report.search.ctr * 100.0)]);
    sc.add_row(vec!["Ø Position", &format!("{:.1}", report.search.average_position)]);
    println!("{sc}\n");

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

    let mut table = Table::new();
    table.set_header(vec![
        Cell::new("Kennzahl"),
        Cell::new(&format!("Vorher\n{}–{}", report.before.start_date, report.before.end_date)),
        Cell::new(&format!("Nachher\n{}–{}", report.after.start_date, report.after.end_date)),
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
