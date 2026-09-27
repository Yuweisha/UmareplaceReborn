//! Hachimi plugin API bindings (interface version 3).
//!
//! Hachimi loads this library and calls `hachimi_init_v3`, handing over a
//! `get_api` function. Every capability is looked up by name; the signatures
//! here mirror `src/core/plugin_api.rs` of Hachimi-Edge (GPL-3.0).

// Calls through the function pointers below are FFI calls; writing them as
// `unsafe` documents that intent even though Rust considers calling a plain
// `extern "C" fn` pointer safe.
#![allow(unused_unsafe)]

use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::atomic::{AtomicPtr, Ordering};

use once_cell::sync::OnceCell;

/// `InitResult` as defined by Hachimi: 0 = Error, 1 = Ok.
pub const INIT_ERROR: i32 = 0;
pub const INIT_OK: i32 = 1;

pub type GetApiFn = extern "C" fn(name: *const c_char) -> *mut c_void;
pub type GuiMenuSectionCallback = extern "C" fn(ui: *mut c_void, userdata: *mut c_void);
pub type GuiUiCallback = extern "C" fn(ui: *mut c_void, userdata: *mut c_void);
pub type GameInitializedCallback = extern "C" fn(userdata: *mut c_void);

// Opaque IL2CPP / runtime types - we only ever pass their pointers around.
pub type Il2CppObject = c_void;
pub type Il2CppClass = c_void;
pub type Il2CppImage = c_void;
pub type FieldInfo = c_void;
pub type Interceptor = c_void;
pub type Hachimi = c_void;

type FnHachimiInstance = extern "C" fn() -> *const Hachimi;
type FnHachimiGetInterceptor = extern "C" fn(*const Hachimi) -> *const Interceptor;
type FnInterceptorHook = extern "C" fn(*const Interceptor, *mut c_void, *mut c_void) -> *mut c_void;
type FnGetAssemblyImage = extern "C" fn(*const c_char) -> *const Il2CppImage;
type FnGetClass = extern "C" fn(*const Il2CppImage, *const c_char, *const c_char) -> *mut Il2CppClass;
type FnGetMethodAddr = extern "C" fn(*mut Il2CppClass, *const c_char, i32) -> *mut c_void;
type FnGetFieldFromName = extern "C" fn(*mut Il2CppClass, *const c_char) -> *mut FieldInfo;
type FnGetFieldValue = extern "C" fn(*mut Il2CppObject, *mut FieldInfo, *mut c_void);
type FnSetFieldValue = extern "C" fn(*mut Il2CppObject, *mut FieldInfo, *const c_void);
type FnRegisterOnGameInitialized = extern "C" fn(Option<GameInitializedCallback>, *mut c_void);
type FnRegisterMenuSectionWithIcon = extern "C" fn(
    *const c_char, *const c_char, *const u8, usize, Option<GuiMenuSectionCallback>, *mut c_void,
) -> bool;
type FnUiHeading = extern "C" fn(*mut c_void, *const c_char);
type FnUiLabel = extern "C" fn(*mut c_void, *const c_char);
type FnUiSmall = extern "C" fn(*mut c_void, *const c_char);
type FnUiSeparator = extern "C" fn(*mut c_void);
type FnUiButton = extern "C" fn(*mut c_void, *const c_char) -> bool;
type FnUiCheckbox = extern "C" fn(*mut c_void, *const c_char, *mut bool) -> bool;
type FnUiHorizontal = extern "C" fn(*mut c_void, Option<GuiUiCallback>, *mut c_void) -> bool;
type FnUiColoredLabel = extern "C" fn(*mut c_void, u8, u8, u8, u8, *const c_char);
type FnUiComboMenu = extern "C" fn(
    *mut c_void, *const c_char, *mut i32, *const *const c_char, usize, *mut c_char, usize,
) -> bool;
type FnShowNotification = extern "C" fn(*const c_char);
type FnLog = extern "C" fn(i32, *const c_char, *const c_char);
type FnGetPath = extern "C" fn() -> *const c_char;

