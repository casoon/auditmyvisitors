mod auth;
mod cli;
mod config;
mod domain;
mod errors;
mod export;
mod google;
mod insights;
mod reports;
mod storage;
mod ui;

use anyhow::Context;
use clap::Parser;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};

use cli::{AuthAction, Cli, Command, ExportAction, PropertiesAction, ReportAction};
use config::AppConfig;

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("{} {}", "Fehler:".red().bold(), e);
        std::process::exit(1);
    }
}

async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if cli.verbose {
        tracing_subscriber::fmt()
            .with_env_filter("debug")
            .init();
    }

    let mut config = AppConfig::load().context("Cannot load config")?;

    match cli.command {
        Command::Auth { action } => handle_auth(action, &config).await?,
        Command::Properties { action } => handle_properties(action, &mut config).await?,
        Command::Report { action } => handle_report(action, &config).await?,
        Command::Export { action } => handle_export(action, &config).await?,
    }

    Ok(())
}

// ─── Auth ─────────────────────────────────────────────────────────────────────

async fn handle_auth(action: AuthAction, _config: &AppConfig) -> anyhow::Result<()> {
    match action {
        AuthAction::Login => {
            auth::run_oauth_login().await?;
            println!("\n{} Du bist jetzt eingeloggt.", "✓".green().bold());
            println!("Nächster Schritt: {}", "audit-my-visitors properties select".cyan());
        }
        AuthAction::Status => {
            let status = auth::auth_status()?;
            ui::print_auth_status(&status);
        }
        AuthAction::Logout => {
            storage::delete_tokens()?;
            println!("{} Ausgeloggt — Tokens wurden gelöscht.", "✓".green().bold());
        }
    }
    Ok(())
}

// ─── Properties ───────────────────────────────────────────────────────────────

async fn handle_properties(action: PropertiesAction, config: &mut AppConfig) -> anyhow::Result<()> {
    let token = auth::ensure_valid_token().await
        .context("Bitte zuerst einloggen: audit-my-visitors auth login")?;

    match action {
        PropertiesAction::List => {
            let pb = spinner("Google Analytics Properties werden geladen…");
            let properties = google::analytics_admin::list_properties(&token).await?;
            pb.finish_and_clear();

            if properties.is_empty() {
                println!("Keine GA4 Properties gefunden.");
                return Ok(());
            }

            println!("\n{}", "GA4 PROPERTIES".bold().underline());
            for prop in &properties {
                println!("  {} — {}", prop.name.cyan(), prop.display_name);
            }

            let pb2 = spinner("Search Console Properties werden geladen…");
            let sites = google::search_console::list_sites(&token).await?;
            pb2.finish_and_clear();

            if !sites.is_empty() {
                println!("\n{}", "SEARCH CONSOLE PROPERTIES".bold().underline());
                for site in &sites {
                    println!("  {}", site.cyan());
                }
            }
        }

        PropertiesAction::Select => {
            let pb = spinner("Verfügbare Properties werden geladen…");
            let (ga4_props, sc_sites) = tokio::join!(
                google::analytics_admin::list_properties(&token),
                google::search_console::list_sites(&token),
            );
            pb.finish_and_clear();

            let ga4_props = ga4_props?;
            let sc_sites = sc_sites?;

            if ga4_props.is_empty() {
                println!("Keine GA4 Properties gefunden.");
                return Ok(());
            }

            // GA4 property selection
            let ga4_labels: Vec<String> = ga4_props
                .iter()
                .map(|p| format!("{} — {}", p.display_name, p.name))
                .collect();

            let ga4_idx = inquire::Select::new("GA4 Property auswählen:", ga4_labels.clone())
                .prompt()
                .context("Auswahl abgebrochen")?;

            let selected_ga4 = ga4_props
                .iter()
                .find(|p| format!("{} — {}", p.display_name, p.name) == ga4_idx)
                .unwrap();

            config.set_ga4_property(selected_ga4.name.clone(), selected_ga4.display_name.clone());

            // Search Console selection (optional)
            if !sc_sites.is_empty() {
                let mut sc_options = vec!["(überspringen)".to_string()];
                sc_options.extend(sc_sites.clone());

                let sc_choice = inquire::Select::new("Search Console Property auswählen:", sc_options)
                    .prompt()
                    .context("Auswahl abgebrochen")?;

                if sc_choice != "(überspringen)" {
                    config.set_search_console_url(sc_choice);
                }
            }

            config.save().context("Konfiguration konnte nicht gespeichert werden")?;
            println!("\n{} Property-Auswahl gespeichert.", "✓".green().bold());
            println!("GA4:             {}", selected_ga4.display_name.cyan());
            if let Some(sc) = &config.properties.search_console_url {
                println!("Search Console:  {}", sc.cyan());
            }
        }
    }
    Ok(())
}

// ─── Reports ──────────────────────────────────────────────────────────────────

async fn handle_report(action: ReportAction, config: &AppConfig) -> anyhow::Result<()> {
    let token = auth::ensure_valid_token().await
        .context("Bitte zuerst einloggen: audit-my-visitors auth login")?;

    match action {
        ReportAction::Overview { days } => {
            let days = days.unwrap_or(config.report.default_days);
            let pb = spinner(&format!("Overview für letzte {} Tage wird geladen…", days));
            let report = reports::overview::build(config, &token, days).await?;
            pb.finish_and_clear();
            ui::print_overview(&report);
        }

        ReportAction::TopPages { days, limit, sort_by } => {
            let days = days.unwrap_or(config.report.default_days);
            let limit = limit.unwrap_or(config.report.top_pages_limit);
            let pb = spinner(&format!("Top {} Seiten für letzte {} Tage werden geladen…", limit, days));
            let report = reports::top_pages::build(config, &token, days, limit, &sort_by).await?;
            pb.finish_and_clear();
            ui::print_top_pages(&report);
        }

        ReportAction::Page { url, days } => {
            let days = days.unwrap_or(config.report.default_days);
            let pb = spinner(&format!("Seiten-Detail für {} wird geladen…", url));
            let report = reports::page_detail::build(config, &token, &url, days).await?;
            pb.finish_and_clear();
            ui::print_page_detail(&report);
        }

        ReportAction::Compare { url, before, after, since } => {
            let pb = spinner("Vergleichsdaten werden geladen…");
            let report = reports::compare::build(
                config, &token, url.as_deref(), before, after, &since,
            ).await?;
            pb.finish_and_clear();
            ui::print_comparison(&report);
        }
    }
    Ok(())
}

// ─── Export ───────────────────────────────────────────────────────────────────

async fn handle_export(action: ExportAction, config: &AppConfig) -> anyhow::Result<()> {
    match action {
        ExportAction::Pdf { report: _, output } => {
            let path = output.unwrap_or_else(|| {
                format!("report-{}.pdf", chrono::Utc::now().format("%Y-%m-%d"))
            });

            // Phase 4: full PDF layout. For now, generate a placeholder.
            let opts = export::pdf::PdfExportOptions {
                output_path: path.clone(),
                title: "Audit My Visitors Report".into(),
                property_name: config.properties.ga4_property_name
                    .clone()
                    .unwrap_or_else(|| "unbekannt".into()),
                date_range: format!("Erstellt am {}", chrono::Utc::now().format("%Y-%m-%d")),
            };

            export::pdf::export_pdf(opts, "Report wird in Phase 4 vollständig implementiert.")
                .context("PDF-Export fehlgeschlagen")?;

            println!("{} PDF gespeichert: {}", "✓".green().bold(), path.cyan());
        }
    }
    Ok(())
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

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
