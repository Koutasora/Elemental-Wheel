//! "Properties" for File Explorer and the desktop.
//!
//! Alt+Enter injected into Explorer does not reliably open the properties window, so for Explorer
//! windows and the desktop the selection is read through the shell (`IShellWindows`, the view of
//! the window, `IFolderView2`) and the properties window is requested directly:
//! - one item, or the folder shown when nothing is selected: `SHObjectProperties`, which needs
//!   only a path or parsing name and shows the window on a thread of its own;
//! - several items: `SHMultiFileProperties` with a data object built here from the parsing names.
//!
//! The window belongs to this process (the shell creates it on a thread of its own, which pumps its
//! messages), so it stays open while the wheel runs and never blocks the action thread.
//! `SHMultiFileProperties` has to be called from the multithreaded apartment: from a
//! single-threaded one the window appears only if that thread pumps messages, and the action
//! thread ends right away. Reading Explorer's view, on the other hand, works only from a
//! single-threaded apartment. Only plain strings pass between the two: objects from Explorer
//! (proxies bound to the action thread) would die with the thread while the window still needs
//! them.
//!
//! Windows that are not Explorer or the desktop get Alt+Enter (see `actions.rs`).

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    CoUninitialize, IBindCtx, IDataObject, IServiceProvider,
};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    BHID_DataObject, IFolderView2, IShellBrowser, IShellView, IShellWindows,
    SHCreateShellItemArrayFromIDLists, SHMultiFileProperties, SHOP_FILEPATH, SHObjectProperties,
    SHParseDisplayName, SID_STopLevelBrowser, SIGDN_DESKTOPABSOLUTEPARSING, SVGIO_SELECTION,
    SWC_DESKTOP, SWFO_NEEDDISPATCH, ShellWindows,
};
use windows::Win32::UI::WindowsAndMessaging::{GA_ROOT, GetAncestor, IsWindowVisible};
use windows::core::{Interface, PCWSTR, PWSTR};

/// How the Properties command reaches a window of this class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// Through the shell: the window shows file system items.
    Shell,
    /// As the keyboard shortcut.
    Keys,
}

pub fn route_for(class: &str) -> Route {
    match class {
        "CabinetWClass" | "ExploreWClass" | "Progman" | "WorkerW" => Route::Shell,
        _ => Route::Keys,
    }
}

/// Shows the properties window of the selection of the Explorer window or desktop `target`, or
/// of the folder shown when nothing is selected (not on the desktop). `Ok(false)` when there is
/// nothing to show properties of.
pub fn show(target: HWND, desktop: bool) -> Result<bool, String> {
    let names = selection(target, desktop).map_err(|e| format!("properties: {e}"))?;
    if names.is_empty() {
        return Ok(false);
    }
    open(&names)?;
    Ok(true)
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

fn open(names: &[String]) -> Result<(), String> {
    if let [name] = names {
        let name = wide(name);
        // SAFETY: `name` is NUL-terminated and lives through the call.
        let shown = unsafe {
            SHObjectProperties(None, SHOP_FILEPATH, PCWSTR(name.as_ptr()), PCWSTR::null())
        };
        if shown.as_bool() {
            return Ok(());
        }
    }
    on_mta_thread(|| multi(names))
}

/// Runs `f` on a thread of its own in the multithreaded apartment and waits for it. The
/// properties window outlives the thread (see the module comment).
fn on_mta_thread<T: Send>(f: impl FnOnce() -> Result<T, String> + Send) -> Result<T, String> {
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                // SAFETY: COM is released on this thread afterwards if this call initialised it.
                let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
                let result = f();
                if com {
                    // SAFETY: matches the successful initialisation above.
                    unsafe { CoUninitialize() };
                }
                result
            })
            .join()
            .unwrap_or_else(|_| Err("properties: the helper thread panicked".to_string()))
    })
}

