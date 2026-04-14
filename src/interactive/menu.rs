use anyhow::Context;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};

use crate::config::AppConfig;
use crate::{auth, export, reports, snapshots, ui};

// ─── Time range ─────────────────────────────────────────────────────────────

fn ask_days() -> anyhow::Result<u32> {
    let options = vec![
        "Letzte 7 Tage",
        "Letzte 28 Tage (Standard)",
        "Letzte 90 Tage",
        "Eigener Zeitraum…",
    ];

    let choice = inquire::Select::new("Zeitraum waehlen:", options)
        .with_starting_cursor(1)
        .prompt()?;

    match choice {
        "Letzte 7 Tage" => Ok(7),
        "Letzte 90 Tage" => Ok(90),
        "Eigener Zeitraum…" => {
            let days: u32 = inquire::CustomType::new("Anzahl Tage:")
                .with_default(28)
                .prompt()?;
            Ok(days)
        }
        _ => Ok(28),
    }
}

// ─── Main menu ──────────────────────────────────────────────────────────────

const MENU_REPORT: &str = "Report starten";
const MENU_PAGE: &str = "Seiten-Detail (einzelne URL)";
const MENU_COMPARE: &str = "Vorher/Nachher-Vergleich";
const MENU_EXPORT: &str = "Exportieren…";
const MENU_PROPERTY: &str = "Property wechseln";
const MENU_EXIT: &str = "Beenden";

pub async fn report_loop(config: &mut AppConfig) -> anyhow::Result<()> {
    let days = ask_days()?;
    println!();

    // Run the full report immediately on first entry
    let mut token = auth::ensure_valid_token()
        .await
        .context("Token konnte nicht erneuert werden")?;
    run_full_report(config, &token, days).await.unwrap_or_else(|e| {
        eprintln!("\n{} {}\n", "Fehler:".red().bold(), e);
    });

    loop {
        let menu = vec![
            MENU_REPORT,
            MENU_PAGE,
            MENU_COMPARE,
            MENU_EXPORT,
            MENU_PROPERTY,
            MENU_EXIT,
        ];

        let choice = inquire::Select::new("Was moechtest du tun?", menu).prompt()?;

        // Refresh token before each action
        if choice != MENU_EXIT {
            token = auth::ensure_valid_token()
                .await
                .context("Token konnte nicht erneuert werden")?;
        }

        match choice {
            MENU_REPORT => run_full_report(config, &token, days).await,
            MENU_PAGE => run_page_detail(config, &token, days).await,
            MENU_COMPARE => run_compare(config, &token).await,
            MENU_EXPORT => run_export(config, &token, days).await,
            MENU_PROPERTY => {
                super::setup::ensure_ready(config).await?;
                token = auth::ensure_valid_token().await?;
                Ok(())
            }
            MENU_EXIT => {
                println!("Bis bald!");
                break;
            }
            _ => Ok(()),
        }
        .unwrap_or_else(|e| {
            eprintln!("\n{} {}\n", "Fehler:".red().bold(), e);
        });
    }

    Ok(())
}

// ─── Full Report ────────────────────────────────────────────────────────────

async fn run_full_report(config: &AppConfig, token: &str, days: u32) -> anyhow::Result<()> {
    // 1. Overview
    let pb = spinner("Uebersicht wird geladen…");
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

    // 2. Top Pages
    let pb = spinner("Top-Seiten werden geladen…");
    let top_pages = reports::top_pages::build(config, token, days, 20, "sessions").await?;
    pb.finish_and_clear();
    ui::print_top_pages(&top_pages);

    // 3. Channels
    let pb = spinner("Kanal-Analyse wird geladen…");
    let channels = reports::channels::build(config, token, days).await?;
    pb.finish_and_clear();
    ui::print_channels(&channels);

    // 4. Queries
    let pb = spinner("Suchanfragen werden analysiert…");
    let queries = reports::queries::build(config, token, days, 30, "clicks").await?;
    pb.finish_and_clear();
    ui::print_queries(&queries);

    // 5. Opportunities
    let pb = spinner("Opportunities werden analysiert…");
    let opportunities = reports::opportunities::build(config, token, days).await?;
    pb.finish_and_clear();
    ui::print_opportunities(&opportunities);

    // 6. AI Traffic
    let pb = spinner("AI-Traffic wird analysiert…");
    let ai = reports::ai_traffic::build(config, token, days).await?;
    pb.finish_and_clear();
    ui::print_ai_traffic(&ai);

    // 7. Devices
    let pb = spinner("Geraete-Analyse wird geladen…");
    let devices = reports::devices::build(config, token, days).await?;
    pb.finish_and_clear();
    ui::print_devices(&devices);

    // 8. Countries
    let pb = spinner("Laender-Analyse wird geladen…");
    let countries = reports::countries::build(config, token, days, 20).await?;
    pb.finish_and_clear();
    ui::print_countries(&countries);

    // 9. Content Decay
    let pb = spinner("Content Decay wird analysiert…");
    let decay = reports::decay::build(config, token, days).await?;
    pb.finish_and_clear();
    ui::print_decay(&decay);

    println!(
        "{}\n",
        "── Report abgeschlossen ──".bold().dimmed()
    );

    // Offer export
    let export_opts = vec![
        "Weiter (kein Export)",
        "Als PDF speichern",
        "Als JSON speichern",
    ];
    let export_choice = inquire::Select::new("Report exportieren?", export_opts).prompt()?;

    match export_choice {
        "Als PDF speichern" => {
            export_pdf(config, &overview, &top_pages).await?;
        }
        "Als JSON speichern" => {
            export_json(&overview, &top_pages)?;
        }
        _ => {}
    }

    Ok(())
}

