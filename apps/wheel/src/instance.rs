//! Single running instance (named mutex) and stopping it from another process.

use std::thread::sleep;
use std::time::Duration;

use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, LPARAM, WPARAM};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW};
use windows::core::{Result, w};

use crate::window::{RUN_CLASS, WM_WHEEL_QUIT};

/// Takes the instance mutex. `Ok(false)` when another instance already holds it. The handle is
/// never closed: the mutex lives until the process exits.
pub fn acquire() -> Result<bool> {
    // SAFETY: plain Win32 calls; `GetLastError` is read right after `CreateMutexW`.
    unsafe {
        let mutex = CreateMutexW(None, false, w!("Local\\ElementalWheel"))?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(mutex);
            return Ok(false);
        }
    }
    Ok(true)
}

/// Asks the running instance to quit and waits (up to 3 s) until its window is gone.
/// False when no instance is running.
pub fn quit_running() -> bool {
    // SAFETY: the window handle comes from FindWindowW; posting a message needs nothing else.
    let posted = unsafe {
        FindWindowW(RUN_CLASS, None)
            .and_then(|hwnd| PostMessageW(Some(hwnd), WM_WHEEL_QUIT, WPARAM(0), LPARAM(0)))
            .is_ok()
    };
    if posted {
        for _ in 0..60 {
            // SAFETY: as above.
            if unsafe { FindWindowW(RUN_CLASS, None) }.is_err() {
                break;
            }
            sleep(Duration::from_millis(50));
        }
    }
    posted
}
