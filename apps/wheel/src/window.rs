//! The wheel window: DPI setup, a non-activating topmost popup, message loop, hover polling.

use wheel_core::{Hit, Layout, hit_test};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONEAREST, MonitorFromPoint};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForMonitor, MDT_EFFECTIVE_DPI,
    SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GWLP_USERDATA, GetCursorPos,
    GetMessageW, GetWindowLongPtrW, IDC_ARROW, LoadCursorW, MA_NOACTIVATE, MSG, PostQuitMessage,
    RegisterClassExW, SW_SHOWNOACTIVATE, SetTimer, SetWindowLongPtrW, ShowWindow, TranslateMessage,
    WM_DESTROY, WM_LBUTTONDOWN, WM_MOUSEACTIVATE, WM_RBUTTONDOWN, WM_TIMER, WNDCLASSEXW,
    WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{Error, Result, w};

use crate::args::DemoOptions;
use crate::render::Renderer;
use crate::theme;

const POLL_TIMER: usize = 1;
const POLL_MS: u32 = 16;
const CLOSE_TIMER: usize = 2;
const CLOSE_AFTER_MS: u32 = 10_000;

struct State {
    renderer: Renderer,
    layout: Layout,
    /// Wheel center in screen pixels.
    center: (f32, f32),
    hit: Hit,
    error: Option<Error>,
}

/// Shows the demo wheel under the cursor until it is clicked or 10 s pass.
pub fn run_demo(options: &DemoOptions) -> Result<()> {
    // SAFETY: called once at startup before any window exists.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)? };

    let cursor = cursor_pos()?;
    let layout = Layout::new(monitor_dpi(cursor)?, options.scale_pct, options.sectors);
    let size = layout.size_px() as i32;
    let left = cursor.x - size / 2;
    let top = cursor.y - size / 2;

    // SAFETY: standard window class and window creation; all handles come from the calls.
    let hwnd = unsafe {
        let instance = GetModuleHandleW(None)?;
        let class_name = w!("ElementalWheel");
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wnd_proc),
            hInstance: instance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            lpszClassName: class_name,
            ..Default::default()
        };
        if RegisterClassExW(&class) == 0 {
            return Err(Error::from_thread());
        }
        CreateWindowExW(
            WS_EX_NOACTIVATE | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOREDIRECTIONBITMAP,
            class_name,
            w!("Elemental Wheel"),
            WS_POPUP,
            left,
            top,
            size,
            size,
            None,
            None,
            Some(instance.into()),
            None,
        )?
    };

    // The wheel center is the window center, at the cursor position at creation (off by
    // half a pixel when the window size is odd).
    let center = (
        left as f32 + size as f32 / 2.0,
        top as f32 + size as f32 / 2.0,
    );
    let hit = hit_at(&layout, center, cursor);
    let renderer = match Renderer::new(hwnd, layout, theme::palette(options.theme))
        .and_then(|r| r.draw(hit).map(|()| r))
    {
        Ok(r) => r,
        Err(e) => {
            // SAFETY: `hwnd` was created above and is destroyed only here.
            let _ = unsafe { DestroyWindow(hwnd) };
            return Err(e);
        }
    };
    let state = Box::into_raw(Box::new(State {
        renderer,
        layout,
        center,
        hit,
        error: None,
    }));

    // SAFETY: `state` stays valid until it is reclaimed below, after the message loop ended.
    let loop_result = unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize);
        // The return value is only the previous visibility.
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        start_timers(hwnd).and_then(|()| message_loop())
    };
    // SAFETY: the window is destroyed (if it still exists) and the pointer cleared before the
    // state is freed, so nothing can reach it afterwards. `state` comes from `Box::into_raw`.
    let state = unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        let _ = DestroyWindow(hwnd);
        Box::from_raw(state)
    };
    loop_result?;
    state.error.map_or(Ok(()), Err)
}

fn start_timers(hwnd: HWND) -> Result<()> {
    for (id, ms) in [(POLL_TIMER, POLL_MS), (CLOSE_TIMER, CLOSE_AFTER_MS)] {
        // SAFETY: `hwnd` is a live window of this thread.
        if unsafe { SetTimer(Some(hwnd), id, ms, None) } == 0 {
            return Err(Error::from_thread());
        }
    }
    Ok(())
}

fn message_loop() -> Result<()> {
    let mut msg = MSG::default();
    loop {
        // SAFETY: `msg` is a valid out buffer.
        let status = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        match status.0 {
            0 => return Ok(()),
            -1 => return Err(Error::from_thread()),
            _ => {
                // SAFETY: `msg` was filled by GetMessageW.
                unsafe {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
    }
}

fn cursor_pos() -> Result<POINT> {
    let mut pt = POINT::default();
    // SAFETY: `pt` is a valid out buffer.
    unsafe { GetCursorPos(&mut pt)? };
    Ok(pt)
}

/// Effective DPI of the monitor under `pt`.
fn monitor_dpi(pt: POINT) -> Result<u32> {
    let (mut x, mut y) = (0u32, 0u32);
    // SAFETY: the monitor handle comes from MonitorFromPoint; `x` and `y` are valid out buffers.
    unsafe {
        let monitor = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y)?;
    }
    Ok(x)
}

fn hit_at(layout: &Layout, center: (f32, f32), cursor: POINT) -> Hit {
    hit_test(
        cursor.x as f32 - center.0,
        cursor.y as f32 - center.1,
        &layout.radii(),
        layout.outer_sectors(),
    )
}

/// # Safety
/// The `GWLP_USERDATA` of `hwnd` is either null or the `State` pointer set in `run_demo`, and no
/// other reference to it is alive.
unsafe fn state<'a>(hwnd: HWND) -> Option<&'a mut State> {
    // SAFETY: guaranteed by the caller.
    unsafe { (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State).as_mut() }
}

/// Re-reads the cursor and redraws when the highlighted sector changed.
fn poll(state: &mut State) -> Result<()> {
    let hit = hit_at(&state.layout, state.center, cursor_pos()?);
    if hit != state.hit {
        state.hit = hit;
        state.renderer.draw(hit)?;
    }
    Ok(())
}

extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: all messages are handled on the one UI thread; the state reference is dropped
    // before anything that can re-enter this function (DestroyWindow).
    unsafe {
        match msg {
            WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
            WM_LBUTTONDOWN | WM_RBUTTONDOWN => {
                let _ = DestroyWindow(hwnd);
                return LRESULT(0);
            }
            WM_TIMER if wparam.0 == CLOSE_TIMER => {
                let _ = DestroyWindow(hwnd);
                return LRESULT(0);
            }
            WM_TIMER if wparam.0 == POLL_TIMER => {
                let failed = match state(hwnd) {
                    Some(s) => match poll(s) {
                        Ok(()) => false,
                        Err(e) => {
                            s.error = Some(e);
                            true
                        }
                    },
                    None => false,
                };
                if failed {
                    let _ = DestroyWindow(hwnd);
                }
                return LRESULT(0);
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                return LRESULT(0);
            }
            _ => {}
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}
