use anyhow::Context;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};

use crate::config::AppConfig;
use crate::domain::{PageSummary, TopPagesReport};
use crate::{auth, export, narrative, reports, snapshots, ui};

// ─── Time range ─────────────────────────────────────────────────────────────

fn ask_days() -> anyhow::Result<u32> {
    let options = vec![
        "Last 7 days",
        "Last 28 days (default)",
        "Last 90 days",
        "Custom range…",
    ];

    let choice = inquire::Select::new("Select time range:", options)
        .with_starting_cursor(1)
        .prompt()?;

    match choice {
        "Last 7 days" => Ok(7),
        "Last 90 days" => Ok(90),
        "Custom range…" => {
            let days: u32 = inquire::CustomType::new("Number of days:")
                .with_default(28)
                .prompt()?;
            Ok(days)
        }
        _ => Ok(28),
    }
}

// ─── Result limit ───────────────────────────────────────────────────────────

fn ask_limit(label: &str) -> anyhow::Result<usize> {
    let options = vec!["20 (min)", "50", "100", "200 (max)"];
    let choice = inquire::Select::new(label, options)
        .with_starting_cursor(0)
        .prompt()?;
    Ok(match choice {
        "50"       => 50,
        "100"      => 100,
        "200 (max)" => 200,
        _          => 20,
    })
}

// ─── Main menu ──────────────────────────────────────────────────────────────

const MENU_REPORT: &str = "Run full report";
const MENU_PAGE: &str = "Page performance: top / weakest pages";
const MENU_PAGE_DETAIL: &str = "Page detail (single URL)";
const MENU_COMPARE: &str = "Before/after comparison";
const MENU_EXPORT: &str = "Export…";
const MENU_PROPERTY: &str = "Switch property";
const MENU_EXIT: &str = "Exit";

pub async fn report_loop(config: &mut AppConfig) -> anyhow::Result<()> {
    let days = ask_days()?;
    println!();

    // Run the full report immediately on first entry
    let mut token = auth::ensure_valid_token()
        .await
        .context("Failed to refresh token")?;
    run_full_report(config, &token, days).await.unwrap_or_else(|e| {
        eprintln!("\n{} {}\n", "Error:".red().bold(), e);
    });

    loop {
        let menu = vec![
            MENU_REPORT,
            MENU_PAGE,
            MENU_PAGE_DETAIL,
            MENU_COMPARE,
            MENU_EXPORT,
            MENU_PROPERTY,
            MENU_EXIT,
        ];

        let choice = inquire::Select::new("What would you like to do next?", menu).prompt()?;

        // Refresh token before each action
        if choice != MENU_EXIT {
            token = auth::ensure_valid_token()
                .await
                .context("Failed to refresh token")?;
        }

        match choice {
            MENU_REPORT => run_full_report(config, &token, days).await,
            MENU_PAGE => run_page_performance(config, &token, days).await,
            MENU_PAGE_DETAIL => run_page_detail(config, &token, days).await,
            MENU_COMPARE => run_compare(config, &token).await,
            MENU_EXPORT => run_export(config, &token, days).await,
            MENU_PROPERTY => {
                super::setup::ensure_ready(config).await?;
                token = auth::ensure_valid_token().await?;
                Ok(())
            }
            MENU_EXIT => {
                println!("Goodbye!");
                break;
            }
            _ => Ok(()),
        }
        .unwrap_or_else(|e| {
            eprintln!("\n{} {}\n", "Error:".red().bold(), e);
        });
    }

    Ok(())
}

// ─── Full Report ────────────────────────────────────────────────────────────

