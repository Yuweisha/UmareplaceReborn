//! Cue-name rewriting at the IL2CPP string level.
//!
//! Why not hook `AudioManager::PlayInternal` directly? Hachimi already hooks it
//! for its captions, and its interceptor is MinHook-based, which refuses to
//! hook the same address twice (`interceptor_hook` then returns null, which is
//! what "failed to hook AudioManager.PlayInternal" in the log means).
//!
//! Hooking the string constructor instead looked impossible at first, because
//! `il2cpp_string_new` is *not* in GameAssembly.dll's export table under that
//! name. Hachimi finds it by walking the jump table inside UnityPlayer.dll,
//! whose entries carry obfuscated names (`_385d313a1cf9df6d8e0b`) that *are*
//! exported by GameAssembly.dll.
//!
//! The API Hachimi hands plugins, however, already contains a wrapper for
//! `il2cpp_string_new`. That wrapper is a thunk, so its opening instructions are
//! a RIP-relative indirect jump to the real function - reading the jump target
//! gives the address to hook, without depending on the game version or on
//! Hachimi's hardcoded table offset.

use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::atomic::{AtomicPtr, Ordering};

use crate::api::{self, Il2CppString};
use crate::audio;

type StringNewFn = extern "C" fn(*const c_char) -> *mut Il2CppString;

static TRAMPOLINE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

/// Every voice cue name starts with this, including the live/story ones
/// (`snd_voi_training_110301`, `snd_voi_live_111400`, `snd_voi_home_111400`).
const VOICE_PREFIX: &str = "snd_voi_";

/// Bytes of the wrapper inspected while looking for the forwarding jump. The
/// jump is within the first few instructions; this is only a safety bound.
const THUNK_SCAN: usize = 32;

/// Page protections that mean "this address holds code".
const PAGE_EXECUTE: u32 = 0x10;
const PAGE_EXECUTE_READ: u32 = 0x20;
const PAGE_EXECUTE_READWRITE: u32 = 0x40;
const PAGE_EXECUTE_WRITECOPY: u32 = 0x80;
const MEM_COMMIT: u32 = 0x1000;

#[repr(C)]
struct MemBasicInfo {
    base_address: *mut c_void,
    allocation_base: *mut c_void,
    allocation_protect: u32,
    partition_id: u16,
    _pad: u16,
    region_size: usize,
    state: u32,
    protect: u32,
    type_: u32,
    _pad2: u32,
}

extern "system" {
    fn VirtualQuery(
        address: *const c_void,
        buffer: *mut MemBasicInfo,
        length: usize,
    ) -> usize;
}

/// Guard against hooking something that is not code: a wrong address would take
/// the game down on the next audio call. Only executable, committed pages pass.
unsafe fn is_executable(address: *const c_void) -> bool {
    let mut info: MemBasicInfo = std::mem::zeroed();
    let written = VirtualQuery(address, &mut info, std::mem::size_of::<MemBasicInfo>());
    if written == 0 || info.state != MEM_COMMIT {
        return false;
    }
    matches!(
        info.protect,
        PAGE_EXECUTE | PAGE_EXECUTE_READ | PAGE_EXECUTE_READWRITE | PAGE_EXECUTE_WRITECOPY
    )
}

pub fn install() -> bool {
    let Some(api) = api::api() else { return false };

    let wrapper = api.il2cpp_string_new as usize as *const c_void;
    let Some(real) = (unsafe { resolve_thunk(wrapper) }) else {
        api::log_warn("could not resolve the real il2cpp_string_new from its wrapper");
        return false;
    };

    if !(unsafe { is_executable(real) }) {
        api::log_warn("resolved il2cpp_string_new does not point at executable memory; not hooking");
        return false;
    }

    let orig = (api.interceptor_hook)(api::interceptor(), real, string_new as *mut c_void);
    if orig.is_null() {
        api::log_warn("failed to hook il2cpp_string_new");
        return false;
    }

    TRAMPOLINE.store(orig, Ordering::SeqCst);
    api::log_info("hooked il2cpp_string_new (voice replacement)");
    true
}

/// Follow the thunk at `wrapper` to the function it forwards to.
///
/// Covers the two shapes rustc emits for a wrapper around a `Lazy<fn>`:
/// `jmp qword ptr [rip+disp]` (FF 25) and `mov rax, [rip+disp]; jmp rax`
/// (48 8B 05 ...). Returns `None` if neither shows up nearby.
unsafe fn resolve_thunk(wrapper: *const c_void) -> Option<*mut c_void> {
    if wrapper.is_null() {
        return None;
    }
    let code = wrapper as *const u8;
    let (instr_off, instr_len, disp) = find_rip_load(std::slice::from_raw_parts(code, THUNK_SCAN))?;
    // The instruction reads from [rip + disp], i.e. from the address just past
    // itself plus the displacement; that slot holds the real function pointer.
    let slot = code.add(instr_off + instr_len).offset(disp as isize);
    let target = *(slot as *const *mut c_void);
    if target.is_null() {
        None
    } else {
        Some(target)
    }
}

/// Locate a RIP-relative indirect jump inside `code`.
///
/// Returns `(instruction offset, instruction length, displacement)` so the
/// caller can compute the address the instruction reads from.
fn find_rip_load(code: &[u8]) -> Option<(usize, usize, i32)> {
    let mut i = 0;
    while i + 6 <= code.len() {
        // FF 25 disp32 - jmp qword ptr [rip+disp]
        if code[i] == 0xff && code[i + 1] == 0x25 {
            let disp = i32::from_le_bytes(code[i + 2..i + 6].try_into().unwrap());
            return Some((i, 6, disp));
        }
        // 48 8B 05 disp32 - mov rax, qword ptr [rip+disp]
        if code[i] == 0x48 && code[i + 1] == 0x8b && code[i + 2] == 0x05 {
            if i + 7 > code.len() {
                return None;
            }
            let disp = i32::from_le_bytes(code[i + 3..i + 7].try_into().unwrap());
            return Some((i, 7, disp));
        }
        i += 1;
    }
    None
}

extern "C" fn string_new(text: *const c_char) -> *mut Il2CppString {
    let trampoline = TRAMPOLINE.load(Ordering::SeqCst);
    if trampoline.is_null() {
        // Only reachable if the hook is live without a trampoline, which the
        // installer never does. Nothing safe to call here.
        return std::ptr::null_mut();
    }

    let original: StringNewFn = unsafe { std::mem::transmute(trampoline) };

    if text.is_null() {
        return original(text);
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_indirect_jmp() {
        // FF 25 10 00 00 00 -> jmp qword ptr [rip+0x10]
        let code = [0x48, 0x83, 0xec, 0x28, 0xff, 0x25, 0x10, 0x00, 0x00, 0x00];
        assert_eq!(find_rip_load(&code), Some((4, 6, 0x10)));
    }

    #[test]
    fn finds_mov_rax_variant() {
        // 48 8B 05 20 00 00 00 -> mov rax, qword ptr [rip+0x20]
        let code = [0x55, 0x48, 0x8b, 0x05, 0x20, 0x00, 0x00, 0x00, 0xff, 0xe0];
        assert_eq!(find_rip_load(&code), Some((1, 7, 0x20)));
    }

    #[test]
    fn gives_up_on_code_without_a_load() {
        let code = [0x90; 32]; // nops
        assert_eq!(find_rip_load(&code), None);
        assert_eq!(find_rip_load(&[]), None);
    }
}
