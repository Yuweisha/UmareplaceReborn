
use std::path::PathBuf;

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateDialogParamW, DispatchMessageW, EndDialog, GetDlgItem, GetMessageW, GetSystemMetrics,
    GetWindowRect, LoadIconW, MessageBoxW, PostQuitMessage, SendMessageW, SetWindowPos,
    SetWindowTextW, ShowWindow, TranslateMessage, ICON_BIG, ICON_SMALL, MB_ICONERROR,
    MB_ICONINFORMATION, MB_OK, MSG, SM_CXSCREEN, SM_CYSCREEN, SWP_NOSIZE, SWP_NOZORDER, SW_SHOW,
    WM_CLOSE, WM_COMMAND, WM_DESTROY, WM_INITDIALOG, WM_SETICON,
};

use crate::find;
use crate::version;
use crate::{install_to, uninstall_from};

const IDD_MAIN: u16 = 129;
const IDI_ICON: u16 = 107;

const IDC_INSTALL: u16 = 1000;
const IDC_UNINSTALL: u16 = 1001;
const IDC_PACKAGED_VER: u16 = 1002;
const IDC_PATH_EDIT: u16 = 1003;
const IDC_BROWSE: u16 = 1004;
const IDC_INSTALLED: u16 = 1006;
const IDC_LOCATION_LABEL: u16 = 1009;

const PACKAGED_VERSION: &str = env!("PACKAGED_VERSION");

const DLL_NAME: &str = "umareplacereborn.dll";

struct State {
    install_dir: Option<PathBuf>,
}

static mut STATE: Option<State> = None;

pub fn run() -> windows::core::Result<()> {
    let detected = find::detect();
    unsafe { STATE = Some(State { install_dir: detected }) };

    let instance = unsafe { GetModuleHandleW(None)? };
    let dialog = unsafe {
        CreateDialogParamW(
            instance,
            PCWSTR::from_raw(IDD_MAIN as usize as *const u16),
            None,
            Some(dlg_proc),
            LPARAM(0),
        )
    }?;

    unsafe {
        center(dialog);
        let _ = ShowWindow(dialog, SW_SHOW);
    }

    let mut message = MSG::default();
    unsafe {
        while GetMessageW(&mut message, None, 0, 0).into() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }

    Ok(())
}

unsafe extern "system" fn dlg_proc(hwnd: HWND, msg: u32, wparam: WPARAM, _lparam: LPARAM) -> isize {
    match msg {
        WM_INITDIALOG => {
            on_init(hwnd);
            1
        }
        WM_COMMAND => {
            on_command(hwnd, wparam);
            1
        }
        WM_CLOSE => {
            unsafe {
                let _ = EndDialog(hwnd, 0);
            }
            1
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            1
        }
        _ => 0,
    }
}

#[allow(static_mut_refs)]
unsafe fn state(_hwnd: HWND) -> &'static mut State {
    unsafe { STATE.as_mut().expect("界面状态还没初始化") }
}

fn on_init(hwnd: HWND) {
    unsafe {
        if let Ok(instance) = GetModuleHandleW(None) {
            if let Ok(icon) = LoadIconW(instance, PCWSTR::from_raw(IDI_ICON as usize as *const u16)) {
                SendMessageW(
                    hwnd,
                    WM_SETICON,
                    WPARAM(ICON_BIG as usize),
                    LPARAM(icon.0 as isize),
                );
                SendMessageW(
                    hwnd,
                    WM_SETICON,
                    WPARAM(ICON_SMALL as usize),
                    LPARAM(icon.0 as isize),
                );
            }
        }
        let _ = SetWindowTextW(hwnd, &HSTRING::from("UmareplaceReborn安装器"));

        set_text(
            hwnd,
            IDC_PACKAGED_VER,
            &format!("打包版本: {PACKAGED_VERSION}"),
        );
        set_text(hwnd, IDC_LOCATION_LABEL, "安装位置:");
        set_text(hwnd, IDC_BROWSE, "浏览...");
        set_text(hwnd, IDC_INSTALL, "安装");
        set_text(hwnd, IDC_UNINSTALL, "卸载");

        refresh_path(hwnd);
        refresh_installed(hwnd);
    }
}