async fn run_full_report(config: &AppConfig, token: &str, days: u32) -> anyhow::Result<()> {
    // 1. Overview
    let pb = spinner("Loading overview…");
    let overview = reports::overview::build(config, token, days).await?;
    pb.finish_and_clear();
    ui::print_overview(&overview);

    // Snapshot comparison
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    if let Ok(Some(prev)) = snapshots::load_previous(&overview.property_name, &today) {
        ui::print_snapshot_comparison(&prev, &overview);
    }

    // Auto-save snapshot
    let snap = snapshots::Snapshot {
        date: today,
        days,
        sessions: overview.traffic.total_sessions,
        organic_sessions: overview.traffic.organic_sessions,
        engagement_rate: overview.engagement_rate,
        clicks: overview.search.clicks,
        impressions: overview.search.impressions,
        ctr: overview.search.ctr,
        avg_position: overview.search.average_position,
    };
    let _ = snapshots::save(&overview.property_name, &snap);

    // 2. Growth Drivers
    let pb = spinner("Analyzing growth drivers…");
    let growth = reports::growth::build(config, token, days).await?;
    pb.finish_and_clear();
    ui::print_growth(&growth);

    // 3. Weekly Trends
    let pb = spinner("Loading weekly trends…");
    let trends = reports::trends::build(config, token, days).await?;
    pb.finish_and_clear();
    ui::print_trends(&trends);

    // 4. Top Pages
    let pb = spinner("Loading top pages…");
    let top_pages = reports::top_pages::build(config, token, days, 500, "sessions").await?;
    pb.finish_and_clear();
    ui::print_top_pages(&top_pages);

    // 5. Channels
    let pb = spinner("Loading channel analysis…");
    let channels = reports::channels::build(config, token, days).await?;
    pb.finish_and_clear();
    ui::print_channels(&channels);

    // 6. Queries
    let pb = spinner("Analyzing search queries…");
    let queries = reports::queries::build(config, token, days, 30, "clicks").await?;
    pb.finish_and_clear();
    ui::print_queries(&queries);

    // 7. Opportunities
    let pb = spinner("Analyzing opportunities…");
    let opportunities = reports::opportunities::build(config, token, days).await?;
    pb.finish_and_clear();
    ui::print_opportunities(&opportunities);

    // 8. Topic Clusters
    let pb = spinner("Analyzing topic clusters…");
    let clusters = reports::clusters::build(config, token, days).await?;
    pb.finish_and_clear();
    ui::print_clusters(&clusters);

    // 9. AI Traffic
    let pb = spinner("Analyzing AI traffic…");
    let ai = reports::ai_traffic::build(config, token, days).await?;
    pb.finish_and_clear();
    ui::print_ai_traffic(&ai);

    // 10. Devices
    let pb = spinner("Loading device analysis…");
    let devices = reports::devices::build(config, token, days).await?;
    pb.finish_and_clear();
    ui::print_devices(&devices);

    // 11. Countries
    let pb = spinner("Loading country analysis…");
    let countries = reports::countries::build(config, token, days, 20).await?;
    pb.finish_and_clear();
    ui::print_countries(&countries);

    // 12. Content Decay
    let pb = spinner("Analyzing content decay…");
    let decay = reports::decay::build(config, token, days).await?;
    pb.finish_and_clear();
    ui::print_decay(&decay);

    // Management Summary (Narrative Engine)
    let narrative_input = narrative::NarrativeInput {
        overview: Some(&overview),
        top_pages: Some(&top_pages),
        opportunities: Some(&opportunities),
        growth: Some(&growth),
        ai_traffic: Some(&ai),
        clusters: Some(&clusters),
    };
    let summary = narrative::management_summary(&narrative_input);
    ui::print_management_summary(&summary);

    println!(
        "{}\n",
        "── Report complete ──".bold().dimmed()
    );

    // Offer export
    let export_opts = vec![
        "Save as PDF (recommended)",
        "Save as JSON",
        "Continue without saving",
    ];
    let export_choice = inquire::Select::new("What should be saved from this report?", export_opts)
        .with_starting_cursor(0)
        .prompt()?;

    match export_choice {
        "Save as PDF (recommended)" => {
            export_pdf(config, &overview, &top_pages).await?;
        }
        "Save as JSON" => {
            export_json(&overview, &top_pages)?;
        }
        _ => {}
    }

    Ok(())
}

// ─── Page Performance ───────────────────────────────────────────────────────

async fn run_page_performance(config: &AppConfig, token: &str, days: u32) -> anyhow::Result<()> {
    let limit = ask_limit("How many pages to show (top / weakest)?")?;

    let pb = spinner(&format!("Loading page performance for last {} days…", days));
    let full_report = reports::top_pages::build(config, token, days, 500, "sessions").await?;
    pb.finish_and_clear();

    let top_report = TopPagesReport {
        property_name: full_report.property_name.clone(),
        date_range: full_report.date_range.clone(),
        pages: full_report.pages.iter().take(limit).cloned().collect(),
        insights: full_report.insights.clone(),
    };
    ui::print_top_pages(&top_report);

    let weakest_pages = weakest_pages(&full_report.pages, limit);
    let weakest_report = TopPagesReport {
        property_name: full_report.property_name,
        date_range: full_report.date_range,
        pages: weakest_pages,
        insights: vec![],
    };
    ui::print_weakest_pages(&weakest_report);

    Ok(())
}

