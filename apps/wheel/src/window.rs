//! The wheel window: DPI setup, a non-activating topmost popup, message loop, hover polling.
//! `--demo` shows it once; `--run` keeps it hidden and the hook thread shows it on demand.

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
    GetMessageW, GetWindowLongPtrW, HWND_TOPMOST, IDC_ARROW, KillTimer, LoadCursorW, MA_NOACTIVATE,
    MSG, PostQuitMessage, RegisterClassExW, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE,
    SWP_SHOWWINDOW, SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow, TranslateMessage,
    WINDOW_EX_STYLE, WM_APP, WM_DESTROY, WM_LBUTTONDOWN, WM_MOUSEACTIVATE, WM_RBUTTONDOWN,
    WM_TIMER, WNDCLASSEXW, WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::{Error, PCWSTR, Result, w};

use crate::args::{DemoOptions, RunOptions};
use crate::hook;
use crate::render::Renderer;
use crate::theme::{self, Palette};

/// Class name of the `--run` window; `--quit` finds the running instance by it.
pub const RUN_CLASS: PCWSTR = w!("ElementalWheelRun");
/// Posted by the hook thread: show the wheel centred at (wparam, lparam) = (x, y) in pixels.
pub const WM_WHEEL_OPEN: u32 = WM_APP + 1;
/// Posted by the hook thread: hide the wheel.
pub const WM_WHEEL_CLOSE: u32 = WM_APP + 2;
/// Posted by `--quit`: end the program.
pub const WM_WHEEL_QUIT: u32 = WM_APP + 3;

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

    let hwnd = create_popup(
        w!("ElementalWheel"),
        wnd_proc,
        WINDOW_EX_STYLE(0),
        (left, top, size),
    )?;

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

/// Creates the (hidden) non-activating topmost popup of class `class_name`; `extra` is added to
/// the extended style. `area` is (left, top, side length) in pixels.
fn create_popup(
    class_name: PCWSTR,
    proc: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
    extra: WINDOW_EX_STYLE,
    area: (i32, i32, i32),
) -> Result<HWND> {
    // SAFETY: standard window class and window creation; all handles come from the calls.
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(proc),
            hInstance: instance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            lpszClassName: class_name,
            ..Default::default()
        };
        if RegisterClassExW(&class) == 0 {
            return Err(Error::from_thread());
        }
        CreateWindowExW(
            WS_EX_NOACTIVATE | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOREDIRECTIONBITMAP | extra,
            class_name,
            w!("Elemental Wheel"),
            WS_POPUP,
            area.0,
            area.1,
            area.2,
            area.2,
            None,
            None,
            Some(instance.into()),
            None,
        )
    }
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

/// Wheel layout for the monitor under `pt` (96 DPI if its DPI cannot be read).
pub fn layout_at(pt: POINT, scale_pct: u16, sectors: u8) -> Layout {
    Layout::new(monitor_dpi(pt).unwrap_or(96), scale_pct, sectors)
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

struct RunState {
    /// `None` only while it is being re-created for another monitor DPI.
    renderer: Option<Renderer>,
    palette: Palette,
    scale_pct: u16,
    sectors: u8,
    layout: Layout,
    /// Wheel center in screen pixels while the wheel is shown.
    center: (f32, f32),
    hit: Hit,
    error: Option<Error>,
}

/// Background mode: a hidden click-through window plus the hook thread; runs until
/// `WM_WHEEL_QUIT`. The render resources are created once up front so that opening is fast.
pub fn run_background(options: &RunOptions) -> Result<()> {
    // SAFETY: called once at startup before any window exists.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)? };

    let (scale_pct, sectors) = (options.wheel.scale_pct, options.wheel.sectors);
    let layout = layout_at(cursor_pos()?, scale_pct, sectors);
    let size = layout.size_px() as i32;
    let hwnd = create_popup(RUN_CLASS, run_wnd_proc, WS_EX_TRANSPARENT, (0, 0, size))?;

    let palette = theme::palette(options.wheel.theme);
    let renderer =
        match Renderer::new(hwnd, layout, palette).and_then(|r| r.draw(Hit::Center).map(|()| r)) {
            Ok(r) => r,
            Err(e) => {
                // SAFETY: `hwnd` was created above and is destroyed only here.
                let _ = unsafe { DestroyWindow(hwnd) };
                return Err(e);
            }
        };
    let state = Box::into_raw(Box::new(RunState {
        renderer: Some(renderer),
        palette,
        scale_pct,
        sectors,
        layout,
        center: (0.0, 0.0),
        hit: Hit::Center,
        error: None,
    }));
    // SAFETY: `state` stays valid until it is reclaimed below, after the message loop ended.
    unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize) };

    let config = hook::Config {
        trigger: options.trigger,
        scale_pct,
        sectors,
        accept_injected: options.accept_injected,
        stall_hook_once: options.stall_hook_once,
    };
    let loop_result = hook::start(config, hwnd).and_then(|hook| {
        let result = message_loop();
        hook.stop();
        result
    });
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

