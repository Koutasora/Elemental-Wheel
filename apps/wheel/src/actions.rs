//! Executes the action of the chosen sector.
//!
//! Windows does not let injected input reach windows of programs run as administrator (UIPI).
//! The wheel does not open over such windows, so the actions never have to.
//!
//! Everything runs on a short-lived thread of its own: `ShellExecuteExW` and `CreateProcessW`
//! can block, and neither the hook thread nor the UI thread may wait.

use std::thread;
use std::time::Duration;

use wheel_core::{Action, EditCommand, Shortcut, SystemAction, is_extended, parse_shortcut};
use windows::Win32::Foundation::{CloseHandle, GlobalFree, HANDLE, HGLOBAL, HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    SetClipboardData,
};
use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
use windows::Win32::System::Memory::{
    GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::Win32::System::Shutdown::LockWorkStation;
use windows::Win32::System::Threading::{
    CREATE_NO_WINDOW, CreateProcessW, PROCESS_INFORMATION, STARTUPINFOW,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
    MAPVK_VK_TO_VSC, MapVirtualKeyW, SendInput, VIRTUAL_KEY,
};
use windows::Win32::UI::Shell::{
    SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, ShellExecuteExW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GA_ROOT, GetAncestor, GetForegroundWindow, IsWindowVisible, PostMessageW, SC_MINIMIZE,
    SW_SHOWNORMAL, SetForegroundWindow, WM_CLOSE, WM_SYSCOMMAND,
};
use windows::core::{PCWSTR, PWSTR, w};

use crate::args::PanicTest;
use crate::guard;
use crate::hook::{MARKER, is_shell_class, window_class};
use crate::properties::{self, Route};

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
        let done = guard::try_run("action thread", "thread ended", || {
            guard::fire(PanicTest::Action);
            wait_hidden(HWND(wheel as *mut _));
            if let Err(e) = execute(&action, HWND(target as *mut _), HWND(wheel as *mut _)) {
                wheel_core::log_line!("action failed: {e}");
            }
        });
        if done.is_none() {
            wheel_core::log_line!("action panicked: {}", kind(&action));
        }
    });
    if let Err(e) = spawned {
        wheel_core::log_line!("action failed: cannot start the thread: {e}");
    }
}

/// Name of the action type for the log (no payload: it may hold text the user typed).
fn kind(action: &Action) -> &'static str {
    match action {
        Action::None => "none",
        Action::Program { .. } => "program",
        Action::Open { .. } => "open",
        Action::Url { .. } => "url",
        Action::Shortcut { .. } => "shortcut",
        Action::Edit { .. } => "edit",
        Action::PasteText { .. } => "paste_text",
        Action::Command { .. } => "command",
        Action::System { .. } => "system",
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

fn execute(action: &Action, target: HWND, wheel: HWND) -> Result<(), String> {
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
            if *command == EditCommand::Properties && properties_by_shell(target) {
                return Ok(());
            }
            focus(target);
            send_keys(&parse_shortcut(command.shortcut())?)
        }
        Action::PasteText { text } => {
            focus(target);
            paste_text(text, wheel)
        }
        Action::Command { line } => command(line),
        Action::System { action } => system(*action, target),
    }
}

