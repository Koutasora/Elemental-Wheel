//! Selects the item under the pointer in Explorer and on the desktop when the right button goes
//! down, as Windows does for a plain right click. The wheel swallows that press, so without this
//! the actions (Copy, Delete, Properties, ...) would work on the previous selection.
//!
//! The item is found and selected through UI Automation on a worker thread of its own (reaching
//! Explorer takes milliseconds), so neither the hook thread nor the opening of the wheel waits.
//! An action started from the wheel calls [`wait`] first, so it never sees the old selection.
//! An item that is already selected is left alone (a multi-selection survives); empty space and
//! errors change nothing.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Condvar, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationElementArray,
    IUIAutomationSelectionItemPattern, TreeScope_Descendants, UIA_BoundingRectanglePropertyId,
    UIA_ControlTypePropertyId, UIA_ListItemControlTypeId, UIA_SelectionItemPatternId,
};

use crate::guard;
use crate::hook::window_class;
use crate::properties::{Route, route_for};
use crate::stats;

/// The longest wait of an action for the selection to finish.
const WAIT_MAX: Duration = Duration::from_millis(500);
/// How many levels above the element under the pointer are searched for the list item.
const MAX_DEPTH: usize = 6;

/// What a selection request did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The item was not selected; now it is the only selected one.
    Selected,
    /// The item was selected already; nothing changed.
    AlreadySelected,
    /// No list item under the pointer.
    NoItem,
}

type Job = (isize, POINT);

static SENDER: OnceLock<Sender<Job>> = OnceLock::new();
/// Requests queued or running.
static PENDING: Mutex<u32> = Mutex::new(0);
static FINISHED: Condvar = Condvar::new();

/// Explorer windows and the desktop (the windows whose Properties go through the shell).
fn should_select(class: &str) -> bool {
    route_for(class) == Route::Shell
}

/// Windows replaces the selection only of an item that is not selected.
fn needs_select(selected: bool) -> bool {
    !selected
}

fn lock_pending() -> std::sync::MutexGuard<'static, u32> {
    PENDING.lock().unwrap_or_else(|e| e.into_inner())
}

fn finish_one() {
    let mut pending = lock_pending();
    *pending = pending.saturating_sub(1);
    FINISHED.notify_all();
}

/// Starts the worker thread. Called once with the hook, so the first request does not pay for
/// creating UI Automation.
pub fn start() {
    let (tx, rx) = channel();
    if SENDER.set(tx).is_err() {
        return;
    }
    if let Err(e) = thread::Builder::new()
        .name("selection".into())
        .spawn(move || worker(rx))
    {
        wheel_core::log_line!("selection: cannot start the thread: {e}");
    }
}

/// Queues the selection of the item at `at` in `target` (top-level window under the pointer),
/// if that is Explorer or the desktop. Cheap enough for the hook thread.
pub fn begin(target: isize, at: POINT) {
    if target == 0 || !should_select(&window_class(HWND(target as *mut _))) {
        return;
    }
    let Some(tx) = SENDER.get() else {
        return;
    };
    *lock_pending() += 1;
    if tx.send((target, at)).is_err() {
        finish_one();
    }
}

/// Waits (at most 500 ms) until the requested selections are done.
pub fn wait() {
    let pending = lock_pending();
    if *pending == 0 {
        return;
    }
    let (pending, timeout) = FINISHED
        .wait_timeout_while(pending, WAIT_MAX, |n| *n > 0)
        .unwrap_or_else(|e| e.into_inner());
    if timeout.timed_out() && *pending > 0 {
        wheel_core::log_line!("selection: still running after {} ms", WAIT_MAX.as_millis());
    }
}

fn worker(rx: Receiver<Job>) {
    // SAFETY: COM is released below on this thread if this call initialised it.
    let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    let mut uia = create_automation();
    for (target, at) in rx {
        let started = Instant::now();
        let result = guard::try_run("selection thread", "selection skipped", || {
            if uia.is_none() {
                uia = create_automation();
            }
            let uia = uia
                .as_ref()
                .ok_or("UI Automation is not available".to_string())?;
            select_at(uia, HWND(target as *mut _), at).map_err(|e| e.to_string())
        });
        match result {
            Some(Ok(outcome)) if stats::enabled() => wheel_core::log_line!(
                "stats select {outcome:?} ms={:.3}",
                started.elapsed().as_secs_f64() * 1e3
            ),
            Some(Err(e)) => wheel_core::log_line!("selection: {e}"),
            _ => {}
        }
        finish_one();
    }
    if com {
        // SAFETY: matches the successful initialisation above.
        unsafe { CoUninitialize() };
    }
}

fn create_automation() -> Option<IUIAutomation> {
    // SAFETY: COM is initialised on this thread.
    unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }
        .map_err(|e| wheel_core::log_line!("selection: cannot start UI Automation: {e}"))
        .ok()
}

