//! The wheel window: DPI setup, a non-activating topmost popup, message loop, hover polling.
//! `--demo` shows it once; `--run` keeps it hidden and the hook thread shows it on demand.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use wheel_core::{Config, Hit, Layout, SubArc, arc_for_hit, hit_test_arc};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Dwm::DwmFlush;
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

use crate::anim::{self, Anim};
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
/// Posted by the hook thread when the right button goes down and the wheel may open at
/// (wparam, lparam) = (x, y): take the snapshot of the desktop now.
pub const WM_WHEEL_PRESS: u32 = WM_APP + 5;
/// Posted by the hook thread when that press turned out to be a click or a drag: drop the snapshot.
pub const WM_WHEEL_DISCARD: u32 = WM_APP + 6;

const POLL_TIMER: usize = 1;
const POLL_MS: u32 = 16;
const CLOSE_TIMER: usize = 2;
const CONFIG_TIMER: usize = 3;
const STATS_TIMER: usize = 4;
/// Hides the window when the closing fade is over.
const CLOSE_FADE_TIMER: usize = 5;
/// Hidden `--demo-linger-ms`: logs the memory of the hidden demo every second, then ends it.
const LINGER_TIMER: usize = 6;
const LINGER_TICK_MS: u32 = 1000;
/// Second trim after the wheel was hidden: the first one frees only part of the memory, the rest
/// is released by the driver once it has gone idle.
const TRIM_TIMER: usize = 7;
const TRIM_DELAY_MS: u32 = 1000;
const STATS_MS: u32 = 60_000;
/// Wait after a change so that an editor can finish saving; further changes restart the wait.
const CONFIG_DEBOUNCE_MS: u32 = 100;
const CLOSE_AFTER_MS: u32 = 10_000;
/// Wait beyond the closing fade, so that its last frame is shown before the window goes.
const FADE_MARGIN_MS: u32 = 15;

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
    /// Highlight and arc transitions; animations also fade the demo out.
    anim: Anim,
    animate: bool,
    /// Hidden `--demo-linger-ms`: how long the process stays after the close.
    linger_ms: Option<u32>,
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
    let animate = anim::should_animate(
        o.animations && !options.no_anim,
        options.force_anim || anim::windows_animations(),
    );
    // An arc already open slides out once the wheel has faded in.
    let anim = Anim::new(
        animate,
        hit,
        arc.map(|_| anim::OPEN_MS as f32),
        Instant::now(),
    );
    let renderer = match Renderer::new(
        hwnd,
        layout,
        theme::palette(o.theme),
        config.clone(),
        options.glass_level(config.options.glass_pct),
    )
    .and_then(|r| {
        // The window is still hidden, so the snapshot shows the desktop without it.
        log_glass("demo", r.prepare(left, top));
        r.prepare_open(animate)?;
        r.draw(hit, arc, &anim).map(|()| r)
    }) {
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
        anim,
        animate,
        linger_ms: options.linger_ms,
        error: None,
    }));

    // SAFETY: `state` stays valid until it is reclaimed below, after the message loop ended.
    let loop_result = unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize);
        // The return value is only the previous visibility.
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        (*state)
            .renderer
            .start_open(animate)
            .and_then(|()| start_timers(hwnd, options.close_ms.unwrap_or(CLOSE_AFTER_MS)))
            .and_then(|()| message_loop())
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

