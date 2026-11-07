//! Watches the folder of the config file and tells the UI window when something in it changes,
//! so that a changed file reaches the program without waiting for the wheel to open.

use std::path::Path;
use std::thread::{self, JoinHandle};

use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, WAIT_OBJECT_0, WPARAM};
use windows::Win32::Storage::FileSystem::{
    FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE, FindCloseChangeNotification,
    FindFirstChangeNotificationW, FindNextChangeNotification,
};
use windows::Win32::System::Threading::{CreateEventW, INFINITE, SetEvent, WaitForMultipleObjects};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;
use windows::core::PCWSTR;

/// The running watcher thread.
pub struct Watcher {
    /// Manual-reset event (as `HANDLE` bits) that ends the thread.
    stop: isize,
    thread: JoinHandle<()>,
}

impl Watcher {
    /// Watches `dir`; every change posts `msg` to `window`. `None` (with a note on stderr) if
    /// the folder cannot be watched, for example because it does not exist.
    pub fn start(dir: &Path, window: HWND, msg: u32) -> Option<Watcher> {
        let dir_w: Vec<u16> = dir
            .as_os_str()
            .to_string_lossy()
            .encode_utf16()
            .chain(Some(0))
            .collect();
        // SAFETY: `dir_w` is NUL-terminated and outlives the call.
        let change = match unsafe {
            FindFirstChangeNotificationW(
                PCWSTR(dir_w.as_ptr()),
                false,
                FILE_NOTIFY_CHANGE_LAST_WRITE | FILE_NOTIFY_CHANGE_FILE_NAME,
            )
        } {
            Ok(handle) => handle,
            Err(e) => {
                eprintln!(
                    "config: cannot watch {} ({e}); changes are picked up when the wheel opens",
                    dir.display()
                );
                return None;
            }
        };
        // SAFETY: plain call; the event is closed in `stop`.
        let stop = match unsafe { CreateEventW(None, true, false, None) } {
            Ok(handle) => handle,
            Err(e) => {
                eprintln!("config: cannot create the stop event ({e}); not watching");
                // SAFETY: the handle was just created.
                let _ = unsafe { FindCloseChangeNotification(change) };
                return None;
            }
        };
        let (change_bits, stop_bits, window_bits) =
            (change.0 as isize, stop.0 as isize, window.0 as isize);
        let thread = thread::spawn(move || {
            watch(
                HANDLE(change_bits as *mut _),
                HANDLE(stop_bits as *mut _),
                HWND(window_bits as *mut _),
                msg,
            )
        });
        Some(Watcher {
            stop: stop_bits,
            thread,
        })
    }

    /// Ends the thread and waits for it.
    pub fn stop(self) {
        let stop = HANDLE(self.stop as *mut _);
        // SAFETY: `stop` is the event created in `start`; it is closed after the thread ended.
        unsafe {
            let _ = SetEvent(stop);
        }
        let _ = self.thread.join();
        // SAFETY: see above.
        unsafe {
            let _ = CloseHandle(stop);
        }
    }
}

fn watch(change: HANDLE, stop: HANDLE, window: HWND, msg: u32) {
    loop {
        // SAFETY: both handles are valid until this function returns / `Watcher::stop` joins.
        let waited = unsafe { WaitForMultipleObjects(&[change, stop], false, INFINITE) };
        if waited != WAIT_OBJECT_0 {
            break;
        }
        // SAFETY: plain calls; the window may be gone while quitting, which is harmless.
        unsafe {
            let _ = PostMessageW(Some(window), msg, WPARAM(0), LPARAM(0));
            if FindNextChangeNotification(change).is_err() {
                break;
            }
        }
    }
    // SAFETY: `change` came from FindFirstChangeNotificationW and is closed only here.
    let _ = unsafe { FindCloseChangeNotification(change) };
}
