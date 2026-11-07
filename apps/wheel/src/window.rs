//! The wheel window: DPI setup, a non-activating topmost popup, message loop, hover polling.
//! `--demo` shows it once; `--run` keeps it hidden and the hook thread shows it on demand.

use std::path::PathBuf;
use std::sync::Arc;

use wheel_core::{Config, Hit, Layout, SubArc, arc_for_hit, hit_test_arc};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONEAREST, MonitorFromPoint};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForMonitor, MDT_EFFECTIVE_DPI,
    SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GWLP_USERDATA, GetCursorPos,
    GetMessageW, GetWindowLongPtrW, HWND_TOPMOST, IDC_ARROW, IsWindowVisible, KillTimer,
    LoadCursorW, MA_NOACTIVATE, MSG, PostQuitMessage, RegisterClassExW, SW_HIDE, SW_SHOWNOACTIVATE,
    SWP_NOACTIVATE, SWP_SHOWWINDOW, SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    TranslateMessage, WINDOW_EX_STYLE, WM_APP, WM_DESTROY, WM_LBUTTONDOWN, WM_MOUSEACTIVATE,
    WM_RBUTTONDOWN, WM_TIMER, WNDCLASSEXW, WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::{Error, PCWSTR, Result, w};

use crate::args::{DemoOptions, PanicTest, RunOptions};
use crate::guard;
use crate::hook::{self, HookOptions};
use crate::render::Renderer;
use crate::settings::Settings;
use crate::stats::{self, OpenKind};
use crate::theme::{self, Palette};
use crate::watch::Watcher;

/// Class name of the `--run` window; `--quit` finds the running instance by it.
pub const RUN_CLASS: PCWSTR = w!("ElementalWheelRun");
/// Posted by the hook thread: show the wheel centred at (wparam, lparam) = (x, y) in pixels.
pub const WM_WHEEL_OPEN: u32 = WM_APP + 1;
/// Posted by the hook thread: hide the wheel.
pub const WM_WHEEL_CLOSE: u32 = WM_APP + 2;
/// Posted by `--quit`: end the program.
pub const WM_WHEEL_QUIT: u32 = WM_APP + 3;
/// Posted by the config folder watcher: something in the folder changed.
pub const WM_CONFIG_CHANGED: u32 = WM_APP + 4;

const POLL_TIMER: usize = 1;
const POLL_MS: u32 = 16;
const CLOSE_TIMER: usize = 2;
const CONFIG_TIMER: usize = 3;
const STATS_TIMER: usize = 4;
const STATS_MS: u32 = 60_000;
/// Wait after a change so that an editor can finish saving; further changes restart the wait.
const CONFIG_DEBOUNCE_MS: u32 = 100;
const CLOSE_AFTER_MS: u32 = 10_000;

struct State {
    renderer: Renderer,
    layout: Layout,
    /// Wheel center in screen pixels.
    center: (f32, f32),
    hit: Hit,
    /// The submenu arc shown; this window owns it.
    arc: Option<SubArc>,
    /// Hidden `--demo-hit`: the highlight stays on this tile whatever the cursor does.
    forced: bool,
    config: Arc<Config>,
    error: Option<Error>,
}

