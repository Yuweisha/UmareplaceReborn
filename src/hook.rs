//! The actual character replacement.
//!
//! Ported from Trainers' Legend G / Hachimi's built-in implementation: hook
//! `Gallop.CharacterBuildInfo.Rebuild` and rewrite the character/dress fields
//! before the model is built, plus `Gallop.WorkSingleModeCharaData.GetRaceDressId`
//! so race outfits follow the replacement as well.

use std::ffi::c_void;
use std::sync::atomic::{AtomicPtr, Ordering};

use crate::api::{self, FieldInfo, Il2CppObject, Symbols};
use crate::config;
use crate::db;

/// Mirrors `Gallop.CharacterBuildInfo.ControllerType`.
const CT_DEFAULT: i32 = 0x0;
const CT_HOME_TALK: i32 = 0x7;
const CT_HOME_WALK: i32 = 0x8;
const CT_HOME_STAND: i32 = 0x6;
const CT_MINI: i32 = 0xd;
/// Pseudo type used for dress ids coming from `GetRaceDressId`.
const CT_ORIG: i32 = 0x1919810;

/// Vanilla id of the mini character model, used when the original dress has no
/// mini model of its own.
const MINI_FALLBACK_DRESS_ID: i32 = 2;

const TRAINER_CHARA_ID: i32 = 9001;

static REBUILD_TRAMPOLINE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static RACE_DRESS_TRAMPOLINE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

static CHARA_ID_FIELD: AtomicPtr<FieldInfo> = AtomicPtr::new(std::ptr::null_mut());
static CARD_ID_FIELD: AtomicPtr<FieldInfo> = AtomicPtr::new(std::ptr::null_mut());
static DRESS_ID_FIELD: AtomicPtr<FieldInfo> = AtomicPtr::new(std::ptr::null_mut());
static CONTROLLER_TYPE_FIELD: AtomicPtr<FieldInfo> = AtomicPtr::new(std::ptr::null_mut());
static HEAD_MODEL_SUB_ID_FIELD: AtomicPtr<FieldInfo> = AtomicPtr::new(std::ptr::null_mut());
static MOTION_DRESS_ID_FIELD: AtomicPtr<FieldInfo> = AtomicPtr::new(std::ptr::null_mut());

fn is_replacable(controller_type: i32) -> bool {
    !matches!(controller_type, CT_DEFAULT | CT_HOME_TALK | CT_HOME_WALK | CT_MINI)
}

fn find_replacement(chara_id: i32, mini: bool) -> Option<(i32, i32)> {
    config::with(|config| {
        config
            .data
            .iter()
            .find(|entry| entry.orig_char_id == chara_id && (!mini || entry.replace_mini))
            .map(|entry| (entry.new_char_id, entry.new_cloth_id))
    })
}

/// Which character a dress belongs to, 0 for the generic ones. A dress from
/// another character leaves the rig with a body from one model and a head from
/// another, which collapses into a T-pose.
fn align_dress_with_chara(dress_id: &mut i32, new_chara_id: i32) {
    if *dress_id == 0 {
        return;
    }
    let owner = db::dress_chara_id(*dress_id);
    if owner == 0 || owner == new_chara_id {
        return;
    }

    let fallback = new_chara_id * 100 + 1;
    api::log_warn(&format!(
        "dressId {} does not belong to chara {}, falling back to {}",
        *dress_id, new_chara_id, fallback
    ));
    *dress_id = fallback;
}

