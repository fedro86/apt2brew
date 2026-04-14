use std::collections::HashMap;

use serde::Deserialize;

use crate::domain::package::PackageMigration;

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

/// Known APT → Brew name mappings where names differ.
fn known_aliases() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        ("fd-find", "fd"),
        ("ripgrep", "ripgrep"),
        ("bat", "bat"),
        ("neovim", "neovim"),
        ("python3", "python@3"),
        ("python3-pip", "python@3"),
        ("nodejs", "node"),
        ("golang", "go"),
        ("golang-go", "go"),
        ("clang", "llvm"),
        ("g++", "gcc"),
        ("silversearcher-ag", "the_silver_searcher"),
        ("ncdu", "ncdu"),
        ("httpie", "httpie"),
        ("tmux", "tmux"),
        ("zsh", "zsh"),
        ("fish", "fish"),
    ])
}

/// Index of brew formulae: name/alias → (canonical name, version).
pub struct BrewIndex {
    lookup: HashMap<String, (String, String)>,
}

impl BrewIndex {
    /// Fetch the full formula list from Homebrew API and build the index.
    pub async fn fetch() -> Result<Self, BrewError> {
        let formulae: Vec<BrewFormula> = reqwest::get("https://formulae.brew.sh/api/formula.json")
            .await?
            .json()
            .await?;

        Ok(Self::from_formulae(&formulae))
    }

    /// Build index from a pre-fetched formula list (for testing).
    pub fn from_formulae(formulae: &[BrewFormula]) -> Self {
        let mut lookup = HashMap::new();

        for f in formulae {
            let version = f.versions.stable.clone().unwrap_or_default();
            let entry = (f.name.clone(), version);

            lookup.insert(f.name.clone(), entry.clone());
            for alias in &f.aliases {
                lookup.insert(alias.clone(), entry.clone());
            }
        }

        Self { lookup }
    }

    /// Try to match an APT package name to a Homebrew formula.
    /// Returns (brew_name, brew_version) if found.
    pub fn find_match(&self, apt_name: &str) -> Option<(String, String)> {
        // 1. Check known APT → Brew aliases first
        if let Some(brew_alias) = known_aliases().get(apt_name)
            && let Some(entry) = self.lookup.get(*brew_alias)
        {
            return Some(entry.clone());
        }

        // 2. Exact name match
        if let Some(entry) = self.lookup.get(apt_name) {
            return Some(entry.clone());
        }

        None
    }

    /// Match all packages, updating their brew_name and brew_version.
    pub fn match_packages(&self, packages: &mut [PackageMigration]) {
        for pkg in packages.iter_mut() {
            if let Some((brew_name, brew_version)) = self.find_match(&pkg.name) {
                pkg.brew_name = Some(brew_name);
                pkg.brew_version = Some(brew_version);
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
        assert_eq!(result, Some(("git".into(), "2.44.0".into())));
    }

    #[test]
    fn known_alias_match() {
        let index = BrewIndex::from_formulae(&sample_formulae());
        let result = index.find_match("fd-find");
        assert_eq!(result, Some(("fd".into(), "9.0.0".into())));
    }

    #[test]
    fn brew_alias_match() {
        let index = BrewIndex::from_formulae(&sample_formulae());
        let result = index.find_match("nodejs");
        assert_eq!(result, Some(("node".into(), "22.0.0".into())));
    }

    #[test]
    fn no_match() {
        let index = BrewIndex::from_formulae(&sample_formulae());
        assert!(index.find_match("some-random-pkg").is_none());
    }
}
