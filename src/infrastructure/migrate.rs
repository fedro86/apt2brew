use crate::domain::package::{BrewType, MigrationResult, PackageSource};
use crate::domain::pkg_name::is_valid_package_name;

/// Cache sudo credentials right before removal. If already cached, this is a no-op.
/// Returns false if the user fails to authenticate.
pub fn warm_sudo() -> bool {
    eprintln!("This operation will need sudo to remove APT/snap packages.");
    let status = std::process::Command::new("sudo")
        .args(["-v"])
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status();

    status.is_ok_and(|s| s.success())
}

/// Errors from migration operations.
#[derive(Debug, thiserror::Error)]
pub enum MigrateError {
    #[error("brew install failed for {0}: {1}")]
    BrewInstall(String, String),

    #[error("PATH verification failed for {0}: brew binary not found")]
    PathVerify(String),

    #[error("apt remove failed for {0}: {1}")]
    AptRemove(String, String),

    #[error("refusing to run with invalid package name: {0:?}")]
    InvalidName(String),

    #[error(
        "apt remove would also remove {} unrequested package(s): {}. \
         Aborting to avoid cascade. Review manually with `apt-get -s remove {}`",
        .extras.len(),
        .extras.join(", "),
        .requested.join(" ")
    )]
    AptRemoveCascade {
        requested: Vec<String>,
        extras: Vec<String>,
    },

    #[error("failed to simulate apt remove: {0}")]
    AptSimulate(String),

    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
}

/// Install a package via Homebrew (formula or cask).
pub fn brew_install(brew_name: &str, brew_type: &BrewType) -> Result<(), MigrateError> {
    if !is_valid_package_name(brew_name) {
        return Err(MigrateError::InvalidName(brew_name.to_string()));
    }
    let args: Vec<&str> = match brew_type {
        BrewType::Formula => vec!["install", "--", brew_name],
        BrewType::Cask => vec!["install", "--cask", "--", brew_name],
    };
    let output = std::process::Command::new("brew")
        .args(&args)
        .output()
        .map_err(|e| MigrateError::BrewInstall(brew_name.to_string(), e.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let reason = extract_brew_error(&stderr, output.status.code());
        return Err(MigrateError::BrewInstall(brew_name.to_string(), reason));
    }

    Ok(())
}

/// Extract a clean error message from brew's verbose stderr output.
/// Includes the exit code when stderr offers nothing actionable, so the user
/// at least sees a non-zero signal rather than a flat "unknown error".
fn extract_brew_error(stderr: &str, exit_code: Option<i32>) -> String {
    for line in stderr.lines() {
        let trimmed = line.trim();
        if let Some(msg) = trimmed.strip_prefix("Error:") {
            return msg.trim().to_string();
        }
    }
    let last = stderr.lines().rev().find(|l| !l.trim().is_empty());
    match (last, exit_code) {
        (Some(line), _) => line.trim().to_string(),
        (None, Some(code)) => format!("brew exited with status {code} (no stderr)"),
        (None, None) => "brew exited unsuccessfully (no stderr, no exit code)".to_string(),
    }
}

/// Verify that brew successfully installed the formula/cask.
pub fn verify_installed(brew_name: &str, brew_type: &BrewType) -> Result<(), MigrateError> {
    if !is_valid_package_name(brew_name) {
        return Err(MigrateError::InvalidName(brew_name.to_string()));
    }
    let args: Vec<&str> = match brew_type {
        BrewType::Formula => vec!["list", "--", brew_name],
        BrewType::Cask => vec!["list", "--cask", "--", brew_name],
    };
    let output = std::process::Command::new("brew")
        .args(&args)
        .output()
        .map_err(|_| MigrateError::PathVerify(brew_name.to_string()))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(MigrateError::PathVerify(brew_name.to_string()))
    }
}

/// Remove multiple snap packages in a single sudo call.
pub fn snap_remove_batch(snap_names: &[&str]) -> Result<(), MigrateError> {
    if snap_names.is_empty() {
        return Ok(());
    }
    if let Some(bad) = snap_names.iter().find(|n| !is_valid_package_name(n)) {
        return Err(MigrateError::InvalidName((*bad).to_string()));
    }

    // `snap` CLI does not accept `--` as end-of-options; names are validated above.
    let mut args = vec!["snap", "remove"];
    args.extend(snap_names);

    let status = std::process::Command::new("sudo")
        .args(&args)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .map_err(|e| MigrateError::AptRemove(snap_names.join(", "), e.to_string()))?;

    if !status.success() {
        return Err(MigrateError::AptRemove(
            snap_names.join(", "),
            "snap remove failed".to_string(),
        ));
    }

    Ok(())
}