/// Properties of Explorer's selection or of the desktop's, asked of the shell (see
/// `properties.rs`). False when the window is of another kind or the shell could not be asked;
/// the caller then sends the shortcut.
fn properties_by_shell(target: HWND) -> bool {
    if target.0.is_null() {
        return false;
    }
    let class = window_class(target);
    if properties::route_for(&class) != Route::Shell {
        return false;
    }
    let desktop = is_shell_class(&class);
    // SAFETY: COM is released below, on this thread, only if this call initialised it.
    let com = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
    let shown = properties::show(target, desktop);
    if com {
        // SAFETY: matches the successful initialisation above.
        unsafe { CoUninitialize() };
    }
    match shown {
        Ok(_) => true,
        Err(e) => {
            wheel_core::log_line!("{e}; sending the shortcut instead");
            false
        }
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
            if is_shell_window(target) {
                return Ok(());
            }
            focus(target);
            post(target, WM_CLOSE, 0)
        }
        SystemAction::MinimizeWindow => {
            if is_shell_window(target) {
                return Ok(());
            }
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

/// The desktop and the taskbars: closing or minimizing them would be wrong, so those do nothing.
fn is_shell_window(target: HWND) -> bool {
    !target.0.is_null() && is_shell_class(&window_class(target))
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

/// `%ComSpec%`, else the cmd.exe in the Windows System32 folder.
fn cmd_exe() -> String {
    match std::env::var("ComSpec") {
        Ok(path) if !path.trim().is_empty() => path,
        _ => expand(r"%SystemRoot%\System32\cmd.exe"),
    }
}

fn command(line: &str) -> Result<(), String> {
    let exe = cmd_exe();
    let exe_w = wide(&exe);
    let mut cmdline = wide(&format!("\"{exe}\" /c {line}"));
    let startup = STARTUPINFOW {
        cb: size_of::<STARTUPINFOW>() as u32,
        ..Default::default()
    };
    let mut info = PROCESS_INFORMATION::default();
    // SAFETY: `cmdline` is a writable NUL-terminated buffer; the handles in `info` are closed.
    unsafe {
        CreateProcessW(
            PCWSTR(exe_w.as_ptr()),
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

/// Hardware scan code of a virtual key (0 when the key has none). Sent along with the virtual
/// key code, because some programs read the scan code of the message.
fn scan_code(vk: u16) -> u16 {
    // SAFETY: plain call.
    unsafe { MapVirtualKeyW(u32::from(vk), MAPVK_VK_TO_VSC) as u16 }
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
                wVk: VIRTUAL_KEY(key.vk),
                wScan: scan_code(key.vk),
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
/// (marked as ours) lets the second try through. Risk: a lone Alt press can activate the menu
/// bar of the foreground window; it is only sent when the first try failed.
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
    fn open(owner: HWND) -> Result<Clipboard, String> {
        for _ in 0..10 {
            // SAFETY: plain call; the clipboard is closed by `Drop`.
            if unsafe { OpenClipboard(Some(owner)) }.is_ok() {
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
    // SAFETY: the clipboard is open; the global memory is locked while it is read, and the text
    // ends at its NUL terminator or at the end of the block, whichever comes first.
    unsafe {
        IsClipboardFormatAvailable(u32::from(CF_UNICODETEXT.0)).ok()?;
        let handle = GetClipboardData(u32::from(CF_UNICODETEXT.0)).ok()?;
        let global = HGLOBAL(handle.0);
        let ptr = GlobalLock(global).cast::<u16>();
        if ptr.is_null() {
            return None;
        }
        let max = GlobalSize(global) / 2;
        let mut len = 0;
        while len < max && *ptr.add(len) != 0 {
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
        // Only now is the old content dropped, so a failed allocation loses nothing.
        if let Err(e) = EmptyClipboard() {
            let _ = GlobalFree(Some(global));
            return Err(format!("clipboard: {e}"));
        }
        if let Err(e) = SetClipboardData(u32::from(CF_UNICODETEXT.0), Some(HANDLE(global.0))) {
            let _ = GlobalFree(Some(global));
            return Err(format!("clipboard: {e}"));
        }
    }
    Ok(())
}

/// Puts `text` on the clipboard, pastes it with Ctrl+V and brings the previous clipboard text
/// back (only if the clipboard still holds our text; non-text content is not restored).
fn paste_text(text: &str, owner: HWND) -> Result<(), String> {
    let wide_text: Vec<u16> = text.encode_utf16().collect();
    let previous = {
        let clipboard = Clipboard::open(owner)?;
        let previous = read_text(&clipboard);
        write_text(&clipboard, &wide_text)?;
        previous
    };
    send_keys(&parse_shortcut("Ctrl+V")?)?;
    thread::sleep(Duration::from_millis(CLIPBOARD_RESTORE_MS));
    if let Some(previous) = previous {
        let clipboard = Clipboard::open(owner)?;
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
    fn input_carries_scan_codes_next_to_the_virtual_keys() {
        let down = |text: &str| {
            let k = key_sequence(&parse_shortcut(text).unwrap());
            // SAFETY: `key_input` always fills the keyboard member.
            k.into_iter()
                .map(|k| unsafe { key_input(k).Anonymous.ki })
                .collect::<Vec<_>>()
        };
        let enter = down("Alt+Enter");
        assert_eq!(
            enter.iter().map(|k| (k.wVk.0, k.wScan)).collect::<Vec<_>>(),
            [(0x12, 0x38), (0x0D, 0x1C), (0x0D, 0x1C), (0x12, 0x38)]
        );
        assert_eq!(down("Ctrl+C")[1].wScan, 0x2E);
        // Windows keys are extended and still get a scan code.
        assert_ne!(down("Win+D")[0].wScan, 0);
    }

    #[test]
    fn cmd_is_a_full_path() {
        let exe = cmd_exe().to_lowercase();
        assert!(exe.ends_with("cmd.exe") && exe.contains('\\'), "{exe}");
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

    /// `cargo test action_handles -- --ignored --nocapture`: kernel handles and memory of the
    /// process after many actions (each runs on a thread of its own with COM and the shell).
    #[test]
    #[ignore = "starts programs; prints handle counts"]
    fn action_handles() {
        use windows::Win32::System::ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX,
        };
        use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessHandleCount};
        let handles = || {
            let mut n = 0u32;
            let mut c = PROCESS_MEMORY_COUNTERS_EX {
                cb: size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
                ..Default::default()
            };
            // SAFETY: valid pseudo handle, out pointers and matching `cb`.
            unsafe {
                GetProcessHandleCount(GetCurrentProcess(), &mut n).unwrap();
                GetProcessMemoryInfo(
                    GetCurrentProcess(),
                    (&mut c as *mut PROCESS_MEMORY_COUNTERS_EX).cast(),
                    c.cb,
                )
                .unwrap();
            }
            format!(
                "{n} handles, {:.1} MB private",
                c.PrivateUsage as f64 / 1048576.0
            )
        };
        let run_all = |action: &Action, times: u32| {
            for _ in 0..times {
                run(action, 0, 0);
                thread::sleep(Duration::from_millis(150));
            }
            thread::sleep(Duration::from_millis(1000));
        };
        println!("start: {}", handles());
        let command = Action::Command {
            line: "exit".to_string(),
        };
        run_all(&command, 20);
        println!("after 20 commands: {}", handles());
        let program = Action::Program {
            path: "cmd.exe".to_string(),
            args: "/c exit".to_string(),
            working_dir: None,
        };
        run_all(&program, 1);
        println!("after 1 program: {}", handles());
        run_all(&program, 20);
        println!("after 21 programs: {}", handles());
    }
}
