
use std::fs;
use std::path::{Path, PathBuf};

use windows::core::PCWSTR;
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};

pub const TARGETS: &[(&str, &str)] = &[
    ("UmamusumePrettyDerby_Jpn.exe", "Steam (JP) (cri_mana_vpx.dll)"),
    ("UmamusumePrettyDerby.exe", "Steam (Global) (cri_mana_vpx.dll)"),
    ("komoeumamusume.exe", "KOMOE (umamusume.exe)"),
    ("umamusume.exe", "DMM (umamusume.exe)"),
];

const KOMOE_REG_KEY: &str = r"Software\komoemumamusume";
const KOMOE_PATH_VALUE: &str = "GameInstallPath";

const HACHIMI_MARKERS: &[&str] = &[
    "cri_mana_vpx.dll",
    "hachimi",
    "umamusume.exe.local",
];

pub fn looks_like_game(dir: &Path) -> bool {
    TARGETS.iter().any(|(exe, _)| dir.join(exe).is_file())
}

pub fn install_dir_for(game_dir: &Path) -> PathBuf {
    let dot_local = game_dir.join("umamusume.exe.local");
    if dot_local.is_dir() {
        dot_local
    } else {
        game_dir.to_path_buf()
    }
}

pub fn hachimi_installed(dir: &Path) -> bool {
    HACHIMI_MARKERS.iter().any(|m| dir.join(m).exists())
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn reg_string(subkey: &str, value: &str) -> Option<String> {
    let subkey = wide(subkey);
    let name = wide(value);
    let mut buf = [0u16; 1024];
    let mut size = (buf.len() * 2) as u32;

    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut _),
            Some(&mut size),
        )
    };

    if status.is_err() {
        return None;
    }
    let len = (size as usize / 2).saturating_sub(1).min(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]))
}

fn dmm_install_dir() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    let config = PathBuf::from(appdata)
        .join("dmmgameplayer5")
        .join("dmmgame.cnf");

    let text = fs::read_to_string(config).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;

    for entry in json.get("contents")?.as_array()? {
        if entry.get("productId").and_then(|v| v.as_str()) != Some("umamusume") {
            continue;
        }
        let dir = PathBuf::from(entry.get("detail")?.get("path")?.as_str()?);
        if dir.is_dir() {
            return Some(dir);
        }
    }
    None
}

fn steam_root_from_registry() -> Option<PathBuf> {
    let path = PathBuf::from(reg_string(r"Software\Valve\Steam", "SteamPath")?);
    path.is_dir().then_some(path)
}

fn komoe_registry_dir() -> Option<PathBuf> {
    let path = PathBuf::from(reg_string(KOMOE_REG_KEY, KOMOE_PATH_VALUE)?);
    looks_like_game(&path).then_some(path)
}

fn steam_libraries() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    let mut add = |p: PathBuf| {
        if p.is_dir() && !roots.contains(&p) {
            roots.push(p);
        }
    };

    if let Some(root) = steam_root_from_registry() {
        add(root.clone());
    }
    for tail in [
        "Program Files (x86)\\Steam",
        "Program Files\\Steam",
        "Steam",
    ] {
        for drive in 'C'..='Z' {
            add(PathBuf::from(format!("{drive}:\\{tail}")));
        }
    }

    let mut all = roots.clone();
    for root in &roots {
        let vdf = root.join("config").join("libraryfolders.vdf");
        let Ok(text) = fs::read_to_string(&vdf) else {
            continue;
        };
        for line in text.lines() {
            let line = line.trim();
            let Some(rest) = line.strip_prefix("\"path\"") else {
                continue;
            };
            let rest = rest.trim();
            let Some(value) = rest.strip_prefix('"').and_then(|v| v.strip_suffix('"')) else {
                continue;
            };
            let path = PathBuf::from(value.replace("\\\\", "\\"));
            if path.is_dir() && !all.contains(&path) {
                all.push(path);
            }
        }
    }

    all
}

fn from_steam_libraries() -> Vec<PathBuf> {
    let mut found = Vec::new();
    for lib in steam_libraries() {
        let common = lib.join("steamapps").join("common");
        let Ok(entries) = fs::read_dir(&common) else {
            continue;
        };
        for entry in entries.flatten() {
            let dir = entry.path();
            if dir.is_dir() && looks_like_game(&dir) {
                found.push(dir);
            }
        }
    }
    found
}

fn from_common_paths() -> Vec<PathBuf> {
    let mut found = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            if looks_like_game(dir) {
                found.push(dir.to_path_buf());
            }
        }
    }

    for drive in 'C'..='Z' {
        for tail in [
            "SteamLibrary\\steamapps\\common",
            "OtherFiles\\SteamLibrary\\steamapps\\common",
            "Games\\Steam\\steamapps\\common",
            "Games\\SteamLibrary\\steamapps\\common",
        ] {
            let common = PathBuf::from(format!("{drive}:\\{tail}"));
            let Ok(entries) = fs::read_dir(&common) else {
                continue;
            };
            for entry in entries.flatten() {
                let dir = entry.path();
                if dir.is_dir() && looks_like_game(&dir) {
                    found.push(dir);
                }
            }
        }
    }

    found
}

pub fn find(override_dir: Option<&Path>) -> Option<PathBuf> {
    if let Some(dir) = override_dir {
        if looks_like_game(dir) {
            return Some(dir.to_path_buf());
        }
    }
    detect()
}

pub fn detect() -> Option<PathBuf> {
    komoe_registry_dir()
        .or_else(dmm_install_dir)
        .or_else(|| from_steam_libraries().into_iter().next())
        .or_else(|| from_common_paths().into_iter().next())
}
