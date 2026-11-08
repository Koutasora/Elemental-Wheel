//! Notification area icon of the `--run` process: a menu with the editor, on/off and quit.
//! The icon sends its events to the background window (`WM_TRAY`).

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetSystemMetricsForDpi};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION, NIN_SELECT,
    NOTIFY_ICON_DATA_FLAGS, NOTIFYICON_VERSION_4, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, DestroyWindow, HICON, IDI_APPLICATION, IMAGE_ICON,
    LR_SHARED, LoadImageW, MB_ICONERROR, MB_OK, MF_SEPARATOR, MF_STRING, MessageBoxW, PostMessageW,
    RegisterWindowMessageW, SM_CXSMICON, SM_CYSMICON, SetForegroundWindow, TPM_BOTTOMALIGN,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, WM_APP, WM_CONTEXTMENU, WM_NULL,
};
use windows::core::{PCWSTR, w};

use crate::hook;

/// Sent by the icon to the background window: the low word of `lparam` is the event.
pub const WM_TRAY: u32 = WM_APP + 7;

const ICON_ID: u32 = 1;
/// Resource number of the program icon in `wheel.exe`.
const ICON_RESOURCE: usize = 1;
/// `NIN_KEYSELECT`: the icon was chosen with the keyboard (Enter).
const NIN_KEYSELECT: u32 = NIN_SELECT | 1;
const EDITOR_EXE: &str = "wheel-editor.exe";

const CMD_EDITOR: usize = 1;
const CMD_TOGGLE: usize = 2;
const CMD_QUIT: usize = 3;
const CMD_OPTIONS: usize = 4;

/// The message Explorer broadcasts after it restarted (0 = not registered yet).
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
/// `--config` of this process, passed on to the editor.
static CUSTOM_CONFIG: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Id of the "Explorer restarted" message; 0 before `add`.
pub fn taskbar_created() -> u32 {
    TASKBAR_CREATED.load(Ordering::Relaxed)
}

fn tip_text(enabled: bool) -> &'static str {
    if enabled {
        "Elemental Wheel"
    } else {
        "Elemental Wheel (wyłączone)"
    }
}

fn toggle_label(enabled: bool) -> &'static str {
    if enabled { "Wyłącz" } else { "Włącz" }
}

/// The editor lies next to the running executable; `--config` is passed only when this process
/// does not use the default configuration, `--tab` only when a tab is asked for.
fn editor_command(
    exe: &Path,
    config: Option<&Path>,
    tab: Option<&str>,
) -> (PathBuf, Vec<OsString>) {
    let mut args = Vec::new();
    if let Some(tab) = tab {
        args.extend([OsString::from("--tab"), OsString::from(tab)]);
    }
    if let Some(path) = config {
        args.extend([OsString::from("--config"), path.as_os_str().to_owned()]);
    }
    (exe.with_file_name(EDITOR_EXE), args)
}

/// Adds the icon (once per process; `custom_config` is the `--config` path, if any).
pub fn add(hwnd: HWND, custom_config: Option<PathBuf>) {
    let _ = CUSTOM_CONFIG.set(custom_config);
    // SAFETY: plain call with a constant string.
    let message = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
    TASKBAR_CREATED.store(message, Ordering::Relaxed);
    add_icon(hwnd);
}

/// Adds the icon again after Explorer restarted.
pub fn readd(hwnd: HWND) {
    add_icon(hwnd);
}

fn add_icon(hwnd: HWND) {
    let icon = load_icon();
    let mut data = icon_data(hwnd, NIF_ICON | NIF_TIP | NIF_MESSAGE);
    data.uCallbackMessage = WM_TRAY;
    data.hIcon = icon;
    set_tip(&mut data, hook::is_enabled());
    // SAFETY: `data` is fully initialised.
    unsafe {
        if Shell_NotifyIconW(NIM_ADD, &data).as_bool() {
            data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
            if !Shell_NotifyIconW(NIM_SETVERSION, &data).as_bool() {
                wheel_core::log_line!("tray: cannot select the icon protocol version 4");
            }
        } else {
            wheel_core::log_line!("tray: cannot add the icon");
        }
    }
}

/// Removes the icon.
pub fn remove(hwnd: HWND) {
    let data = icon_data(hwnd, NOTIFY_ICON_DATA_FLAGS(0));
    // SAFETY: `data` is fully initialised.
    let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &data) };
}

fn update_tip(hwnd: HWND) {
    let mut data = icon_data(hwnd, NIF_TIP);
    set_tip(&mut data, hook::is_enabled());
    // SAFETY: `data` is fully initialised.
    let _ = unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) };
}

fn icon_data(hwnd: HWND, flags: NOTIFY_ICON_DATA_FLAGS) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: ICON_ID,
        uFlags: flags,
        ..Default::default()
    }
}

fn set_tip(data: &mut NOTIFYICONDATAW, enabled: bool) {
    data.szTip = [0; 128];
    // The last element stays zero: the terminator.
    for (slot, unit) in data
        .szTip
        .iter_mut()
        .take(127)
        .zip(tip_text(enabled).encode_utf16())
    {
        *slot = unit;
    }
}