// ─── Page Detail ────────────────────────────────────────────────────────────

async fn run_page_detail(config: &AppConfig, token: &str, days: u32) -> anyhow::Result<()> {
    let url: String = inquire::Text::new("URL oder Pfad der Seite:")
        .with_placeholder("/blog/mein-artikel")
        .prompt()?;

    let pb = spinner(&format!("Detail fuer {} wird geladen…", url));
    let report = reports::page_detail::build(config, token, &url, days).await?;
    pb.finish_and_clear();
    ui::print_page_detail(&report);

    Ok(())
}

// ─── Compare ────────────────────────────────────────────────────────────────

async fn run_compare(config: &AppConfig, token: &str) -> anyhow::Result<()> {
    let since: String = inquire::Text::new("Stichtag (YYYY-MM-DD):")
        .with_placeholder("2026-04-01")
        .prompt()?;
    let url: String = inquire::Text::new("URL (leer = gesamte Website):")
        .with_default("")
        .prompt()?;
    let url_opt = if url.is_empty() { None } else { Some(url.as_str()) };

    let before: u32 = inquire::CustomType::new("Tage vor Stichtag:")
        .with_default(30)
        .prompt()?;
    let after: u32 = inquire::CustomType::new("Tage nach Stichtag:")
        .with_default(30)
        .prompt()?;

    let pb = spinner("Vergleichsdaten werden geladen…");
    let report =
        reports::compare::build(config, token, url_opt, before, after, &since).await?;
    pb.finish_and_clear();
    ui::print_comparison(&report);

    Ok(())
}

// ─── Export ─────────────────────────────────────────────────────────────────

const EXP_PDF: &str = "PDF (Vollstaendiger Report)";
const EXP_JSON: &str = "JSON (Uebersicht + Top-Seiten)";
const EXP_CSV: &str = "CSV (Report waehlen)";
const EXP_BACK: &str = "<- Zurueck";

async fn run_export(config: &AppConfig, token: &str, days: u32) -> anyhow::Result<()> {
    let options = vec![EXP_PDF, EXP_JSON, EXP_CSV, EXP_BACK];
    let choice = inquire::Select::new("Export-Format:", options).prompt()?;

    match choice {
        EXP_PDF => {
            let pb = spinner("Daten werden geladen…");
            let (overview, top_pages) = tokio::join!(
                reports::overview::build(config, token, days),
                reports::top_pages::build(config, token, days, 15, "sessions"),
            );
            let overview = overview?;
            let top_pages = top_pages?;
            pb.finish_and_clear();

            export_pdf(config, &overview, &top_pages).await?;
        }
        EXP_JSON => {
            let pb = spinner("Daten werden geladen…");
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
                "Top-Seiten",
                "Suchanfragen",
                "Opportunities",
                "Kanaele",
                "Geraete",
                "Laender",
                "Content Decay",
            ];
            let csv_choice = inquire::Select::new("Welchen Report als CSV?", csv_options).prompt()?;

            let default_name = format!(
                "{}-{}.csv",
                csv_choice.to_lowercase().replace(' ', "-"),
                chrono::Utc::now().format("%Y-%m-%d")
            );
            let path: String = inquire::Text::new("Speicherpfad:")
                .with_default(&default_name)
                .prompt()?;

            let pb = spinner("Daten werden geladen…");

            let csv_bytes: Vec<u8> = match csv_choice {
                "Top-Seiten" => {
                    let r = reports::top_pages::build(config, token, days, 50, "sessions").await?;
                    pb.finish_and_clear();
                    let mut buf = Vec::new();
                    export::csv::write_top_pages(&r, &mut buf)?;
                    buf
                }
                "Suchanfragen" => {
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
                "Kanaele" => {
                    let r = reports::channels::build(config, token, days).await?;
                    pb.finish_and_clear();
                    let mut buf = Vec::new();
                    export::csv::write_channels(&r, &mut buf)?;
                    buf
                }
                "Geraete" => {
                    let r = reports::devices::build(config, token, days).await?;
                    pb.finish_and_clear();
                    let mut buf = Vec::new();
                    export::csv::write_devices(&r, &mut buf)?;
                    buf
                }
                "Laender" => {
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
            println!("{} CSV gespeichert: {}", "✓".green().bold(), path.cyan());
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

    let path: String = inquire::Text::new("Speicherpfad:")
        .with_default(&default_path)
        .prompt()?;

    if let Some(parent) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Verzeichnis {} kann nicht erstellt werden", parent.display()))?;
    }

    let vm = export::builder::build_view_model(overview, top_pages);
    export::pdf::generate(&vm, &path).context("PDF-Export fehlgeschlagen")?;

    println!("{} PDF gespeichert: {}", "✓".green().bold(), path.cyan());
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

    let path: String = inquire::Text::new("Speicherpfad:")
        .with_default(&default_path)
        .prompt()?;

    if let Some(parent) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_string_pretty(&JsonExport { overview, top_pages })?;
    std::fs::write(&path, &json)?;

    println!("{} JSON gespeichert: {}", "✓".green().bold(), path.cyan());
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