fn replace_char_controller(
    chara_id: &mut i32,
    dress_id: &mut i32,
    head_id: &mut i32,
    controller_type: i32,
    allow_orig: bool,
) -> bool {
    let (enabled, replace_universal) =
        config::with(|config| (config.enable, config.replace_universal));
    if !enabled || config::with(|config| config.data.is_empty()) {
        return false;
    }

    // Generic dresses (dress_id < 100000: race wear, uniform, gym clothes)
    // belong to scenes the game choreographs itself. Skipping the dress alone
    // is not enough: rewriting the character id while leaving the generic
    // dress behind produces combos the game has no assets for - the race
    // screen ends up without a thumbnail and a cutscene hangs. Skip the whole
    // replacement instead.
    if !replace_universal && *dress_id < 100000 {
        return false;
    }

    if controller_type == CT_MINI {
        if let Some((new_chara_id, new_dress_id)) = find_replacement(*chara_id, true) {
            if db::dress_has_mini(new_dress_id) {
                *chara_id = new_chara_id;
                *dress_id = new_dress_id;
                align_dress_with_chara(dress_id, new_chara_id);
                *head_id = db::dress_head_sub_id(*dress_id);
                return true;
            }
            api::log_warn(&format!("dressId {} has no mini character", new_dress_id));
            return false;
        }

        if !db::dress_has_mini(*dress_id) {
            api::log_warn(&format!(
                "dressId {} has no mini character, falling back to {}",
                *dress_id, MINI_FALLBACK_DRESS_ID
            ));
            *dress_id = MINI_FALLBACK_DRESS_ID;
            return true;
        }
        return false;
    }

    if !is_replacable(controller_type) {
        return false;
    }

    // CT_ORIG is not a real controller: the game uses it as a marker when it
    // asks what dress a record carries. Rewriting values there makes it ask
    // again, looping hundreds of times a second and hanging the load. Only
    // GetRaceDressId, which needs the rewrite, passes allow_orig.
    if controller_type == CT_ORIG && !allow_orig {
        return false;
    }

    // The trainer can't be replaced while standing at home.
    if *chara_id == TRAINER_CHARA_ID && controller_type == CT_HOME_STAND {
        return false;
    }

    if let Some((new_chara_id, new_dress_id)) = find_replacement(*chara_id, false) {
        *chara_id = new_chara_id;
        *dress_id = new_dress_id;
        align_dress_with_chara(dress_id, new_chara_id);
        *head_id = db::dress_head_sub_id(*dress_id);
        return true;
    }

    false
}

extern "C" fn rebuild_hook(this: *mut Il2CppObject) {
    let chara_field = CHARA_ID_FIELD.load(Ordering::SeqCst);
    if !chara_field.is_null() {
        let mut chara_id = Symbols::get_field_i32(this, chara_field);
        let mut dress_id = Symbols::get_field_i32(this, DRESS_ID_FIELD.load(Ordering::SeqCst));
        let controller_type =
            Symbols::get_field_i32(this, CONTROLLER_TYPE_FIELD.load(Ordering::SeqCst));
        let mut head_model_sub_id =
            Symbols::get_field_i32(this, HEAD_MODEL_SUB_ID_FIELD.load(Ordering::SeqCst));

        if replace_char_controller(
            &mut chara_id,
            &mut dress_id,
            &mut head_model_sub_id,
            controller_type,
            false,
        ) {
            Symbols::set_field_i32(this, chara_field, chara_id);
            Symbols::set_field_i32(this, DRESS_ID_FIELD.load(Ordering::SeqCst), dress_id);
            Symbols::set_field_i32(
                this,
                HEAD_MODEL_SUB_ID_FIELD.load(Ordering::SeqCst),
                head_model_sub_id,
            );
            Symbols::set_field_i32(this, MOTION_DRESS_ID_FIELD.load(Ordering::SeqCst), dress_id);
            Symbols::set_field_i32(this, CARD_ID_FIELD.load(Ordering::SeqCst), -1);
        }
    }

    let trampoline = REBUILD_TRAMPOLINE.load(Ordering::SeqCst);
    if !trampoline.is_null() {
        let original: extern "C" fn(*mut Il2CppObject) = unsafe { std::mem::transmute(trampoline) };
        original(this);
    }
}

