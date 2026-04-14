use crate::application::scan::ScanResult;
use crate::domain::package::RiskLevel;

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
            "  {:<28} {:<18} {:<18} Reason",
            "APT Package", "APT Version", "Brew Formula"
        );
        println!("  {}", "-".repeat(80));
        for (pkg, reason) in &migratable {
            println!(
                "  {:<28} {:<18} {:<18} {}",
                truncate(&pkg.name, 27),
                truncate(&pkg.apt_version, 17),
                truncate(pkg.brew_name.as_deref().unwrap_or("-"), 17),
                reason,
            );
        }
    }

    // Skipped (brew match but High risk)
    if !skipped.is_empty() {
        println!("\n  SKIPPED — HIGH RISK ({})\n", skipped.len());
        println!(
            "  {:<28} {:<18} {:<18} Reason",
            "APT Package", "APT Version", "Brew Formula"
        );
        println!("  {}", "-".repeat(80));
        for (pkg, reason) in &skipped {
            println!(
                "  {:<28} {:<18} {:<18} {}",
                truncate(&pkg.name, 27),
                truncate(&pkg.apt_version, 17),
                truncate(pkg.brew_name.as_deref().unwrap_or("-"), 17),
                reason,
            );
        }
    }

    // No match
    if !unmatched.is_empty() {
        println!("\n  NO BREW MATCH ({})\n", unmatched.len());
        println!("  {:<28} {:<18} Reason", "APT Package", "APT Version");
        println!("  {}", "-".repeat(50));
        for (pkg, reason) in &unmatched {
            let display_reason = match pkg.risk {
                RiskLevel::High => *reason,
                RiskLevel::Low => "no Homebrew formula",
            };
            println!(
                "  {:<28} {:<18} {}",
                truncate(&pkg.name, 27),
                truncate(&pkg.apt_version, 17),
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

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}…", &s[..max - 1])
    } else {
        s.to_string()
    }
}
