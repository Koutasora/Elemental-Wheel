//! Single running editor (named mutex) and bringing the running one to the front.

use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HWND, LPARAM, TRUE,
};
use windows::Win32::System::Threading::{
    CreateMutexW, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible, SW_RESTORE,
    SetForegroundWindow, ShowWindow,
};
use windows::core::{BOOL, PWSTR, Result, w};

/// Takes the editor mutex. `Ok(false)` when another editor already holds it. The handle is
/// never closed: the mutex lives until the process exits.
pub fn acquire() -> Result<bool> {
    // SAFETY: plain Win32 calls; `GetLastError` is read right after `CreateMutexW`.
    unsafe {
        let mutex = CreateMutexW(None, false, w!(r"Local\ElementalWheelEditor"))?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(mutex);
            return Ok(false);
        }
    }
    Ok(true)
}

/// Whether the title is the one of an editor window (with or without the "unsaved" mark).
fn is_editor_title(title: &str, base: &str) -> bool {
    title == base || title.strip_prefix(base) == Some(" *")
}

/// File name of the executable of the process owning `hwnd`.
fn process_exe_name(hwnd: HWND) -> Option<String> {
    let mut pid = 0u32;
    // SAFETY: `pid` outlives the call; the process handle is closed before returning.
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
        .is_ok();
        let _ = CloseHandle(process);
        ok.then(|| {
            let path = String::from_utf16_lossy(&buf[..len as usize]);
            Path::new(&path).file_name()?.to_str().map(str::to_string)
        })?
    }
}

struct Search<'a> {
    title: &'a str,
    exe: &'a str,
    own_pid: u32,
    found: Option<HWND>,
}

unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: `lparam` is the `Search` passed by `find_window`, alive during `EnumWindows`.
    let search = unsafe { &mut *(lparam.0 as *mut Search) };
    let mut buf = [0u16; 256];
    // SAFETY: plain calls on a window handle given by EnumWindows.
    let (len, visible, pid) = unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        (
            GetWindowTextW(hwnd, &mut buf) as usize,
            IsWindowVisible(hwnd).as_bool(),
            pid,
        )
    };
    let title = String::from_utf16_lossy(&buf[..len]);
    if visible
        && pid != search.own_pid
        && is_editor_title(&title, search.title)
        && process_exe_name(hwnd).is_some_and(|n| n.eq_ignore_ascii_case(search.exe))
    {
        search.found = Some(hwnd);
        return BOOL(0);
    }
    TRUE
}

fn find_window(title: &str, exe: &str) -> Option<HWND> {
    let mut search = Search {
        title,
        exe,
        own_pid: std::process::id(),
        found: None,
    };
    // SAFETY: the callback only uses `search`, which outlives the call.
    unsafe {
        let _ = EnumWindows(Some(visit), LPARAM(&mut search as *mut Search as isize));
    }
    search.found
}

/// Restores and raises the window of the running editor. The window may not exist yet when the
/// other editor has only just started, so the search is repeated for up to 2 s.
pub fn show_running(title: &str, exe: &str) {
    for _ in 0..20 {
        if let Some(hwnd) = find_window(title, exe) {
            // SAFETY: the handle was just found; both calls tolerate a window that is gone.
            unsafe {
                if IsIconic(hwnd).as_bool() {
                    let _ = ShowWindow(hwnd, SW_RESTORE);
                }
                let _ = SetForegroundWindow(hwnd);
            }
            return;
        }
        sleep(Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_matches_with_and_without_the_unsaved_mark() {
        assert!(is_editor_title(
            "Elemental Wheel – edytor",
            "Elemental Wheel – edytor"
        ));
        assert!(is_editor_title(
            "Elemental Wheel – edytor *",
            "Elemental Wheel – edytor"
        ));
        assert!(!is_editor_title(
            "Elemental Wheel – edytor.txt",
            "Elemental Wheel – edytor"
        ));
        assert!(!is_editor_title(
            "Elemental Wheel",
            "Elemental Wheel – edytor"
        ));
    }
}
