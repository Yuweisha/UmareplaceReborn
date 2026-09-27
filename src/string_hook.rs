//! Cue-name rewriting at the IL2CPP string level.
//!
//! Why not hook `AudioManager::PlayInternal` directly? Hachimi already hooks it
//! for its captions, and its interceptor is MinHook-based, which refuses to
//! hook the same address twice (`interceptor_hook` then returns null, which is
//! what "failed to hook AudioManager.PlayInternal" in the log means). So this
//! hooks the point every cue name passes through on its way into the game
//! instead: the managed string constructor `il2cpp_string_new`, exported by
//! GameAssembly.dll and not used by Hachimi.
//!
//! Rewriting at this layer has a useful side effect: Hachimi's caption hook
//! reads the cue sheet from the cue info object, which by then *already* holds
//! the replacement character, so its subtitles follow the voice.
//!
//! The filter is deliberately narrow - only names starting with `snd_voi_` and
//! ending in a segment that starts with a character id are touched, and the id
//! has to be in the character range. Everything else is passed straight through
//! to the original constructor.

use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::atomic::{AtomicPtr, Ordering};

use crate::api::{self, Il2CppString};
use crate::audio;

type StringNewFn = extern "C" fn(*const c_char) -> *mut Il2CppString;

static TRAMPOLINE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

/// Every voice cue name starts with this, including the live/story ones
/// (`snd_voi_training_110301`, `snd_voi_live_111400`, `snd_voi_home_111400`).
const VOICE_PREFIX: &str = "snd_voi_";

extern "system" {
    fn GetModuleHandleW(name: *const u16) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
}

pub fn install() -> bool {
    let Some(api) = api::api() else { return false };

    let module = unsafe { GetModuleHandleW(wide("GameAssembly.dll").as_ptr()) };
    if module.is_null() {
        api::log_warn("GameAssembly.dll is not loaded; cue names cannot be rewritten");
        return false;
    }

    let addr = unsafe { GetProcAddress(module, b"il2cpp_string_new\0".as_ptr()) };
    if addr.is_null() {
        api::log_warn("il2cpp_string_new is not exported by GameAssembly.dll");
        return false;
    }

    let orig = (api.interceptor_hook)(api::interceptor(), addr, string_new as *mut c_void);
    if orig.is_null() {
        api::log_warn("failed to hook il2cpp_string_new");
        return false;
    }

    TRAMPOLINE.store(orig, Ordering::SeqCst);
    api::log_info("hooked il2cpp_string_new (voice replacement)");
    true
}

extern "C" fn string_new(text: *const c_char) -> *mut Il2CppString {
    let trampoline = TRAMPOLINE.load(Ordering::SeqCst);
    if trampoline.is_null() {
        // Only reachable if the hook is live without a trampoline, which the
        // installer never does. Nothing safe to call here.
        return std::ptr::null_mut();
    }
    if text.is_null() {
        return unsafe { std::mem::transmute::<*mut c_void, StringNewFn>(trampoline)(text) };
    }

    let original: StringNewFn = unsafe { std::mem::transmute(trampoline) };

    // Everything the game builds goes through here, so bail out before doing any
    // real work unless it even looks like a voice cue.
    let Ok(name) = unsafe { CStr::from_ptr(text) }.to_str() else {
        return original(text);
    };
    if !name.starts_with(VOICE_PREFIX) {
        return original(text);
    }

    let Some(rewritten) = audio::rewrite_cue_name(name) else {
        return original(text);
    };

    let Ok(c_rewritten) = CString::new(rewritten.clone()) else {
        return original(text);
    };

    let result = original(c_rewritten.as_ptr());
    api::log_info(&format!("[audio] {} -> {}", name, rewritten));
    result
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}
