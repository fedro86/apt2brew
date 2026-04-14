/// Risk level for migrating a package from APT to Homebrew.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RiskLevel {
    /// User-space: CLI tools, runtimes, dev libraries.
    /// Safe to migrate — pre-selected by default.
    Low,
    /// System-space: daemons, drivers, networking, kernel-related.
    /// Must remain under APT — deselected by default.
    High,
}

/// Which package manager installed this package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageSource {
    Manual,
    Automatic,
    Snap,
}

/// A single APT package as read from dpkg.
#[derive(Debug, Clone)]
pub struct AptPackage {
    pub name: String,
    pub version: String,
    pub section: Option<String>,
    pub priority: Option<String>,
    pub depends: Vec<String>,
    pub source: PackageSource,
    // File analysis results (populated by dpkg -L)
    pub has_systemd_unit: bool,
    pub has_init_script: bool,
    pub has_sbin_files: bool,
    pub has_etc_config: bool,
    pub is_library: bool,
    pub reverse_dep_count: usize,
}

/// Whether a brew match is a formula or a cask.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrewType {
    Formula,
    Cask,
}

/// A package evaluated for migration from APT to Homebrew.
#[derive(Debug, Clone)]
pub struct PackageMigration {
    pub name: String,
    pub apt_version: String,
    pub brew_name: Option<String>,
    pub brew_version: Option<String>,
    pub brew_type: Option<BrewType>,
    pub source: PackageSource,
    pub risk: RiskLevel,
    pub is_selected: bool,
}

impl PackageMigration {
    pub fn from_apt(apt: &AptPackage) -> Self {
        Self {
            name: apt.name.clone(),
            apt_version: apt.version.clone(),
            brew_name: None,
            brew_version: None,
            brew_type: None,
            source: apt.source.clone(),
            risk: RiskLevel::Low,
            is_selected: false,
        }
    }
}

/// Result of a single package migration.
#[derive(Debug, Clone)]
pub struct MigrationResult {
    pub package: String,
    pub brew_name: String,
    pub brew_installed: bool,
    pub path_verified: bool,
    pub apt_removed: bool,
    pub error: Option<String>,
}

impl MigrationResult {
    pub fn success(package: &str, brew_name: &str) -> Self {
        Self {
            package: package.to_string(),
            brew_name: brew_name.to_string(),
            brew_installed: true,
            path_verified: true,
            apt_removed: true,
            error: None,
        }
    }

    pub fn failed(package: &str, brew_name: &str, error: String) -> Self {
        Self {
            package: package.to_string(),
            brew_name: brew_name.to_string(),
            brew_installed: false,
            path_verified: false,
            apt_removed: false,
            error: Some(error),
        }
    }
}
