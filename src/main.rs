mod auth;
mod cli;
mod clusters;
mod config;
mod domain;
mod errors;
mod export;
mod google;
mod helpers;
mod insights;
mod intent;
mod interactive;
mod narrative;
mod opportunities;
mod page_audit;
mod reports;
mod snapshots;
mod storage;
mod ui;

use anyhow::Context;
use clap::Parser;
use crate::ui::spinner;
use crate::ui::style::Paint;

use cli::{AuthAction, Cli, Command, ExportAction, PropertiesAction, ReportAction, SnapshotAction};
use config::AppConfig;
use google::api::GoogleApi;

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("{} {}", "Error:".err(), e);
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
        Some(Command::Auth { action }) => handle_auth(action, &config).await?,
        Some(Command::Properties { action }) => handle_properties(action, &mut config).await?,
        Some(Command::Report { action }) => handle_report(action, &config).await?,
        Some(Command::Export { action }) => handle_export(action, &config).await?,
        Some(Command::Snapshot { action }) => handle_snapshot(action, &config)?,
        None => interactive::run(&mut config).await?,
    }

    Ok(())
}

// ─── Auth ─────────────────────────────────────────────────────────────────────

async fn handle_auth(action: AuthAction, _config: &AppConfig) -> anyhow::Result<()> {
    match action {
        AuthAction::Login => {
            auth::run_oauth_login().await?;
            println!("\n{} You are now logged in.", "✓".ok());
            println!("Next step: {}", "auditmyvisitors properties select".accent());
        }
        AuthAction::Status => {
            let status = auth::auth_status()?;
            ui::print_auth_status(&status);
        }
        AuthAction::Logout => {
            storage::delete_tokens()?;
            println!("{} Logged out — tokens deleted.", "✓".ok());
        }
    }
    Ok(())
}

// ─── Properties ───────────────────────────────────────────────────────────────

async fn handle_properties(action: PropertiesAction, config: &mut AppConfig) -> anyhow::Result<()> {
    let api = google::api::HttpGoogleApi::new(
        auth::ensure_valid_token().await
            .context("Please log in first: auditmyvisitors auth login")?,
    );

    match action {
        PropertiesAction::List => {
            let pb = spinner("Loading Google Analytics properties…");
            let properties = api.list_properties().await?;
            pb.finish_and_clear();

            if properties.is_empty() {
                println!("No GA4 properties found.");
                return Ok(());
            }

            println!("\n{}", "GA4 PROPERTIES".heading());
            for prop in &properties {
                println!("  {} — {}", prop.name.accent(), prop.display_name);
            }

            let pb2 = spinner("Loading Search Console properties…");
            let sites = api.list_sites().await?;
            pb2.finish_and_clear();

            if !sites.is_empty() {
                println!("\n{}", "SEARCH CONSOLE PROPERTIES".heading());
                for site in &sites {
                    println!("  {}", site.accent());
                }
            }
        }

        PropertiesAction::Select => {
            let pb = spinner("Loading available properties…");
            let (ga4_props, sc_sites) = tokio::join!(
                api.list_properties(),
                api.list_sites(),
            );
            pb.finish_and_clear();

            let ga4_props = ga4_props?;
            let sc_sites = sc_sites?;

            if ga4_props.is_empty() {
                println!("No GA4 properties found.");
                return Ok(());
            }

            // GA4 property selection
            let ga4_labels: Vec<String> = ga4_props
                .iter()
                .map(|p| format!("{} — {}", p.display_name, p.name))
                .collect();

            let ga4_idx = inquire::Select::new("Select GA4 property:", ga4_labels.clone())
                .prompt()
                .context("Selection cancelled")?;

            let selected_ga4 = ga4_props
                .iter()
                .find(|p| format!("{} — {}", p.display_name, p.name) == ga4_idx)
                .unwrap();

            config.set_ga4_property(selected_ga4.name.clone(), selected_ga4.display_name.clone());

            // Search Console selection (optional)
            if !sc_sites.is_empty() {
                let mut sc_options = vec!["(skip)".to_string()];
                sc_options.extend(sc_sites.clone());

                let sc_choice = inquire::Select::new("Select Search Console property:", sc_options)
                    .prompt()
                    .context("Selection cancelled")?;

                if sc_choice != "(skip)" {
                    config.set_search_console_url(sc_choice);
                }
            }

            config.save().context("Could not save configuration")?;
            println!("\n{} Property selection saved.", "✓".ok());
            println!("GA4:             {}", selected_ga4.display_name.accent());
            if let Some(sc) = &config.properties.search_console_url {
                println!("Search Console:  {}", sc.accent());
            }
        }
    }
    Ok(())
}

