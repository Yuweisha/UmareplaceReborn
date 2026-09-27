//! Hachimi plugin: global character replacement.
//!
//! Ported from Trainers' Legend G's `replaceGlobalChar` (via Hachimi's own
//! built-in implementation) so it can live outside Hachimi itself and survive
//! Hachimi updates.
//!
//! Load it by adding the library to `windows.load_libraries` in
//! `<game dir>/hachimi/config.json`, e.g. `"hachimi\\charreplace.dll"`.

mod api;
mod audio;
mod config;
mod db;
mod hook;
mod string_hook;
mod ui;

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Menu icon (32x32 PNG), embedded so the plugin stays a single file.
static ICON: &[u8] = include_bytes!("icon.png");

static HOOKS_INSTALLED: AtomicBool = AtomicBool::new(false);

/// Entry point for Hachimi's plugin interface v3.
///
/// Hachimi calls this from `on_hooking_finished()`, right after it installed
/// its own IL2CPP hooks - so the game's classes are already resolvable here.
///
/// Do *not* defer the setup to `hachimi_register_on_game_initialized`:
/// Hachimi fires those callbacks (in `GameSystem::on_game_initialized`) before
/// it initialises plugins, so a callback registered here would never run.
#[no_mangle]
pub extern "C" fn hachimi_init_v3(get_api: api::GetApiFn, _version: i32) -> i32 {
    if api::init(get_api).is_none() {
        return api::INIT_ERROR;
    }

    api::log_info("charreplace plugin loaded");
    config::load();

    // Installing the hooks is just a few symbol lookups, so keep it on this
    // thread. The database load takes seconds, so it goes to a worker thread
    // (the menu shows a "loading" note until it finishes).
    install_hooks_with_retry();
    std::thread::spawn(db::load);

    if !api::register_menu_section("角色替换", ICON, ui::section_callback) {
        api::log_error("failed to register the menu section");
        return api::INIT_ERROR;
    }

    api::INIT_OK
}

/// Install the hooks, retrying briefly in case the classes are not resolvable
/// the very first time (should not happen, but a stale symbol table would
/// otherwise silently disable the whole plugin).
fn install_hooks_with_retry() {
    if hook::install() {
        HOOKS_INSTALLED.store(true, Ordering::SeqCst);
        return;
    }

    std::thread::spawn(|| {
        for _ in 0..20 {
            std::thread::sleep(Duration::from_millis(500));
            if HOOKS_INSTALLED.load(Ordering::SeqCst) {
                return;
            }
            if hook::install() {
                HOOKS_INSTALLED.store(true, Ordering::SeqCst);
                return;
            }
        }
        api::log_error("could not install hooks: game classes not found");
    });
}