/// The icon resource 1 of this executable, the stock application icon when there is none.
/// The icons are shared (`LR_SHARED`): the system owns them, nothing to destroy.
fn load_icon() -> HICON {
    // SAFETY: plain calls; the resource id is passed as a MAKEINTRESOURCE value.
    unsafe {
        let dpi = GetDpiForSystem();
        let (cx, cy) = (
            GetSystemMetricsForDpi(SM_CXSMICON, dpi),
            GetSystemMetricsForDpi(SM_CYSMICON, dpi),
        );
        let load = |module: Option<HINSTANCE>, name: PCWSTR| {
            LoadImageW(module, name, IMAGE_ICON, cx, cy, LR_SHARED).map(|h| HICON(h.0))
        };
        if let Ok(module) = GetModuleHandleW(None).map(HINSTANCE::from)
            && let Ok(icon) = load(Some(module), PCWSTR(ICON_RESOURCE as *const u16))
        {
            return icon;
        }
        load(None, IDI_APPLICATION).unwrap_or_default()
    }
}

/// Handles `WM_TRAY` (called from the window procedure of `hwnd`).
pub fn on_message(hwnd: HWND, wparam: WPARAM, lparam: LPARAM) {
    let event = (lparam.0 & 0xFFFF) as u32;
    if matches!(event, WM_CONTEXTMENU | NIN_SELECT | NIN_KEYSELECT) {
        // Version 4: the anchor point is packed in `wparam` (signed 16-bit halves).
        let at = POINT {
            x: i32::from((wparam.0 & 0xFFFF) as u16 as i16),
            y: i32::from(((wparam.0 >> 16) & 0xFFFF) as u16 as i16),
        };
        show_menu(hwnd, at);
    }
}

fn show_menu(hwnd: HWND, at: POINT) {
    let toggle = wide(toggle_label(hook::is_enabled()));
    // SAFETY: the menu is created and destroyed here; the strings outlive the calls.
    let command = unsafe {
        let Ok(menu) = CreatePopupMenu() else {
            return;
        };
        let _ = AppendMenuW(menu, MF_STRING, CMD_EDITOR, w!("Edytor"));
        let _ = AppendMenuW(menu, MF_STRING, CMD_OPTIONS, w!("Opcje"));
        let _ = AppendMenuW(menu, MF_STRING, CMD_TOGGLE, PCWSTR(toggle.as_ptr()));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(menu, MF_STRING, CMD_QUIT, w!("Zamknij"));
        // The menu closes on a click elsewhere only if its owner is the foreground window.
        let _ = SetForegroundWindow(hwnd);
        let chosen = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
            at.x,
            at.y,
            None,
            hwnd,
            None,
        );
        // Documented requirement: without it the menu may not close on the next click.
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        chosen.0 as usize
    };
    match command {
        CMD_EDITOR => open_editor(None),
        CMD_OPTIONS => open_editor(Some("opcje")),
        CMD_TOGGLE => {
            let now = !hook::is_enabled();
            hook::set_enabled(now);
            wheel_core::log_line!("wheel {}", if now { "enabled" } else { "disabled" });
            update_tip(hwnd);
        }
        CMD_QUIT => {
            // SAFETY: `hwnd` is the window whose procedure is running.
            let _ = unsafe { DestroyWindow(hwnd) };
        }
        _ => {}
    }
}

/// Starts the editor (`tab` is its `--tab` value); on failure logs and tells the user.
pub fn open_editor(tab: Option<&str>) {
    let started = std::env::current_exe()
        .map_err(|e| format!("cannot find the program path: {e}"))
        .and_then(|exe| {
            let config = CUSTOM_CONFIG.get().and_then(Option::as_deref);
            let (path, args) = editor_command(&exe, config, tab);
            Command::new(&path)
                .args(args)
                .spawn()
                .map(drop)
                .map_err(|e| format!("cannot start {}: {e}", path.display()))
        });
    if let Err(e) = started {
        wheel_core::log_line!("tray: {e}");
        // SAFETY: constant strings.
        unsafe {
            MessageBoxW(
                None,
                w!("Nie można uruchomić edytora."),
                w!("Elemental Wheel"),
                MB_OK | MB_ICONERROR,
            );
        }
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tip_and_label_follow_the_state() {
        assert_eq!(tip_text(true), "Elemental Wheel");
        assert_eq!(tip_text(false), "Elemental Wheel (wyłączone)");
        assert_eq!(toggle_label(true), "Wyłącz");
        assert_eq!(toggle_label(false), "Włącz");
    }

    #[test]
    fn tips_fit_the_tooltip_buffer() {
        assert!(tip_text(false).encode_utf16().count() < 128);
    }

    #[test]
    fn editor_lies_next_to_the_exe_without_arguments_for_the_default_config() {
        let (path, args) = editor_command(Path::new(r"C:\Apps\Wheel\wheel.exe"), None, None);
        assert_eq!(path, PathBuf::from(r"C:\Apps\Wheel\wheel-editor.exe"));
        assert!(args.is_empty());
    }

    #[test]
    fn editor_gets_the_custom_config() {
        let config = Path::new(r"D:\cfg\my config.json");
        let (_, args) = editor_command(Path::new(r"C:\Apps\Wheel\wheel.exe"), Some(config), None);
        assert_eq!(
            args,
            vec![
                OsString::from("--config"),
                OsString::from(r"D:\cfg\my config.json")
            ]
        );
    }

    #[test]
    fn editor_gets_the_tab_before_the_config() {
        let config = Path::new(r"D:\cfg\a.json");
        let (_, args) = editor_command(
            Path::new(r"C:\Apps\Wheel\wheel.exe"),
            Some(config),
            Some("opcje"),
        );
        assert_eq!(
            args,
            vec![
                OsString::from("--tab"),
                OsString::from("opcje"),
                OsString::from("--config"),
                OsString::from(r"D:\cfg\a.json")
            ]
        );
    }
}
