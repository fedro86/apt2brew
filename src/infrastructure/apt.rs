use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::domain::package::{AptPackage, PackageSource};

/// Errors from APT scanning operations.
#[derive(Debug, thiserror::Error)]
pub enum AptError {
    #[error("failed to read dpkg status file: {0}")]
    DpkgRead(#[from] std::io::Error),

    #[error("failed to run apt-mark: {0}")]
    AptMark(String),
}

/// Parse the dpkg status file and return all installed packages.
pub fn parse_dpkg_status(path: &Path) -> Result<Vec<AptPackage>, AptError> {
    let content = std::fs::read_to_string(path)?;
    Ok(parse_dpkg_status_content(&content))
}

/// Parse dpkg status content from a string (testable without filesystem).
pub fn parse_dpkg_status_content(content: &str) -> Vec<AptPackage> {
    let mut packages = Vec::new();

    for block in content.split("\n\n") {
        let mut name = None;
        let mut version = None;
        let mut status = None;
        let mut section = None;
        let mut priority = None;
        let mut depends = Vec::new();

        for line in block.lines() {
            if let Some(val) = line.strip_prefix("Package: ") {
                name = Some(val.trim().to_string());
            } else if let Some(val) = line.strip_prefix("Version: ") {
                version = Some(val.trim().to_string());
            } else if let Some(val) = line.strip_prefix("Status: ") {
                status = Some(val.trim().to_string());
            } else if let Some(val) = line.strip_prefix("Section: ") {
                section = Some(val.trim().to_string());
            } else if let Some(val) = line.strip_prefix("Priority: ") {
                priority = Some(val.trim().to_string());
            } else if let Some(val) = line.strip_prefix("Depends: ") {
                depends = parse_depends(val);
            }
        }

        // Only include packages that are fully installed
        let is_installed = status
            .as_deref()
            .is_some_and(|s| s.contains("install ok installed"));

        if let (Some(name), Some(version), true) = (name, version, is_installed) {
            let is_library = name.starts_with("lib");
            packages.push(AptPackage {
                name,
                version,
                section,
                priority,
                depends,
                source: PackageSource::Automatic,
                has_systemd_unit: false,
                has_init_script: false,
                has_sbin_files: false,
                has_etc_config: false,
                is_library,
                reverse_dep_count: 0,
            });
        }
    }

    packages
}

/// Parse a Depends line into a list of package names (stripping versions and alternatives).
fn parse_depends(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|dep| {
            // Take the first alternative (before |), strip version constraint
            let dep = dep.split('|').next().unwrap_or(dep).trim();
            dep.split_once(' ')
                .map_or(dep, |(name, _)| name)
                .to_string()
        })
        .filter(|d| !d.is_empty())
        .collect()
}

/// Result of analyzing files installed by a package.
pub struct FileAnalysis {
    pub has_systemd_unit: bool,
    pub has_init_script: bool,
    pub has_sbin_files: bool,
    pub has_etc_config: bool,
}

/// Analyze files installed by a package via `dpkg -L`.
pub fn analyze_installed_files(package_name: &str) -> FileAnalysis {
    let output = std::process::Command::new("dpkg")
        .args(["-L", package_name])
        .output();

    let Ok(output) = output else {
        return FileAnalysis {
            has_systemd_unit: false,
            has_init_script: false,
            has_sbin_files: false,
            has_etc_config: false,
        };
    };

    if !output.status.success() {
        return FileAnalysis {
            has_systemd_unit: false,
            has_init_script: false,
            has_sbin_files: false,
            has_etc_config: false,
        };
    }

    let file_list = String::from_utf8_lossy(&output.stdout);
    let mut result = FileAnalysis {
        has_systemd_unit: false,
        has_init_script: false,
        has_sbin_files: false,
        has_etc_config: false,
    };

    for line in file_list.lines() {
        let line = line.trim();

        // Systemd units
        let is_systemd_path = line.starts_with("/lib/systemd/system/")
            || line.starts_with("/usr/lib/systemd/system/")
            || line.starts_with("/etc/systemd/system/");
        if is_systemd_path
            && (line.ends_with(".service") || line.ends_with(".timer") || line.ends_with(".socket"))
        {
            result.has_systemd_unit = true;
        }

        // Init.d scripts
        if line.starts_with("/etc/init.d/") && line != "/etc/init.d/" {
            result.has_init_script = true;
        }

        // System administration binaries
        if (line.starts_with("/usr/sbin/") || line.starts_with("/sbin/")) && !line.ends_with('/') {
            result.has_sbin_files = true;
        }

        // Configuration files in /etc/ (excluding directories and init.d)
        if line.starts_with("/etc/")
            && !line.ends_with('/')
            && !line.starts_with("/etc/init.d/")
            && !line.starts_with("/etc/systemd/")
        {
            result.has_etc_config = true;
        }
    }

    result
}