// ─── Reports ──────────────────────────────────────────────────────────────────

async fn handle_report(action: ReportAction, config: &AppConfig) -> anyhow::Result<()> {
    let api = google::api::HttpGoogleApi::new(
        auth::ensure_valid_token().await
            .context("Please log in first: auditmyvisitors auth login")?,
    );

    match action {
        ReportAction::Overview { days } => {
            let days = days.unwrap_or(config.report.default_days);
            let pb = spinner(&format!("Loading overview for last {} days…", days));
            let (report, top_pages, opportunities, growth, trends, clusters) = tokio::join!(
                reports::overview::build(config, &api, days),
                reports::top_pages::build(config, &api, days, 10, "sessions"),
                reports::opportunities::build(config, &api, days),
                reports::growth::build(config, &api, days),
                reports::trends::build(config, &api, days),
                reports::clusters::build(config, &api, days),
            );
            let report = report?;
            let top_pages = top_pages?;
            let opportunities = opportunities?;
            let growth = growth?;
            let trends = trends?;
            let clusters = clusters?;
            pb.finish_and_clear();
            ui::print_overview(&report);

            let narrative_input = narrative::NarrativeInput {
                overview: Some(&report),
                top_pages: Some(&top_pages),
                opportunities: Some(&opportunities),
                growth: Some(&growth),
                ai_traffic: None,
                clusters: Some(&clusters),
            };
            let summary = narrative::management_summary(&narrative_input);
            ui::print_management_summary(&summary);
            ui::print_growth_highlights(&growth);
            ui::print_trend_highlights(&trends);
            ui::print_cluster_highlights(&clusters);

            // Show snapshot comparison if a previous snapshot exists
            let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
            if let Ok(Some(prev)) = snapshots::load_previous(&report.property_name, &today) {
                ui::print_snapshot_comparison(&prev, &report);
            }

            // Auto-save snapshot
            let snap = snapshots::Snapshot {
                date: chrono::Utc::now().format("%Y-%m-%d").to_string(),
                days,
                sessions: report.traffic.total_sessions,
                organic_sessions: report.traffic.organic_sessions,
                engagement_rate: report.engagement_rate,
                clicks: report.search.clicks,
                impressions: report.search.impressions,
                ctr: report.search.ctr,
                avg_position: report.search.average_position,
            };
            if let Err(e) = snapshots::save(&report.property_name, &snap) {
                tracing::warn!("Could not save snapshot: {}", e);
            }
        }

        ReportAction::TopPages { days, limit, sort_by } => {
            let days = days.unwrap_or(config.report.default_days);
            let limit = limit.unwrap_or(config.report.top_pages_limit);
            let pb = spinner(&format!("Loading top {} pages for last {} days…", limit, days));
            let report = reports::top_pages::build(config, &api, days, limit, &sort_by).await?;
            pb.finish_and_clear();
            ui::print_top_pages(&report);
        }

        ReportAction::Page { url, days } => {
            let days = days.unwrap_or(config.report.default_days);
            let pb = spinner(&format!("Loading page detail for {}…", url));
            let report = reports::page_detail::build(config, &api, &url, days).await?;
            pb.finish_and_clear();
            ui::print_page_detail(&report);
        }

        ReportAction::Compare { url, before, after, since } => {
            let pb = spinner("Loading comparison data…");
            let report = reports::compare::build(
                config, &api, url.as_deref(), before, after, &since,
            ).await?;
            pb.finish_and_clear();
            ui::print_comparison(&report);
        }

        ReportAction::Opportunities { days } => {
            let days = days.unwrap_or(config.report.default_days);
            let pb = spinner(&format!("Analyzing opportunities for last {} days…", days));
            let report = reports::opportunities::build(config, &api, days).await?;
            pb.finish_and_clear();
            ui::print_opportunities(&report);
        }

        ReportAction::Queries { days, limit, sort_by } => {
            let days = days.unwrap_or(config.report.default_days);
            let limit = limit.unwrap_or(30);
            let pb = spinner(&format!("Loading query analysis for last {} days…", days));
            let report = reports::queries::build(config, &api, days, limit, &sort_by).await?;
            pb.finish_and_clear();
            ui::print_queries(&report);
        }

        ReportAction::AiTraffic { days } => {
            let days = days.unwrap_or(config.report.default_days);
            let pb = spinner(&format!("Analyzing AI traffic for last {} days…", days));
            let report = reports::ai_traffic::build(config, &api, days).await?;
            pb.finish_and_clear();
            ui::print_ai_traffic(&report);
        }

        ReportAction::Channels { days } => {
            let days = days.unwrap_or(config.report.default_days);
            let pb = spinner(&format!("Loading channel analysis for last {} days…", days));
            let report = reports::channels::build(config, &api, days).await?;
            pb.finish_and_clear();
            ui::print_channels(&report);
        }

        ReportAction::Clusters { days } => {
            let days = days.unwrap_or(config.report.default_days);
            let pb = spinner(&format!("Analyzing topic clusters for last {} days…", days));
            let report = reports::clusters::build(config, &api, days).await?;
            pb.finish_and_clear();
            ui::print_clusters(&report);
        }

        ReportAction::Decay { days } => {
            let days = days.unwrap_or(config.report.default_days);
            let pb = spinner(&format!("Analyzing content decay for last {} days…", days));
            let report = reports::decay::build(config, &api, days).await?;
            pb.finish_and_clear();
            ui::print_decay(&report);
        }

        ReportAction::Devices { days } => {
            let days = days.unwrap_or(config.report.default_days);
            let pb = spinner(&format!("Loading device analysis for last {} days…", days));
            let report = reports::devices::build(config, &api, days).await?;
            pb.finish_and_clear();
            ui::print_devices(&report);
        }

        ReportAction::Countries { days, limit } => {
            let days = days.unwrap_or(config.report.default_days);
            let limit = limit.unwrap_or(20);
            let pb = spinner(&format!("Loading country analysis for last {} days…", days));
            let report = reports::countries::build(config, &api, days, limit).await?;
            pb.finish_and_clear();
            ui::print_countries(&report);
        }
    }

    ui::print_truncation_warnings(&api.truncations());
    Ok(())
}

