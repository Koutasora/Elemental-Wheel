//! Executes the action of the chosen sector.
//!
//! Windows does not let injected input reach windows of programs run as administrator (UIPI).
//! The wheel does not open over such windows, so the actions never have to.
//!
//! Everything runs on a short-lived thread of its own: `ShellExecuteExW` and `CreateProcessW`
//! can block, and neither the hook thread nor the UI thread may wait.

use std::thread;
use std::time::Duration;

use wheel_core::{Action, Shortcut, SystemAction, is_extended, parse_shortcut};
use windows::Win32::Foundation::{CloseHandle, GlobalFree, HANDLE, HGLOBAL, HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    SetClipboardData,
};
use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::Win32::System::Shutdown::LockWorkStation;
use windows::Win32::System::Threading::{
    CREATE_NO_WINDOW, CreateProcessW, PROCESS_INFORMATION, STARTUPINFOW,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, SendInput,
};
use windows::Win32::UI::Shell::{
    SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, ShellExecuteExW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GA_ROOT, GetAncestor, GetForegroundWindow, IsWindowVisible, PostMessageW, SC_MINIMIZE,
    SW_SHOWNORMAL, SetForegroundWindow, WM_CLOSE, WM_SYSCOMMAND,
};
use windows::core::{PCWSTR, PWSTR, w};

use crate::hook::MARKER;

/// How long the clipboard keeps the pasted text before the previous text comes back.
const CLIPBOARD_RESTORE_MS: u64 = 500;

/// Runs `action` against `target` (the top-level window under the cursor when the right button
/// went down, as `HWND` bits). It starts once the wheel window `wheel` is hidden.
pub fn run(action: &Action, target: isize, wheel: isize) {
    if matches!(action, Action::None) {
        return;
    }
    let action = action.clone();
    let spawned = thread::Builder::new().name("action".into()).spawn(move || {
        wait_hidden(HWND(wheel as *mut _));
        if let Err(e) = execute(&action, HWND(target as *mut _)) {
            eprintln!("action failed: {e}");
        }
    });
    if let Err(e) = spawned {
        eprintln!("action failed: cannot start the thread: {e}");
    }
}

