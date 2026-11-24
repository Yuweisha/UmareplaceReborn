
use std::ffi::c_void;
use std::sync::atomic::{AtomicPtr, Ordering};

use crate::api::{self, FieldInfo, Il2CppObject, Symbols};
use crate::config;

static GET_BY_SYSTEM_TEXT_ID: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static CREATE_SYSTEM_TEXT_BUILD_INFO: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static EVENT_CHARA_MOTION_PATH: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static CHARA_OVERRIDE_MOTION_PATH: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static EVENT_CHARA_MOTION_BASE_PATH: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static CHARA_ID_FIELD: AtomicPtr<FieldInfo> = AtomicPtr::new(std::ptr::null_mut());

fn motion_source(chara_id: i32) -> Option<i32> {
    if !config::with(|c| c.enable && c.borrow_motion) {
        return None;
    }
    config::with(|c| {
        c.data
            .iter()
            .find(|e| {
                e.new_char_id == chara_id && e.orig_char_id > 0 && e.orig_char_id != chara_id
            })
            .map(|e| e.orig_char_id)
    })
}

fn trampoline(slot: &AtomicPtr<c_void>) -> *mut c_void {
    slot.load(Ordering::SeqCst)
}

type GetBySystemTextIdFn = extern "C" fn(i32, i32, *mut c_void) -> *mut Il2CppObject;
extern "C" fn get_by_system_text_id(
    chara_id: i32,
    system_text_id: i32,
    method: *mut c_void,
) -> *mut Il2CppObject {
    let ptr = trampoline(&GET_BY_SYSTEM_TEXT_ID);
    if ptr.is_null() {
        return std::ptr::null_mut();
    }
    let original: GetBySystemTextIdFn = unsafe { std::mem::transmute(ptr) };
    match motion_source(chara_id) {
        Some(orig) => original(orig, system_text_id, method),
        None => original(chara_id, system_text_id, method),
    }
}

type CreateSystemTextBuildInfoFn =
    extern "C" fn(*mut Il2CppObject, i32, *mut c_void) -> *mut Il2CppObject;
extern "C" fn create_system_text_build_info(
    build_info: *mut Il2CppObject,
    motion_chara_id: i32,
    method: *mut c_void,
) -> *mut Il2CppObject {
    let ptr = trampoline(&CREATE_SYSTEM_TEXT_BUILD_INFO);
    if ptr.is_null() {
        return std::ptr::null_mut();
    }
    let original: CreateSystemTextBuildInfoFn = unsafe { std::mem::transmute(ptr) };
    let ret = original(build_info, motion_chara_id, method);
    if ret.is_null() {
        return ret;
    }
    let Some(orig) = motion_source(motion_chara_id) else {
        return ret;
    };
    let field = CHARA_ID_FIELD.load(Ordering::SeqCst);
    if field.is_null() {
        return ret;
    }
    if Symbols::get_field_i32(ret, field) == motion_chara_id {
        Symbols::set_field_i32(ret, field, orig);
        api::log_info(&format!(
            "motion build info {} -> {} (model unchanged)",
            motion_chara_id, orig
        ));
    }
    ret
}

type EventCharaMotionPathFn =
    extern "C" fn(i32, *mut c_void, *mut c_void, bool, *mut c_void) -> *mut c_void;
extern "C" fn event_chara_motion_path(
    chara_id: i32,
    motion_name: *mut c_void,
    suffix: *mut c_void,
    is_mirror: bool,
    method: *mut c_void,
) -> *mut c_void {
    let ptr = trampoline(&EVENT_CHARA_MOTION_PATH);
    if ptr.is_null() {
        return std::ptr::null_mut();
    }
    let original: EventCharaMotionPathFn = unsafe { std::mem::transmute(ptr) };
    let use_id = motion_source(chara_id).unwrap_or(chara_id);
    original(use_id, motion_name, suffix, is_mirror, method)
}