/// Selects the list item at `at` in the window `target`, unless it is selected already.
///
/// # Errors
/// UI Automation failed to answer.
pub(crate) fn select_at(
    uia: &IUIAutomation,
    target: HWND,
    at: POINT,
) -> windows::core::Result<Outcome> {
    // SAFETY: COM is initialised on the calling thread; plain UI Automation calls.
    unsafe {
        let hit = uia.ElementFromPoint(at)?;
        // The wheel window can already cover the point: look the item up in the target instead.
        let item = if hit.CurrentProcessId()? as u32 == std::process::id() {
            item_in_window(uia, target, at)?
        } else {
            item_above(uia, hit, target)
        };
        let Some(item) = item else {
            return Ok(Outcome::NoItem);
        };
        if !needs_select(item.CurrentIsSelected()?.as_bool()) {
            return Ok(Outcome::AlreadySelected);
        }
        item.Select()?;
        Ok(Outcome::Selected)
    }
}

/// The selection pattern of the list item `element` or its nearest ancestor below `target`.
///
/// # Safety
/// COM is initialised on the calling thread.
unsafe fn item_above(
    uia: &IUIAutomation,
    element: IUIAutomationElement,
    target: HWND,
) -> Option<IUIAutomationSelectionItemPattern> {
    // SAFETY: guaranteed by the caller.
    unsafe {
        let walker = uia.ControlViewWalker().ok()?;
        let mut element = element;
        for _ in 0..MAX_DEPTH {
            if element
                .CurrentNativeWindowHandle()
                .is_ok_and(|h| h == target)
            {
                return None;
            }
            if let Some(pattern) = list_item_pattern(&element) {
                return Some(pattern);
            }
            element = walker.GetParentElement(&element).ok()?;
        }
        None
    }
}

/// # Safety
/// COM is initialised on the calling thread.
unsafe fn list_item_pattern(
    element: &IUIAutomationElement,
) -> Option<IUIAutomationSelectionItemPattern> {
    // SAFETY: guaranteed by the caller.
    unsafe {
        if element.CurrentControlType().ok()? != UIA_ListItemControlTypeId {
            return None;
        }
        element
            .GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(UIA_SelectionItemPatternId)
            .ok()
    }
}

/// All list items of the window `target`, with their bounding rectangles cached.
///
/// # Safety
/// COM is initialised on the calling thread.
pub(crate) unsafe fn list_items(
    uia: &IUIAutomation,
    target: HWND,
) -> windows::core::Result<IUIAutomationElementArray> {
    // SAFETY: guaranteed by the caller.
    unsafe {
        let condition = uia.CreatePropertyCondition(
            UIA_ControlTypePropertyId,
            &VARIANT::from(UIA_ListItemControlTypeId.0),
        )?;
        let cache = uia.CreateCacheRequest()?;
        cache.AddProperty(UIA_BoundingRectanglePropertyId)?;
        uia.ElementFromHandle(target)?
            .FindAllBuildCache(TreeScope_Descendants, &condition, &cache)
    }
}

fn contains(rect: &RECT, at: POINT) -> bool {
    (rect.left..rect.right).contains(&at.x) && (rect.top..rect.bottom).contains(&at.y)
}