fn wait_hidden(wheel: HWND) {
    for _ in 0..50 {
        // SAFETY: plain query; a destroyed window just reads as not visible.
        if !unsafe { IsWindowVisible(wheel) }.as_bool() {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn execute(action: &Action, target: HWND) -> Result<(), String> {
    match action {
        Action::None => Ok(()),
        Action::Program {
            path,
            args,
            working_dir,
        } => shell_execute(
            &expand(path),
            &expand(args),
            working_dir.as_deref().map(expand).as_deref(),
        ),
        Action::Open { path } => shell_execute(&expand(path), "", None),
        Action::Url { url } => shell_execute(url, "", None),
        Action::Shortcut { keys } => {
            focus(target);
            send_keys(&parse_shortcut(keys)?)
        }
        Action::Edit { command } => {
            focus(target);
            send_keys(&parse_shortcut(command.shortcut())?)
        }
        Action::PasteText { text } => {
            focus(target);
            paste_text(text)
        }
        Action::Command { line } => command(line),
        Action::System { action } => system(*action, target),
    }
}

fn system(action: SystemAction, target: HWND) -> Result<(), String> {
    let keys = |text: &str| parse_shortcut(text).and_then(|s| send_keys(&s));
    match action {
        SystemAction::Lock => {
            // SAFETY: plain call.
            unsafe { LockWorkStation() }.map_err(|e| format!("lock: {e}"))
        }
        SystemAction::CloseWindow => {
            focus(target);
            post(target, WM_CLOSE, 0)
        }
        SystemAction::MinimizeWindow => {
            focus(target);
            post(target, WM_SYSCOMMAND, SC_MINIMIZE as usize)
        }
        SystemAction::ShowDesktop => keys("Win+D"),
        SystemAction::VolumeMute => keys("VolumeMute"),
        SystemAction::VolumeUp => keys("VolumeUp"),
        SystemAction::VolumeDown => keys("VolumeDown"),
        SystemAction::Screenshot => keys("Win+Shift+S"),
    }
}

fn post(target: HWND, msg: u32, wparam: usize) -> Result<(), String> {
    if target.0.is_null() {
        return Err("no window under the cursor".to_string());
    }
    // SAFETY: plain call; a window that is gone makes it fail, which is reported.
    unsafe { PostMessageW(Some(target), msg, WPARAM(wparam), LPARAM(0)) }
        .map_err(|e| format!("cannot post to the window: {e}"))
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// Expands `%VARIABLE%` references; the text itself if that fails.
fn expand(text: &str) -> String {
    let src = wide(text);
    // SAFETY: `src` is NUL-terminated; the buffer is sized by the first call.
    unsafe {
        let needed = ExpandEnvironmentStringsW(PCWSTR(src.as_ptr()), None) as usize;
        if needed == 0 {
            return text.to_string();
        }
        let mut buf = vec![0u16; needed];
        let written = ExpandEnvironmentStringsW(PCWSTR(src.as_ptr()), Some(&mut buf)) as usize;
        if written == 0 || written > buf.len() {
            return text.to_string();
        }
        String::from_utf16_lossy(&buf[..written - 1])
    }
}

fn shell_execute(file: &str, args: &str, dir: Option<&str>) -> Result<(), String> {
    let file_w = wide(file);
    let args_w = wide(args);
    let dir_w = dir.map(wide);
    let or_null = |text: &str, w: &[u16]| {
        if text.is_empty() {
            PCWSTR::null()
        } else {
            PCWSTR(w.as_ptr())
        }
    };
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_FLAG_NO_UI | SEE_MASK_NOASYNC,
        lpVerb: w!("open"),
        lpFile: PCWSTR(file_w.as_ptr()),
        lpParameters: or_null(args, &args_w),
        lpDirectory: dir_w
            .as_ref()
            .map_or(PCWSTR::null(), |d| PCWSTR(d.as_ptr())),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    // SAFETY: `info` and the strings it points to live through the call; COM is initialised
    // for this thread and released afterwards.
    unsafe {
        let com = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let result = ShellExecuteExW(&mut info);
        if com {
            CoUninitialize();
        }
        result.map_err(|e| format!("cannot open \"{file}\": {e}"))
    }
}

fn command(line: &str) -> Result<(), String> {
    let mut cmdline = wide(&format!("cmd.exe /c {line}"));
    let startup = STARTUPINFOW {
        cb: size_of::<STARTUPINFOW>() as u32,
        ..Default::default()
    };
    let mut info = PROCESS_INFORMATION::default();
    // SAFETY: `cmdline` is a writable NUL-terminated buffer; the handles in `info` are closed.
    unsafe {
        CreateProcessW(
            PCWSTR::null(),
            Some(PWSTR(cmdline.as_mut_ptr())),
            None,
            None,
            false,
            CREATE_NO_WINDOW,
            None,
            PCWSTR::null(),
            &startup,
            &mut info,
        )
        .map_err(|e| format!("cannot run \"{line}\": {e}"))?;
        let _ = CloseHandle(info.hProcess);
        let _ = CloseHandle(info.hThread);
    }
    Ok(())
}

/// One key event of a shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Key {
    vk: u16,
    up: bool,
}

/// Modifiers down, key down and up, modifiers up in reverse order.
fn key_sequence(shortcut: &Shortcut) -> Vec<Key> {
    let down = |vk| Key { vk, up: false };
    let up = |vk| Key { vk, up: true };
    let mut keys: Vec<Key> = shortcut.modifiers.iter().copied().map(down).collect();
    keys.push(down(shortcut.key));
    keys.push(up(shortcut.key));
    keys.extend(shortcut.modifiers.iter().rev().copied().map(up));
    keys
}

fn key_input(key: Key) -> INPUT {
    let mut flags = if key.up {
        KEYEVENTF_KEYUP
    } else {
        Default::default()
    };
    if is_extended(key.vk) {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(key.vk),
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: MARKER,
            },
        },
    }
}

fn send_keys(shortcut: &Shortcut) -> Result<(), String> {
    send(&key_sequence(shortcut))
}

fn send(keys: &[Key]) -> Result<(), String> {
    let inputs: Vec<INPUT> = keys.iter().copied().map(key_input).collect();
    // SAFETY: `inputs` is a valid slice of fully initialised INPUT structures.
    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
    if sent as usize == inputs.len() {
        Ok(())
    } else {
        Err(format!(
            "SendInput injected {sent} of {} events",
            inputs.len()
        ))
    }
}

/// Brings `target` to the foreground if it is not there. Windows may refuse; a tap of Alt
/// (marked as ours) lets the second try through.
fn focus(target: HWND) {
    if target.0.is_null() {
        return;
    }
    // SAFETY: plain window calls.
    unsafe {
        if GetAncestor(GetForegroundWindow(), GA_ROOT) == target {
            return;
        }
        if !SetForegroundWindow(target).as_bool() {
            let alt = 0x12;
            let _ = send(&[Key { vk: alt, up: false }, Key { vk: alt, up: true }]);
            let _ = SetForegroundWindow(target);
        }
    }
    // Let the focus settle before the keys arrive.
    thread::sleep(Duration::from_millis(50));
}

/// Closes the clipboard when dropped.
struct Clipboard;

impl Clipboard {
    fn open() -> Result<Clipboard, String> {
        for _ in 0..10 {
            // SAFETY: plain call; the clipboard is closed by `Drop`.
            if unsafe { OpenClipboard(None) }.is_ok() {
                return Ok(Clipboard);
            }
            thread::sleep(Duration::from_millis(20));
        }
        Err("the clipboard is in use".to_string())
    }
}

impl Drop for Clipboard {
    fn drop(&mut self) {
        // SAFETY: the clipboard was opened by this thread.
        let _ = unsafe { CloseClipboard() };
    }
}

/// The clipboard text (UTF-16, without the terminator), if there is any.
fn read_text(_open: &Clipboard) -> Option<Vec<u16>> {
    // SAFETY: the clipboard is open; the global memory is locked while it is read and the
    // text ends at its NUL terminator.
    unsafe {
        IsClipboardFormatAvailable(u32::from(CF_UNICODETEXT.0)).ok()?;
        let handle = GetClipboardData(u32::from(CF_UNICODETEXT.0)).ok()?;
        let global = HGLOBAL(handle.0);
        let ptr = GlobalLock(global).cast::<u16>();
        if ptr.is_null() {
            return None;
        }
        let mut len = 0;
        while *ptr.add(len) != 0 {
            len += 1;
        }
        let text = std::slice::from_raw_parts(ptr, len).to_vec();
        let _ = GlobalUnlock(global);
        Some(text)
    }
}

fn write_text(_open: &Clipboard, text: &[u16]) -> Result<(), String> {
    // SAFETY: the clipboard is open; the block is filled while locked and handed to the
    // clipboard, which then owns it (freed here only if that fails).
    unsafe {
        EmptyClipboard().map_err(|e| format!("clipboard: {e}"))?;
        let global = GlobalAlloc(GMEM_MOVEABLE, (text.len() + 1) * 2)
            .map_err(|e| format!("clipboard: {e}"))?;
        let ptr = GlobalLock(global).cast::<u16>();
        if ptr.is_null() {
            let _ = GlobalFree(Some(global));
            return Err("clipboard: cannot lock memory".to_string());
        }
        std::ptr::copy_nonoverlapping(text.as_ptr(), ptr, text.len());
        *ptr.add(text.len()) = 0;
        let _ = GlobalUnlock(global);
        if let Err(e) = SetClipboardData(u32::from(CF_UNICODETEXT.0), Some(HANDLE(global.0))) {
            let _ = GlobalFree(Some(global));
            return Err(format!("clipboard: {e}"));
        }
    }
    Ok(())
}

/// Puts `text` on the clipboard, pastes it with Ctrl+V and brings the previous clipboard text
/// back (only if the clipboard still holds our text; non-text content is not restored).
fn paste_text(text: &str) -> Result<(), String> {
    let wide_text: Vec<u16> = text.encode_utf16().collect();
    let previous = {
        let clipboard = Clipboard::open()?;
        let previous = read_text(&clipboard);
        write_text(&clipboard, &wide_text)?;
        previous
    };
    send_keys(&parse_shortcut("Ctrl+V")?)?;
    thread::sleep(Duration::from_millis(CLIPBOARD_RESTORE_MS));
    if let Some(previous) = previous {
        let clipboard = Clipboard::open()?;
        if read_text(&clipboard).as_deref() == Some(&wide_text[..]) {
            write_text(&clipboard, &previous)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(text: &str) -> Vec<(u16, bool)> {
        key_sequence(&parse_shortcut(text).unwrap())
            .into_iter()
            .map(|k| (k.vk, k.up))
            .collect()
    }

    #[test]
    fn modifiers_wrap_the_key() {
        assert_eq!(
            keys("Ctrl+Shift+V"),
            [
                (0x11, false),
                (0x10, false),
                (b'V'.into(), false),
                (b'V'.into(), true),
                (0x10, true),
                (0x11, true),
            ]
        );
        assert_eq!(keys("F2"), [(0x71, false), (0x71, true)]);
    }

    #[test]
    fn input_is_marked_and_extended_keys_flagged() {
        let down = |text: &str| {
            let k = key_sequence(&parse_shortcut(text).unwrap())[0];
            let input = key_input(k);
            // SAFETY: `key_input` always fills the keyboard member.
            unsafe { input.Anonymous.ki }
        };
        assert_eq!(down("Delete").dwExtraInfo, MARKER);
        assert_eq!(down("Delete").dwFlags, KEYEVENTF_EXTENDEDKEY);
        assert_eq!(down("A").dwFlags.0, 0);
    }

    #[test]
    fn expands_environment_variables() {
        let root = expand("%SystemRoot%\\notepad.exe");
        assert!(
            !root.contains('%') && root.ends_with("notepad.exe"),
            "{root}"
        );
        assert_eq!(expand("plain text"), "plain text");
    }
}