async fn run_page_detail(config: &AppConfig, token: &str, days: u32) -> anyhow::Result<()> {
    let url: String = inquire::Text::new("Which page should be analyzed?")
        .with_placeholder("/blog/my-article")
        .prompt()?;

    let pb = spinner(&format!("Loading details for {}…", url));
    let report = reports::page_detail::build(config, token, &url, days).await?;
    pb.finish_and_clear();
    ui::print_page_detail(&report);

    Ok(())
}

fn weakest_pages(pages: &[PageSummary], limit: usize) -> Vec<PageSummary> {
    let mut ranked: Vec<(f64, &PageSummary)> = pages
        .iter()
        .filter_map(|page| {
            let score = page_weakness_score(page)?;
            Some((score, page))
        })
        .collect();

    ranked.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    ranked
        .into_iter()
        .take(limit)
        .map(|(_, page)| page.clone())
        .collect()
}

fn page_weakness_score(page: &PageSummary) -> Option<f64> {
    if page.sessions < 20 && page.search.impressions < 200.0 {
        return None;
    }

    let mut score = 0.0;

    if page.engagement_rate < 0.30 && page.sessions >= 20 {
        score += (0.30 - page.engagement_rate) * 200.0;
    }

    if page.bounce_rate > 0.70 && page.sessions >= 20 {
        score += (page.bounce_rate - 0.70) * 120.0;
    }

    if page.search.impressions > 200.0
        && page.search.average_position > 0.0
        && page.search.average_position <= 10.0
    {
        let expected_ctr = crate::opportunities::expected_ctr(page.search.average_position);
        if expected_ctr > page.search.ctr {
            score += (expected_ctr - page.search.ctr) * page.search.impressions;
        }
    }

    if page.search.impressions > 500.0 && page.search.clicks < 10.0 {
        score += 20.0;
    }

    if page.search.average_position > 10.0 && page.search.average_position <= 20.0 && page.search.impressions > 200.0 {
        score += 15.0;
    }

    if score <= 0.0 {
        None
    } else {
        Some(score)
    }
}

// ─── Compare ────────────────────────────────────────────────────────────────

async fn run_compare(config: &AppConfig, token: &str) -> anyhow::Result<()> {
    let since: String = inquire::Text::new("Change date (YYYY-MM-DD):")
        .with_placeholder("2026-04-01")
        .prompt()?;
    let url: String = inquire::Text::new("URL (empty = entire site):")
        .with_default("")
        .prompt()?;
    let url_opt = if url.is_empty() { None } else { Some(url.as_str()) };

    let before: u32 = inquire::CustomType::new("Days before change date:")
        .with_default(30)
        .prompt()?;
    let after: u32 = inquire::CustomType::new("Days after change date:")
        .with_default(30)
        .prompt()?;

    let pb = spinner("Loading comparison data…");
    let report =
        reports::compare::build(config, token, url_opt, before, after, &since).await?;
    pb.finish_and_clear();
    ui::print_comparison(&report);

    Ok(())
}

// ─── Export ─────────────────────────────────────────────────────────────────

const EXP_PDF: &str = "PDF (full report)";
const EXP_JSON: &str = "JSON (overview + top pages)";
const EXP_CSV: &str = "CSV (choose report)";
const EXP_BACK: &str = "<- Back";

