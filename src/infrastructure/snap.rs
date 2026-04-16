use crate::domain::package::{AptPackage, PackageSource};
use crate::domain::pkg_name::is_valid_package_name;

/// Snap packages that are part of the snap infrastructure itself — never migrate.
const SNAP_SYSTEM: &[&str] = &[
    "bare",
    "core",
    "core18",
    "core20",
    "core22",
    "core24",
    "snapd",
    "snap-store",
    "firmware-updater",
    "gtk-common-themes",
    "canonical-livepatch",
];

use super::aliases;

/// Scan installed snap packages, filtering out system snaps and gnome libraries.
pub fn scan_snaps() -> Vec<AptPackage> {
    let output = match std::process::Command::new("snap").arg("list").output() {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(), // snap not installed or failed
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_snap_list(&stdout)
}

/// Parse the output of `snap list`. Validates the header before reading any
/// rows so a future format change in snapd can't silently produce wrong data.
fn parse_snap_list(stdout: &str) -> Vec<AptPackage> {
    let mut lines = stdout.lines();
    let Some(header) = lines.next() else {
        return Vec::new();
    };
    // Expected layout (snapd ≥ 2.x): "Name Version Rev Tracking Publisher Notes".
    // We only consume name (col 0) and version (col 1); refuse to parse if the
    // first two header tokens drift.
    let header_tokens: Vec<&str> = header.split_whitespace().collect();
    if header_tokens
        .first()
        .map(|s| s.eq_ignore_ascii_case("name"))
        != Some(true)
        || header_tokens
            .get(1)
            .map(|s| s.eq_ignore_ascii_case("version"))
            != Some(true)
    {
        eprintln!("  Warning: unexpected `snap list` header ({header:?}); skipping snap detection");
        return Vec::new();
    }

    lines
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 2 {
                return None;
            }

            let name = parts[0];
            let version = parts[1];

            // Reject any name that wouldn't be safe to pass to `snap`/`sudo snap` argv.
            if !is_valid_package_name(name) {
                return None;
            }

            // Skip system snaps
            if SNAP_SYSTEM.contains(&name) {
                return None;
            }

            // Skip gnome runtime snaps
            if name.starts_with("gnome-") {
                return None;
            }

            // Skip ffmpeg snap runtime (not user-facing)
            if name.starts_with("ffmpeg-") {
                return None;
            }

            Some(AptPackage {
                name: name.to_string(),
                version: version.to_string(),
                section: Some("snap".to_string()),
                priority: None,
                depends: vec![],
                source: PackageSource::Snap,
                has_systemd_unit: false,
                has_init_script: false,
                has_sbin_files: false,
                has_etc_config: false,
                is_library: false,
                reverse_dep_count: 0,
            })
        })
        .collect()
}

/// Get the brew name for a snap package from JSON aliases.
pub fn snap_brew_alias(snap_name: &str) -> Option<String> {
    let snap_aliases = aliases::snap_aliases();
    snap_aliases.get(snap_name).map(|(name, _)| name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_snap_list_extracts_user_snaps() {
        let out = "Name      Version  Rev    Tracking       Publisher  Notes\n\
                   code      1.85.1   162    latest/stable  vscode✓    classic\n\
                   firefox   119.0    3358   latest/stable  mozilla✓   -\n\
                   core22    20231001 1033   latest/stable  canonical✓ base\n";
        let pkgs = parse_snap_list(out);
        let names: Vec<&str> = pkgs.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["code", "firefox"]); // core22 filtered as system
    }

    #[test]
    fn parse_snap_list_rejects_unexpected_header() {
        let out = "Foo      Bar  Rev\nfoo 1.0 1\n";
        assert!(parse_snap_list(out).is_empty());
    }

    #[test]
    fn parse_snap_list_handles_empty_output() {
        assert!(parse_snap_list("").is_empty());
        assert!(parse_snap_list("Name Version Rev\n").is_empty());
    }

    #[test]
    fn parse_snap_list_filters_invalid_names() {
        let out = "Name Version Rev\n--evil 1.0 1\ncode 2.0 2\n";
        let pkgs = parse_snap_list(out);
        assert_eq!(pkgs.len(), 1);
        assert_eq!(pkgs[0].name, "code");
    }
}
