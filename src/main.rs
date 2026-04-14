mod application;
mod domain;
mod infrastructure;
mod presentation;

use std::path::Path;

use clap::Parser;
use presentation::cli::{Cli, Commands};
use presentation::output::print_scan_table;
use presentation::tui::app::TuiOutcome;
use presentation::tui::run_tui;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Scan => {
            let result = scan_packages().await;
            print_scan_table(&result);
        }

        Commands::Migrate { dry_run, yes } => {
            if !dry_run {
                require_brew();
                warm_sudo();
            }

            let result = scan_packages().await;

            if dry_run {
                infrastructure::filesystem::print_dry_run(&result.migrations);
                return;
            }

            if yes {
                // Non-interactive: execute with pre-selected packages
                application::migrate::execute_migration(&result.migrations);
                return;
            }

            // Interactive: TUI selection → confirm → TUI progress
            let packages = match run_tui(result) {
                Ok(TuiOutcome::Confirmed(pkgs)) => pkgs,
                Ok(TuiOutcome::Cancelled) => {
                    println!("Cancelled.");
                    return;
                }
                Err(e) => {
                    eprintln!("TUI error: {e}");
                    std::process::exit(1);
                }
            };

            let selected_count = packages.iter().filter(|p| p.is_selected).count();
            if selected_count == 0 {
                println!("No packages selected for migration.");
                return;
            }

            if let Err(e) = presentation::tui::progress::run_migration_tui(&packages) {
                eprintln!("TUI error: {e}");
                std::process::exit(1);
            }
        }

        Commands::Rollback { package, yes } => {
            if let Some(pkg_name) = package {
                // Single package rollback stays non-interactive
                if let Err(e) = application::rollback::run_rollback_single(&pkg_name, yes) {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            } else if yes {
                // Non-interactive full rollback
                if let Err(e) = application::rollback::run_rollback(true) {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            } else {
                // Interactive TUI rollback
                if let Err(e) = presentation::tui::rollback_tui::run_rollback_tui() {
                    eprintln!("TUI error: {e}");
                    std::process::exit(1);
                }
            }
        }
    }
}

/// Ask for sudo password upfront so it's cached for later apt/snap remove.
fn warm_sudo() {
    eprintln!("This operation will need sudo to remove APT/snap packages.");
    let status = std::process::Command::new("sudo")
        .args(["-v"])
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status();

    if !status.is_ok_and(|s| s.success()) {
        eprintln!("Failed to obtain sudo. Aborting.");
        std::process::exit(1);
    }
}

fn require_brew() {
    if which::which("brew").is_ok() {
        return;
    }

    let linuxbrew = "/home/linuxbrew/.linuxbrew/bin/brew";
    if std::path::Path::new(linuxbrew).exists() {
        let shell = std::env::var("SHELL").unwrap_or_default();
        let (rc_file, add_cmd) = if shell.ends_with("fish") {
            (
                "~/.config/fish/config.fish",
                "/home/linuxbrew/.linuxbrew/bin/brew shellenv fish | source",
            )
        } else if shell.ends_with("zsh") {
            (
                "~/.zshrc",
                "eval \"$(/home/linuxbrew/.linuxbrew/bin/brew shellenv zsh)\"",
            )
        } else {
            (
                "~/.bashrc",
                "eval \"$(/home/linuxbrew/.linuxbrew/bin/brew shellenv)\"",
            )
        };

        eprintln!("Homebrew is installed but not in your PATH.");
        eprintln!();
        eprintln!("  Add brew to your shell profile and reload:");
        eprintln!();
        eprintln!("    echo '{add_cmd}' >> {rc_file}");
        eprintln!("    source {rc_file}");
        eprintln!();
        eprintln!("  Then re-run: apt2brew migrate");
        std::process::exit(1);
    }

    eprintln!("Error: Homebrew is not installed.");
    eprintln!();
    eprintln!("  Run this in your terminal:");
    eprintln!();
    eprintln!(
        "    /bin/bash -c \"$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)\""
    );
    eprintln!();
    eprintln!("  Then re-run: apt2brew migrate");
    std::process::exit(1);
}

async fn scan_packages() -> application::scan::ScanResult {
    let dpkg_path = Path::new("/var/lib/dpkg/status");

    eprintln!("Scanning APT packages...");
    match application::scan::run_scan(dpkg_path).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}
