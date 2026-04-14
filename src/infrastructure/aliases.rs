use std::collections::{HashMap, HashSet};

use serde::Deserialize;

use crate::domain::package::BrewType;

/// A single alias entry from the JSON files.
#[derive(Debug, Deserialize)]
pub struct AliasEntry {
    pub brew: String,
    #[serde(rename = "type")]
    pub brew_type: String,
}

/// Blocklist data from JSON.
#[derive(Debug, Deserialize)]
struct BlocklistData {
    cask_blocklist: Vec<String>,
}

/// Parsed alias map: source name → (brew name, BrewType).
pub type AliasMap = HashMap<String, (String, BrewType)>;

fn parse_type(t: &str) -> BrewType {
    match t {
        "cask" => BrewType::Cask,
        _ => BrewType::Formula,
    }
}

fn load_alias_map(json: &str) -> AliasMap {
    let raw: HashMap<String, serde_json::Value> = serde_json::from_str(json).unwrap_or_default();
    let mut map = HashMap::new();

    for (key, value) in raw {
        if key.starts_with('_') {
            continue; // skip _comment
        }
        if let Ok(entry) = serde_json::from_value::<AliasEntry>(value) {
            map.insert(key, (entry.brew, parse_type(&entry.brew_type)));
        }
    }

    map
}

/// Load APT → Brew aliases (compiled into the binary).
pub fn apt_aliases() -> AliasMap {
    load_alias_map(include_str!("../../aliases/apt-to-brew.json"))
}

/// Load Snap → Brew aliases (compiled into the binary).
pub fn snap_aliases() -> AliasMap {
    load_alias_map(include_str!("../../aliases/snap-to-brew.json"))
}

/// Build reverse map: brew name → apt name (from both apt and snap aliases).
pub fn brew_to_apt_map() -> HashMap<String, String> {
    let mut reverse = HashMap::new();
    for (apt_name, (brew_name, _)) in apt_aliases() {
        reverse.entry(brew_name).or_insert(apt_name);
    }
    for (snap_name, (brew_name, _)) in snap_aliases() {
        reverse.entry(brew_name).or_insert(snap_name);
    }
    reverse
}

/// Load the cask blocklist (compiled into the binary).
pub fn cask_blocklist() -> HashSet<String> {
    let json = include_str!("../../aliases/blocklist.json");
    let data: BlocklistData = serde_json::from_str(json).unwrap_or(BlocklistData {
        cask_blocklist: vec![],
    });
    data.cask_blocklist.into_iter().collect()
}