// ─── Export ───────────────────────────────────────────────────────────────────

#[derive(serde::Serialize)]
struct JsonExport {
    overview: domain::SiteOverviewReport,
    top_pages: domain::TopPagesReport,
}

async fn handle_export(action: ExportAction, config: &AppConfig) -> anyhow::Result<()> {
    let api = google::api::HttpGoogleApi::new(
        auth::ensure_valid_token().await
            .context("Please log in first: auditmyvisitors auth login")?,
    );

    match action {
        ExportAction::Json { days, output } => {
            let days = days.unwrap_or(config.report.default_days);
            let pb = spinner(&format!("Loading data for last {} days…", days));

            let (overview, top_pages) = tokio::join!(
                reports::overview::build(config, &api, days),
                reports::top_pages::build(config, &api, days, 50, "sessions"),
            );
            let overview = overview?;
            let top_pages = top_pages?;
            pb.finish_and_clear();

            let export = JsonExport { overview, top_pages };
            let json = serde_json::to_string_pretty(&export)?;

            if let Some(path) = output {
                if let Some(parent) = std::path::Path::new(&path).parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&path, &json)?;
                println!("{} JSON saved: {}", "✓".ok(), path.accent());
            } else {
                println!("{json}");
            }
        }

        ExportAction::Csv { report: report_type, days, limit, output } => {
            let days = days.unwrap_or(config.report.default_days);
            let pb = spinner(&format!("Loading data for last {} days…", days));

            let csv_bytes: Vec<u8> = match report_type.as_str() {
                "top-pages" => {
                    let limit = limit.unwrap_or(config.report.top_pages_limit);
                    let report = reports::top_pages::build(config, &api, days, limit, "sessions").await?;
                    pb.finish_and_clear();
                    let mut buf = Vec::new();
                    export::csv::write_top_pages(&report, &mut buf)?;
                    buf
                }
                "queries" => {
                    let limit = limit.unwrap_or(30);
                    let report = reports::queries::build(config, &api, days, limit, "clicks").await?;
                    pb.finish_and_clear();
                    let mut buf = Vec::new();
                    export::csv::write_queries(&report, &mut buf)?;
                    buf
                }
                "opportunities" => {
                    let report = reports::opportunities::build(config, &api, days).await?;
                    pb.finish_and_clear();
                    let mut buf = Vec::new();
                    export::csv::write_opportunities(&report, &mut buf)?;
                    buf
                }
                "channels" => {
                    let report = reports::channels::build(config, &api, days).await?;
                    pb.finish_and_clear();
                    let mut buf = Vec::new();
                    export::csv::write_channels(&report, &mut buf)?;
                    buf
                }
                "clusters" => {
                    let report = reports::clusters::build(config, &api, days).await?;
                    pb.finish_and_clear();
                    let mut buf = Vec::new();
                    export::csv::write_clusters(&report, &mut buf)?;
                    buf
                }
                "devices" => {
                    let report = reports::devices::build(config, &api, days).await?;
                    pb.finish_and_clear();
                    let mut buf = Vec::new();
                    export::csv::write_devices(&report, &mut buf)?;
                    buf
                }
                "countries" => {
                    let limit = limit.unwrap_or(20);
                    let report = reports::countries::build(config, &api, days, limit).await?;
                    pb.finish_and_clear();
                    let mut buf = Vec::new();
                    export::csv::write_countries(&report, &mut buf)?;
                    buf
                }
                "decay" => {
                    let report = reports::decay::build(config, &api, days).await?;
                    pb.finish_and_clear();
                    let mut buf = Vec::new();
                    export::csv::write_decay(&report, &mut buf)?;
                    buf
                }
                other => {
                    pb.finish_and_clear();
                    anyhow::bail!(
                        "Unknown report type: '{}'. Available: top-pages, queries, opportunities, channels, clusters, devices, countries, decay",
                        other
                    );
                }
            };

            if let Some(path) = output {
                if let Some(parent) = std::path::Path::new(&path).parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&path, &csv_bytes)?;
                println!("{} CSV saved: {}", "✓".ok(), path.accent());
            } else {
                use std::io::Write;
                std::io::stdout().write_all(&csv_bytes)?;
            }
        }

        ExportAction::Pdf { days, limit, output } => {
            let days = days.unwrap_or(config.report.default_days);

            let pb = spinner(&format!("Loading data for last {} days…", days));

            let (overview, top_pages, queries) = tokio::join!(
                reports::overview::build(config, &api, days),
                reports::top_pages::build(config, &api, days, 500, "sessions"),
                reports::queries::build(config, &api, days, 500, "clicks"),
            );
            let overview = overview?;
            let top_pages = top_pages?;
            let queries = queries.ok();

            // Invisible pages (traffic but 0 GSC impressions) are prime candidates for URL inspection
            let invisible: Vec<_> = top_pages.pages.iter()
                .filter(|p| p.search.impressions == 0.0 && p.sessions > 10)
                .cloned()
                .collect();
            let site_health = reports::site_health::build(config, &api, &invisible).await.ok();

            pb.set_message("Creating PDF…");

            let property_slug = config
                .properties
                .ga4_property_name
                .as_deref()
                .unwrap_or("report")
                .to_lowercase()
                .replace(' ', "-");

            let path = output.unwrap_or_else(|| {
                let date = chrono::Utc::now().format("%Y-%m-%d");
                format!("output/{}-{}.pdf", property_slug, date)
            });

            // Ensure output dir exists
            if let Some(parent) = std::path::Path::new(&path).parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("Cannot create directory {}", parent.display()))?;
            }

            let vm = export::builder::build_view_model(&overview, &top_pages, queries.as_ref(), site_health.as_ref(), limit);
            export::pdf::generate(&vm, &path).context("PDF export failed")?;

            pb.finish_and_clear();
            println!("{} PDF saved: {}", "✓".ok(), path.accent());
        }
    }

    ui::print_truncation_warnings(&api.truncations());
    Ok(())
}