/// Properties of several items (also the fallback for a single one `SHObjectProperties` refuses,
/// such as the Recycle Bin).
fn multi(names: &[String]) -> Result<(), String> {
    let mut pidls: Vec<*mut ITEMIDLIST> = Vec::new();
    let result = (|| {
        for name in names {
            let name = wide(name);
            let mut pidl = std::ptr::null_mut();
            // SAFETY: `name` is NUL-terminated; `pidl` is a valid out pointer.
            unsafe { SHParseDisplayName(PCWSTR(name.as_ptr()), None, &mut pidl, 0, None) }
                .map_err(|e| format!("properties: cannot resolve an item: {e}"))?;
            pidls.push(pidl);
        }
        let list: Vec<*const ITEMIDLIST> = pidls.iter().map(|p| p.cast_const()).collect();
        // SAFETY: the absolute item lists stay valid through the calls; the data object copies them.
        unsafe {
            let data: IDataObject = SHCreateShellItemArrayFromIDLists(&list)
                .and_then(|items| items.BindToHandler(None::<&IBindCtx>, &BHID_DataObject))
                .map_err(|e| format!("properties: cannot build the data object: {e}"))?;
            SHMultiFileProperties(&data, 0)
                .map_err(|e| format!("properties: cannot show the window: {e}"))
        }
    })();
    for pidl in pidls {
        // SAFETY: allocated by `SHParseDisplayName`.
        unsafe { CoTaskMemFree(Some(pidl.cast_const().cast())) };
    }
    result
}

/// Parsing names of the selected items; without a selection those of the folder shown (on the
/// desktop: none).
pub(crate) fn selection(target: HWND, desktop: bool) -> windows::core::Result<Vec<String>> {
    // SAFETY: COM is initialised on the calling thread; the views are used only here.
    unsafe {
        let folder_view: IFolderView2 = find_view(target, desktop)?.cast()?;
        if desktop && folder_view.ItemCount(SVGIO_SELECTION)? == 0 {
            return Ok(Vec::new());
        }
        let selected = folder_view.GetSelection(!desktop)?;
        (0..selected.GetCount()?)
            .map(|i| {
                let name = selected
                    .GetItemAt(i)?
                    .GetDisplayName(SIGDN_DESKTOPABSOLUTEPARSING)?;
                Ok(parsing_name(&name))
            })
            .collect()
    }
}

/// The text of a shell-allocated string, which is freed.
///
/// # Safety
/// `name` came from the shell and is not used afterwards.
unsafe fn parsing_name(name: &PWSTR) -> String {
    // SAFETY: guaranteed by the caller.
    unsafe {
        let text = name.to_string().unwrap_or_default();
        CoTaskMemFree(Some(name.0.cast_const().cast()));
        text
    }
}

/// The shell view of the Explorer window `target`, or of the desktop. With tabs, several shell
/// windows share one top-level window; the one whose view is visible is the active tab.
///
/// # Safety
/// COM is initialised on the calling thread.
unsafe fn find_view(target: HWND, desktop: bool) -> windows::core::Result<IShellView> {
    // SAFETY: guaranteed by the caller; plain COM calls.
    unsafe {
        let windows: IShellWindows = CoCreateInstance(&ShellWindows, None, CLSCTX_ALL)?;
        if desktop {
            let (location, root) = (VARIANT::from(0i32), VARIANT::default());
            let mut hwnd = 0;
            let dispatch = windows.FindWindowSW(
                &location,
                &root,
                SWC_DESKTOP,
                &mut hwnd,
                SWFO_NEEDDISPATCH,
            )?;
            return browser_of(&dispatch)?.QueryActiveShellView();
        }
        let mut first = None;
        for i in 0..windows.Count()? {
            let Ok(dispatch) = windows.Item(&VARIANT::from(i)) else {
                continue;
            };
            let Ok(browser) = browser_of(&dispatch) else {
                continue;
            };
            if !browser
                .GetWindow()
                .is_ok_and(|w| GetAncestor(w, GA_ROOT) == target)
            {
                continue;
            }
            let Ok(view) = browser.QueryActiveShellView() else {
                continue;
            };
            if view.GetWindow().is_ok_and(|w| IsWindowVisible(w).as_bool()) {
                return Ok(view);
            }
            first.get_or_insert(view);
        }
        first.ok_or_else(|| windows::Win32::Foundation::E_FAIL.into())
    }
}