/// Shows the demo wheel under the cursor until it is clicked or 10 s pass.
pub fn run_demo(options: &DemoOptions, config_path: PathBuf) -> Result<()> {
    // SAFETY: called once at startup before any window exists.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)? };

    let cursor = match options.at {
        Some((x, y)) => POINT { x, y },
        None => cursor_pos()?,
    };
    let (_, config) = Settings::load(config_path, options.overrides);
    let o = &config.options;
    let layout = Layout::new(
        monitor_dpi(cursor)?,
        o.scale_pct,
        o.inner_sectors,
        o.outer_sectors,
    );
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
    let (hit, arc) = match options.demo_hit {
        Some(hit) => (hit, arc_for_hit(hit, &config)),
        None => track(&layout, &config, center, cursor, None),
    };
    let renderer = match Renderer::new(
        hwnd,
        layout,
        theme::palette(o.theme),
        config.clone(),
        !options.no_glass,
    )
    .and_then(|r| r.draw(hit, arc).map(|()| r))
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
        arc,
        forced: options.demo_hit.is_some(),
        config,
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
pub fn layout_at(pt: POINT, scale_pct: u16, inner_sectors: u8, outer_sectors: u8) -> Layout {
    Layout::new(
        monitor_dpi(pt).unwrap_or(96),
        scale_pct,
        inner_sectors,
        outer_sectors,
    )
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

fn hit_at(layout: &Layout, center: (f32, f32), cursor: POINT, arc: Option<SubArc>) -> Hit {
    hit_test_arc(
        cursor.x as f32 - center.0,
        cursor.y as f32 - center.1,
        &layout.radii(),
        layout.inner_sectors(),
        layout.outer_sectors(),
        arc,
    )
}

/// What is under the cursor and which arc is open, given the arc that was open before. When the
/// arc changes, the hit is computed again with the new one, so a cursor standing in the arc band
/// at once hits the arc tile (what is shown is what gets selected).
fn track(
    layout: &Layout,
    config: &Config,
    center: (f32, f32),
    cursor: POINT,
    arc: Option<SubArc>,
) -> (Hit, Option<SubArc>) {
    let hit = hit_at(layout, center, cursor, arc);
    let new_arc = arc_for_hit(hit, config);
    if new_arc == arc {
        return (hit, arc);
    }
    (hit_at(layout, center, cursor, new_arc), new_arc)
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
    if state.forced {
        return Ok(());
    }
    let (hit, arc) = track(
        &state.layout,
        &state.config,
        state.center,
        cursor_pos()?,
        state.arc,
    );
    if hit != state.hit || arc != state.arc {
        state.hit = hit;
        state.arc = arc;
        state.renderer.draw(hit, arc)?;
    }
    Ok(())
}

extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    guard::try_run("window proc", "window closed", || {
        wnd_proc_inner(hwnd, msg, wparam, lparam)
    })
    .unwrap_or_else(|| {
        // SAFETY: `hwnd` is the live window of this thread; the demo closes by destroying it.
        let _ = unsafe { DestroyWindow(hwnd) };
        // SAFETY: passing the arguments on unchanged.
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    })
}

fn wnd_proc_inner(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
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
                guard::fire(PanicTest::Window);
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
    /// Glass look wanted (the renderer still falls back to opaque when it cannot be had).
    glass: bool,
    config: Arc<Config>,
    settings: Settings,
    /// Size of the wheel in use. A reloaded file changes it only after the open that found the
    /// change, because the hook thread has already laid that one out with the old values.
    scale_pct: u16,
    inner_sectors: u8,
    sectors: u8,
    layout: Layout,
    /// Wheel center in screen pixels while the wheel is shown.
    center: (f32, f32),
    hit: Hit,
    /// The submenu arc shown; this window owns it and publishes it to the hook thread.
    arc: Option<SubArc>,
    error: Option<Error>,
}

/// Background mode: a hidden click-through window plus the hook thread; runs until
/// `WM_WHEEL_QUIT`. The render resources are created once up front so that opening is fast.
pub fn run_background(options: &RunOptions, config_path: PathBuf) -> Result<()> {
    // SAFETY: called once at startup before any window exists.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)? };

    let (settings, config) = Settings::load(config_path, options.wheel.overrides);
    let watch_dir = match settings.path().parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let o = &config.options;
    let (scale_pct, inner_sectors, sectors) = (o.scale_pct, o.inner_sectors, o.outer_sectors);
    let layout = layout_at(cursor_pos()?, scale_pct, inner_sectors, sectors);
    let size = layout.size_px() as i32;
    let hwnd = create_popup(RUN_CLASS, run_wnd_proc, WS_EX_TRANSPARENT, (0, 0, size))?;

    let palette = theme::palette(config.options.theme);
    let glass = !options.wheel.no_glass;
    let renderer = match Renderer::new(hwnd, layout, palette, config.clone(), glass)
        .and_then(|r| r.draw(Hit::Center, None).map(|()| r))
    {
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
        glass,
        config: config.clone(),
        settings,
        scale_pct,
        inner_sectors,
        sectors,
        layout,
        center: (0.0, 0.0),
        hit: Hit::Center,
        arc: None,
        error: None,
    }));
    // SAFETY: `state` stays valid until it is reclaimed below, after the message loop ended.
    unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize) };

    if options.stats {
        stats::enable();
        stats::log_memory();
        // SAFETY: `hwnd` is a live window of this thread.
        if unsafe { SetTimer(Some(hwnd), STATS_TIMER, STATS_MS, None) } == 0 {
            wheel_core::log_line!(
                "stats: cannot start the memory timer: {}",
                Error::from_thread()
            );
        }
    }

    let hook_options = HookOptions {
        accept_injected: options.accept_injected,
        stall_hook_once: options.stall_hook_once,
    };
    let watcher = Watcher::start(&watch_dir, hwnd, WM_CONFIG_CHANGED);
    let loop_result = hook::start(hook_options, hwnd, config).and_then(|hook| {
        let result = message_loop();
        hook.stop();
        result
    });
    if let Some(watcher) = watcher {
        watcher.stop();
    }
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

