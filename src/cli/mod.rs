use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "auditmyvisitors",
    about = "Google Analytics 4 & Search Console reporting for website owners",
    version
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Enable verbose logging
    #[arg(long, global = true)]
    pub verbose: bool,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Manage Google authentication
    Auth {
        #[command(subcommand)]
        action: AuthAction,
    },

    /// List or select GA4 / Search Console properties
    Properties {
        #[command(subcommand)]
        action: PropertiesAction,
    },

    /// Generate reports
    Report {
        #[command(subcommand)]
        action: ReportAction,
    },

    /// Export reports
    Export {
        #[command(subcommand)]
        action: ExportAction,
    },
}

#[derive(Debug, Subcommand)]
pub enum AuthAction {
    /// Log in with your Google account
    Login,
    /// Show current authentication status
    Status,
    /// Log out and remove stored tokens
    Logout,
}

#[derive(Debug, Subcommand)]
pub enum PropertiesAction {
    /// List available GA4 and Search Console properties
    List,
    /// Interactively select the active property
    Select,
}

#[derive(Debug, Subcommand)]
pub enum ReportAction {
    /// Overall traffic overview for a time range
    Overview {
        /// Number of days to look back (default: 28)
        #[arg(long, short = 'd')]
        days: Option<u32>,
    },

    /// Top pages ranked by sessions
    TopPages {
        /// Number of days to look back (default: 28)
        #[arg(long, short = 'd')]
        days: Option<u32>,

        /// How many pages to show (default: 20)
        #[arg(long, short = 'n')]
        limit: Option<usize>,

        /// Sort by: sessions (default), clicks, impressions
        #[arg(long, default_value = "sessions")]
        sort_by: String,
    },

    /// Detailed report for a single URL
    Page {
        /// The page URL to analyze
        #[arg(long)]
        url: String,

        /// Number of days to look back (default: 28)
        #[arg(long, short = 'd')]
        days: Option<u32>,
    },

    /// Before/after comparison around a change date
    Compare {
        /// The page URL (or omit for full-site comparison)
        #[arg(long)]
        url: Option<String>,

        /// Days before the change date to include
        #[arg(long, default_value = "30")]
        before: u32,

        /// Days after the change date to include
        #[arg(long, default_value = "30")]
        after: u32,

        /// The change date (YYYY-MM-DD), e.g. a deploy date
        #[arg(long)]
        since: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum ExportAction {
    /// Export a full PDF report (fetches live data)
    Pdf {
        /// Number of days to look back (default: 28)
        #[arg(long, short = 'd')]
        days: Option<u32>,

        /// Output file path (default: ./output/<property>-YYYY-MM-DD.pdf)
        #[arg(long, short = 'o')]
        output: Option<String>,
    },
}
