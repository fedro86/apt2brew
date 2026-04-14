use std::collections::HashSet;

use super::package::{AptPackage, RiskLevel};

/// Safety-net list for packages that heuristics alone can't reliably catch.
/// Two categories:
/// 1. Boot/init — removing these bricks the system
/// 2. GNU coreutils/POSIX standard — these install to /usr/bin/ with no /etc/
///    config, so heuristics miss them. But they're part of the OS contract:
///    scripts, other packages, and the user's muscle memory all assume
///    the system grep/curl/tar is the APT-managed one.
const SAFETY_NET: &[&str] = &[
    // Boot / init / package management
    "systemd",
    "init",
    "grub",
    "linux-image",
    "linux-headers",
    "linux-modules",
    "dpkg",
    "apt",
    // GNU coreutils / POSIX standard tools (install to /usr/bin/, no /etc/)
    "coreutils",
    "findutils",
    "diffutils",
    "grep",
    "sed",
    "gawk",
    "tar",
    "gzip",
    "bzip2",
    "curl",
    "wget",
    // Security / crypto (often no /etc/ or sbin but critical)
    "gnupg",
    "gpg",
    "openssl",
    "ca-certificates",
    // Build toolchain (metapackages with no files, missed by heuristics)
    "gcc",
    "g++",
    "make",
    "build-essential",
];

/// Threshold: if N or more installed packages depend on this package,
/// it's too deeply integrated to migrate safely.
const REVERSE_DEP_THRESHOLD: usize = 5;

/// Classify a package's migration risk using heuristic analysis.
///
/// Rules (in order of priority):
/// 1. Safety-net list (minimal, only what heuristics can't catch)
/// 2. Installs systemd units or init.d scripts → daemon
/// 3. Installs binaries in /sbin or /usr/sbin → system admin tool
/// 4. Is a library (name starts with lib) → dependency, not user tool
/// 5. Has high reverse dependency count → too integrated
/// 6. Depended upon by essential/required packages
/// 7. Installs config files in /etc/ → system integration
/// 8. Package section is system-level (kernel, base, libs, drivers)
/// 9. Default → Low (safe to migrate)
pub fn classify(pkg: &AptPackage, essential_deps: &HashSet<String>) -> RiskLevel {
    // Rule 1: safety-net for un-detectable critical packages
    for pattern in SAFETY_NET {
        if pkg.name == *pattern || pkg.name.starts_with(&format!("{pattern}-")) {
            return RiskLevel::High;
        }
    }

    // Rule 2: installs daemon files
    if pkg.has_systemd_unit || pkg.has_init_script {
        return RiskLevel::High;
    }

    // Rule 3: installs sbin binaries (system administration)
    if pkg.has_sbin_files {
        return RiskLevel::High;
    }

    // Rule 4: is a library
    if pkg.is_library {
        return RiskLevel::High;
    }

    // Rule 5: many packages depend on this
    if pkg.reverse_dep_count >= REVERSE_DEP_THRESHOLD {
        return RiskLevel::High;
    }

    // Rule 6: essential/required packages depend on this
    if essential_deps.contains(&pkg.name) {
        return RiskLevel::High;
    }

    // Rule 7: installs config in /etc/
    if pkg.has_etc_config {
        return RiskLevel::High;
    }

    // Rule 8: system-level section
    if let Some(section) = &pkg.section {
        let s = section.to_lowercase();
        if matches!(
            s.as_str(),
            "kernel" | "base" | "libs" | "libdevel" | "drivers"
        ) {
            return RiskLevel::High;
        }
    }

    // Default: user-space tool, safe to migrate
    RiskLevel::Low
}

