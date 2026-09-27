//! Hachimi plugin: global character replacement.
//!
//! Ported from Trainers' Legend G's `replaceGlobalChar` (via Hachimi's own
//! built-in implementation) so it can live outside Hachimi itself and survive
//! Hachimi updates.
//!
//! Load it by adding the library to `windows.load_libraries` in
//! `<game dir>/hachimi/config.json`, e.g. `"charreplace.dll"`.

mod api;
mod config;
mod db;
mod hook;
mod ui;

use std::ffi::c_void;

/// Menu icon (32x32 PNG), embedded so the plugin stays a single file.
static ICON: &[u8] = include_bytes!("icon.png");

/// Entry point for Hachimi's plugin interface v3.
#[no_mangle]
pub extern "C" fn hachimi_init_v3(get_api: api::GetApiFn, _version: i32) -> i32 {
    if api::init(get_api).is_none() {
        return api::INIT_ERROR;
    }

    api::log_info("charreplace plugin loaded");
    config::load();

    api::register_on_game_initialized(on_game_initialized);

    if !api::register_menu_section("角色替换", ICON, ui::section_callback) {
        api::log_error("failed to register the menu section");
        return api::INIT_ERROR;
    }

    api::INIT_OK
}

/// Called once the game finished initializing: the IL2CPP classes exist now,
/// so we can read the database and install the hooks.
extern "C" fn on_game_initialized(_userdata: *mut c_void) {
    std::thread::spawn(|| {
        db::load();
        hook::install();
    });
}
