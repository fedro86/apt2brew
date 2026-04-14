use crate::domain::package::{MigrationResult, PackageMigration, PackageSource};
use crate::infrastructure::filesystem;
use crate::infrastructure::migrate as infra_migrate;

/// Full migration workflow:
/// 1. brew install all selected (one by one, with progress)
/// 2. Batch removal from source package managers (apt and/or snap)
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

        let brew_type = pkg
            .brew_type
            .as_ref()
            .unwrap_or(&crate::domain::package::BrewType::Formula);
        let result = infra_migrate::brew_install_and_verify(&pkg.name, brew_name, brew_type);

        if result.error.is_some() {
            println!("          FAILED: {}", result.error.as_deref().unwrap());
        } else {
            println!("          OK");
        }

        results.push(result);
    }

    // Phase 2: batch removal from source managers
    // Split successful results by source (APT vs Snap)
    let succeeded_names: Vec<String> = results
        .iter()
        .filter(|r| r.error.is_none())
        .map(|r| r.package.clone())
        .collect();

    let apt_to_remove: Vec<&str> = succeeded_names
        .iter()
        .filter(|name| {
            selected
                .iter()
                .find(|p| p.name == name.as_str())
                .is_some_and(|p| !is_snap_package(p))
        })
        .map(|s| s.as_str())
        .collect();

    let snap_to_remove: Vec<&str> = succeeded_names
        .iter()
        .filter(|name| {
            selected
                .iter()
                .find(|p| p.name == name.as_str())
                .is_some_and(|p| is_snap_package(p))
        })
        .map(|s| s.as_str())
        .collect();

    if !apt_to_remove.is_empty() {
        println!(
            "\n  Phase 2a: Removing {} APT packages (requires sudo)...\n",
            apt_to_remove.len()
        );
        match infra_migrate::apt_remove_batch(&apt_to_remove) {
            Ok(()) => {
                mark_removed(&mut results, &apt_to_remove);
                println!("\n  APT removal complete.");
            }
            Err(e) => println!("\n  APT removal failed: {e}"),
        }
    }

    if !snap_to_remove.is_empty() {
        println!(
            "\n  Phase 2b: Removing {} snap packages (requires sudo)...\n",
            snap_to_remove.len()
        );
        match infra_migrate::snap_remove_batch(&snap_to_remove) {
            Ok(()) => {
                mark_removed(&mut results, &snap_to_remove);
                println!("\n  Snap removal complete.");
            }
            Err(e) => println!("\n  Snap removal failed: {e}"),
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

fn is_snap_package(pkg: &PackageMigration) -> bool {
    pkg.source == PackageSource::Snap
}

fn mark_removed(results: &mut [MigrationResult], names: &[&str]) {
    for r in results.iter_mut() {
        if r.error.is_none() && names.contains(&r.package.as_str()) {
            r.apt_removed = true;
        }
    }
}