/// # Safety
/// COM is initialised on the calling thread.
unsafe fn browser_of(
    dispatch: &windows::Win32::System::Com::IDispatch,
) -> windows::core::Result<IShellBrowser> {
    // SAFETY: guaranteed by the caller.
    unsafe {
        let provider: IServiceProvider = dispatch.cast()?;
        provider.QueryService::<IShellBrowser>(&SID_STopLevelBrowser)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hook::window_class;

    #[test]
    fn explorer_and_desktop_go_through_the_shell() {
        for class in ["CabinetWClass", "ExploreWClass", "Progman", "WorkerW"] {
            assert_eq!(route_for(class), Route::Shell, "{class}");
        }
    }

    #[test]
    fn other_windows_get_the_shortcut() {
        for class in ["Notepad", "Shell_TrayWnd", "Chrome_WidgetWin_1", ""] {
            assert_eq!(route_for(class), Route::Keys, "{class}");
        }
    }

    /// `cargo test explorer_properties -- --ignored --nocapture`: opens Explorer windows (one with
    /// notepad.exe selected, one on a folder with nothing selected), asks for the properties and
    /// closes everything again.
    #[test]
    #[ignore = "opens Explorer windows and properties windows on the desktop"]
    fn explorer_properties() {
        use std::time::Duration;
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        use windows::Win32::UI::WindowsAndMessaging::{
            EnumWindows, GetWindowTextW, GetWindowThreadProcessId, PostMessageW, WM_CLOSE,
        };
        use windows::core::BOOL;
        type Found = (String, Vec<isize>);
        unsafe extern "system" fn explorers(hwnd: HWND, found: LPARAM) -> BOOL {
            let mut title = [0u16; 128];
            // SAFETY: `found` is the `Found` passed to `EnumWindows` below.
            unsafe {
                let len = GetWindowTextW(hwnd, &mut title).max(0) as usize;
                let found = &mut *(found.0 as *mut Found);
                if window_class(hwnd) == "CabinetWClass"
                    && String::from_utf16_lossy(&title[..len]).starts_with(&found.0)
                {
                    found.1.push(hwnd.0 as isize);
                }
            }
            true.into()
        }
        unsafe extern "system" fn own_dialogs(hwnd: HWND, found: LPARAM) -> BOOL {
            let mut pid = 0;
            // SAFETY: `found` is the `Found` passed to `EnumWindows` below.
            unsafe {
                GetWindowThreadProcessId(hwnd, Some(&mut pid));
                if pid == std::process::id() && IsWindowVisible(hwnd).as_bool() {
                    let mut title = [0u16; 128];
                    let len = GetWindowTextW(hwnd, &mut title).max(0) as usize;
                    println!(
                        "  dialog: {} [{}]",
                        String::from_utf16_lossy(&title[..len]),
                        window_class(hwnd)
                    );
                    (*(found.0 as *mut Found)).1.push(hwnd.0 as isize);
                }
            }
            true.into()
        }
        let list = |f: unsafe extern "system" fn(HWND, LPARAM) -> BOOL, prefix: &str| {
            let mut found: Found = (prefix.to_string(), Vec::new());
            // SAFETY: `found` outlives the call.
            let _ = unsafe { EnumWindows(Some(f), LPARAM(&mut found as *mut Found as isize)) };
            found.1
        };
        // SAFETY: plain call.
        let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        for (argument, title) in [
            (r"/select,C:\Windows\notepad.exe", "Windows"),
            (r"C:\Windows\Web", "Web"),
        ] {
            std::process::Command::new("explorer.exe")
                .arg(argument)
                .status()
                .expect("explorer");
            let mut explorer = HWND::default();
            for _ in 0..40 {
                std::thread::sleep(Duration::from_millis(250));
                if let Some(&h) = list(explorers, title).first() {
                    explorer = HWND(h as *mut _);
                    break;
                }
            }
            assert!(!explorer.0.is_null(), "no Explorer window titled {title}");
            std::thread::sleep(Duration::from_millis(1500));
            println!("subject: {:?}", selection(explorer, false));
            println!("show: {:?}", show(explorer, false));
            std::thread::sleep(Duration::from_millis(2500));
            // SAFETY: plain calls on windows found above.
            unsafe {
                for h in list(own_dialogs, "") {
                    let _ = PostMessageW(Some(HWND(h as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
                for h in list(explorers, title) {
                    let _ = PostMessageW(Some(HWND(h as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        println!("desktop: {:?}", selection(HWND::default(), true));
        // Like an action thread: single-threaded COM for the call only, gone before the window shows.
        let several = std::thread::spawn(|| {
            use windows::Win32::System::Com::CoUninitialize;
            // SAFETY: plain calls; COM is released on this thread afterwards.
            unsafe {
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                let result = open(&[
                    r"C:\Windows\notepad.exe".to_string(),
                    r"C:\Windows\regedit.exe".to_string(),
                ]);
                CoUninitialize();
                result
            }
        });
        println!("several: {:?}", several.join().unwrap());
        std::thread::sleep(Duration::from_millis(2500));
        // SAFETY: plain calls on windows found above.
        unsafe {
            for h in list(own_dialogs, "") {
                let _ = PostMessageW(Some(HWND(h as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
    }
}