/// The resolved function table, filled once from Hachimi's `get_api`.
/// Bound in full even where this plugin does not use every entry yet.
#[allow(dead_code)]
pub struct Api {
    pub hachimi_instance: FnHachimiInstance,
    pub hachimi_get_interceptor: FnHachimiGetInterceptor,
    pub interceptor_hook: FnInterceptorHook,
    pub il2cpp_get_assembly_image: FnGetAssemblyImage,
    pub il2cpp_get_class: FnGetClass,
    pub il2cpp_get_method_addr: FnGetMethodAddr,
    pub il2cpp_get_field_from_name: FnGetFieldFromName,
    pub il2cpp_get_field_value: FnGetFieldValue,
    pub il2cpp_set_field_value: FnSetFieldValue,
    pub register_on_game_initialized: FnRegisterOnGameInitialized,
    pub register_menu_section_with_icon: FnRegisterMenuSectionWithIcon,
    pub ui_heading: FnUiHeading,
    pub ui_label: FnUiLabel,
    pub ui_small: FnUiSmall,
    pub ui_separator: FnUiSeparator,
    pub ui_button: FnUiButton,
    pub ui_checkbox: FnUiCheckbox,
    pub ui_horizontal: FnUiHorizontal,
    pub ui_colored_label: FnUiColoredLabel,
    pub ui_combo_menu: FnUiComboMenu,
    pub show_notification: FnShowNotification,
    pub log: FnLog,
    pub get_base_dir: FnGetPath,
    pub get_data_path: FnGetPath,
}

static API: OnceCell<Api> = OnceCell::new();
static HACHIMI_INSTANCE: AtomicPtr<Hachimi> = AtomicPtr::new(std::ptr::null_mut());
static INTERCEPTOR: AtomicPtr<Interceptor> = AtomicPtr::new(std::ptr::null_mut());

pub fn api() -> Option<&'static Api> {
    API.get()
}

fn fetch(get_api: GetApiFn, name: &str) -> *mut c_void {
    let c_name = CString::new(name).unwrap();
    get_api(c_name.as_ptr())
}

macro_rules! bind {
    ($get_api:expr, $name:literal) => {{
        let ptr = fetch($get_api, $name);
        if ptr.is_null() {
            log_error(&format!("missing hachimi api: {}", $name));
            return None;
        }
        unsafe { std::mem::transmute(ptr) }
    }};
}

pub fn init(get_api: GetApiFn) -> Option<&'static Api> {
    if let Some(api) = API.get() {
        return Some(api);
    }

    let api = Api {
        hachimi_instance: bind!(get_api, "hachimi_instance"),
        hachimi_get_interceptor: bind!(get_api, "hachimi_get_interceptor"),
        interceptor_hook: bind!(get_api, "interceptor_hook"),
        il2cpp_get_assembly_image: bind!(get_api, "il2cpp_get_assembly_image"),
        il2cpp_get_class: bind!(get_api, "il2cpp_get_class"),
        il2cpp_get_method_addr: bind!(get_api, "il2cpp_get_method_addr"),
        il2cpp_get_field_from_name: bind!(get_api, "il2cpp_get_field_from_name"),
        il2cpp_get_field_value: bind!(get_api, "il2cpp_get_field_value"),
        il2cpp_set_field_value: bind!(get_api, "il2cpp_set_field_value"),
        register_on_game_initialized: bind!(get_api, "hachimi_register_on_game_initialized"),
        register_menu_section_with_icon: bind!(get_api, "gui_register_menu_section_with_icon"),
        ui_heading: bind!(get_api, "gui_ui_heading"),
        ui_label: bind!(get_api, "gui_ui_label"),
        ui_small: bind!(get_api, "gui_ui_small"),
        ui_separator: bind!(get_api, "gui_ui_separator"),
        ui_button: bind!(get_api, "gui_ui_button"),
        ui_checkbox: bind!(get_api, "gui_ui_checkbox"),
        ui_horizontal: bind!(get_api, "gui_ui_horizontal"),
        ui_colored_label: bind!(get_api, "gui_ui_colored_label"),
        ui_combo_menu: bind!(get_api, "gui_ui_combo_menu"),
        show_notification: bind!(get_api, "gui_show_notification"),
        log: bind!(get_api, "log"),
        get_base_dir: bind!(get_api, "hachimi_get_base_dir"),
        get_data_path: bind!(get_api, "hachimi_get_data_path"),
    };

    let _ = API.set(api);
    let api = API.get().expect("api just set");

    // Cache the two instance pointers while we are still on the game thread.
    let instance = unsafe { (api.hachimi_instance)() };
    if !instance.is_null() {
        HACHIMI_INSTANCE.store(instance as *mut _, Ordering::SeqCst);
        let interceptor = unsafe { (api.hachimi_get_interceptor)(instance) };
        INTERCEPTOR.store(interceptor as *mut _, Ordering::SeqCst);
    }

    API.get()
}

