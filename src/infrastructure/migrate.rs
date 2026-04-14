use crate::domain::package::{BrewType, MigrationResult};

/// Errors from migration operations.
#[derive(Debug, thiserror::Error)]
pub enum MigrateError {
    #[error("brew install failed for {0}: {1}")]
    BrewInstall(String, String),

    #[error("PATH verification failed for {0}: brew binary not found")]
    PathVerify(String),

    #[error("apt remove failed for {0}: {1}")]
    AptRemove(String, String),

    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
}

/// Install a package via Homebrew (formula or cask).
pub fn brew_install(brew_name: &str, brew_type: &BrewType) -> Result<(), MigrateError> {
    let args = match brew_type {
        BrewType::Formula => vec!["install", brew_name],
        BrewType::Cask => vec!["install", "--cask", brew_name],
    };
    let output = std::process::Command::new("brew")
        .args(&args)
        .output()
        .map_err(|e| MigrateError::BrewInstall(brew_name.to_string(), e.to_string()))?;

    if !output.status.success() {
        return Err(MigrateError::BrewInstall(
            brew_name.to_string(),
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    Ok(())
}

/// Verify that brew successfully installed the formula/cask.
pub fn verify_installed(brew_name: &str, brew_type: &BrewType) -> Result<(), MigrateError> {
    let args = match brew_type {
        BrewType::Formula => vec!["list", brew_name],
        BrewType::Cask => vec!["list", "--cask", brew_name],
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

/// Remove multiple packages via APT in a single sudo call.
pub fn apt_remove_batch(apt_names: &[&str]) -> Result<(), MigrateError> {
    if apt_names.is_empty() {
        return Ok(());
    }

    let mut args = vec!["apt", "remove", "-y"];
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

/// Install a single package via brew and verify. Does NOT remove from APT.
pub fn brew_install_and_verify(
    apt_name: &str,
    brew_name: &str,
    brew_type: &BrewType,
) -> MigrationResult {
    // Step 1: Install via brew
    if let Err(e) = brew_install(brew_name, brew_type) {
        return MigrationResult {
            package: apt_name.to_string(),
            brew_name: brew_name.to_string(),
            brew_installed: false,
            path_verified: false,
            apt_removed: false,
            error: Some(e.to_string()),
        };
    }

    // Step 2: Verify brew actually has the formula/cask
    if let Err(e) = verify_installed(brew_name, brew_type) {
        return MigrationResult {
            package: apt_name.to_string(),
            brew_name: brew_name.to_string(),
            brew_installed: true,
            path_verified: false,
            apt_removed: false,
            error: Some(format!(
                "brew install succeeded but verification failed: {e}"
            )),
        };
    }

    MigrationResult {
        package: apt_name.to_string(),
        brew_name: brew_name.to_string(),
        brew_installed: true,
        path_verified: true,
        apt_removed: false, // will be set by batch apt remove
        error: None,
    }
}
