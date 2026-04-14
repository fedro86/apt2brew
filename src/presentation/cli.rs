use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "apt2brew",
    about = "Intelligent migration from APT to Homebrew",
    version,
    author
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Scan installed APT packages and show migration analysis (read-only)
    Scan,

    /// Migrate packages from APT to Homebrew
    Migrate {
        /// Non-interactive: show what would happen without executing
        #[arg(long)]
        dry_run: bool,

        /// Non-interactive: use pre-selected packages and execute immediately
        #[arg(long)]
        yes: bool,
    },

    /// Rollback a previous migration
    Rollback {
        /// Rollback a specific package only
        #[arg(long)]
        package: Option<String>,

        /// Skip confirmation prompt
        #[arg(long)]
        yes: bool,
    },
}
