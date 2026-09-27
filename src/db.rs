//! master.mdb access.
//!
//! Hachimi itself queries the database through the game's own IL2CPP sqlite
//! wrapper; a plugin cannot reach that, but the file is a plain SQLite
//! database, so we simply open it read-only with rusqlite.

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::Mutex;

use once_cell::sync::{Lazy, OnceCell};
use rusqlite::{Connection, OpenFlags};

use crate::api;

#[derive(Debug, Clone)]
pub struct CharacterEntry {
    pub id: i32,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct DressEntry {
    pub id: i32,
    pub chara_id: i32,
    pub name: String,
}

#[derive(Debug, Clone, Copy)]
pub struct DressInfo {
    pub head_sub_id: i32,
    pub have_mini: bool,
}

static CHARACTERS: OnceCell<Vec<CharacterEntry>> = OnceCell::new();
static DRESSES: OnceCell<Vec<DressEntry>> = OnceCell::new();
static DRESS_INFO_CACHE: Lazy<Mutex<HashMap<i32, Option<DressInfo>>>> = Lazy::new(|| Mutex::new(HashMap::new()));

pub fn masterdb_path() -> Option<PathBuf> {
    let path = api::data_path()?.join("master").join("master.mdb");
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

fn open() -> Option<Connection> {
    let path = masterdb_path()?;
    match Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(conn) => Some(conn),
        Err(e) => {
            api::log_error(&format!("failed to open master.mdb: {}", e));
            None
        }
    }
}

/// Load character and dress tables. Runs on a background thread - the first
/// call does all the SQL work, later calls are free.
pub fn load() {
    let Some(conn) = open() else { return };

    let localized = load_localized_names();

    let characters = match load_characters(&conn, &localized) {
        Ok(characters) => characters,
        Err(e) => {
            api::log_error(&format!("failed to load characters: {}", e));
            Vec::new()
        }
    };
    let dresses = match load_dresses(&conn, &localized) {
        Ok(dresses) => dresses,
        Err(e) => {
            api::log_error(&format!("failed to load dresses: {}", e));
            Vec::new()
        }
    };

    api::log_info(&format!(
        "loaded {} character(s) and {} dress(es) from master.mdb",
        characters.len(),
        dresses.len()
    ));

    warm_dress_info(&conn);

    let _ = CHARACTERS.set(characters);
    let _ = DRESSES.set(dresses);
}

/// Cache every dress' head model id and mini flag up front, so the hook never
/// has to touch the database while the game is rendering.
fn warm_dress_info(conn: &Connection) {
    let Ok(mut stmt) = conn.prepare("SELECT id, head_sub_id, have_mini FROM dress_data") else {
        return;
    };
    let Ok(rows) = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i32>(0)?,
            row.get::<_, i32>(1)?,
            row.get::<_, i32>(2)?,
        ))
    }) else {
        return;
    };

    let mut cache = DRESS_INFO_CACHE.lock().unwrap();
    for (id, head_sub_id, have_mini) in rows.flatten() {
        cache.insert(
            id,
            Some(DressInfo {
                head_sub_id,
                have_mini: have_mini != 0,
            }),
        );
    }
    api::log_info(&format!("dress info cached: {} entries", cache.len()));
}

fn load_characters(
    conn: &Connection,
    localized: &HashMap<String, String>,
) -> rusqlite::Result<Vec<CharacterEntry>> {
    let mut stmt = conn.prepare(
        "SELECT C.id, T.text FROM chara_data AS C \
         JOIN text_data AS T ON C.id = T.\"index\" WHERE T.id = 6",
    )?;
    let rows = stmt.query_map([], |row| {
        let id: i32 = row.get(0)?;
        let name: String = row.get(1)?;
        Ok(CharacterEntry { id, name })
    })?;

    let mut characters: Vec<CharacterEntry> = Vec::new();
    for entry in rows.flatten() {
        let name = localized
            .get(&entry.id.to_string())
            .cloned()
            .unwrap_or(entry.name);
        characters.push(CharacterEntry { id: entry.id, name });
    }
    characters.sort_by_key(|c| c.id);
    Ok(characters)
}

