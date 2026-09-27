//! Voice replacement.
//!
//! Every piece of audio the game plays goes through
//! `Gallop.AudioManager::PlayInternal(SoundGroup, RequestCueInfo, PlayParameters, AutoStopType)`.
//! The cue sheet name carries the character id - `snd_voi_title_100100` belongs
//! to character 1001 - which is also how Hachimi derives the character for its
//! captions. Rewriting that id to the replacement character makes the game play
//! the other character's voice, with no asset unpacked or repacked.
//!
//! The cue sheet is rewritten *before* the original method runs. Hachimi's
//! caption hook sits in the same call chain and reads the cue info, so it picks
//! up the replacement character and shows their subtitles as well.
//!
//! Only cue sheets whose last `_`-separated segment starts with a four digit
//! character id can be rewritten. Cue sheets that do not follow that convention
//! (system sounds, some songs) are deliberately left alone; turn on
//! `log_audio_cues` in the config to dump every cue the game plays, which is how
//! the remaining groups can be mapped.

use std::ffi::c_void;
use std::sync::atomic::{AtomicPtr, Ordering};

use crate::api::{self, Il2CppObject, Il2CppString, Symbols};
use crate::config;

/// `Cute.Cri.RequestCueInfo`
#[repr(C)]
pub struct RequestCueInfo {
    pub cue_sheet_name: *mut Il2CppString,
    pub cue_name: *mut Il2CppString,
    pub cue_id: i32,
}

/// `Cute.Cri.AudioPlayback` - passed back by value, so the field order and
/// nesting have to match the managed struct exactly.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct AudioPlayback {
    /// `CriAtomExPlayback { id: u32 }`
    pub cri_atom_ex_playback: u32,
    pub is_error: bool,
    pub sound_group: i32,
    pub is_3d_sound: bool,
    pub atom_source_list_index: i32,
    pub cue_sheet_name: *mut Il2CppString,
    pub cue_name: *mut Il2CppString,
    pub cue_id: i32,
}

type PlayInternalFn = extern "C" fn(
    this: *mut Il2CppObject,
    group: i32,
    cue_info: *mut RequestCueInfo,
    play_param: *mut Il2CppObject,
    stop_type: i32,
) -> AudioPlayback;

static PLAY_INTERNAL_TRAMPOLINE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

/// Holds on to the last string we handed the game. CriWare copies what it needs
/// synchronously, but a collection in the same frame could still free it.
static LAST_CUE_SHEET: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

/// `Cute.Cri.SoundGroup`
const SOUND_GROUP_VOICE: i32 = 2;

/// Character ids live in the 1000..2000 range in every game version so far.
const CHARA_ID_MIN: i32 = 1000;
const CHARA_ID_MAX: i32 = 1999;

pub fn install() -> bool {
    let Some(api) = api::api() else { return false };

    let class = Symbols::get_class("umamusume", "Gallop", "AudioManager");
    if class.is_null() {
        api::log_error("class not found: Gallop.AudioManager");
        return false;
    }

    let addr = Symbols::get_method_addr(class, "PlayInternal", 4);
    if addr.is_null() {
        api::log_error("method not found: AudioManager.PlayInternal(int)");
        return false;
    }

    let orig = (api.interceptor_hook)(api::interceptor(), addr, play_internal as *mut c_void);
    if orig.is_null() {
        // Hachimi hooks this method itself for its captions and MinHook refuses
        // to hook the same address twice - expected. Voice replacement then
        // happens one layer down, in string_hook.
        api::log_warn(
            "AudioManager.PlayInternal already hooked (Hachimi captions); falling back to the string hook",
        );
        return false;
    }

    PLAY_INTERNAL_TRAMPOLINE.store(orig, Ordering::SeqCst);
    api::log_info("hooked AudioManager.PlayInternal (voice replacement)");
    true
}

extern "C" fn play_internal(
    this: *mut Il2CppObject,
    group: i32,
    cue_info: *mut RequestCueInfo,
    play_param: *mut Il2CppObject,
    stop_type: i32,
) -> AudioPlayback {
    rewrite_cue_sheet(group, cue_info);

    let trampoline = PLAY_INTERNAL_TRAMPOLINE.load(Ordering::SeqCst);
    if trampoline.is_null() {
        // Should not happen: the hook is only installed together with the
        // trampoline. Returning an "error" playback is the least bad option.
        api::log_error("PlayInternal trampoline missing");
        return AudioPlayback {
            cri_atom_ex_playback: 0,
            is_error: true,
            sound_group: group,
            is_3d_sound: false,
            atom_source_list_index: -1,
            cue_sheet_name: std::ptr::null_mut(),
            cue_name: std::ptr::null_mut(),
            cue_id: -1,
        };
    }

    let original: PlayInternalFn = unsafe { std::mem::transmute(trampoline) };
    original(this, group, cue_info, play_param, stop_type)
}