extern "C" fn race_dress_hook(this: *mut Il2CppObject, is_apply_dress_change: bool) -> i32 {
    let trampoline = RACE_DRESS_TRAMPOLINE.load(Ordering::SeqCst);
    if trampoline.is_null() {
        return 0;
    }
    let original: extern "C" fn(*mut Il2CppObject, bool) -> i32 =
        unsafe { std::mem::transmute(trampoline) };
    let ret = original(this, is_apply_dress_change);

    // Dress ids of owned characters are `<chara id> * 100 + <cloth number>`.
    if ret > 100000 && ret <= 999999 {
        let chara_id = if ret / 10000 == 90 { ret % 10000 } else { ret / 100 };
        let mut new_chara_id = chara_id;
        let mut new_dress_id = ret;
        let mut new_head_id = 0;
        if replace_char_controller(
            &mut new_chara_id,
            &mut new_dress_id,
            &mut new_head_id,
            CT_ORIG,
            true,
        ) {
            return new_dress_id;
        }
    }

    ret
}

fn hook_method(
    class: *mut c_void,
    name: &str,
    args_count: i32,
    hook: *mut c_void,
    trampoline_slot: &AtomicPtr<c_void>,
) -> bool {
    let Some(api) = api::api() else { return false };
    if class.is_null() {
        return false;
    }

    let addr = Symbols::get_method_addr(class as _, name, args_count);
    if addr.is_null() {
        api::log_error(&format!("method not found: {}", name));
        return false;
    }

    let interceptor = api::interceptor();
    if interceptor.is_null() {
        api::log_error("interceptor unavailable");
        return false;
    }

    let trampoline = (api.interceptor_hook)(interceptor, addr, hook);
    if trampoline.is_null() {
        api::log_error(&format!("failed to hook {}", name));
        return false;
    }
    trampoline_slot.store(trampoline, Ordering::SeqCst);
    api::log_info(&format!("hooked {}", name));
    true
}

/// Install both hooks. Must run once Hachimi has initialised IL2CPP (its own
/// hooks are installed by then, so the classes resolve). Re-running is safe:
/// Hachimi returns the existing trampoline for an already hooked function.
///
/// Returns false when the classes could not be resolved, so the caller can
/// retry later.
pub fn install() -> bool {
    let mut ok = true;

    let class = Symbols::get_class("umamusume", "Gallop", "CharacterBuildInfo");
    if class.is_null() {
        api::log_error("class not found: Gallop.CharacterBuildInfo");
        ok = false;
    } else {
        CHARA_ID_FIELD.store(Symbols::get_field_from_name(class, "_charaId"), Ordering::SeqCst);
        CARD_ID_FIELD.store(Symbols::get_field_from_name(class, "_cardId"), Ordering::SeqCst);
        DRESS_ID_FIELD.store(Symbols::get_field_from_name(class, "_dressId"), Ordering::SeqCst);
        CONTROLLER_TYPE_FIELD.store(
            Symbols::get_field_from_name(class, "_controllerType"),
            Ordering::SeqCst,
        );
        HEAD_MODEL_SUB_ID_FIELD.store(
            Symbols::get_field_from_name(class, "_headModelSubId"),
            Ordering::SeqCst,
        );
        MOTION_DRESS_ID_FIELD.store(
            Symbols::get_field_from_name(class, "_motionDressId"),
            Ordering::SeqCst,
        );

        ok &= hook_method(
            class as *mut c_void,
            "Rebuild",
            0,
            rebuild_hook as *mut c_void,
            &REBUILD_TRAMPOLINE,
        );
    }

    let race_class = Symbols::get_class("umamusume", "Gallop", "WorkSingleModeCharaData");
    if race_class.is_null() {
        api::log_error("class not found: Gallop.WorkSingleModeCharaData");
        ok = false;
    } else {
        ok &= hook_method(
            race_class as *mut c_void,
            "GetRaceDressId",
            1,
            race_dress_hook as *mut c_void,
            &RACE_DRESS_TRAMPOLINE,
        );
    }

    // Voice replacement. The audio entry point is the cleaner place, but
    // Hachimi normally holds it for its captions, so the string-level hook is
    // what actually carries the feature.
    crate::audio::install();
    crate::string_hook::install();

    ok
}
