
#![windows_subsystem = "windows"]

mod find;
mod gui;
mod ui;
mod version;

use std::fs;
use std::path::{Path, PathBuf};

#[cfg(feature = "compress_bin")]
#[macro_use]
extern crate include_bytes_zstd;

const DLL_NAME: &str = "umareplacereborn.dll";

const LEGACY_DLL_NAME: &str = "charreplace.dll";

#[cfg(feature = "compress_bin")]
fn dll_bytes() -> Vec<u8> {
    include_bytes_zstd!("umareplacereborn.dll", 19).to_vec()
}

#[cfg(not(feature = "compress_bin"))]
fn dll_bytes() -> Vec<u8> {
    include_bytes!("umareplacereborn.dll").to_vec()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let silent = args.iter().any(|a| a == "--silent" || a == "-s");
    let uninstall = args.iter().any(|a| a == "--uninstall" || a == "-u");
    let dry_run = args.iter().any(|a| a == "--dry-run");
    let out_file = args
        .iter()
        .position(|a| a == "--out")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from);
    let dir_override = args
        .iter()
        .position(|a| a == "--dir" || a == "-d")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from);

    if args.len() <= 1 {
        if let Err(e) = gui::run() {
            ui::error(&format!("界面打开失败：{e}"));
        }
        return;
    }

    let result = run(dir_override, uninstall, dry_run);

    if let Some(path) = out_file {
        let text = match &result {
            Ok(msg) => format!("OK\n{msg}\n"),
            Err(e) => format!("FAIL\n{e}\n"),
        };
        let _ = std::fs::write(&path, text);
        std::process::exit(if result.is_ok() { 0 } else { 1 });
    }

    if silent {
        std::process::exit(if result.is_ok() { 0 } else { 1 });
    }
    match result {
        Ok(msg) => ui::info(&msg),
        Err(e) => ui::error(&e),
    }
}

fn run(dir_override: Option<PathBuf>, uninstall: bool, dry_run: bool) -> Result<String, String> {
    let game_dir = match find::find(dir_override.as_deref()) {
        Some(dir) => dir,
        None => {
            ui::warn(
                "没有自动找到游戏目录。\n\n\
                 请在下一步里选择游戏安装目录（里面有 UmamusumePrettyDerby_Jpn.exe 的那个）。",
            );
            match ui::pick_folder() {
                Some(dir) if find::looks_like_game(&dir) => dir,
                Some(_) => return Err("选的目录里没有找到游戏主程序。".into()),
                None => return Err("已取消。".into()),
            }
        }
    };

    if dry_run {
        return Ok(format!(
            "找到游戏目录：\n{}\n\nHachimi 本体：{}\n\n（--dry-run，没有改动任何文件）",
            game_dir.display(),
            if find::hachimi_installed(&game_dir) {
                "已安装"
            } else {
                "未检测到"
            }
        ));
    }

    if !find::hachimi_installed(&game_dir) {
        return Err(format!(
            "这个目录里没有检测到 Hachimi 本体：\n{}\n\n\
             请先把 Hachimi 装好再装本插件 —— 插件自己跑不起来，必须由 Hachimi 加载。",
            game_dir.display()
        ));
    }

    if uninstall {
        uninstall_from(&game_dir)
    } else {
        install_to(&game_dir)
    }
}

pub(crate) fn install_to(game_dir: &Path) -> Result<String, String> {
    let target_dir = find::install_dir_for(game_dir);
    let dll_path = target_dir.join(DLL_NAME);

    let legacy = target_dir.join(LEGACY_DLL_NAME);
    if legacy.is_file() {
        let _ = fs::remove_file(&legacy);
    }
    let _ = fs::remove_file(target_dir.join(format!("{LEGACY_DLL_NAME}.bak")));

    if dll_path.is_file() {
        let _ = fs::copy(&dll_path, dll_path.with_extension("dll.bak"));
    }

    fs::write(&dll_path, dll_bytes()).map_err(|e| {
        format!(
            "写入插件失败：\n{}\n\n{e}\n\n\
             如果游戏正在运行，请先完全退出游戏再试一次。",
            dll_path.display()
        )
    })?;

    let config_changed = patch_config(game_dir, true)?;

    let mut msg = format!("安装完成。\n\n游戏目录：\n{}", game_dir.display());
    if config_changed {
        msg.push_str(&format!(
            "\n\n已把 {DLL_NAME} 加进 Hachimi 的 load_libraries。"
        ));
    } else {
        msg.push_str("\n\nHachimi 的 load_libraries 里本来就有它，无需改动。");
    }
    msg.push_str("\n\n请完全退出游戏后重新启动，插件就会生效。");
    Ok(msg)
}

pub(crate) fn uninstall_from(game_dir: &Path) -> Result<String, String> {
    let target_dir = find::install_dir_for(game_dir);
    let dll_path = target_dir.join(DLL_NAME);
    for name in [LEGACY_DLL_NAME, &format!("{LEGACY_DLL_NAME}.bak")] {
        let _ = fs::remove_file(target_dir.join(name));
    }
    if dll_path.is_file() {
        fs::remove_file(&dll_path).map_err(|e| {
            format!("删除插件失败：\n{}\n\n{e}\n\n如果游戏正在运行，请先退出游戏。", dll_path.display())
        })?;
    }
    let _ = fs::remove_file(dll_path.with_extension("dll.bak"));
    patch_config(game_dir, false)?;
    Ok(format!(
        "已卸载。\n\n游戏目录：\n{}\n\n重新启动游戏后插件不再加载。",
        game_dir.display()
    ))
}

fn patch_config(game_dir: &Path, add: bool) -> Result<bool, String> {
    let config_path = game_dir.join("hachimi").join("config.json");

    if !config_path.is_file() {
        return Err(format!(
            "找不到 Hachimi 的配置：\n{}\n\n\
             先启动一次游戏，让 Hachimi 生成配置，然后再装插件。",
            config_path.display()
        ));
    }

    let text = fs::read_to_string(&config_path)
        .map_err(|e| format!("读取配置失败：\n{}\n\n{e}", config_path.display()))?;
    let mut json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("配置不是合法的 JSON：\n{}\n\n{e}", config_path.display()))?;

    let root = json
        .as_object_mut()
        .ok_or_else(|| "配置的顶层不是对象，无法处理。".to_string())?;

    let list = root
        .entry("load_libraries")
        .or_insert_with(|| serde_json::Value::Array(Vec::new()));
    let arr = list
        .as_array_mut()
        .ok_or_else(|| "配置里的 load_libraries 不是数组，不敢动它。".to_string())?;

    let mut changed = false;

    let before = arr.len();
    arr.retain(|v| v.as_str() != Some(LEGACY_DLL_NAME));
    if arr.len() != before {
        changed = true;
    }

    let present = arr.iter().any(|v| v.as_str() == Some(DLL_NAME));
    if add && !present {
        arr.push(serde_json::Value::String(DLL_NAME.to_owned()));
        changed = true;
    } else if !add && present {
        arr.retain(|v| v.as_str() != Some(DLL_NAME));
        changed = true;
    }

    if changed {
        let _ = fs::copy(&config_path, config_path.with_extension("json.bak"));
        let pretty = serde_json::to_string_pretty(&json)
            .map_err(|e| format!("重新生成配置失败：{e}"))?;
        fs::write(&config_path, pretty)
            .map_err(|e| format!("写入配置失败：\n{}\n\n{e}", config_path.display()))?;
    }

    Ok(changed)
}
