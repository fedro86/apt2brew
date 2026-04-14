use std::path::Path;

use crate::domain::package::{PackageMigration, RiskLevel};
use crate::domain::risk;
use crate::infrastructure::apt;
use crate::infrastructure::brew::BrewIndex;

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

/// Run a full scan: read APT packages, match against Homebrew, classify risk.
pub async fn run_scan(dpkg_path: &Path) -> Result<ScanResult, ScanError> {
    // 1. Scan APT packages (manual + all for dependency analysis)
    let (manual_packages, all_packages) = apt::scan_installed(dpkg_path)?;

    // 2. Build essential dependency set from ALL packages
    let essential_deps = apt::find_essential_dependencies(&all_packages);

    // 3. Build initial migration entries
    let mut migrations: Vec<PackageMigration> = manual_packages
        .iter()
        .map(PackageMigration::from_apt)
        .collect();

    // 4. Fetch Homebrew index and match
    let brew_index = BrewIndex::fetch().await?;
    brew_index.match_packages(&mut migrations);

    // 5. Classify risk and set default selection
    let mut risk_reasons = Vec::new();
    for (migration, apt_pkg) in migrations.iter_mut().zip(manual_packages.iter()) {
        let risk_level = risk::classify(apt_pkg, &essential_deps);
        let reason = risk::classify_reason(apt_pkg, &essential_deps);
        migration.is_selected = risk_level == RiskLevel::Low && migration.brew_name.is_some();
        migration.risk = risk_level;
        risk_reasons.push(reason);
    }

    Ok(ScanResult {
        migrations,
        risk_reasons,
    })
}