/// Build a set of package names that are depended upon by essential/required packages.
pub fn find_essential_dependencies(all_packages: &[AptPackage]) -> HashSet<String> {
    let mut deps_of_essential = HashSet::new();

    for pkg in all_packages {
        let is_essential = pkg
            .priority
            .as_deref()
            .is_some_and(|p| p == "required" || p == "important" || p == "essential");

        if is_essential {
            for dep in &pkg.depends {
                deps_of_essential.insert(dep.clone());
            }
        }
    }

    deps_of_essential
}

/// Get the set of manually installed package names using `apt-mark showmanual`.
pub fn get_manual_packages() -> Result<HashSet<String>, AptError> {
    let output = std::process::Command::new("apt-mark")
        .arg("showmanual")
        .output()
        .map_err(|e| AptError::AptMark(e.to_string()))?;

    if !output.status.success() {
        return Err(AptError::AptMark(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    let manual: HashSet<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    Ok(manual)
}

/// Count how many packages depend on each package name.
pub fn count_reverse_deps(all_packages: &[AptPackage]) -> HashMap<String, usize> {
    let mut counts: HashMap<String, usize> = HashMap::new();

    for pkg in all_packages {
        for dep in &pkg.depends {
            *counts.entry(dep.clone()).or_insert(0) += 1;
        }
    }

    counts
}

/// Scan the system: parse dpkg, filter manual, analyze files and dependencies.
/// Returns (manual_packages, all_packages) — all_packages needed for dependency analysis.
pub fn scan_installed(dpkg_path: &Path) -> Result<(Vec<AptPackage>, Vec<AptPackage>), AptError> {
    let all_packages = parse_dpkg_status(dpkg_path)?;
    let manual = get_manual_packages()?;
    let reverse_deps = count_reverse_deps(&all_packages);

    let mut manual_packages: Vec<AptPackage> = all_packages
        .iter()
        .filter(|pkg| manual.contains(&pkg.name))
        .cloned()
        .map(|mut pkg| {
            pkg.source = PackageSource::Manual;

            // Analyze installed files
            let analysis = analyze_installed_files(&pkg.name);
            pkg.has_systemd_unit = analysis.has_systemd_unit;
            pkg.has_init_script = analysis.has_init_script;
            pkg.has_sbin_files = analysis.has_sbin_files;
            pkg.has_etc_config = analysis.has_etc_config;

            // Set reverse dependency count
            pkg.reverse_dep_count = reverse_deps.get(&pkg.name).copied().unwrap_or(0);

            pkg
        })
        .collect();

    // Sort for consistent output
    manual_packages.sort_by(|a, b| a.name.cmp(&b.name));

    Ok((manual_packages, all_packages))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_DPKG: &str = r#"Package: git
Status: install ok installed
Priority: optional
Section: vcs
Version: 1:2.43.0-1ubuntu7
Depends: libc6 (>= 2.34), libcurl4
Description: fast, scalable, distributed revision control system

Package: docker-ce
Status: install ok installed
Priority: optional
Section: admin
Version: 5:24.0.7-1~ubuntu.22.04~jammy
Depends: containerd.io (>= 1.6), iptables
Description: Docker container runtime

Package: base-files
Status: install ok installed
Priority: required
Section: admin
Version: 12ubuntu4
Depends: libc6 (>= 2.34), coreutils
Description: Debian base system miscellaneous files

Package: libfoo
Status: deinstall ok config-files
Priority: optional
Section: libs
Version: 1.0.0-1
Description: A removed library
"#;

    #[test]
    fn parse_dpkg_extracts_installed_packages() {
        let packages = parse_dpkg_status_content(SAMPLE_DPKG);

        assert_eq!(packages.len(), 3);
        assert_eq!(packages[0].name, "git");
        assert_eq!(packages[0].section.as_deref(), Some("vcs"));
        assert_eq!(packages[1].name, "docker-ce");
        assert_eq!(packages[1].section.as_deref(), Some("admin"));
    }

    #[test]
    fn parse_dpkg_skips_removed_packages() {
        let packages = parse_dpkg_status_content(SAMPLE_DPKG);
        assert!(!packages.iter().any(|p| p.name == "libfoo"));
    }

    #[test]
    fn parse_dpkg_extracts_depends() {
        let packages = parse_dpkg_status_content(SAMPLE_DPKG);
        let git = &packages[0];
        assert!(git.depends.contains(&"libc6".to_string()));
        assert!(git.depends.contains(&"libcurl4".to_string()));
    }

    #[test]
    fn parse_dpkg_extracts_priority() {
        let packages = parse_dpkg_status_content(SAMPLE_DPKG);
        let base = packages.iter().find(|p| p.name == "base-files").unwrap();
        assert_eq!(base.priority.as_deref(), Some("required"));
    }

    #[test]
    fn find_essential_deps_works() {
        let packages = parse_dpkg_status_content(SAMPLE_DPKG);
        let deps = find_essential_dependencies(&packages);
        // base-files is "required" and depends on libc6 and coreutils
        assert!(deps.contains("libc6"));
        assert!(deps.contains("coreutils"));
        // docker-ce is "optional", its deps should not be in the set
        assert!(!deps.contains("containerd.io"));
    }
}