/// # Safety
/// The `GWLP_USERDATA` of `hwnd` is either null or the `RunState` pointer set in
/// `run_background`, and no other reference to it is alive.
unsafe fn run_state<'a>(hwnd: HWND) -> Option<&'a mut RunState> {
    // SAFETY: guaranteed by the caller.
    unsafe { (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut RunState).as_mut() }
}

/// Shows the wheel centred at `center` without activating the window.
fn open_wheel(hwnd: HWND, s: &mut RunState, center: POINT) -> Result<()> {
    let layout = layout_at(center, s.scale_pct, s.sectors);
    if layout != s.layout || s.renderer.is_none() {
        // The monitor DPI differs: size and fonts change, so the render resources are rebuilt.
        // The old ones go first, a window can have only one composition target.
        s.renderer = None;
        s.renderer = Some(Renderer::new(hwnd, layout, s.palette)?);
        s.layout = layout;
    }
    let size = layout.size_px() as i32;
    let (left, top) = (center.x - size / 2, center.y - size / 2);
    s.center = (
        left as f32 + size as f32 / 2.0,
        top as f32 + size as f32 / 2.0,
    );
    s.hit = hit_at(&layout, s.center, cursor_pos()?);
    if let Some(renderer) = &s.renderer {
        renderer.draw(s.hit)?;
    }
    // SAFETY: `hwnd` is the live window of this thread.
    unsafe {
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            left,
            top,
            size,
            size,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        )?;
        if SetTimer(Some(hwnd), POLL_TIMER, POLL_MS, None) == 0 {
            return Err(Error::from_thread());
        }
    }
    Ok(())
}

fn close_wheel(hwnd: HWND) {
    // SAFETY: `hwnd` is the live window of this thread; the return values carry no news.
    unsafe {
        let _ = KillTimer(Some(hwnd), POLL_TIMER);
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
}

fn poll_run(s: &mut RunState) -> Result<()> {
    let hit = hit_at(&s.layout, s.center, cursor_pos()?);
    if hit != s.hit {
        s.hit = hit;
        if let Some(renderer) = &s.renderer {
            renderer.draw(hit)?;
        }
    }
    Ok(())
}

unsafe extern "system" fn run_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: all messages are handled on the one UI thread; the state reference is dropped
    // before anything that can re-enter this function (DestroyWindow).
    unsafe {
        let result = match msg {
            WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
            WM_WHEEL_OPEN => {
                let center = POINT {
                    x: wparam.0 as i32,
                    y: lparam.0 as i32,
                };
                run_state(hwnd).map_or(Ok(()), |s| open_wheel(hwnd, s, center))
            }
            WM_WHEEL_CLOSE => {
                close_wheel(hwnd);
                return LRESULT(0);
            }
            WM_TIMER if wparam.0 == POLL_TIMER => run_state(hwnd).map_or(Ok(()), poll_run),
            WM_WHEEL_QUIT => {
                let _ = DestroyWindow(hwnd);
                return LRESULT(0);
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                return LRESULT(0);
            }
            _ => return DefWindowProcW(hwnd, msg, wparam, lparam),
        };
        if let Err(e) = result {
            if let Some(s) = run_state(hwnd) {
                s.error = Some(e);
            }
            let _ = DestroyWindow(hwnd);
        }
        LRESULT(0)
    }
}