type OverrideMotionPathFn = extern "C" fn(i32, i32, *mut c_void, *mut c_void) -> *mut c_void;
extern "C" fn chara_override_motion_path(
    chara_id: i32,
    sub_id: i32,
    motion_name: *mut c_void,
    method: *mut c_void,
) -> *mut c_void {
    let ptr = trampoline(&CHARA_OVERRIDE_MOTION_PATH);
    if ptr.is_null() {
        return std::ptr::null_mut();
    }
    let original: OverrideMotionPathFn = unsafe { std::mem::transmute(ptr) };
    let use_id = motion_source(chara_id).unwrap_or(chara_id);
    original(use_id, sub_id, motion_name, method)
}

type BasePathFn = extern "C" fn(i32, i32, *mut c_void, *mut c_void) -> *mut c_void;
extern "C" fn event_chara_motion_base_path(
    chara_id: i32,
    sub_id: i32,
    anim_command: *mut c_void,
    method: *mut c_void,
) -> *mut c_void {
    let ptr = trampoline(&EVENT_CHARA_MOTION_BASE_PATH);
    if ptr.is_null() {
        return std::ptr::null_mut();
    }
    let original: BasePathFn = unsafe { std::mem::transmute(ptr) };
    let use_id = motion_source(chara_id).unwrap_or(chara_id);
    original(use_id, sub_id, anim_command, method)
}

fn hook(
    class: *mut c_void,
    name: &str,
    args_count: i32,
    detour: *mut c_void,
    slot: &AtomicPtr<c_void>,
) -> bool {
    let Some(api) = api::api() else { return false };
    if class.is_null() {
        return false;
    }
    let addr = Symbols::get_method_addr(class as _, name, args_count);
    if addr.is_null() {
        api::log_warn(&format!("motion: method not found: {}", name));
        return false;
    }
    let interceptor = api::interceptor();
    if interceptor.is_null() {
        return false;
    }
    let tramp = (api.interceptor_hook)(interceptor, addr, detour);
    if tramp.is_null() {
        api::log_error(&format!("motion: failed to hook {}", name));
        return false;
    }
    slot.store(tramp, Ordering::SeqCst);
    api::log_info(&format!("motion: hooked {}", name));
    true
}

pub fn install() -> bool {
    let mut ok = false;

    if let Some(clazz) = class_of("CharacterBuildInfo") {
        let field = Symbols::get_field_from_name(clazz, "_charaId");
        if !field.is_null() {
            CHARA_ID_FIELD.store(field, Ordering::SeqCst);
        }
    }

    if let Some(clazz) = class_of("MasterCharacterSystemText") {
        ok |= hook(
            clazz,
            "GetBySystemTextId",
            2,
            get_by_system_text_id as *mut c_void,
            &GET_BY_SYSTEM_TEXT_ID,
        );
    }

    if let Some(clazz) = class_of("LiveUtil") {
        ok |= hook(
            clazz,
            "CreateSystemTextMotionBuildInfo",
            2,
            create_system_text_build_info as *mut c_void,
            &CREATE_SYSTEM_TEXT_BUILD_INFO,
        );
    }

    if let Some(clazz) = class_of("ResourcePath") {
        ok |= hook(
            clazz,
            "GetEventCharaMotionPath",
            4,
            event_chara_motion_path as *mut c_void,
            &EVENT_CHARA_MOTION_PATH,
        );
        ok |= hook(
            clazz,
            "GetCharaOverrideMotionPath",
            3,
            chara_override_motion_path as *mut c_void,
            &CHARA_OVERRIDE_MOTION_PATH,
        );
    }

    if let Some(clazz) = class_of("StoryMotionInfo") {
        ok |= hook(
            clazz,
            "GetEventCharaMotionBasePath",
            3,
            event_chara_motion_base_path as *mut c_void,
            &EVENT_CHARA_MOTION_BASE_PATH,
        );
    }

    ok
}

fn class_of(name: &str) -> Option<*mut c_void> {
    let clazz = Symbols::get_class("umamusume", "Gallop", name);
    if clazz.is_null() {
        api::log_error(&format!("motion: class not found: Gallop.{}", name));
        None
    } else {
        Some(clazz as *mut c_void)
    }
}