/// # Safety
/// COM is initialised on the calling thread.
unsafe fn item_in_window(
    uia: &IUIAutomation,
    target: HWND,
    at: POINT,
) -> windows::core::Result<Option<IUIAutomationSelectionItemPattern>> {
    // SAFETY: guaranteed by the caller.
    unsafe {
        let items = list_items(uia, target)?;
        for i in 0..items.Length()? {
            let item = items.GetElement(i)?;
            if item
                .CachedBoundingRectangle()
                .is_ok_and(|rect| contains(&rect, at))
            {
                return Ok(list_item_pattern(&item));
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explorer_and_desktop_windows_are_selected() {
        for class in ["CabinetWClass", "ExploreWClass", "Progman", "WorkerW"] {
            assert!(should_select(class), "{class}");
        }
    }

    #[test]
    fn other_windows_are_left_alone() {
        for class in ["Notepad", "Shell_TrayWnd", "Chrome_WidgetWin_1", ""] {
            assert!(!should_select(class), "{class}");
        }
    }

    #[test]
    fn only_an_unselected_item_is_selected() {
        assert!(needs_select(false));
        assert!(!needs_select(true));
    }

    #[test]
    fn a_point_is_inside_the_left_top_edges_and_outside_the_right_bottom_ones() {
        let rect = RECT {
            left: 10,
            top: 20,
            right: 30,
            bottom: 40,
        };
        assert!(contains(&rect, POINT { x: 10, y: 20 }));
        assert!(contains(&rect, POINT { x: 29, y: 39 }));
        assert!(!contains(&rect, POINT { x: 30, y: 30 }));
        assert!(!contains(&rect, POINT { x: 20, y: 40 }));
    }

    /// `cargo test -p wheel -- --ignored --nocapture explorer_selection`: opens an Explorer window
    /// on a folder with three files (under `target/test-tmp`), selects items by their UI Automation
    /// position, reads the selection back through the shell and closes the window. No mouse
    /// or keyboard input is sent.
    #[test]
    #[ignore = "opens an Explorer window on the desktop"]
    fn explorer_selection() {
        use crate::properties::selection;
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        use windows::Win32::UI::WindowsAndMessaging::{
            EnumWindows, GetWindowTextW, PostMessageW, WM_CLOSE,
        };
        use windows::core::BOOL;

        const FOLDER_NAME: &str = "wheel-sel-test";
        unsafe extern "system" fn find(hwnd: HWND, found: LPARAM) -> BOOL {
            let mut title = [0u16; 128];
            // SAFETY: `found` is the `Vec<isize>` passed to `EnumWindows` below.
            unsafe {
                let len = GetWindowTextW(hwnd, &mut title).max(0) as usize;
                if window_class(hwnd) == "CabinetWClass"
                    && String::from_utf16_lossy(&title[..len]).starts_with(FOLDER_NAME)
                {
                    (*(found.0 as *mut Vec<isize>)).push(hwnd.0 as isize);
                }
            }
            true.into()
        }
        let explorers = || {
            let mut found: Vec<isize> = Vec::new();
            // SAFETY: `found` outlives the call.
            let _ = unsafe { EnumWindows(Some(find), LPARAM(&mut found as *mut _ as isize)) };
            found
        };

        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("target")
            .join("test-tmp")
            .join(FOLDER_NAME);
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("folder");
        // Explorer wants a plain path: no `..`, no `\?\` prefix.
        let folder = folder.canonicalize().expect("canonical folder");
        let folder = folder.to_string_lossy();
        let folder = folder.strip_prefix(r"\?\").unwrap_or(&folder).to_owned();
        for name in ["a.txt", "b.txt", "c.txt"] {
            std::fs::write(format!(r"{folder}\{name}"), name).expect("file");
        }
        std::process::Command::new("explorer.exe")
            .arg(&folder)
            .status()
            .expect("explorer");
        let mut window = HWND::default();
        for _ in 0..40 {
            std::thread::sleep(Duration::from_millis(250));
            if let Some(&h) = explorers().first() {
                window = HWND(h as *mut _);
                break;
            }
        }
        assert!(!window.0.is_null(), "no Explorer window on the test folder");
        std::thread::sleep(Duration::from_millis(2000));

        // SAFETY: plain call; the shell is asked from this single-threaded apartment.
        let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        let target = window.0 as isize;
        // UI Automation runs in the multithreaded apartment, like the worker.
        let step = move |index: i32| {
            std::thread::spawn(move || {
                let window = HWND(target as *mut _);
                // SAFETY: COM is released on this thread afterwards.
                unsafe {
                    let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                    let uia: IUIAutomation =
                        CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).unwrap();
                    let items = list_items(&uia, window).unwrap();
                    assert!(items.Length().unwrap() >= 3, "fewer than 3 items");
                    let rect = items
                        .GetElement(index)
                        .unwrap()
                        .CachedBoundingRectangle()
                        .unwrap();
                    let at = POINT {
                        x: (rect.left + rect.right) / 2,
                        y: (rect.top + rect.bottom) / 2,
                    };
                    let started = Instant::now();
                    let outcome = select_at(&uia, window, at).map_err(|e| e.to_string());
                    let ms = started.elapsed().as_secs_f64() * 1e3;
                    CoUninitialize();
                    (outcome, ms)
                }
            })
            .join()
            .unwrap()
        };
        let mut failure = None;
        let mut check = |index: i32, expected: Outcome, only: &str| {
            let (outcome, ms) = step(index);
            let now = selection(window, false).expect("selection");
            println!("item {index}: {outcome:?} in {ms:.1} ms, selection {now:?}");
            if outcome.as_ref().ok() != Some(&expected) || now.len() != 1 || !now[0].ends_with(only)
            {
                failure.get_or_insert(format!("item {index}: {outcome:?}, selection {now:?}"));
            }
        };
        check(0, Outcome::Selected, "a.txt");
        check(1, Outcome::Selected, "b.txt");
        check(1, Outcome::AlreadySelected, "b.txt");

        // SAFETY: plain call on a window found above.
        unsafe {
            let _ = PostMessageW(Some(window), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        std::thread::sleep(Duration::from_millis(500));
        if let Some(failure) = failure {
            panic!("{failure}");
        }
    }
}
