use std::path::Path;

use crate::domain::package::{BrewType, PackageMigration, PackageSource, RiskLevel};
use crate::domain::risk;
use crate::infrastructure::apt;
use crate::infrastructure::brew::BrewIndex;
use crate::infrastructure::snap;

/// Errors from the scan use case.
#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("APT scan failed: {0}")]
    Apt(#[from] apt::AptError),

    #[error("Homebrew API failed: {0}")]
    Brew(#[from] crate::infrastructure::brew::BrewError),
}

/// Result of a scan, including risk reasons for display.
pub struct ScanResult {
    pub migrations: Vec<PackageMigration>,
    pub risk_reasons: Vec<&'static str>,
}

/// Run a full scan: read APT + Snap packages, match against Homebrew, classify risk.
pub async fn run_scan(dpkg_path: &Path) -> Result<ScanResult, ScanError> {
    // 1. Scan APT packages (manual + all for dependency analysis)
    let (manual_packages, all_packages) = apt::scan_installed(dpkg_path)?;

    // 2. Build essential dependency set from ALL packages
    let essential_deps = apt::find_essential_dependencies(&all_packages);

    // 3. Build initial migration entries from APT
    let mut migrations: Vec<PackageMigration> = manual_packages
        .iter()
        .map(PackageMigration::from_apt)
        .collect();

    // 4. Scan snap packages and add them
    let snap_packages = snap::scan_snaps();
    let snap_start = migrations.len();
    for snap_pkg in &snap_packages {
        migrations.push(PackageMigration::from_apt(snap_pkg));
    }

    // 5. Fetch Homebrew index and match all
    let brew_index = BrewIndex::fetch().await?;
    brew_index.match_packages(&mut migrations);

    // 6. For snap packages, also try snap-specific aliases
    for (i, snap_pkg) in snap_packages.iter().enumerate() {
        let migration = &mut migrations[snap_start + i];
        if migration.brew_name.is_none() {
            if let Some(brew_name) = snap::snap_brew_alias(&snap_pkg.name) {
                if let Some((name, version, brew_type)) = brew_index.find_match(&brew_name) {
                    migration.brew_name = Some(name);
                    migration.brew_version = Some(version);
                    migration.brew_type = Some(brew_type);
                }
            }
        }
    }

    // 7. Classify risk and set default selection
    let all_apt_and_snap: Vec<_> = manual_packages.iter().chain(snap_packages.iter()).collect();

    let mut risk_reasons = Vec::new();
    for (migration, pkg) in migrations.iter_mut().zip(all_apt_and_snap.iter()) {
        if pkg.source == PackageSource::Snap {
            // Snap packages are always user-space — if they have a brew match, select them
            migration.risk = RiskLevel::Low;
            migration.is_selected = migration.brew_name.is_some();
            risk_reasons.push(if migration.brew_name.is_some() {
                "snap → brew"
            } else {
                "snap (no brew match)"
            });
        } else {
            let risk_level = risk::classify(pkg, &essential_deps);
            let reason = risk::classify_reason(pkg, &essential_deps);
            migration.is_selected = risk_level == RiskLevel::Low && migration.brew_name.is_some();
            migration.risk = risk_level;
            risk_reasons.push(reason);
        }
    }

    Ok(ScanResult {
        migrations,
        risk_reasons,
    })
}