/// Ends the demo after the close: at once, or with `--demo-linger-ms` hides the window like the
/// background process does, frees the snapshot and keeps logging memory until the time is up.
///
/// # Safety
/// `hwnd` is the demo window of this thread; no `State` reference is alive.
unsafe fn end_demo(hwnd: HWND) {
    // SAFETY: guaranteed by the caller.
    unsafe {
        let Some(s) = state(hwnd).filter(|s| s.linger_ms.is_some()) else {
            let _ = DestroyWindow(hwnd);
            return;
        };
        let _ = KillTimer(Some(hwnd), POLL_TIMER);
        let _ = ShowWindow(hwnd, SW_HIDE);
        s.renderer.release_snapshot();
        s.renderer.trim();
        stats::log_memory();
        if SetTimer(Some(hwnd), LINGER_TIMER, LINGER_TICK_MS, None) == 0 {
            let _ = DestroyWindow(hwnd);
        }
    }
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

fn start_timers(hwnd: HWND, close_ms: u32) -> Result<()> {
    for (id, ms) in [(POLL_TIMER, POLL_MS), (CLOSE_TIMER, close_ms)] {
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

/// Re-reads the cursor and redraws when the highlighted sector changed or a transition is
/// still running.
fn poll(state: &mut State) -> Result<()> {
    let mut changed = false;
    if !state.forced {
        let (hit, arc) = track(
            &state.layout,
            &state.config,
            state.center,
            cursor_pos()?,
            state.arc,
        );
        changed = hit != state.hit || arc != state.arc;
        if arc != state.arc && arc.is_some() {
            state.anim.restart_arc();
        }
        state.hit = hit;
        state.arc = arc;
        state.anim.set_target(hit);
    }
    if state.anim.tick(changed, Instant::now()) {
        state.renderer.draw(state.hit, state.arc, &state.anim)?;
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
                let _ = KillTimer(Some(hwnd), CLOSE_TIMER);
                stats::log_memory();
                let fading =
                    state(hwnd).is_some_and(|s| s.animate && s.renderer.start_close().is_ok());
                if fading {
                    let _ = KillTimer(Some(hwnd), POLL_TIMER);
                    if SetTimer(
                        Some(hwnd),
                        CLOSE_FADE_TIMER,
                        anim::CLOSE_MS + FADE_MARGIN_MS,
                        None,
                    ) != 0
                    {
                        return LRESULT(0);
                    }
                }
                end_demo(hwnd);
                return LRESULT(0);
            }
            WM_TIMER if wparam.0 == CLOSE_FADE_TIMER => {
                let _ = KillTimer(Some(hwnd), CLOSE_FADE_TIMER);
                end_demo(hwnd);
                return LRESULT(0);
            }
            WM_TIMER if wparam.0 == LINGER_TIMER => {
                // A second trim a moment after the first, as the background process does.
                if let Some(s) = state(hwnd) {
                    s.renderer.trim();
                }
                stats::log_memory();
                if state(hwnd).is_some_and(|s| {
                    s.linger_ms = s.linger_ms.map(|ms| ms.saturating_sub(LINGER_TICK_MS));
                    s.linger_ms == Some(0)
                }) {
                    let _ = DestroyWindow(hwnd);
                }
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
    /// Glass transparency forced by a command line flag; otherwise the file's `glass_pct` counts.
    glass_override: Option<f32>,
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
    /// Highlight and arc transitions of the wheel shown.
    anim: Anim,
    /// The wheel shown was opened with animations, so it also fades out.
    animate: bool,
    /// The closing fade is running; the window is still visible.
    closing: bool,
    error: Option<Error>,
}

impl RunState {
    /// Glass look wanted (the renderer still falls back to opaque when it cannot be had).
    fn glass_level(&self) -> f32 {
        glass_level(self.glass_override, &self.config)
    }
}

fn glass_level(glass_override: Option<f32>, config: &Config) -> f32 {
    glass_override.unwrap_or(f32::from(config.options.glass_pct) / 100.0)
}

/// Background mode: a hidden click-through window plus the hook thread; runs until
/// `WM_WHEEL_QUIT`. The render resources are created up front (and again after a configuration
/// change) so that opening is fast.
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
    let glass_override = options.wheel.glass_override();
    let renderer = match build_renderer(
        hwnd,
        layout,
        palette,
        &config,
        glass_level(glass_override, &config),
    ) {
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
        glass_override,
        config: config.clone(),
        settings,
        scale_pct,
        inner_sectors,
        sectors,
        layout,
        center: (0.0, 0.0),
        hit: Hit::Center,
        arc: None,
        anim: Anim::settled(Hit::Center),
        animate: false,
        closing: false,
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

/// Creates the renderer with everything it would otherwise build at the first open (see
/// `Renderer::warm`) and shows its first frame.
fn build_renderer(
    hwnd: HWND,
    layout: Layout,
    palette: Palette,
    config: &Arc<Config>,
    glass: f32,
) -> Result<Renderer> {
    let renderer = Renderer::new(hwnd, layout, palette, config.clone(), glass)?;
    renderer.warm();
    renderer.draw(Hit::Center, None, &Anim::settled(Hit::Center))?;
    // The window is still hidden here; what the warm-up left in caches is not needed any more.
    renderer.trim();
    Ok(renderer)
}

/// Picks up a changed config file: the hook thread gets the new configuration. When the wheel is
/// hidden (`apply_layout`) the renderer with the new look is built at once; otherwise it is
/// rebuilt in `open_wheel`.
fn reload(hwnd: HWND, s: &mut RunState, apply_layout: bool) {
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
        // The old renderer goes first, a window can have only one composition target.
        s.renderer = None;
        if apply_layout {
            let at = cursor_pos().unwrap_or_default();
            let layout = layout_at(at, s.scale_pct, s.inner_sectors, s.sectors);
            match build_renderer(hwnd, layout, s.palette, &s.config, s.glass_level()) {
                Ok(renderer) => {
                    s.renderer = Some(renderer);
                    s.layout = layout;
                }
                // The open will try again and report the error.
                Err(e) => wheel_core::log_line!("cannot rebuild the renderer after a reload: {e}"),
            }
        }
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
        s.renderer = Some(build_renderer(
            hwnd,
            layout,
            s.palette,
            &s.config,
            s.glass_level(),
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
    s.animate = anim::should_animate(s.config.options.animations, anim::windows_animations());
    s.anim = Anim::new(s.animate, s.hit, None, Instant::now());
    if let Some(renderer) = &s.renderer {
        // The window is hidden until SetWindowPos below, so the snapshot shows the desktop
        // without it.
        let times = renderer.prepare(left, top);
        if stats::enabled() {
            log_glass("open", times);
        }
        renderer.prepare_open(s.animate)?;
        renderer.draw(s.hit, s.arc, &s.anim)?;
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
    if let Some(renderer) = &s.renderer {
        renderer.start_open(s.animate)?;
    }
    Ok(kind)
}

/// Takes the snapshot of the desktop for a wheel that may open at `at`; the window is hidden
/// now. `open_wheel` uses it if the wheel does open there with the same layout.
fn take_snapshot(s: &RunState, at: POINT) {
    let Some(renderer) = &s.renderer else {
        return;
    };
    let layout = layout_at(at, s.scale_pct, s.inner_sectors, s.sectors);
    if layout != s.layout {
        // Another monitor DPI: the open rebuilds the renderer anyway.
        return;
    }
    let size = layout.size_px() as i32;
    let times = renderer.snapshot(at.x - size / 2, at.y - size / 2);
    if stats::enabled() {
        log_glass("press", times);
    }
}

/// Logs how long the snapshot of the desktop and its blur took (capture, upload plus blur).
fn log_glass(when: &str, times: Option<(f64, f64)>) {
    if let Some((capture, blur)) = times {
        wheel_core::log_line!("glass {when}: capture_ms={capture:.3} blur_ms={blur:.3}");
    }
}

/// Hides the wheel at once.
fn close_wheel(hwnd: HWND) {
    hook::publish_arc(None);
    // SAFETY: `hwnd` is the live window of this thread; the return values carry no news.
    unsafe {
        let _ = KillTimer(Some(hwnd), POLL_TIMER);
        let _ = KillTimer(Some(hwnd), CLOSE_FADE_TIMER);
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
}

/// Frees the snapshot and its effects, and once the window is hidden also the memory the
/// graphics stack keeps for reuse. While the closing fade still runs the window is visible, and
/// the fade timer calls this again when it ends.
fn release_gpu(hwnd: HWND) {
    // SAFETY: `hwnd` is the live window of this thread; the state reference is short-lived.
    unsafe {
        let Some(renderer) = run_state(hwnd).and_then(|s| s.renderer.as_ref()) else {
            return;
        };
        renderer.release_snapshot();
        if !IsWindowVisible(hwnd).as_bool() {
            renderer.trim();
            // A failure only means the second trim does not happen.
            let _ = SetTimer(Some(hwnd), TRIM_TIMER, TRIM_DELAY_MS, None);
        }
    }
}

/// Closes the wheel: fades it out and hides it when the fade is over, or hides it at once when
/// the wheel was opened without animations.
fn start_closing(hwnd: HWND, s: &mut RunState) {
    if s.closing {
        return;
    }
    // SAFETY: `hwnd` is the live window of this thread.
    unsafe {
        let visible = IsWindowVisible(hwnd).as_bool();
        let fading = s.animate
            && visible
            && s.renderer.as_ref().is_some_and(|r| r.start_close().is_ok())
            && SetTimer(
                Some(hwnd),
                CLOSE_FADE_TIMER,
                anim::CLOSE_MS + FADE_MARGIN_MS,
                None,
            ) != 0;
        if fading {
            hook::publish_arc(None);
            let _ = KillTimer(Some(hwnd), POLL_TIMER);
            s.closing = true;
            return;
        }
    }
    close_wheel(hwnd);
}

/// If the closing fade is still running, hides the window now and waits until the desktop is
/// composed without it, so that a snapshot of the desktop cannot catch the fading wheel.
fn finish_closing(hwnd: HWND, s: &mut RunState) {
    if s.closing {
        s.closing = false;
        close_wheel(hwnd);
        // SAFETY: plain call; a failure only means the wait did not happen.
        let _ = unsafe { DwmFlush() };
    }
}

fn poll_run(s: &mut RunState) -> Result<()> {
    if s.closing {
        // A timer message that was already queued when the fade started.
        return Ok(());
    }
    let (hit, arc) = track(&s.layout, &s.config, s.center, cursor_pos()?, s.arc);
    let changed = hit != s.hit || arc != s.arc;
    if changed {
        if arc != s.arc {
            hook::publish_arc(arc);
            if arc.is_some() {
                s.anim.restart_arc();
            }
        }
        s.hit = hit;
        s.arc = arc;
        s.anim.set_target(hit);
    }
    if s.anim.tick(changed, Instant::now())
        && let Some(renderer) = &s.renderer
    {
        renderer.draw(s.hit, s.arc, &s.anim)?;
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
                    finish_closing(hwnd, s);
                    reload(hwnd, s, false);
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
            WM_WHEEL_PRESS => {
                let at = POINT {
                    x: wparam.0 as i32,
                    y: lparam.0 as i32,
                };
                if let Some(s) = run_state(hwnd) {
                    // A wheel still fading out must be gone before the snapshot.
                    finish_closing(hwnd, s);
                }
                // Only while the wheel is hidden; a failure costs nothing, the open takes the
                // snapshot itself then.
                if !IsWindowVisible(hwnd).as_bool()
                    && let Some(s) = run_state(hwnd)
                {
                    take_snapshot(s, at);
                }
                return LRESULT(0);
            }
            WM_WHEEL_DISCARD | WM_WHEEL_CLOSE => {
                if msg == WM_WHEEL_CLOSE {
                    match run_state(hwnd) {
                        Some(s) => start_closing(hwnd, s),
                        None => close_wheel(hwnd),
                    }
                }
                release_gpu(hwnd);
                return LRESULT(0);
            }
            WM_TIMER if wparam.0 == POLL_TIMER => run_state(hwnd).map_or(Ok(()), poll_run),
            WM_TIMER if wparam.0 == CLOSE_FADE_TIMER => {
                if let Some(s) = run_state(hwnd) {
                    s.closing = false;
                }
                close_wheel(hwnd);
                release_gpu(hwnd);
                return LRESULT(0);
            }
            WM_CONFIG_CHANGED => {
                // One reload for a burst of changes.
                if SetTimer(Some(hwnd), CONFIG_TIMER, CONFIG_DEBOUNCE_MS, None) == 0 {
                    Err(Error::from_thread())
                } else {
                    Ok(())
                }
            }
            WM_TIMER if wparam.0 == TRIM_TIMER => {
                let _ = KillTimer(Some(hwnd), TRIM_TIMER);
                if !IsWindowVisible(hwnd).as_bool()
                    && let Some(renderer) = run_state(hwnd).and_then(|s| s.renderer.as_ref())
                {
                    renderer.trim();
                }
                return LRESULT(0);
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
                    reload(hwnd, s, true);
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
