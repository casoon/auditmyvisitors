mod setup;
mod menu;

use std::io::IsTerminal;

use crate::config::AppConfig;
use crate::ui;

pub async fn run(config: &mut AppConfig) -> anyhow::Result<()> {
    // Guard: interactive mode needs a terminal
    if !std::io::stdin().is_terminal() {
        anyhow::bail!(
            "Kein Subcommand angegeben und stdin ist kein Terminal.\n\
             Nutze `auditmyvisitors --help` fuer die Befehlsuebersicht."
        );
    }

    ui::print_welcome();

    setup::ensure_ready(config).await?;

    menu::report_loop(config).await?;

    Ok(())
}