pub fn interceptor() -> *mut Interceptor {
    INTERCEPTOR.load(Ordering::SeqCst)
}

pub fn log_error(message: &str) {
    log_raw(1, "ERROR", message);
}

pub fn log_info(message: &str) {
    log_raw(3, "INFO", message);
}

pub fn log_warn(message: &str) {
    log_raw(2, "WARN", message);
}

fn log_raw(level: i32, target: &str, message: &str) {
    if let Some(api) = API.get() {
        if let (Ok(target), Ok(message)) = (CString::new(target), CString::new(message)) {
            unsafe { (api.log)(level, target.as_ptr(), message.as_ptr()) };
            return;
        }
    }
    // Before the API table exists we can only fall back to stderr (goes to the
    // console / hachimi.log depending on how the game was started).
    eprintln!("[charreplace] {}: {}", target, message);
}

/// Path helper shared by every string-returning path API.
fn path_from(f: FnGetPath) -> Option<std::path::PathBuf> {
    let ptr = unsafe { f() };
    if ptr.is_null() {
        return None;
    }
    let s = unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned();
    Some(std::path::PathBuf::from(s))
}

/// `<game dir>/hachimi` - where Hachimi keeps config.json and where this
/// plugin keeps its own config.
pub fn base_dir() -> Option<std::path::PathBuf> {
    path_from(api()?.get_base_dir)
}

/// Game persistent data dir, the parent of `master/master.mdb`.
pub fn data_path() -> Option<std::path::PathBuf> {
    path_from(api()?.get_data_path)
}

// ---- thin safe wrappers -------------------------------------------------

pub fn ui_heading(ui: *mut c_void, text: &str) {
    if let (Some(api), Ok(c)) = (api(), CString::new(text)) {
        unsafe { (api.ui_heading)(ui, c.as_ptr()) };
    }
}

pub fn ui_label(ui: *mut c_void, text: &str) {
    if let (Some(api), Ok(c)) = (api(), CString::new(text)) {
        unsafe { (api.ui_label)(ui, c.as_ptr()) };
    }
}

pub fn ui_small(ui: *mut c_void, text: &str) {
    if let (Some(api), Ok(c)) = (api(), CString::new(text)) {
        unsafe { (api.ui_small)(ui, c.as_ptr()) };
    }
}

pub fn ui_separator(ui: *mut c_void) {
    if let Some(api) = api() {
        unsafe { (api.ui_separator)(ui) };
    }
}

pub fn ui_button(ui: *mut c_void, text: &str) -> bool {
    match (api(), CString::new(text)) {
        (Some(api), Ok(c)) => unsafe { (api.ui_button)(ui, c.as_ptr()) },
        _ => false,
    }
}

pub fn ui_checkbox(ui: *mut c_void, text: &str, value: &mut bool) -> bool {
    match (api(), CString::new(text)) {
        (Some(api), Ok(c)) => unsafe { (api.ui_checkbox)(ui, c.as_ptr(), value as *mut bool) },
        _ => false,
    }
}

#[allow(dead_code)]
pub fn ui_colored_label(ui: *mut c_void, color: (u8, u8, u8, u8), text: &str) {
    if let (Some(api), Ok(c)) = (api(), CString::new(text)) {
        let (r, g, b, a) = color;
        unsafe { (api.ui_colored_label)(ui, r, g, b, a, c.as_ptr()) };
    }
}

pub fn ui_horizontal(ui: *mut c_void, callback: GuiUiCallback, userdata: *mut c_void) -> bool {
    match api() {
        Some(api) => unsafe { (api.ui_horizontal)(ui, Some(callback), userdata) },
        None => false,
    }
}