/// Swap the character id inside the cue sheet name, in place.
fn rewrite_cue_sheet(group: i32, cue_info: *mut RequestCueInfo) {
    if cue_info.is_null() {
        return;
    }

    let info = unsafe { &mut *cue_info };
    let Some(sheet) = Symbols::string_to_rust(info.cue_sheet_name) else {
        return;
    };

    // Diagnostic mode: dump what the game is playing, so unmapped cue families
    // (song tracks in particular) can be identified from a log.
    if config::with(|config| config.log_audio_cues) {
        api::log_info(&format!(
            "[audio] group={} cue_sheet={} cue_name={} cue_id={}",
            group,
            sheet,
            Symbols::string_to_rust(info.cue_name).unwrap_or_default(),
            info.cue_id
        ));
    }

    if !config::with(|config| config.replace_voice) || group != SOUND_GROUP_VOICE {
        return;
    }

    let Some(new_sheet) = rewrite_cue_name(&sheet) else {
        return;
    };

    let new_ptr = Symbols::rust_to_string(&new_sheet);
    if new_ptr.is_null() {
        return;
    }

    info.cue_sheet_name = new_ptr;
    LAST_CUE_SHEET.store(new_ptr as *mut c_void, Ordering::SeqCst);
    api::log_info(&format!("[audio] {} -> {}", sheet, new_sheet));
}

/// Rewrite the character id inside a cue sheet name, if a rule maps it.
///
/// Returns `None` for cue names that carry no per-character id, so callers can
/// pass them through untouched.
pub fn rewrite_cue_name(sheet: &str) -> Option<String> {
    let (chara_id, range) = chara_id_in(sheet)?;
    let new_chara_id = config::lookup_new_char(chara_id)?;
    if new_chara_id == chara_id {
        return None;
    }

    let mut new_sheet = sheet.to_owned();
    new_sheet.replace_range(range, &format!("{:04}", new_chara_id));
    Some(new_sheet)
}

/// Find the character id at the start of the cue sheet's last `_`-separated
/// segment, plus the byte range it occupies.
///
/// Same convention Hachimi uses for its captions (`snd_voi_title_100100` ->
/// 1001). Real cue names come in two shapes - `snd_voi_training_110301` and
/// `snd_voi_title_1053` - so four digits is the minimum, and the id has to fall
/// in the character range to avoid rewriting unrelated numbers.
fn chara_id_in(sheet: &str) -> Option<(i32, std::ops::Range<usize>)> {
    let start = sheet.rfind('_').map(|index| index + 1).unwrap_or(0);
    let segment = &sheet[start..];
    if segment.len() < 4 {
        return None;
    }

    let head = &segment[..4];
    if !head.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let chara_id: i32 = head.parse().ok()?;
    if !(CHARA_ID_MIN..=CHARA_ID_MAX).contains(&chara_id) {
        return None;
    }

    Some((chara_id, start..start + 4))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(sheet: &str) -> Option<i32> {
        chara_id_in(sheet).map(|(id, _)| id)
    }

    #[test]
    fn reads_character_ids_from_real_cue_names() {
        assert_eq!(parse("snd_voi_training_110301"), Some(1103));
        assert_eq!(parse("snd_voi_training_110300"), Some(1103));
        assert_eq!(parse("snd_voi_live_111400"), Some(1114));
        assert_eq!(parse("snd_voi_home_111401"), Some(1114));
        assert_eq!(parse("snd_voi_outgame_102901"), Some(1029));
        assert_eq!(parse("snd_voi_title_1053"), Some(1053));
        assert_eq!(parse("snd_voi_tc_1053"), Some(1053));
    }

    #[test]
    fn ignores_names_without_a_character_id() {
        assert_eq!(parse("snd_sfx_common"), None); // 最后一段不是数字
        assert_eq!(parse("snd_voi_abc_123"), None); // 不足四位
        assert_eq!(parse("snd_voi_xyz_9999"), None); // 超出角色范围
        assert_eq!(parse("snd_voi_"), None);
        assert_eq!(parse(""), None);
    }

    #[test]
    fn only_the_id_is_replaced() {
        let (id, range) = chara_id_in("snd_voi_live_111400").unwrap();
        assert_eq!(id, 1114);
        let mut out = "snd_voi_live_111400".to_owned();
        out.replace_range(range, "1030");
        assert_eq!(out, "snd_voi_live_103000");
    }
}
