use std::fs;
use std::path::PathBuf;

/// Errors from rollback operations.
#[derive(Debug, thiserror::Error)]
pub enum RollbackError {
    #[error("no rollback scripts found in ~/.apt2brew/")]
    NoScripts,

    #[error("failed to read rollback script: {0}")]
    Read(#[from] std::io::Error),

    #[error("apt install failed for {0}: {1}")]
    AptInstall(String, String),

    #[error("brew uninstall failed for {0}: {1}")]
    BrewUninstall(String, String),
}

/// A parsed entry from a rollback script.
#[derive(Debug, Clone)]
pub struct RollbackEntry {
    pub apt_name: String,
    pub brew_name: String,
    pub is_snap: bool,
}

fn base_dir() -> PathBuf {
    std::env::var("HOME")
        .map(|h| PathBuf::from(h).join(".apt2brew"))
        .unwrap_or_else(|_| PathBuf::from("/tmp/.apt2brew"))
}

/// Find all rollback scripts, sorted by timestamp (oldest first).
pub fn find_rollback_scripts() -> Result<Vec<PathBuf>, RollbackError> {
    let dir = base_dir();
    if !dir.exists() {
        return Err(RollbackError::NoScripts);
    }

    let mut scripts: Vec<PathBuf> = fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("rollback-") && n.ends_with(".sh"))
        })
        .collect();

    scripts.sort();
    Ok(scripts)
}

/// Parse a rollback script to extract package entries.
/// Looks for pairs of install commands (apt or snap) and `brew uninstall <name>`.
pub fn parse_rollback_script(path: &PathBuf) -> Result<Vec<RollbackEntry>, RollbackError> {
    let content = fs::read_to_string(path)?;
    let mut entries = Vec::new();
    let mut current_pkg: Option<(String, bool)> = None; // (name, is_snap)

    for line in content.lines() {
        let line = line.trim();

        if let Some(rest) = line.strip_prefix("sudo apt install -y ") {
            current_pkg = Some((rest.trim().to_string(), false));
        } else if let Some(rest) = line.strip_prefix("sudo snap install ") {
            current_pkg = Some((rest.trim().to_string(), true));
        }

        if let Some(rest) = line.strip_prefix("brew uninstall ") {
            let brew_name = rest.trim_end_matches(" || true").trim().to_string();
            if let Some((apt_name, is_snap)) = current_pkg.take() {
                entries.push(RollbackEntry {
                    apt_name,
                    brew_name,
                    is_snap,
                });
            }
        }
    }

    // Fix legacy scripts: check snap aliases for entries marked as apt
    let snap_aliases = super::aliases::snap_aliases();
    for entry in &mut entries {
        if !entry.is_snap && snap_aliases.contains_key(&entry.apt_name) {
            entry.is_snap = true;
        }
    }

    Ok(entries)
}

/// List top-level (user-installed) Homebrew formulae, excluding auto-installed dependencies.
pub fn brew_list_formulae() -> Vec<String> {
    brew_list_cmd(&["leaves"])
}

/// List all installed Homebrew casks (casks have no dependency hierarchy).
pub fn brew_list_casks() -> Vec<String> {
    brew_list_cmd(&["list", "--cask", "-1"])
}

fn brew_list_cmd(args: &[&str]) -> Vec<String> {
    let output = match std::process::Command::new("brew").args(args).output() {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.trim().to_string())
        .collect()
}

/// Execute rollback for a single brew uninstall (no sudo needed).
pub fn brew_uninstall(brew_name: &str) -> Result<(), RollbackError> {
    let output = std::process::Command::new("brew")
        .args(["uninstall", brew_name])
        .output()
        .map_err(|e| RollbackError::BrewUninstall(brew_name.to_string(), e.to_string()))?;

    if !output.status.success() {
        eprintln!("    Warning: brew uninstall {brew_name} failed (may already be removed)");
    }

    Ok(())
}

/// Reinstall snap packages one by one. Returns names that failed.
pub fn snap_install_batch(snap_names: &[&str]) -> Vec<String> {
    let mut failed = Vec::new();
    for name in snap_names {
        let status = std::process::Command::new("sudo")
            .args(["snap", "install", name])
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status();

        if !status.is_ok_and(|s| s.success()) {
            eprintln!("  Failed to reinstall snap: {name}");
            failed.push(name.to_string());
        }
    }
    failed
}

/// Reinstall APT packages one by one. Returns names that failed.
pub fn apt_install_batch(apt_names: &[&str]) -> Vec<String> {
    let mut failed = Vec::new();
    for name in apt_names {
        let status = std::process::Command::new("sudo")
            .args(["apt", "install", "-y", name])
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status();

        if !status.is_ok_and(|s| s.success()) {
            eprintln!("  Failed to reinstall apt: {name}");
            failed.push(name.to_string());
        }
    }
    failed
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn parse_rollback_script_extracts_entries() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"#!/bin/bash
# apt2brew rollback script
set -e

# Rollback: git
echo 'Reinstalling git via APT...'
sudo apt install -y git
echo 'Removing git from Homebrew...'
brew uninstall git || true

# Rollback: bat
echo 'Reinstalling bat via APT...'
sudo apt install -y bat
echo 'Removing bat from Homebrew...'
brew uninstall bat || true
"#
        )
        .unwrap();

        let path = file.path().to_path_buf();
        let entries = parse_rollback_script(&path).unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].apt_name, "git");
        assert_eq!(entries[0].brew_name, "git");
        assert!(!entries[0].is_snap);
        assert_eq!(entries[1].apt_name, "bat");
        assert_eq!(entries[1].brew_name, "bat");
        assert!(!entries[1].is_snap);
    }

    #[test]
    fn parse_empty_rollback_script() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "#!/bin/bash\necho 'Nothing to rollback.'\n").unwrap();

        let path = file.path().to_path_buf();
        let entries = parse_rollback_script(&path).unwrap();
        assert!(entries.is_empty());
    }
}
