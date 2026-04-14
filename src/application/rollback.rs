use std::path::PathBuf;

use crate::infrastructure::rollback::{self, RollbackEntry, RollbackError};

/// List available rollback scripts.
pub fn list_rollbacks() -> Result<Vec<(PathBuf, Vec<RollbackEntry>)>, RollbackError> {
    let scripts = rollback::find_rollback_scripts()?;

    let mut results = Vec::new();
    for path in scripts {
        let entries = rollback::parse_rollback_script(&path)?;
        results.push((path, entries));
    }

    Ok(results)
}

/// Execute a full rollback from the most recent script.
pub fn run_rollback(yes: bool) -> Result<(), RollbackError> {
    let scripts = rollback::find_rollback_scripts()?;
    let script = scripts.last().ok_or(RollbackError::NoScripts)?;
    let entries = rollback::parse_rollback_script(script)?;

    if entries.is_empty() {
        println!("  No packages to rollback in the latest script.");
        return Ok(());
    }

    println!("\n  Rollback will restore {} packages:\n", entries.len());
    for entry in &entries {
        println!(
            "    apt install {}  +  brew uninstall {}",
            entry.apt_name, entry.brew_name
        );
    }

    if !yes {
        println!("\n  Proceed? [y/N] ");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).ok();
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("  Cancelled.");
            return Ok(());
        }
    }

    execute_rollback_entries(&entries);
    Ok(())
}

/// Rollback a single package by name.
pub fn run_rollback_single(package: &str, yes: bool) -> Result<(), RollbackError> {
    let scripts = rollback::find_rollback_scripts()?;

    for script in scripts.iter().rev() {
        let entries = rollback::parse_rollback_script(script)?;
        if let Some(entry) = entries.iter().find(|e| e.apt_name == package) {
            println!(
                "\n  Will rollback: apt install {}  +  brew uninstall {}",
                entry.apt_name, entry.brew_name
            );

            if !yes {
                println!("\n  Proceed? [y/N] ");
                let mut input = String::new();
                std::io::stdin().read_line(&mut input).ok();
                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("  Cancelled.");
                    return Ok(());
                }
            }

            execute_rollback_entries(std::slice::from_ref(entry));
            return Ok(());
        }
    }

    println!("  Package '{package}' not found in any rollback script.");
    Ok(())
}

/// Execute rollback: brew uninstall each, then batch sudo apt install.
fn execute_rollback_entries(entries: &[RollbackEntry]) {
    // Phase 1: brew uninstall (no sudo needed)
    println!("\n  Phase 1: Removing from Homebrew...\n");
    for (i, entry) in entries.iter().enumerate() {
        println!(
            "  [{}/{}] brew uninstall {}",
            i + 1,
            entries.len(),
            entry.brew_name
        );
        match rollback::brew_uninstall(&entry.brew_name) {
            Ok(()) => println!("          OK"),
            Err(e) => println!("          FAILED: {e}"),
        }
    }

    // Phase 2: batch apt install (single sudo)
    let apt_names: Vec<&str> = entries.iter().map(|e| e.apt_name.as_str()).collect();
    println!(
        "\n  Phase 2: Reinstalling {} packages via APT (requires sudo)...\n",
        apt_names.len()
    );

    match rollback::apt_install_batch(&apt_names) {
        Ok(()) => println!("\n  Rollback complete.\n"),
        Err(e) => println!("\n  APT reinstall failed: {e}\n"),
    }
}