/// Human-readable reason for the risk classification.
pub fn classify_reason(pkg: &AptPackage, essential_deps: &HashSet<String>) -> &'static str {
    for pattern in SAFETY_NET {
        if pkg.name == *pattern || pkg.name.starts_with(&format!("{pattern}-")) {
            return "system-critical package";
        }
    }

    if pkg.has_systemd_unit {
        return "installs systemd service";
    }
    if pkg.has_init_script {
        return "installs init.d script";
    }
    if pkg.has_sbin_files {
        return "installs sbin binaries";
    }
    if pkg.is_library {
        return "library (not a user tool)";
    }
    if pkg.reverse_dep_count >= REVERSE_DEP_THRESHOLD {
        return "many packages depend on this";
    }
    if essential_deps.contains(&pkg.name) {
        return "required by essential package";
    }
    if pkg.has_etc_config {
        return "has system config in /etc/";
    }

    if let Some(section) = &pkg.section {
        let s = section.to_lowercase();
        if matches!(
            s.as_str(),
            "kernel" | "base" | "libs" | "libdevel" | "drivers"
        ) {
            return "system-level section";
        }
    }

    "user-space tool"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::package::PackageSource;

    fn make_pkg(name: &str, section: Option<&str>) -> AptPackage {
        AptPackage {
            name: name.to_string(),
            version: "1.0".to_string(),
            section: section.map(|s| s.to_string()),
            priority: None,
            depends: vec![],
            source: PackageSource::Manual,
            has_systemd_unit: false,
            has_init_script: false,
            has_sbin_files: false,
            has_etc_config: false,
            is_library: name.starts_with("lib"),
            reverse_dep_count: 0,
        }
    }

    fn empty_deps() -> HashSet<String> {
        HashSet::new()
    }

    #[test]
    fn safety_net_is_high() {
        let deps = empty_deps();
        assert_eq!(classify(&make_pkg("systemd", None), &deps), RiskLevel::High);
        assert_eq!(classify(&make_pkg("dpkg", None), &deps), RiskLevel::High);
        assert_eq!(classify(&make_pkg("apt", None), &deps), RiskLevel::High);
        assert_eq!(
            classify(&make_pkg("linux-image-6.8", None), &deps),
            RiskLevel::High
        );
    }

    #[test]
    fn daemon_is_high() {
        let deps = empty_deps();
        let mut pkg = make_pkg("my-custom-server", Some("net"));
        pkg.has_systemd_unit = true;
        assert_eq!(classify(&pkg, &deps), RiskLevel::High);

        let mut pkg2 = make_pkg("legacy-daemon", None);
        pkg2.has_init_script = true;
        assert_eq!(classify(&pkg2, &deps), RiskLevel::High);
    }

    #[test]
    fn sbin_is_high() {
        let deps = empty_deps();
        let mut pkg = make_pkg("some-admin-tool", None);
        pkg.has_sbin_files = true;
        assert_eq!(classify(&pkg, &deps), RiskLevel::High);
        assert_eq!(classify_reason(&pkg, &deps), "installs sbin binaries");
    }

    #[test]
    fn library_is_high() {
        let deps = empty_deps();
        assert_eq!(
            classify(&make_pkg("libcurl4", None), &deps),
            RiskLevel::High
        );
        assert_eq!(
            classify_reason(&make_pkg("libcurl4", None), &deps),
            "library (not a user tool)"
        );
    }

    #[test]
    fn high_reverse_deps_is_high() {
        let deps = empty_deps();
        let mut pkg = make_pkg("some-core-pkg", None);
        pkg.reverse_dep_count = 10;
        assert_eq!(classify(&pkg, &deps), RiskLevel::High);
        assert_eq!(classify_reason(&pkg, &deps), "many packages depend on this");
    }

    #[test]
    fn etc_config_is_high() {
        let deps = empty_deps();
        let mut pkg = make_pkg("some-config-tool", None);
        pkg.has_etc_config = true;
        assert_eq!(classify(&pkg, &deps), RiskLevel::High);
        assert_eq!(classify_reason(&pkg, &deps), "has system config in /etc/");
    }

    #[test]
    fn essential_dependency_is_high() {
        let mut deps = HashSet::new();
        deps.insert("important-dep".to_string());
        let pkg = make_pkg("important-dep", Some("utils"));
        assert_eq!(classify(&pkg, &deps), RiskLevel::High);
    }

    #[test]
    fn kernel_section_is_high() {
        let deps = empty_deps();
        assert_eq!(
            classify(&make_pkg("some-module", Some("kernel")), &deps),
            RiskLevel::High
        );
    }

    #[test]
    fn cli_tools_are_low() {
        let deps = empty_deps();
        assert_eq!(classify(&make_pkg("bat", None), &deps), RiskLevel::Low);
        assert_eq!(classify(&make_pkg("eza", None), &deps), RiskLevel::Low);
        assert_eq!(classify(&make_pkg("fzf", None), &deps), RiskLevel::Low);
        assert_eq!(
            classify(&make_pkg("htop", Some("utils")), &deps),
            RiskLevel::Low
        );
        assert_eq!(
            classify(&make_pkg("neovim", Some("editors")), &deps),
            RiskLevel::Low
        );
        assert_eq!(
            classify_reason(&make_pkg("bat", None), &deps),
            "user-space tool"
        );
    }
}
