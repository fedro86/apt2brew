use crate::domain::package::MigrationResult;

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

/// Install a package via Homebrew.
pub fn brew_install(brew_name: &str) -> Result<(), MigrateError> {
    let output = std::process::Command::new("brew")
        .args(["install", brew_name])
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

/// Verify that brew successfully installed the formula by checking `brew list`.
pub fn verify_installed(brew_name: &str) -> Result<(), MigrateError> {
    let output = std::process::Command::new("brew")
        .args(["list", brew_name])
        .output()
        .map_err(|_| MigrateError::PathVerify(brew_name.to_string()))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(MigrateError::PathVerify(brew_name.to_string()))
    }
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
pub fn brew_install_and_verify(apt_name: &str, brew_name: &str) -> MigrationResult {
    // Step 1: Install via brew
    if let Err(e) = brew_install(brew_name) {
        return MigrationResult {
            package: apt_name.to_string(),
            brew_name: brew_name.to_string(),
            brew_installed: false,
            path_verified: false,
            apt_removed: false,
            error: Some(e.to_string()),
        };
    }

    // Step 2: Verify brew actually has the formula
    if let Err(e) = verify_installed(brew_name) {
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