fn on_command(hwnd: HWND, wparam: WPARAM) {
    let id = (wparam.0 & 0xffff) as u16;

    match id {
        IDC_INSTALL => do_install(hwnd),
        IDC_UNINSTALL => do_uninstall(hwnd),
        IDC_BROWSE => {
            if let Some(dir) = crate::ui::pick_folder() {
                unsafe { state(hwnd).install_dir = Some(dir) };
                unsafe {
                    refresh_path(hwnd);
                    refresh_installed(hwnd);
                }
            }
        }
        _ => {}
    }
}

unsafe fn refresh_path(hwnd: HWND) {
    let dir = unsafe { state(hwnd).install_dir.clone() };
    let text = dir.map(|d| d.display().to_string()).unwrap_or_default();
    unsafe { set_text(hwnd, IDC_PATH_EDIT, &text) };
}

unsafe fn refresh_installed(hwnd: HWND) {
    let dir = unsafe { state(hwnd).install_dir.clone() };
    let text = match dir {
        Some(dir) => {
            let dll = find::install_dir_for(&dir).join(DLL_NAME);
            if dll.is_file() {
                match version::product_version(&dll) {
                    Some(v) => format!("已安装: {v}"),
                    None => "已安装: 版本未知".to_string(),
                }
            } else {
                "已安装: 无".to_string()
            }
        }
        None => "已安装: 无".to_string(),
    };
    unsafe { set_text(hwnd, IDC_INSTALLED, &text) };
}

fn do_install(hwnd: HWND) {
    let dir = match unsafe { state(hwnd).install_dir.clone() } {
        Some(d) => d,
        None => {
            message(hwnd, "请先选择游戏安装目录。", MB_ICONERROR);
            return;
        }
    };

    if !find::hachimi_installed(&dir) {
        message(
            hwnd,
            &format!(
                "这个目录里没有检测到 Hachimi 本体：\n{}\n\n插件是挂在 Hachimi 上的，请先装好 Hachimi。",
                dir.display()
            ),
            MB_ICONERROR,
        );
        return;
    }

    match install_to(&dir) {
        Ok(msg) => message(hwnd, &msg, MB_ICONINFORMATION),
        Err(err) => message(hwnd, &err, MB_ICONERROR),
    }
    unsafe { refresh_installed(hwnd) };
}

fn do_uninstall(hwnd: HWND) {
    let dir = match unsafe { state(hwnd).install_dir.clone() } {
        Some(d) => d,
        None => {
            message(hwnd, "请先选择游戏安装目录。", MB_ICONERROR);
            return;
        }
    };

    if !find::install_dir_for(&dir).join(DLL_NAME).is_file() {
        message(hwnd, "这个目录里没有装插件。", MB_ICONINFORMATION);
        return;
    }

    match uninstall_from(&dir) {
        Ok(msg) => message(hwnd, &msg, MB_ICONINFORMATION),
        Err(err) => message(hwnd, &err, MB_ICONERROR),
    }
    unsafe { refresh_installed(hwnd) };
}

unsafe fn set_text(hwnd: HWND, id: u16, text: &str) {
    if let Ok(control) = unsafe { GetDlgItem(hwnd, id as i32) } {
        let _ = unsafe { SetWindowTextW(control, &HSTRING::from(text)) };
    }
}

fn message(hwnd: HWND, text: &str, icon: windows::Win32::UI::WindowsAndMessaging::MESSAGEBOX_STYLE) {
    unsafe {
        MessageBoxW(
            hwnd,
            &HSTRING::from(text),
            &HSTRING::from("UmareplaceReborn安装器"),
            MB_OK | icon,
        );
    }
}

unsafe fn center(hwnd: HWND) {
    let mut rect = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut rect) }.is_err() {
        return;
    }
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    let x = (unsafe { GetSystemMetrics(SM_CXSCREEN) } - width) / 2;
    let y = (unsafe { GetSystemMetrics(SM_CYSCREEN) } - height) / 2;
    let _ = unsafe { SetWindowPos(hwnd, None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER) };
}
