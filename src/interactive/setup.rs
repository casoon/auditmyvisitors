use anyhow::Context;
use colored::Colorize;

use crate::auth;
use crate::config::AppConfig;
use crate::google;

/// Ensure the user is authenticated and has a property selected.
/// Returns the access token.
pub async fn ensure_ready(config: &mut AppConfig) -> anyhow::Result<String> {
    // ── Step 1: Authentication ──────────────────────────────────────────────
    let status = auth::auth_status()?;

    match status {
        auth::AuthStatus::NotLoggedIn => {
            println!("Zuerst verbinden wir dein Google-Konto.\n");
            auth::run_oauth_login().await?;
            println!("\n{} Erfolgreich verbunden.\n", "✓".green().bold());
        }
        auth::AuthStatus::TokenExpired => {
            println!("Session wird erneuert…");
            auth::ensure_valid_token().await?;
            println!("{} Session erneuert.\n", "✓".green().bold());
        }
        auth::AuthStatus::LoggedIn => {
            println!("{} Google-Konto verbunden.\n", "✓".green().bold());
        }
    }

    let token = auth::ensure_valid_token()
        .await
        .context("Token konnte nicht geladen werden")?;

    // ── Step 2: Property selection ──────────────────────────────────────────
    if config.properties.ga4_property_id.is_some() {
        let name = config
            .properties
            .ga4_property_name
            .as_deref()
            .unwrap_or("(unbenannt)");
        let sc = config
            .properties
            .search_console_url
            .as_deref()
            .unwrap_or("nicht gesetzt");

        println!("Aktuelle Property:");
        println!("  GA4:             {}", name.cyan());
        println!("  Search Console:  {}\n", sc.cyan());

        let keep = inquire::Confirm::new("Mit dieser Property weiterarbeiten?")
            .with_default(true)
            .prompt()?;

        if !keep {
            select_properties(config, &token).await?;
        }
    } else {
        println!("Noch keine Property ausgewaehlt.\n");
        select_properties(config, &token).await?;
    }

    println!();
    Ok(token)
}

/// Interactive GA4 + Search Console property selection (shared logic).
async fn select_properties(config: &mut AppConfig, token: &str) -> anyhow::Result<()> {
    let (ga4_props, sc_sites) = tokio::join!(
        google::analytics_admin::list_properties(token),
        google::search_console::list_sites(token),
    );
    let ga4_props = ga4_props?;
    let sc_sites = sc_sites?;

    if ga4_props.is_empty() {
        anyhow::bail!("Keine GA4 Properties gefunden. Pruefe die Berechtigungen im Google-Konto.");
    }

    // GA4 selection
    let ga4_labels: Vec<String> = ga4_props
        .iter()
        .map(|p| format!("{} — {}", p.display_name, p.name))
        .collect();

    let ga4_choice = inquire::Select::new("GA4 Property auswaehlen:", ga4_labels)
        .prompt()
        .context("Auswahl abgebrochen")?;

    let selected_ga4 = ga4_props
        .iter()
        .find(|p| format!("{} — {}", p.display_name, p.name) == ga4_choice)
        .unwrap();

    config.set_ga4_property(selected_ga4.name.clone(), selected_ga4.display_name.clone());

    // Search Console selection
    if !sc_sites.is_empty() {
        let mut sc_options = vec!["(ueberspringen)".to_string()];
        sc_options.extend(sc_sites);

        let sc_choice = inquire::Select::new("Search Console Property auswaehlen:", sc_options)
            .prompt()
            .context("Auswahl abgebrochen")?;

        if sc_choice != "(ueberspringen)" {
            config.set_search_console_url(sc_choice);
        }
    }

    config.save().context("Konfiguration konnte nicht gespeichert werden")?;

    println!(
        "\n{} Property gespeichert: {}",
        "✓".green().bold(),
        selected_ga4.display_name.cyan()
    );

    Ok(())
}
