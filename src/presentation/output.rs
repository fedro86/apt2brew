use crate::application::scan::ScanResult;
use crate::domain::package::{BrewType, PackageSource, RiskLevel};

/// Print the scan results as a formatted table to stdout.
pub fn print_scan_table(result: &ScanResult) {
    let packages = &result.migrations;
    let reasons = &result.risk_reasons;

    let migratable: Vec<_> = packages
        .iter()
        .zip(reasons.iter())
        .filter(|(p, _)| p.is_selected)
        .collect();

    let skipped: Vec<_> = packages
        .iter()
        .zip(reasons.iter())
        .filter(|(p, _)| !p.is_selected && p.brew_name.is_some())
        .collect();

    let unmatched: Vec<_> = packages
        .iter()
        .zip(reasons.iter())
        .filter(|(p, _)| p.brew_name.is_none())
        .collect();

    // Migratable
    if !migratable.is_empty() {
        println!("\n  MIGRATABLE ({})\n", migratable.len());
        println!(
            "  {:<26} {:<16} {:<16} {:<14} Reason",
            "APT Package", "APT Ver", "Brew Formula", "Brew Ver"
        );
        println!("  {}", "-".repeat(95));
        for (pkg, reason) in &migratable {
            println!(
                "  {:<26} {:<16} {:<16} {:<14} {}",
                truncate(&pkg_display(pkg), 25),
                truncate(&pkg.apt_version, 15),
                truncate(&brew_display(pkg), 15),
                truncate(pkg.brew_version.as_deref().unwrap_or("-"), 13),
                reason,
            );
        }
    }

    // Skipped (brew match but High risk)
    if !skipped.is_empty() {
        println!("\n  SKIPPED — HIGH RISK ({})\n", skipped.len());
        println!(
            "  {:<26} {:<16} {:<16} {:<14} Reason",
            "APT Package", "APT Ver", "Brew Formula", "Brew Ver"
        );
        println!("  {}", "-".repeat(95));
        for (pkg, reason) in &skipped {
            println!(
                "  {:<26} {:<16} {:<16} {:<14} {}",
                truncate(&pkg_display(pkg), 25),
                truncate(&pkg.apt_version, 15),
                truncate(&brew_display(pkg), 15),
                truncate(pkg.brew_version.as_deref().unwrap_or("-"), 13),
                reason,
            );
        }
    }

    // No match
    if !unmatched.is_empty() {
        println!("\n  NO BREW MATCH ({})\n", unmatched.len());
        println!("  {:<26} {:<16} Reason", "APT Package", "APT Ver");
        println!("  {}", "-".repeat(55));
        for (pkg, reason) in &unmatched {
            let display_reason = match pkg.risk {
                RiskLevel::High => *reason,
                RiskLevel::Low => "no Homebrew formula",
            };
            println!(
                "  {:<26} {:<16} {}",
                truncate(&pkg_display(pkg), 25),
                truncate(&pkg.apt_version, 15),
                display_reason,
            );
        }
    }

    // Summary
    println!(
        "\n  {} total | {} migratable | {} skipped (high risk) | {} no match\n",
        packages.len(),
        migratable.len(),
        skipped.len(),
        unmatched.len(),
    );
}

fn pkg_display(pkg: &crate::domain::package::PackageMigration) -> String {
    match pkg.source {
        PackageSource::Snap => format!("{} [snap]", pkg.name),
        _ => pkg.name.clone(),
    }
}

fn brew_display(pkg: &crate::domain::package::PackageMigration) -> String {
    let name = pkg.brew_name.as_deref().unwrap_or("-");
    match &pkg.brew_type {
        Some(BrewType::Cask) => format!("{name} (cask)"),
        _ => name.to_string(),
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}…", &s[..max - 1])
    } else {
        s.to_string()
    }
}
