//! The in-game menu section.
//!
//! Hachimi renders plugin sections inside its config editor. Every widget call
//! goes through the plugin API; combos are the same searchable dropdown used
//! by "Live vocals swap", and because labels are `"<id> <name>"` the search
//! box matches ids as well as names.

use std::collections::HashMap;
use std::ffi::{c_char, c_void, CString};
use std::sync::{Arc, Mutex};

use once_cell::sync::Lazy;

use crate::api;
use crate::config;
use crate::db;

/// A stable list of combo box entries. The `CString`s must outlive the
/// pointers handed to Hachimi, so both live in the same allocation.
pub struct ComboList {
    _labels: Vec<CString>,
    ptrs: Vec<*const c_char>,
    ids: Vec<i32>,
    texts: Vec<String>,
}

unsafe impl Send for ComboList {}
unsafe impl Sync for ComboList {}

impl ComboList {
    fn from_items(items: Vec<(i32, String)>) -> Self {
        let mut labels = Vec::with_capacity(items.len());
        let mut ids = Vec::with_capacity(items.len());
        let mut texts = Vec::with_capacity(items.len());
        for (id, label) in items {
            labels.push(CString::new(label.clone()).unwrap_or_default());
            ids.push(id);
            texts.push(label);
        }
        let ptrs = labels.iter().map(|label| label.as_ptr()).collect();
        Self {
            _labels: labels,
            ptrs,
            ids,
            texts,
        }
    }

    fn index_of(&self, id: i32) -> usize {
        self.ids.iter().position(|candidate| *candidate == id).unwrap_or(0)
    }

    fn contains(&self, id: i32) -> bool {
        self.ids.contains(&id)
    }

    pub fn pointers(&self) -> &[*const c_char] {
        &self.ptrs
    }

    fn entries(&self) -> impl Iterator<Item = (i32, String)> + '_ {
        self.ids.iter().copied().zip(self.texts.iter().cloned())
    }
}

static COMBO_CACHE: Lazy<Mutex<HashMap<String, Arc<ComboList>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn cached(key: &str, build: impl FnOnce() -> Vec<(i32, String)>) -> Arc<ComboList> {
    let mut cache = COMBO_CACHE.lock().unwrap();
    if let Some(list) = cache.get(key) {
        return list.clone();
    }
    let list = Arc::new(ComboList::from_items(build()));
    cache.insert(key.to_string(), list.clone());
    list
}

/// How many glyphs a dropdown label may take up (a CJK glyph counts double).
/// Hachimi renders combos at a fixed width, so long names have to be trimmed
/// instead of overflowing the widget.
const LABEL_BUDGET: usize = 20;

/// Trim a dropdown label to the widget width. The id stays visible - it also
/// keeps the entry searchable by id - and the name is cut to whatever room is
/// left.
fn fit_label(id: i32, name: &str, budget: usize) -> String {
    let id_text = id.to_string();
    if name.is_empty() {
        return id_text;
    }

    let mut room = budget.saturating_sub(id_text.chars().count() + 1);
    let mut kept = String::new();
    let mut truncated = false;
    for ch in name.chars() {
        let width = if ch.is_ascii() { 1 } else { 2 };
        if width > room {
            truncated = true;
            break;
        }
        room -= width;
        kept.push(ch);
    }

    if truncated {
        // The ellipsis is a wide glyph, so free up room for it first.
        while room < 2 {
            match kept.pop() {
                Some(last) => room += if last.is_ascii() { 1 } else { 2 },
                None => break,
            }
        }
        kept.push('…');
    }

    format!("{} {}", id_text, kept)
}

/// Everything the player can pick, with `0` meaning "no replacement"/default.
fn chara_list() -> Arc<ComboList> {
    cached("chara", || {
        let mut items = vec![(0, "默认".to_string())];
        for character in db::characters() {
            items.push((
                character.id,
                fit_label(character.id, &character.name, LABEL_BUDGET),
            ));
        }
        items
    })
}

/// Dresses of the selected new character (all dresses when it has none).
fn dress_list(chara_id: i32) -> Arc<ComboList> {
    cached(&format!("dress:{}", chara_id), || {
        let mut items: Vec<(i32, String)> = db::dresses()
            .iter()
            .filter(|dress| {
                chara_id <= 0 || dress.chara_id == 0 || dress.chara_id == chara_id
            })
            .map(|dress| {
                let label = fit_label(dress.id, &dress.name, LABEL_BUDGET);
                (dress.id, label)
            })
            .collect();
        items.sort_by_key(|(id, _)| *id);
        items
    })
}

struct RowCtx {
    index: usize,
    orig_char_id: i32,
    new_char_id: i32,
    new_cloth_id: i32,
    replace_mini: bool,
    remove: bool,
    changed: bool,
}

extern "C" fn draw_row_character(ui: *mut c_void, userdata: *mut c_void) {
    let ctx = unsafe { &mut *(userdata as *mut RowCtx) };
    let list = chara_list();

    api::ui_label(ui, &format!("#{}", ctx.index + 1));
    api::ui_label(ui, "原角色");
    let mut selected = list.index_of(ctx.orig_char_id) as i32;
    if api::ui_combo_menu(
        ui,
        &format!("charreplace_orig_{}", ctx.index),
        &mut selected,
        list.pointers(),
    ) {
        if let Some(id) = list.ids.get(selected as usize) {
            ctx.orig_char_id = *id;
            ctx.changed = true;
        }
    }

    api::ui_label(ui, "→ 新角色");
    let mut selected = list.index_of(ctx.new_char_id) as i32;
    if api::ui_combo_menu(
        ui,
        &format!("charreplace_new_{}", ctx.index),
        &mut selected,
        list.pointers(),
    ) {
        if let Some(id) = list.ids.get(selected as usize) {
            ctx.new_char_id = *id;
            ctx.changed = true;
        }
    }
}

