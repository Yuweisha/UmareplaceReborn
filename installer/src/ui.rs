
use std::path::PathBuf;

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::{
    FileOpenDialog, IFileOpenDialog, FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
};
use windows::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, MB_ICONERROR, MB_ICONINFORMATION, MB_ICONWARNING, MB_OK, MESSAGEBOX_RESULT,
    MESSAGEBOX_STYLE,
};

const TITLE: &str = "角色替换插件";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn msgbox(text: &str, flags: MESSAGEBOX_STYLE) -> MESSAGEBOX_RESULT {
    let text = wide(text);
    let title = wide(TITLE);
    unsafe {
        MessageBoxW(
            HWND(std::ptr::null_mut()),
            PCWSTR(text.as_ptr()),
            PCWSTR(title.as_ptr()),
            flags,
        )
    }
}

pub fn info(text: &str) {
    msgbox(text, MB_OK | MB_ICONINFORMATION);
}

pub fn error(text: &str) {
    msgbox(text, MB_OK | MB_ICONERROR);
}

pub fn warn(text: &str) {
    msgbox(text, MB_OK | MB_ICONWARNING);
}

pub fn pick_folder() -> Option<PathBuf> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let dialog: IFileOpenDialog =
            match CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER) {
                Ok(d) => d,
                Err(_) => {
                    CoUninitialize();
                    return None;
                }
            };

        let result = (|| -> Option<PathBuf> {
            let options = dialog.GetOptions().ok()?;
            dialog.SetOptions(options | FOS_PICKFOLDERS).ok()?;
            dialog.Show(None).ok()?;
            let item = dialog.GetResult().ok()?;
            let raw: PWSTR = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
            let path = raw.to_string().ok().map(PathBuf::from);
            CoTaskMemFree(Some(raw.0 as *const _));
            path
        })();

        CoUninitialize();
        result
    }
}
