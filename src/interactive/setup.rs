use anyhow::Context;
use crate::ui::style::Paint;

use crate::auth;
use crate::config::AppConfig;
use crate::google::api::GoogleApi;
use crate::google;

/// Ensure the user is authenticated and has a property selected.
/// Returns the access token.
pub async fn ensure_ready(config: &mut AppConfig) -> anyhow::Result<String> {
    // ── Step 1: Authentication ──────────────────────────────────────────────
    let status = auth::auth_status()?;

    match status {
        auth::AuthStatus::NotLoggedIn => {
            println!("First, let's connect your Google account.\n");
            auth::run_oauth_login().await?;
            println!("\n{} Successfully connected.\n", "✓".ok());
        }
        auth::AuthStatus::TokenExpired => {
            println!("Refreshing session…");
            auth::ensure_valid_token().await?;
            println!("{} Session refreshed.\n", "✓".ok());
        }
        auth::AuthStatus::LoggedIn => {
            println!("{} Google account connected.\n", "✓".ok());
        }
    }

    let token = auth::ensure_valid_token()
        .await
        .context("Failed to load token")?;
    let api = google::api::HttpGoogleApi::new(token.clone());

    // ── Step 2: Property selection ──────────────────────────────────────────
    if config.properties.ga4_property_id.is_some() {
        let name = config
            .properties
            .ga4_property_name
            .as_deref()
            .unwrap_or("(unnamed)");
        let sc = config
            .properties
            .search_console_url
            .as_deref()
            .unwrap_or("not set");

        println!("Current property:");
        println!("  GA4:             {}", name.accent());
        println!("  Search Console:  {}\n", sc.accent());

        let keep = inquire::Confirm::new("Continue with this property?")
            .with_default(true)
            .prompt()?;

        if !keep {
            select_properties(config, &api).await?;
        }
    } else {
        println!("No property selected yet.\n");
        select_properties(config, &api).await?;
    }

    println!();
    Ok(token)
}

/// Interactive GA4 + Search Console property selection (shared logic).
async fn select_properties(config: &mut AppConfig, api: &impl GoogleApi) -> anyhow::Result<()> {
    let (ga4_props, sc_sites) = tokio::join!(
        api.list_properties(),
        api.list_sites(),
    );
    let ga4_props = ga4_props?;
    let sc_sites = sc_sites?;

    if ga4_props.is_empty() {
        anyhow::bail!("No GA4 properties found. Check the permissions on your Google account.");
    }

    // GA4 selection
    let ga4_labels: Vec<String> = ga4_props
        .iter()
        .map(|p| format!("{} — {}", p.display_name, p.name))
        .collect();

    let ga4_choice = inquire::Select::new("Select GA4 property:", ga4_labels)
        .prompt()
        .context("Selection cancelled")?;

    let selected_ga4 = ga4_props
        .iter()
        .find(|p| format!("{} — {}", p.display_name, p.name) == ga4_choice)
        .unwrap();

    config.set_ga4_property(selected_ga4.name.clone(), selected_ga4.display_name.clone());

    // Search Console selection
    if !sc_sites.is_empty() {
        let mut sc_options = vec!["(skip)".to_string()];
        sc_options.extend(sc_sites);

        let sc_choice = inquire::Select::new("Select Search Console property:", sc_options)
            .prompt()
            .context("Selection cancelled")?;

        if sc_choice != "(skip)" {
            config.set_search_console_url(sc_choice);
        }
    }

    config.save().context("Failed to save configuration")?;

    println!(
        "\n{} Property saved: {}",
        "✓".ok(),
        selected_ga4.display_name.accent()
    );

    Ok(())
}