/// Parse `apt-get -s remove` stdout to extract the packages that would be removed.
///
/// The relevant section looks like:
/// ```text
/// The following packages will be REMOVED:
///   foo bar:amd64 baz
/// 0 upgraded, 0 newly installed, 3 to remove and 0 not upgraded.
/// ```
pub fn parse_apt_simulation(output: &str) -> Vec<String> {
    let mut in_section = false;
    let mut pkgs = Vec::new();
    for line in output.lines() {
        if line.starts_with("The following packages will be REMOVED") {
            in_section = true;
            continue;
        }
        if in_section {
            if line.starts_with(' ') || line.starts_with('\t') {
                for token in line.split_whitespace() {
                    // Strip architecture suffix ("foo:amd64" -> "foo") and trailing *
                    let name = token.split(':').next().unwrap_or(token);
                    let name = name.trim_end_matches('*');
                    if is_valid_package_name(name) {
                        pkgs.push(name.to_string());
                    }
                }
            } else {
                break;
            }
        }
    }
    pkgs
}

/// Run `apt-get -s remove` to simulate a removal. Returns the list of packages
/// apt would actually remove (may be larger than `apt_names` due to reverse
/// dependencies). Blocks shell interpretation via the `--` end-of-options marker.
fn simulate_apt_remove(apt_names: &[&str]) -> Result<Vec<String>, MigrateError> {
    let mut args = vec![
        "apt-get",
        "-s",
        "remove",
        "-y",
        "-o",
        "APT::Get::AutomaticRemove=false",
        "--",
    ];
    args.extend(apt_names);

    let output = std::process::Command::new("sudo")
        .args(&args)
        .output()
        .map_err(|e| MigrateError::AptSimulate(e.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(MigrateError::AptSimulate(stderr.trim().to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_apt_simulation(&stdout))
}

/// Remove multiple packages via APT in a single sudo call.
///
/// Simulates the removal first (`apt-get -s remove`) and refuses if apt would
/// also pull in unrequested packages — a common failure mode when the user
/// selects a library that other packages still depend on.
pub fn apt_remove_batch(apt_names: &[&str]) -> Result<(), MigrateError> {
    if apt_names.is_empty() {
        return Ok(());
    }
    if let Some(bad) = apt_names.iter().find(|n| !is_valid_package_name(n)) {
        return Err(MigrateError::InvalidName((*bad).to_string()));
    }

    // Simulate first to detect cascade.
    let will_remove = simulate_apt_remove(apt_names)?;
    let requested: std::collections::HashSet<&str> = apt_names.iter().copied().collect();
    let extras: Vec<String> = will_remove
        .iter()
        .filter(|n| !requested.contains(n.as_str()))
        .cloned()
        .collect();

    if !extras.is_empty() {
        return Err(MigrateError::AptRemoveCascade {
            requested: apt_names.iter().map(|s| (*s).to_string()).collect(),
            extras,
        });
    }

    let mut args = vec![
        "apt",
        "remove",
        "-y",
        "-o",
        "APT::Get::AutomaticRemove=false",
        "--",
    ];
    args.extend(apt_names);

    let status = std::process::Command::new("sudo")
        .args(&args)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .map_err(|e| MigrateError::AptRemove(apt_names.join(", "), e.to_string()))?;

    if !status.success() {
        return Err(MigrateError::AptRemove(
            apt_names.join(", "),
            "apt remove failed".to_string(),
        ));
    }

    Ok(())
}

/// Cheap check: does brew already have this formula/cask installed?
/// Used to distinguish "we installed it in this run" from "it was already there"
/// so the user can tell whether their brew copy is fresh or pre-existing.
fn already_installed(brew_name: &str, brew_type: &BrewType) -> bool {
    if !is_valid_package_name(brew_name) {
        return false;
    }
    let args: Vec<&str> = match brew_type {
        BrewType::Formula => vec!["list", "--", brew_name],
        BrewType::Cask => vec!["list", "--cask", "--", brew_name],
    };
    std::process::Command::new("brew")
        .args(&args)
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Install a single package via brew and verify. Does NOT remove from APT.
///
/// If brew already has the formula/cask, the `brew install` call is skipped
/// and `was_already_installed` is set to true — APT removal is still safe
/// (brew has the package) but the user is informed the copy wasn't refreshed.
pub fn brew_install_and_verify(
    apt_name: &str,
    brew_name: &str,
    brew_type: &BrewType,
    source: PackageSource,
) -> MigrationResult {
    let preexisting = already_installed(brew_name, brew_type);

    // Step 1: Install via brew (skip if already present to avoid confusing
    // "already installed" warnings and to preserve the user's existing copy).
    if !preexisting && let Err(e) = brew_install(brew_name, brew_type) {
        return MigrationResult {
            package: apt_name.to_string(),
            brew_name: brew_name.to_string(),
            source,
            brew_installed: false,
            path_verified: false,
            apt_removed: false,
            was_already_installed: false,
            error: Some(e.to_string()),
        };
    }

    // Step 2: Verify brew actually has the formula/cask (covers both
    // "we just installed it" and "it was pre-existing").
    if let Err(e) = verify_installed(brew_name, brew_type) {
        return MigrationResult {
            package: apt_name.to_string(),
            brew_name: brew_name.to_string(),
            source,
            brew_installed: !preexisting,
            path_verified: false,
            apt_removed: false,
            was_already_installed: preexisting,
            error: Some(format!(
                "brew install succeeded but verification failed: {e}"
            )),
        };
    }

    MigrationResult {
        package: apt_name.to_string(),
        brew_name: brew_name.to_string(),
        source,
        brew_installed: true,
        path_verified: true,
        apt_removed: false, // will be set by batch apt/snap remove
        was_already_installed: preexisting,
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brew_install_rejects_invalid_name_without_spawning() {
        let err = brew_install("--reinstall=evil", &BrewType::Formula).unwrap_err();
        assert!(matches!(err, MigrateError::InvalidName(_)));
    }

    #[test]
    fn verify_installed_rejects_invalid_name_without_spawning() {
        let err = verify_installed("-oEvil", &BrewType::Formula).unwrap_err();
        assert!(matches!(err, MigrateError::InvalidName(_)));
    }

    #[test]
    fn apt_remove_batch_rejects_any_invalid_name_without_spawning() {
        let err = apt_remove_batch(&["git", "--purge"]).unwrap_err();
        assert!(matches!(err, MigrateError::InvalidName(_)));
    }

    #[test]
    fn apt_remove_batch_empty_is_noop() {
        assert!(apt_remove_batch(&[]).is_ok());
    }

    #[test]
    fn snap_remove_batch_rejects_any_invalid_name_without_spawning() {
        let err = snap_remove_batch(&["-oFoo"]).unwrap_err();
        assert!(matches!(err, MigrateError::InvalidName(_)));
    }

    #[test]
    fn parse_apt_simulation_extracts_flat_list() {
        let out = "Reading package lists...\n\
                   The following packages will be REMOVED:\n  \
                   git bat fd\n\
                   0 upgraded, 0 newly installed, 3 to remove and 0 not upgraded.\n";
        let pkgs = parse_apt_simulation(out);
        assert_eq!(pkgs, vec!["git", "bat", "fd"]);
    }

    #[test]
    fn parse_apt_simulation_strips_arch_suffix() {
        let out = "The following packages will be REMOVED:\n  \
                   git:amd64 libfoo:i386\n\
                   0 upgraded, 0 newly installed, 2 to remove and 0 not upgraded.\n";
        let pkgs = parse_apt_simulation(out);
        assert_eq!(pkgs, vec!["git", "libfoo"]);
    }

    #[test]
    fn parse_apt_simulation_handles_multiline_list() {
        let out = "The following packages will be REMOVED:\n  \
                   git bat\n  \
                   fd ripgrep\n\
                   0 upgraded, 0 newly installed, 4 to remove and 0 not upgraded.\n";
        let pkgs = parse_apt_simulation(out);
        assert_eq!(pkgs, vec!["git", "bat", "fd", "ripgrep"]);
    }

    #[test]
    fn parse_apt_simulation_ignores_unrelated_output() {
        let out = "Reading package lists... Done\n\
                   Building dependency tree\n\
                   0 upgraded, 0 newly installed, 0 to remove and 0 not upgraded.\n";
        let pkgs = parse_apt_simulation(out);
        assert!(pkgs.is_empty());
    }

    #[test]
    fn parse_apt_simulation_rejects_flag_shaped_tokens() {
        // Defense in depth: even if a hostile apt lookalike echoed a flag,
        // the name validator filters it out.
        let out = "The following packages will be REMOVED:\n  \
                   git --reinstall foo\n\
                   0 upgraded, 0 newly installed, 2 to remove and 0 not upgraded.\n";
        let pkgs = parse_apt_simulation(out);
        assert_eq!(pkgs, vec!["git", "foo"]);
    }
}
