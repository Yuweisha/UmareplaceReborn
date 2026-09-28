//! Plugin configuration.
//!
//! Stored as `<game dir>/hachimi/charreplace.json`. The shape (and the serde
//! aliases) intentionally match Trainers' Legend G's `replaceGlobalChar`
//! block, so an existing TLG config can be pasted in as-is.

use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use crate::api;

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    /// `origCharId` in TLG configs.
    #[serde(default, alias = "origCharId")]
    pub orig_char_id: i32,
    /// `newChrId` in TLG configs.
    #[serde(default, alias = "newChrId")]
    pub new_char_id: i32,
    /// `newClothId` in TLG configs.
    #[serde(default, alias = "newClothId")]
    pub new_cloth_id: i32,
    /// `replaceMini` in TLG configs.
    #[serde(default, alias = "replaceMini")]
    pub replace_mini: bool,
}

fn default_replace_universal() -> bool {
    default_true()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharReplaceConfig {
    #[serde(default)]
    pub enable: bool,
    /// Replace the dress too (TLG's `replaceUniversal`).
    #[serde(default = "default_replace_universal", alias = "replaceUniversal")]
    pub replace_universal: bool,
    /// Also replace inside cutscenes (EventTimeline / CutIn). Off by default:
    /// a swapped character there has no matching motion, so the model collapses
    /// into a T-pose and the scene waits on assets forever.
    #[serde(default)]
    pub replace_in_cutscene: bool,
    #[serde(default)]
    pub data: Vec<Entry>,
}

impl Default for CharReplaceConfig {
    fn default() -> Self {
        Self {
            enable: false,
            // TLG defaults this to true.
            replace_universal: true,
            replace_in_cutscene: false,
            data: Vec::new(),
        }
    }
}

static CONFIG: Lazy<RwLock<CharReplaceConfig>> = Lazy::new(|| RwLock::new(CharReplaceConfig::default()));

pub fn config_path() -> Option<PathBuf> {
    Some(api::base_dir()?.join("charreplace.json"))
}

/// Read the plugin config, migrating an existing `replaceGlobalChar` block out
/// of Hachimi's own config.json the first time we run.
pub fn load() {
    let mut loaded = None;

    if let Some(path) = config_path() {
        if path.exists() {
            match fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|text| {
                serde_json::from_str::<CharReplaceConfig>(&text).map_err(|e| e.to_string())
            }) {
                Ok(config) => {
                    loaded = Some(config);
                }
                Err(e) => api::log_error(&format!("failed to parse {}: {}", path.display(), e)),
            }
        }
    }

    if loaded.is_none() {
        loaded = migrate_from_hachimi_config();
    }

    if let Some(config) = loaded {
        let entries = config.data.len();
        *CONFIG.write().unwrap() = config;
        api::log_info(&format!("charreplace config loaded ({} rule(s))", entries));
    }
}

fn migrate_from_hachimi_config() -> Option<CharReplaceConfig> {
    let path = api::base_dir()?.join("config.json");
    let text = fs::read_to_string(&path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let block = value.get("replaceGlobalChar")?;
    match serde_json::from_value::<CharReplaceConfig>(block.clone()) {
        Ok(config) => {
            api::log_info("imported replaceGlobalChar from hachimi/config.json");
            Some(config)
        }
        Err(e) => {
            api::log_warn(&format!("could not import replaceGlobalChar: {}", e));
            None
        }
    }
}

pub fn with<R>(f: impl FnOnce(&CharReplaceConfig) -> R) -> R {
    f(&CONFIG.read().unwrap())
}

pub fn with_mut<R>(f: impl FnOnce(&mut CharReplaceConfig) -> R) -> R {
    f(&mut CONFIG.write().unwrap())
}

pub fn save() {
    let Some(path) = config_path() else { return };
    let text = {
        let config = CONFIG.read().unwrap();
        match serde_json::to_string_pretty(&*config) {
            Ok(text) => text,
            Err(e) => {
                api::log_error(&format!("failed to serialize config: {}", e));
                return;
            }
        }
    };

    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Err(e) = fs::write(&path, text) {
        api::log_error(&format!("failed to write {}: {}", path.display(), e));
    }
}