// ─── Snapshots ──────────────────────────────────────────────────────────────

fn handle_snapshot(action: SnapshotAction, config: &AppConfig) -> anyhow::Result<()> {
    match action {
        SnapshotAction::List => {
            let property_name = config
                .properties
                .ga4_property_name
                .as_deref()
                .unwrap_or("unknown");

            let snaps = snapshots::list(property_name)?;
            if snaps.is_empty() {
                println!("No snapshots available. Run {} first.", "report overview".accent());
                return Ok(());
            }

            println!("\n{}", "SNAPSHOTS".heading());
            println!("Property: {}\n", property_name.accent());

            let mut table = comfy_table::Table::new();
            table.set_header(vec![
                comfy_table::Cell::new("Date"),
                comfy_table::Cell::new("Days"),
                comfy_table::Cell::new("Sessions"),
                comfy_table::Cell::new("Organic"),
                comfy_table::Cell::new("Clicks"),
                comfy_table::Cell::new("Impressions"),
                comfy_table::Cell::new("CTR"),
                comfy_table::Cell::new("Position"),
            ]);

            for snap in &snaps {
                table.add_row(vec![
                    comfy_table::Cell::new(&snap.date),
                    comfy_table::Cell::new(snap.days),
                    comfy_table::Cell::new(snap.sessions),
                    comfy_table::Cell::new(snap.organic_sessions),
                    comfy_table::Cell::new(format!("{:.0}", snap.clicks)),
                    comfy_table::Cell::new(format!("{:.0}", snap.impressions)),
                    comfy_table::Cell::new(format!("{:.1}%", snap.ctr * 100.0)),
                    comfy_table::Cell::new(format!("{:.1}", snap.avg_position)),
                ]);
            }
            println!("{table}\n");
        }
    }
    Ok(())
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