fn load_dresses(
    conn: &Connection,
    localized: &HashMap<String, String>,
) -> rusqlite::Result<Vec<DressEntry>> {
    let mut stmt = conn.prepare(
        "SELECT D.id, D.chara_id, T.text FROM dress_data AS D \
         LEFT JOIN text_data AS T ON T.\"index\" = D.id AND T.id = 5",
    )?;
    let rows = stmt.query_map([], |row| {
        let id: i32 = row.get(0)?;
        let chara_id: i32 = row.get(1)?;
        let name: Option<String> = row.get(2)?;
        Ok(DressEntry {
            id,
            chara_id,
            name: name.unwrap_or_default(),
        })
    })?;

    let mut dresses: Vec<DressEntry> = Vec::new();
    for entry in rows.flatten() {
        let name = localized
            .get(&entry.id.to_string())
            .cloned()
            .unwrap_or(entry.name);
        dresses.push(DressEntry {
            id: entry.id,
            chara_id: entry.chara_id,
            name,
        });
    }
    dresses.sort_by_key(|d| d.id);
    Ok(dresses)
}

/// Translated character and dress names from Hachimi's localization data
/// (text_data_dict.json). Falls back to the Japanese text in master.mdb.
fn load_localized_names() -> HashMap<String, String> {
    let mut names = HashMap::new();
    let Some(base_dir) = api::base_dir() else { return names };
    let path = base_dir.join("localized_data").join("text_data_dict.json");
    let Ok(file) = File::open(&path) else { return names };
    let reader = BufReader::new(file);

    // category -> index -> text
    let parsed: Result<HashMap<String, HashMap<String, String>>, _> =
        serde_json::from_reader(reader);
    let Ok(mut parsed) = parsed else {
        api::log_warn("failed to parse text_data_dict.json, using Japanese names");
        return names;
    };

    // 170 and 6 both hold character names; 5 holds dress names.
    for category in ["170", "6", "5"] {
        if let Some(entries) = parsed.remove(category) {
            for (index, text) in entries {
                names.entry(index).or_insert(text);
            }
        }
    }

    names
}

pub fn characters() -> &'static [CharacterEntry] {
    CHARACTERS.get().map(|v| v.as_slice()).unwrap_or(&[])
}

pub fn dresses() -> &'static [DressEntry] {
    DRESSES.get().map(|v| v.as_slice()).unwrap_or(&[])
}

pub fn loaded() -> bool {
    CHARACTERS.get().is_some() && DRESSES.get().is_some()
}

pub fn dress_info(dress_id: i32) -> Option<DressInfo> {
    if dress_id == 0 {
        return None;
    }
    if let Some(cached) = DRESS_INFO_CACHE.lock().unwrap().get(&dress_id) {
        return *cached;
    }

    let info = query_dress_info(dress_id);
    DRESS_INFO_CACHE.lock().unwrap().insert(dress_id, info);
    info
}

fn query_dress_info(dress_id: i32) -> Option<DressInfo> {
    let conn = open()?;
    let mut stmt = conn
        .prepare("SELECT head_sub_id, have_mini FROM dress_data WHERE id = ?1")
        .ok()?;
    let mut rows = stmt.query([dress_id]).ok()?;
    let row = rows.next().ok()??;

    let head_sub_id: i32 = row.get(0).unwrap_or(0);
    let have_mini: i32 = row.get(1).unwrap_or(0);
    Some(DressInfo {
        head_sub_id,
        have_mini: have_mini != 0,
    })
}

pub fn dress_head_sub_id(dress_id: i32) -> i32 {
    dress_info(dress_id).map(|info| info.head_sub_id).unwrap_or(0)
}

pub fn dress_has_mini(dress_id: i32) -> bool {
    dress_info(dress_id).map(|info| info.have_mini).unwrap_or(false)
}
