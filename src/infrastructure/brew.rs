use std::collections::{HashMap, HashSet};

use serde::Deserialize;

use super::aliases;
use crate::domain::package::{BrewType, PackageMigration};

/// Errors from Homebrew operations.
#[derive(Debug, thiserror::Error)]
pub enum BrewError {
    #[error("failed to fetch Homebrew formulae: {0}")]
    Fetch(#[from] reqwest::Error),

    #[error("failed to parse Homebrew API response: {0}")]
    #[allow(dead_code)]
    Parse(String),
}

/// Minimal representation of a Homebrew formula from the API.
#[derive(Debug, Deserialize)]
pub(crate) struct BrewFormula {
    name: String,
    #[serde(default)]
    aliases: Vec<String>,
    versions: BrewVersions,
}

#[derive(Debug, Deserialize)]
struct BrewVersions {
    stable: Option<String>,
}

/// Minimal representation of a Homebrew cask from the API.
#[derive(Debug, Deserialize)]
struct BrewCask {
    token: String,
    #[serde(default)]
    old_tokens: Vec<String>,
    version: Option<String>,
}

/// APT suffixes to strip when fuzzy matching.
const APT_SUFFIXES: &[&str] = &[
    "-dev", "-bin", "-tools", "-cli", "-common", "-data", "-doc", "-dbg", "-utils",
];

/// Generate candidate brew names from an APT package name.
/// Each transformation is independent — we try all of them.
fn fuzzy_candidates(apt_name: &str) -> Vec<String> {
    let mut candidates = Vec::new();

    // Strip common APT suffixes: "gdal-bin" → "gdal", "libonig-dev" → "libonig"
    for suffix in APT_SUFFIXES {
        if let Some(base) = apt_name.strip_suffix(suffix) {
            candidates.push(base.to_string());

            // Also try stripping lib prefix: "libonig-dev" → "onig"
            if let Some(without_lib) = base.strip_prefix("lib") {
                candidates.push(without_lib.to_string());
            }
        }
    }

    // Strip lib prefix alone: "libfoo" → "foo"
    if let Some(without_lib) = apt_name.strip_prefix("lib") {
        candidates.push(without_lib.to_string());
    }

    // Strip Debian python3- prefix: "python3-gdal" → "gdal"
    if let Some(without_py) = apt_name.strip_prefix("python3-") {
        candidates.push(without_py.to_string());
    }

    // Version suffix: "python3" → "python@3" (only if name ends with digits)
    if apt_name.chars().last().is_some_and(|c| c.is_ascii_digit()) {
        let alpha_end = apt_name
            .rfind(|c: char| !c.is_ascii_digit())
            .map(|i| i + 1)
            .unwrap_or(0);
        if alpha_end > 0 {
            let base = &apt_name[..alpha_end];
            let ver = &apt_name[alpha_end..];
            candidates.push(format!("{base}@{ver}"));
        }
    }

    candidates
}

/// A matched brew entry: name, version, and whether it's a formula or cask.
#[derive(Clone, Debug, PartialEq, Eq)]
struct BrewEntry {
    name: String,
    version: String,
    brew_type: BrewType,
}

/// Index of brew formulae + casks + external aliases.
pub struct BrewIndex {
    lookup: HashMap<String, BrewEntry>,
    apt_aliases: aliases::AliasMap,
    cask_blocklist: HashSet<String>,
}

impl BrewIndex {
    /// Fetch formulae from Homebrew API and build the index.
    /// Casks are macOS-only (.dmg/.app) and not usable on Linux.
    pub async fn fetch() -> Result<Self, BrewError> {
        let formulae: Vec<BrewFormula> = reqwest::get("https://formulae.brew.sh/api/formula.json")
            .await?
            .json()
            .await?;

        Ok(Self::build(&formulae, &[]))
    }

    /// Build index from pre-fetched data (for testing).
    pub fn from_formulae(formulae: &[BrewFormula]) -> Self {
        Self::build(formulae, &[])
    }

    fn build(formulae: &[BrewFormula], casks: &[BrewCask]) -> Self {
        let apt_aliases = aliases::apt_aliases();
        let cask_blocklist = aliases::cask_blocklist();
        let mut lookup = HashMap::new();

        // Formulae
        for f in formulae {
            let entry = BrewEntry {
                name: f.name.clone(),
                version: f.versions.stable.clone().unwrap_or_default(),
                brew_type: BrewType::Formula,
            };

            lookup.insert(f.name.clone(), entry.clone());
            for alias in &f.aliases {
                lookup.insert(alias.clone(), entry.clone());
            }
        }

        // Casks (don't overwrite formulae — formulae take priority)
        for c in casks {
            let entry = BrewEntry {
                name: c.token.clone(),
                version: c.version.clone().unwrap_or_default(),
                brew_type: BrewType::Cask,
            };

            lookup.entry(c.token.clone()).or_insert(entry.clone());
            for old in &c.old_tokens {
                lookup.entry(old.clone()).or_insert(entry.clone());
            }
        }

        Self {
            lookup,
            apt_aliases,
            cask_blocklist,
        }
    }

    /// Try to match an APT package name to a Homebrew formula or cask.
    pub fn find_match(&self, apt_name: &str) -> Option<(String, String, BrewType)> {
        // 1. External aliases from JSON (apt-to-brew.json)
        if let Some((brew_name, brew_type)) = self.apt_aliases.get(apt_name) {
            if let Some(entry) = self.lookup.get(brew_name.as_str()) {
                return self.entry_result(entry, apt_name);
            }
            // Alias points to a name not in the index — might be valid on user's system
            // but we can't verify version, so skip
        }

        // 2. Exact name match in brew index
        if let Some(entry) = self.lookup.get(apt_name) {
            return self.entry_result(entry, apt_name);
        }

        // 3. Fuzzy matching: try name transformations
        for candidate in fuzzy_candidates(apt_name) {
            if let Some(entry) = self.lookup.get(&candidate) {
                return self.entry_result(entry, apt_name);
            }
        }

        None
    }

