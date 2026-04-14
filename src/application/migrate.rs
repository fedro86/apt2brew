use crate::domain::package::{MigrationResult, PackageMigration};
use crate::infrastructure::filesystem;
use crate::infrastructure::migrate as infra_migrate;

/// Full migration workflow:
/// 1. brew install all selected (one by one, with progress)
/// 2. Single sudo apt remove for all successfully installed (one password prompt)
/// 3. Generate artifacts (Brewfile, rollback script, log)
pub fn execute_migration(packages: &[PackageMigration]) {
    let selected: Vec<_> = packages
        .iter()
        .filter(|p| p.is_selected && p.brew_name.is_some())
        .collect();

    if selected.is_empty() {
        println!("No packages selected for migration.");
        return;
    }

    // Phase 1: brew install all
    println!("\n  Phase 1: Installing via Homebrew...\n");

    let mut results: Vec<MigrationResult> = Vec::new();

    for (i, pkg) in selected.iter().enumerate() {
        let brew_name = pkg.brew_name.as_deref().unwrap();
        println!(
            "  [{}/{}] brew install {brew_name}  ({})",
            i + 1,
            selected.len(),
            pkg.name
        );

        let result = infra_migrate::brew_install_and_verify(&pkg.name, brew_name);

        if result.error.is_some() {
            println!("          FAILED: {}", result.error.as_deref().unwrap());
        } else {
            println!("          OK");
        }

        results.push(result);
    }

    // Phase 2: batch apt remove for successful installs
    let succeeded: Vec<&str> = results
        .iter()
        .filter(|r| r.error.is_none())
        .map(|r| r.package.as_str())
        .collect();

    if !succeeded.is_empty() {
        println!(
            "\n  Phase 2: Removing {} packages from APT (requires sudo)...\n",
            succeeded.len()
        );

        match infra_migrate::apt_remove_batch(&succeeded) {
            Ok(()) => {
                for r in &mut results {
                    if r.error.is_none() {
                        r.apt_removed = true;
                    }
                }
                println!("\n  APT removal complete.");
            }
            Err(e) => {
                println!("\n  APT removal failed: {e}");
                println!("  Brew packages are installed but APT versions were not removed.");
            }
        }
    }

    // Phase 3: generate artifacts
    match filesystem::write_brewfile(packages) {
        Ok(path) => println!("\n  Brewfile: {}", path.display()),
        Err(e) => eprintln!("  Warning: could not write Brewfile: {e}"),
    }

    let rollback_path = match filesystem::write_rollback_script(&results) {
        Ok(path) => path,
        Err(e) => {
            eprintln!("  Warning: could not write rollback script: {e}");
            std::path::PathBuf::from("(failed)")
        }
    };

    let log_path = match filesystem::write_log(&results) {
        Ok(path) => path,
        Err(e) => {
            eprintln!("  Warning: could not write log: {e}");
            std::path::PathBuf::from("(failed)")
        }
    };

    filesystem::print_results(&results, &rollback_path, &log_path);
}