/// Dropdown with a built-in search box. `selected` is an index into `items`;
/// Hachimi filters the list by case-insensitive substring, so putting the
/// character id in the label makes ids searchable too.
pub fn ui_combo_menu(ui: *mut c_void, id: &str, selected: &mut i32, items: &[*const c_char]) -> bool {
    let (Some(api), Ok(c_id)) = (api(), CString::new(id)) else {
        return false;
    };
    if items.is_empty() {
        return false;
    }
    unsafe {
        (api.ui_combo_menu)(
            ui,
            c_id.as_ptr(),
            selected as *mut i32,
            items.as_ptr(),
            items.len(),
            std::ptr::null_mut(),
            0,
        )
    }
}

#[allow(dead_code)]
pub fn show_notification(message: &str) {
    if let (Some(api), Ok(c)) = (api(), CString::new(message)) {
        unsafe { (api.show_notification)(c.as_ptr()) };
    }
}

#[allow(dead_code)]
pub fn register_on_game_initialized(callback: GameInitializedCallback) {
    if let Some(api) = api() {
        unsafe { (api.register_on_game_initialized)(Some(callback), std::ptr::null_mut()) };
    }
}

pub fn register_menu_section(title: &str, icon_png: &[u8], callback: GuiMenuSectionCallback) -> bool {
    let Some(api) = api() else { return false };
    let Ok(c_title) = CString::new(title) else { return false };
    let uri = format!("bytes://charreplace/{}.png", title);
    let Ok(c_uri) = CString::new(uri) else { return false };
    unsafe {
        (api.register_menu_section_with_icon)(
            c_title.as_ptr(),
            c_uri.as_ptr(),
            icon_png.as_ptr(),
            icon_png.len(),
            Some(callback),
            std::ptr::null_mut(),
        )
    }
}

// ---- IL2CPP helpers ------------------------------------------------------

pub struct Symbols;

impl Symbols {
    pub fn get_class(assembly: &str, namespace: &str, class_name: &str) -> *mut Il2CppClass {
        let Some(api) = api() else { return std::ptr::null_mut() };
        let (Ok(a), Ok(ns), Ok(cn)) = (
            CString::new(assembly),
            CString::new(namespace),
            CString::new(class_name),
        ) else {
            return std::ptr::null_mut();
        };
        unsafe {
            let image = (api.il2cpp_get_assembly_image)(a.as_ptr());
            if image.is_null() {
                return std::ptr::null_mut();
            }
            (api.il2cpp_get_class)(image, ns.as_ptr(), cn.as_ptr())
        }
    }

    pub fn get_method_addr(class: *mut Il2CppClass, name: &str, args_count: i32) -> *mut c_void {
        let Some(api) = api() else { return std::ptr::null_mut() };
        if class.is_null() {
            return std::ptr::null_mut();
        }
        let Ok(c_name) = CString::new(name) else { return std::ptr::null_mut() };
        unsafe { (api.il2cpp_get_method_addr)(class, c_name.as_ptr(), args_count) }
    }

    pub fn get_field_from_name(class: *mut Il2CppClass, name: &str) -> *mut FieldInfo {
        let Some(api) = api() else { return std::ptr::null_mut() };
        if class.is_null() {
            return std::ptr::null_mut();
        }
        let Ok(c_name) = CString::new(name) else { return std::ptr::null_mut() };
        unsafe { (api.il2cpp_get_field_from_name)(class, c_name.as_ptr()) }
    }

    pub fn get_field_i32(obj: *mut Il2CppObject, field: *mut FieldInfo) -> i32 {
        let mut out: i32 = 0;
        if let Some(api) = api() {
            unsafe { (api.il2cpp_get_field_value)(obj, field, &mut out as *mut i32 as *mut c_void) };
        }
        out
    }

    pub fn set_field_i32(obj: *mut Il2CppObject, field: *mut FieldInfo, value: i32) {
        if let Some(api) = api() {
            unsafe {
                (api.il2cpp_set_field_value)(obj, field, &value as *const i32 as *const c_void)
            };
        }
    }
}