extern "C" fn draw_row_dress(ui: *mut c_void, userdata: *mut c_void) {
    let ctx = unsafe { &mut *(userdata as *mut RowCtx) };

    api::ui_label(ui, "服装");

    let base = dress_list(ctx.new_char_id);
    // A value already stored in the config stays selectable, even when it does
    // not belong to the selected new character.
    let combined;
    let list: &ComboList = if ctx.new_cloth_id != 0 && !base.contains(ctx.new_cloth_id) {
        let mut items = vec![(ctx.new_cloth_id, format!("{}", ctx.new_cloth_id))];
        items.extend(base.entries());
        combined = ComboList::from_items(items);
        &combined
    } else {
        &base
    };

    let mut selected = list.index_of(ctx.new_cloth_id) as i32;
    if api::ui_combo_menu(
        ui,
        &format!("charreplace_cloth_{}", ctx.index),
        &mut selected,
        list.pointers(),
    ) {
        if let Some(id) = list.ids.get(selected as usize) {
            ctx.new_cloth_id = *id;
            ctx.changed = true;
        }
    }

    draw_row_tail(ui, ctx);
}

fn draw_row_tail(ui: *mut c_void, ctx: &mut RowCtx) {
    let mut mini = ctx.replace_mini;
    if api::ui_checkbox(ui, "替换迷你角色", &mut mini) {
        ctx.replace_mini = mini;
        ctx.changed = true;
    }
    if api::ui_button(ui, "删除配置") {
        ctx.remove = true;
    }
}

/// The section callback Hachimi calls to draw our page.
pub extern "C" fn section_callback(ui: *mut c_void, _userdata: *mut c_void) {
    if api::api().is_none() {
        return;
    }

    api::ui_heading(ui, "角色替换");
    api::ui_small(
        ui,
        "把游戏里的角色替换成别的角色（配置来自 Trainers' Legend G 的 replaceGlobalChar）。",
    );

    if !db::loaded() {
        api::ui_separator(ui);
        api::ui_label(ui, "正在读取角色数据…");
        return;
    }

    api::ui_separator(ui);

    let mut enable = config::with(|config| config.enable);
    if api::ui_checkbox(ui, "启用角色替换", &mut enable) {
        config::with_mut(|config| config.enable = enable);
        config::save();
    }

    let mut universal = config::with(|config| config.replace_universal);
    if api::ui_checkbox(ui, "同时替换服装", &mut universal) {
        config::with_mut(|config| config.replace_universal = universal);
        config::save();
    }

    let mut voice = config::with(|config| config.replace_voice);
    if api::ui_checkbox(ui, "同时替换语音", &mut voice) {
        config::with_mut(|config| config.replace_voice = voice);
        config::save();
    }

    let mut log_cues = config::with(|config| config.log_audio_cues);
    if api::ui_checkbox(ui, "记录音频 cue（诊断用）", &mut log_cues) {
        config::with_mut(|config| config.log_audio_cues = log_cues);
        config::save();
    }

    api::ui_separator(ui);

    let count = config::with(|config| config.data.len());
    if count == 0 {
        api::ui_small(ui, "还没有规则，点下面的按钮添加。");
    }

    let mut remove_index: Option<usize> = None;

    for index in 0..count {
        let (orig, new_char, cloth, mini) = config::with(|config| {
            let entry = &config.data[index];
            (
                entry.orig_char_id,
                entry.new_char_id,
                entry.new_cloth_id,
                entry.replace_mini,
            )
        });

        let mut ctx = RowCtx {
            index,
            orig_char_id: orig,
            new_char_id: new_char,
            new_cloth_id: cloth,
            replace_mini: mini,
            remove: false,
            changed: false,
        };

        let ptr = &mut ctx as *mut RowCtx as *mut c_void;
        api::ui_horizontal(ui, draw_row_character, ptr);
        api::ui_horizontal(ui, draw_row_dress, ptr);

        if ctx.changed {
            let RowCtx {
                orig_char_id,
                new_char_id,
                new_cloth_id,
                replace_mini,
                ..
            } = ctx;
            config::with_mut(|config| {
                if let Some(entry) = config.data.get_mut(index) {
                    entry.orig_char_id = orig_char_id;
                    entry.new_char_id = new_char_id;
                    entry.new_cloth_id = new_cloth_id;
                    entry.replace_mini = replace_mini;
                }
            });
            config::save();
        }

        if ctx.remove {
            remove_index = Some(index);
        }
    }

    if let Some(index) = remove_index {
        config::with_mut(|config| {
            if index < config.data.len() {
                config.data.remove(index);
            }
        });
        config::save();
    }

    if api::ui_button(ui, "＋ 添加规则") {
        config::with_mut(|config| {
            config.data.push(config::Entry {
                orig_char_id: 0,
                new_char_id: 0,
                new_cloth_id: 0,
                replace_mini: false,
            });
        });
        config::save();
    }
}
