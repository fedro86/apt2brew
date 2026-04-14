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
/// Looks for pairs of `sudo apt install -y <name>` and `brew uninstall <name>`.
pub fn parse_rollback_script(path: &PathBuf) -> Result<Vec<RollbackEntry>, RollbackError> {
    let content = fs::read_to_string(path)?;
    let mut entries = Vec::new();
    let mut current_apt: Option<String> = None;

    for line in content.lines() {
        let line = line.trim();

        if let Some(rest) = line.strip_prefix("sudo apt install -y ") {
            current_apt = Some(rest.trim().to_string());
        }

        if let Some(rest) = line.strip_prefix("brew uninstall ") {
            let brew_name = rest.trim_end_matches(" || true").trim().to_string();
            if let Some(apt_name) = current_apt.take() {
                entries.push(RollbackEntry {
                    apt_name,
                    brew_name,
                });
            }
        }
    }

    Ok(entries)
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

/// Batch reinstall packages via APT in a single sudo call.
pub fn apt_install_batch(apt_names: &[&str]) -> Result<(), RollbackError> {
    if apt_names.is_empty() {
        return Ok(());
    }

    let mut args = vec!["apt", "install", "-y"];
    args.extend(apt_names);

    let status = std::process::Command::new("sudo")
        .args(&args)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .map_err(|e| RollbackError::AptInstall(apt_names.join(", "), e.to_string()))?;

    if !status.success() {
        return Err(RollbackError::AptInstall(
            apt_names.join(", "),
            "apt install failed".to_string(),
        ));
    }

    Ok(())
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
        assert_eq!(entries[1].apt_name, "bat");
        assert_eq!(entries[1].brew_name, "bat");
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