async fn run_export(config: &AppConfig, token: &str, days: u32) -> anyhow::Result<()> {
    let options = vec![EXP_PDF, EXP_JSON, EXP_CSV, EXP_BACK];
    let choice = inquire::Select::new("Export format:", options).prompt()?;

    match choice {
        EXP_PDF => {
            let pb = spinner("Loading data…");
            let (overview, top_pages) = tokio::join!(
                reports::overview::build(config, token, days),
                reports::top_pages::build(config, token, days, 500, "sessions"),
            );
            let overview = overview?;
            let top_pages = top_pages?;
            pb.finish_and_clear();

            export_pdf(config, &overview, &top_pages).await?;
        }
        EXP_JSON => {
            let pb = spinner("Loading data…");
            let (overview, top_pages) = tokio::join!(
                reports::overview::build(config, token, days),
                reports::top_pages::build(config, token, days, 50, "sessions"),
            );
            let overview = overview?;
            let top_pages = top_pages?;
            pb.finish_and_clear();

            export_json(&overview, &top_pages)?;
        }
        EXP_CSV => {
            let csv_options = vec![
                "Top Pages",
                "Search Queries",
                "Opportunities",
                "Topic Clusters",
                "Channels",
                "Devices",
                "Countries",
                "Content Decay",
            ];
            let csv_choice = inquire::Select::new("Which report as CSV?", csv_options).prompt()?;

            let default_name = format!(
                "{}-{}.csv",
                csv_choice.to_lowercase().replace(' ', "-"),
                chrono::Utc::now().format("%Y-%m-%d")
            );
            let path: String = inquire::Text::new("Save path:")
                .with_default(&default_name)
                .prompt()?;

            let pb = spinner("Loading data…");

            let csv_bytes: Vec<u8> = match csv_choice {
                "Top Pages" => {
                    let r = reports::top_pages::build(config, token, days, 50, "sessions").await?;
                    pb.finish_and_clear();
                    let mut buf = Vec::new();
                    export::csv::write_top_pages(&r, &mut buf)?;
                    buf
                }
                "Search Queries" => {
                    let r = reports::queries::build(config, token, days, 100, "clicks").await?;
                    pb.finish_and_clear();
                    let mut buf = Vec::new();
                    export::csv::write_queries(&r, &mut buf)?;
                    buf
                }
                "Opportunities" => {
                    let r = reports::opportunities::build(config, token, days).await?;
                    pb.finish_and_clear();
                    let mut buf = Vec::new();
                    export::csv::write_opportunities(&r, &mut buf)?;
                    buf
                }
                "Topic Clusters" => {
                    let r = reports::clusters::build(config, token, days).await?;
                    pb.finish_and_clear();
                    let mut buf = Vec::new();
                    export::csv::write_clusters(&r, &mut buf)?;
                    buf
                }
                "Channels" => {
                    let r = reports::channels::build(config, token, days).await?;
                    pb.finish_and_clear();
                    let mut buf = Vec::new();
                    export::csv::write_channels(&r, &mut buf)?;
                    buf
                }
                "Devices" => {
                    let r = reports::devices::build(config, token, days).await?;
                    pb.finish_and_clear();
                    let mut buf = Vec::new();
                    export::csv::write_devices(&r, &mut buf)?;
                    buf
                }
                "Countries" => {
                    let r = reports::countries::build(config, token, days, 50).await?;
                    pb.finish_and_clear();
                    let mut buf = Vec::new();
                    export::csv::write_countries(&r, &mut buf)?;
                    buf
                }
                "Content Decay" => {
                    let r = reports::decay::build(config, token, days).await?;
                    pb.finish_and_clear();
                    let mut buf = Vec::new();
                    export::csv::write_decay(&r, &mut buf)?;
                    buf
                }
                _ => {
                    pb.finish_and_clear();
                    return Ok(());
                }
            };

            if let Some(parent) = std::path::Path::new(&path).parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, &csv_bytes)?;
            println!("{} CSV saved: {}", "✓".green().bold(), path.cyan());
        }
        _ => {} // EXP_BACK
    }

    Ok(())
}

// ─── Export helpers ─────────────────────────────────────────────────────────

async fn export_pdf(
    config: &AppConfig,
    overview: &crate::domain::SiteOverviewReport,
    top_pages: &crate::domain::TopPagesReport,
) -> anyhow::Result<()> {
    let property_slug = config
        .properties
        .ga4_property_name
        .as_deref()
        .unwrap_or("report")
        .to_lowercase()
        .replace(' ', "-");

    let default_path = format!(
        "output/{}-{}.pdf",
        property_slug,
        chrono::Utc::now().format("%Y-%m-%d")
    );

    let path: String = inquire::Text::new("Save path:")
        .with_default(&default_path)
        .prompt()?;

    if let Some(parent) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Cannot create directory {}", parent.display()))?;
    }

    let vm = export::builder::build_view_model(overview, top_pages, None, None, 20);
    export::pdf::generate(&vm, &path).context("PDF export failed")?;

    println!("{} PDF saved: {}", "✓".green().bold(), path.cyan());
    Ok(())
}

fn export_json(
    overview: &crate::domain::SiteOverviewReport,
    top_pages: &crate::domain::TopPagesReport,
) -> anyhow::Result<()> {
    #[derive(serde::Serialize)]
    struct JsonExport<'a> {
        overview: &'a crate::domain::SiteOverviewReport,
        top_pages: &'a crate::domain::TopPagesReport,
    }

    let default_path = format!(
        "output/report-{}.json",
        chrono::Utc::now().format("%Y-%m-%d")
    );

    let path: String = inquire::Text::new("Save path:")
        .with_default(&default_path)
        .prompt()?;

    if let Some(parent) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_string_pretty(&JsonExport { overview, top_pages })?;
    std::fs::write(&path, &json)?;

    println!("{} JSON saved: {}", "✓".green().bold(), path.cyan());
    Ok(())
}

// ─── Spinner ────────────────────────────────────────────────────────────────

fn spinner(msg: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb
}
