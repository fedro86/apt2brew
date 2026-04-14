use crate::domain::package::{AptPackage, PackageSource};

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

    stdout
        .lines()
        .skip(1) // header row
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 2 {
                return None;
            }

            let name = parts[0];
            let version = parts[1];

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