/// Picks up a changed config file: the hook thread gets the new configuration, the wheel the
/// new look (rebuilt in `open_wheel`).
fn reload(s: &mut RunState, apply_layout: bool) {
    if let Some(config) = s.settings.refresh() {
        hook::publish(config.clone());
        s.palette = theme::palette(config.options.theme);
        if apply_layout {
            // The wheel is hidden, so no open is in flight with the old size.
            s.scale_pct = config.options.scale_pct;
            s.inner_sectors = config.options.inner_sectors;
            s.sectors = config.options.outer_sectors;
        }
        s.config = config;
        s.renderer = None;
    }
}

/// Shows the wheel centred at `center` without activating the window.
fn open_wheel(hwnd: HWND, s: &mut RunState, center: POINT) -> Result<OpenKind> {
    let layout = layout_at(center, s.scale_pct, s.inner_sectors, s.sectors);
    let mut kind = OpenKind::Same;
    if layout != s.layout || s.renderer.is_none() {
        kind = if s.renderer.is_none() {
            OpenKind::Reload
        } else {
            OpenKind::Dpi
        };
        // The monitor DPI differs: size and fonts change, so the render resources are rebuilt.
        // The old ones go first, a window can have only one composition target.
        s.renderer = None;
        s.renderer = Some(Renderer::new(
            hwnd,
            layout,
            s.palette,
            s.config.clone(),
            s.glass,
        )?);
        s.layout = layout;
    }
    let size = layout.size_px() as i32;
    let (left, top) = (center.x - size / 2, center.y - size / 2);
    s.center = (
        left as f32 + size as f32 / 2.0,
        top as f32 + size as f32 / 2.0,
    );
    // A new wheel starts without an arc.
    (s.hit, s.arc) = track(&layout, &s.config, s.center, cursor_pos()?, None);
    hook::publish_arc(s.arc);
    if let Some(renderer) = &s.renderer {
        renderer.draw(s.hit, s.arc)?;
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
    Ok(kind)
}

fn close_wheel(hwnd: HWND) {
    hook::publish_arc(None);
    // SAFETY: `hwnd` is the live window of this thread; the return values carry no news.
    unsafe {
        let _ = KillTimer(Some(hwnd), POLL_TIMER);
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
}

fn poll_run(s: &mut RunState) -> Result<()> {
    let (hit, arc) = track(&s.layout, &s.config, s.center, cursor_pos()?, s.arc);
    if hit != s.hit || arc != s.arc {
        if arc != s.arc {
            hook::publish_arc(arc);
        }
        s.hit = hit;
        s.arc = arc;
        if let Some(renderer) = &s.renderer {
            renderer.draw(hit, arc)?;
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
    guard::try_run("window proc", "wheel hidden", || {
        // SAFETY: same contract as this function.
        unsafe { run_wnd_proc_inner(hwnd, msg, wparam, lparam) }
    })
    .unwrap_or_else(|| {
        close_wheel(hwnd);
        // The hook thread's trigger still believes the wheel is open.
        hook::request_recover();
        // SAFETY: passing the arguments on unchanged.
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    })
}

unsafe fn run_wnd_proc_inner(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
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
                let started = stats::open_started();
                guard::fire(PanicTest::Window);
                run_state(hwnd).map_or(Ok(()), |s| {
                    reload(s, false);
                    let opened = open_wheel(hwnd, s, center);
                    if let (Some(started), Ok(kind)) = (started, &opened) {
                        stats::open_finished(started, *kind);
                    }
                    s.scale_pct = s.config.options.scale_pct;
                    s.inner_sectors = s.config.options.inner_sectors;
                    s.sectors = s.config.options.outer_sectors;
                    opened.map(|_| ())
                })
            }
            WM_WHEEL_CLOSE => {
                close_wheel(hwnd);
                return LRESULT(0);
            }
            WM_TIMER if wparam.0 == POLL_TIMER => run_state(hwnd).map_or(Ok(()), poll_run),
            WM_CONFIG_CHANGED => {
                // One reload for a burst of changes.
                if SetTimer(Some(hwnd), CONFIG_TIMER, CONFIG_DEBOUNCE_MS, None) == 0 {
                    Err(Error::from_thread())
                } else {
                    Ok(())
                }
            }
            WM_TIMER if wparam.0 == STATS_TIMER => {
                stats::log_memory();
                return LRESULT(0);
            }
            WM_TIMER if wparam.0 == CONFIG_TIMER => {
                if IsWindowVisible(hwnd).as_bool() {
                    // Not while the wheel is shown; try again shortly.
                    return LRESULT(0);
                }
                let _ = KillTimer(Some(hwnd), CONFIG_TIMER);
                if let Some(s) = run_state(hwnd) {
                    reload(s, true);
                }
                return LRESULT(0);
            }
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