    fn entry_result(
        &self,
        entry: &BrewEntry,
        apt_name: &str,
    ) -> Option<(String, String, BrewType)> {
        if entry.brew_type == BrewType::Cask && self.cask_blocklist.contains(apt_name) {
            return None;
        }
        Some((
            entry.name.clone(),
            entry.version.clone(),
            entry.brew_type.clone(),
        ))
    }

    /// Match all packages, updating their brew_name, brew_version, and brew_type.
    pub fn match_packages(&self, packages: &mut [PackageMigration]) {
        for pkg in packages.iter_mut() {
            if let Some((brew_name, brew_version, brew_type)) = self.find_match(&pkg.name) {
                pkg.brew_name = Some(brew_name);
                pkg.brew_version = Some(brew_version);
                pkg.brew_type = Some(brew_type);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_formulae() -> Vec<BrewFormula> {
        vec![
            BrewFormula {
                name: "git".into(),
                aliases: vec![],
                versions: BrewVersions {
                    stable: Some("2.44.0".into()),
                },
            },
            BrewFormula {
                name: "fd".into(),
                aliases: vec![],
                versions: BrewVersions {
                    stable: Some("9.0.0".into()),
                },
            },
            BrewFormula {
                name: "node".into(),
                aliases: vec!["nodejs".into()],
                versions: BrewVersions {
                    stable: Some("22.0.0".into()),
                },
            },
        ]
    }

    #[test]
    fn exact_match() {
        let index = BrewIndex::from_formulae(&sample_formulae());
        let result = index.find_match("git");
        assert_eq!(
            result,
            Some(("git".into(), "2.44.0".into(), BrewType::Formula))
        );
    }

    #[test]
    fn known_alias_match() {
        let index = BrewIndex::from_formulae(&sample_formulae());
        let result = index.find_match("fd-find");
        assert_eq!(
            result,
            Some(("fd".into(), "9.0.0".into(), BrewType::Formula))
        );
    }

    #[test]
    fn brew_alias_match() {
        let index = BrewIndex::from_formulae(&sample_formulae());
        let result = index.find_match("nodejs");
        assert_eq!(
            result,
            Some(("node".into(), "22.0.0".into(), BrewType::Formula))
        );
    }

    #[test]
    fn no_match() {
        let index = BrewIndex::from_formulae(&sample_formulae());
        assert!(index.find_match("some-random-pkg").is_none());
    }

    // Cask tests removed: casks are macOS-only and not indexed on Linux.

    #[test]
    fn fuzzy_strip_bin_suffix() {
        let formulae = vec![BrewFormula {
            name: "gdal".into(),
            aliases: vec![],
            versions: BrewVersions {
                stable: Some("3.12.0".into()),
            },
        }];
        let index = BrewIndex::from_formulae(&formulae);
        let result = index.find_match("gdal-bin");
        assert_eq!(
            result,
            Some(("gdal".into(), "3.12.0".into(), BrewType::Formula))
        );
    }

    #[test]
    fn fuzzy_strip_lib_dev() {
        let formulae = vec![BrewFormula {
            name: "oniguruma".into(),
            aliases: vec!["onig".into()],
            versions: BrewVersions {
                stable: Some("6.9.9".into()),
            },
        }];
        let index = BrewIndex::from_formulae(&formulae);
        // libonig-dev → strip -dev → "libonig" → strip lib → "onig" → alias match
        let result = index.find_match("libonig-dev");
        assert_eq!(
            result,
            Some(("oniguruma".into(), "6.9.9".into(), BrewType::Formula))
        );
    }

    #[test]
    fn fuzzy_strip_vendor_prefix() {
        let formulae = vec![BrewFormula {
            name: "mongosh".into(),
            aliases: vec![],
            versions: BrewVersions {
                stable: Some("2.8.0".into()),
            },
        }];
        let index = BrewIndex::from_formulae(&formulae);
        let result = index.find_match("mongodb-mongosh");
        assert_eq!(
            result,
            Some(("mongosh".into(), "2.8.0".into(), BrewType::Formula))
        );
    }

    #[test]
    fn fuzzy_strip_python3_prefix() {
        let formulae = vec![BrewFormula {
            name: "gdal".into(),
            aliases: vec![],
            versions: BrewVersions {
                stable: Some("3.12.0".into()),
            },
        }];
        let index = BrewIndex::from_formulae(&formulae);
        let result = index.find_match("python3-gdal");
        assert_eq!(
            result,
            Some(("gdal".into(), "3.12.0".into(), BrewType::Formula))
        );
    }

    #[test]
    fn fuzzy_no_false_positive() {
        let index = BrewIndex::from_formulae(&sample_formulae());
        // Random package shouldn't fuzzy-match anything
        assert!(index.find_match("ubuntu-desktop").is_none());
        assert!(index.find_match("language-pack-en").is_none());
    }

    #[test]
    fn formula_match_ignores_cask_data() {
        let formulae = vec![BrewFormula {
            name: "ffmpeg".into(),
            aliases: vec![],
            versions: BrewVersions {
                stable: Some("7.0".into()),
            },
        }];
        let index = BrewIndex::from_formulae(&formulae);
        let result = index.find_match("ffmpeg");
        assert_eq!(
            result,
            Some(("ffmpeg".into(), "7.0".into(), BrewType::Formula))
        );
    }
}
