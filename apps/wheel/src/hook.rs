//! Global low-level mouse and keyboard hooks on a dedicated thread with its own message loop.
//!
//! The callbacks only classify the event, step the `Trigger` and queue its effects; the effects
//! (timer, SendInput, window messages, console output) run later in the thread's message loop,
//! because Windows removes a low-level hook whose callback takes too long. The `Trigger` and all
//! other state live in a thread-local that only this thread touches, so there are no locks.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use wheel_core::{
    Config as WheelConfig, Effect, Event, EventKind, Hit, Origin, Point, SubArc, Trigger, Verdict,
};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
};
use windows::Win32::Security::{
    GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TOKEN_MANDATORY_LABEL,
    TOKEN_QUERY, TokenIntegrityLevel,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::SystemInformation::{GetTickCount, GetTickCount64};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentThreadId, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_MOUSE, MOUSE_EVENT_FLAGS, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_MOVE,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK, MOUSEINPUT, SendInput,
    VK_ESCAPE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GA_ROOT, GWL_STYLE, GetAncestor, GetClassNameW, GetMessageW, GetSystemMetrics,
    GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId, HC_ACTION, HHOOK, IsZoomed,
    KBDLLHOOKSTRUCT, KillTimer, LLKHF_INJECTED, LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT, PM_NOREMOVE,
    PeekMessageW, PostMessageW, PostThreadMessageW, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SetTimer, SetWindowsHookExW, UnhookWindowsHookEx,
    WH_KEYBOARD_LL, WH_MOUSE_LL, WM_APP, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MOUSEMOVE, WM_QUIT, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_TIMER,
    WM_USER, WS_CAPTION, WindowFromPoint,
};
use windows::core::{Error, HRESULT, PWSTR, Result};

use crate::actions;
use crate::watchdog;
use crate::window::{WM_WHEEL_CLOSE, WM_WHEEL_OPEN, layout_at};

/// `dwExtraInfo` of the input this program injects ("ELWH").
pub const MARKER: usize = 0x454C_5748;

/// Posted to the hook thread: run the queued effects.
const WM_EFFECTS: u32 = WM_APP + 1;
/// The timer fires `HoldElapsed` with `GetTickCount`, which is coarser than the event times, so
/// it is started a little late to make sure the hold time really has elapsed.
const HOLD_TIMER_SLACK_MS: u32 = 16;
/// How long the integrity level and image path of a process are remembered.
const ELEVATION_CACHE_MS: u64 = 5000;
const ELEVATION_CACHE_MAX: usize = 64;
/// Integrity level assumed for this process if it cannot be read (medium).
const MEDIUM_INTEGRITY: u32 = 0x2000;

/// Startup flags of the hook thread. The settings from the config file travel separately
/// (see `publish`).
#[derive(Debug, Clone, Copy)]
pub struct HookOptions {
    pub accept_injected: bool,
    /// Hidden test flag: the first right-button press stalls the mouse callback past the system
    /// timeout, so Windows removes the hook and the watchdog has to reinstall it.
    pub stall_hook_once: bool,
}

/// Set from `HookOptions::stall_hook_once`; cleared by the first stall.
static STALL_PENDING: AtomicBool = AtomicBool::new(false);
const STALL_MS: u64 = 3000;

/// The configuration in force. The UI thread swaps it after a reload; the hook thread clones the
/// `Arc` (the lock is held only for that) and never touches the file.
static SHARED: Mutex<Option<Arc<WheelConfig>>> = Mutex::new(None);

/// Makes `config` the one the hook thread uses from now on.
pub fn publish(config: Arc<WheelConfig>) {
    *SHARED.lock().unwrap_or_else(PoisonError::into_inner) = Some(config);
}

/// The submenu arc shown by the wheel window, as `parent << 8 | count`; 0 = none (a real arc has
/// at least one tile). The UI thread owns the arc and stores it on every change; the hook thread
/// only loads it, so no lock sits on the path of a mouse event.
static ARC: AtomicU16 = AtomicU16::new(0);

/// Publishes the arc shown now (`None` when there is none or the wheel is closed).
pub fn publish_arc(arc: Option<SubArc>) {
    let bits = arc.map_or(0, |a| u16::from(a.parent) << 8 | u16::from(a.count));
    ARC.store(bits, Ordering::Relaxed);
}

fn current_arc() -> Option<SubArc> {
    let bits = ARC.load(Ordering::Relaxed);
    (bits != 0).then_some(SubArc {
        parent: (bits >> 8) as u8,
        count: (bits & 0xFF) as u8,
    })
}

fn current() -> Option<Arc<WheelConfig>> {
    SHARED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// The running hook thread.
pub struct HookThread {
    thread_id: u32,
    handle: JoinHandle<()>,
}

impl HookThread {
    /// Ends the message loop, removes the hooks and waits for the thread.
    pub fn stop(self) {
        // SAFETY: plain call; if the thread is already gone, the failure is harmless.
        let _ = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
        let _ = self.handle.join();
    }
}

/// Starts the hook thread; `window` is the wheel window that receives open / close messages.
pub fn start(config: HookOptions, window: HWND, settings: Arc<WheelConfig>) -> Result<HookThread> {
    publish(settings);
    let window = window.0 as isize;
    STALL_PENDING.store(config.stall_hook_once, Ordering::Relaxed);
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || thread_main(config, window, tx));
    match rx.recv() {
        Ok(Ok(thread_id)) => Ok(HookThread { thread_id, handle }),
        Ok(Err(code)) => Err(Error::from(HRESULT(code))),
        Err(_) => Err(Error::from_thread()),
    }
}

thread_local! {
    static STATE: RefCell<Option<HookState>> = const { RefCell::new(None) };
}

/// Runs `f` on the hook thread's state; `None` if there is none or it is already in use
/// (a hook callback re-entered while an effect held it).
fn with_state<R>(f: impl FnOnce(&mut HookState) -> R) -> Option<R> {
    STATE.with(|cell| {
        let mut guard = cell.try_borrow_mut().ok()?;
        guard.as_mut().map(f)
    })
}

struct HookState {
    trigger: Trigger,
    config: HookOptions,
    window: HWND,
    thread_id: u32,
    mouse_hook: Option<HHOOK>,
    keyboard_hook: Option<HHOOK>,
    effects: VecDeque<Effect>,
    /// Trace lines (`pass: ...`) printed by the effect loop, not inside the hook callback.
    notes: VecDeque<String>,
    /// Tick of the last event either hook saw.
    last_event: u32,
    hold_timer: usize,
    watchdog_timer: usize,
    /// A physical right press went to an elevated window; its release must pass untouched.
    passthrough_right: bool,
    /// A physical Escape press was swallowed; swallow its repeats and release too.
    swallow_escape: bool,
    own_integrity: u32,
    /// Top-level window under the cursor at the last right press (`HWND` bits): the target of
    /// the action.
    target: isize,
    /// (pid, what is known about the process, tick of the check)
    process_cache: Vec<(u32, ProcessInfo, u64)>,
}

impl HookState {
    fn origin(&self, extra: usize, injected: bool) -> Origin {
        if extra == MARKER {
            Origin::Own
        } else if injected && !self.config.accept_injected {
            Origin::Foreign
        } else {
            Origin::Physical
        }
    }

    /// Steps the machine and queues its effects. True if the event is to be swallowed.
    fn feed(&mut self, event: Event) -> bool {
        let step = self.trigger.step(event);
        if !step.effects.is_empty() {
            self.effects.extend(step.effects);
            // SAFETY: plain call to this thread's own queue.
            let _ = unsafe { PostThreadMessageW(self.thread_id, WM_EFFECTS, WPARAM(0), LPARAM(0)) };
        }
        step.verdict == Verdict::Swallow
    }

    fn on_mouse(&mut self, msg: u32, info: &MSLLHOOKSTRUCT) -> bool {
        // SAFETY: plain call.
        self.last_event = unsafe { GetTickCount() };
        let pos = Point::new(info.pt.x, info.pt.y);
        let time = info.time;
        let kind = match msg {
            WM_MOUSEMOVE => EventKind::Move { pos, time },
            WM_RBUTTONDOWN => EventKind::RightDown { pos, time },
            WM_RBUTTONUP => EventKind::RightUp { pos, time },
            WM_LBUTTONDOWN => EventKind::LeftDown { pos, time },
            WM_LBUTTONUP => EventKind::LeftUp { pos, time },
            _ => return false,
        };
        let origin = self.origin(info.dwExtraInfo, info.flags & LLMHF_INJECTED != 0);
        if origin == Origin::Physical {
            // The events that select classify the position with the arc the user sees.
            if matches!(kind, EventKind::RightUp { .. } | EventKind::LeftDown { .. }) {
                self.trigger.set_arc(current_arc());
            }
            match kind {
                // Every press while the wheel is closed decides afresh, so a release lost after
                // an elevated press (UAC, secure desktop) cannot leave the flag set.
                EventKind::RightDown { .. } if !self.trigger.is_wheel_open() => {
                    let settings = current();
                    if let Some(c) = &settings {
                        let trigger = c.options.trigger();
                        if self.trigger.settings() != trigger {
                            self.trigger = Trigger::new(trigger);
                        }
                    }
                    let root = root_window_at(info.pt);
                    self.target = root.0 as isize;
                    self.passthrough_right = self.should_pass(root, settings.as_deref());
                    if self.passthrough_right {
                        self.trigger.reset();
                        return false;
                    }
                }
                EventKind::RightUp { .. } if self.passthrough_right => {
                    self.passthrough_right = false;
                    return false;
                }
                _ => {}
            }
        }
        self.feed(Event { kind, origin })
    }

    fn on_key(&mut self, msg: u32, info: &KBDLLHOOKSTRUCT) -> bool {
        // SAFETY: plain call.
        self.last_event = unsafe { GetTickCount() };
        if info.vkCode != u32::from(VK_ESCAPE.0) {
            return false;
        }
        let origin = self.origin(info.dwExtraInfo, info.flags.0 & LLKHF_INJECTED.0 != 0);
        if origin != Origin::Physical {
            return false;
        }
        match msg {
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                if self.swallow_escape {
                    return true;
                }
                if !self.trigger.is_wheel_open() {
                    return false;
                }
                self.swallow_escape = self.feed(Event::physical(EventKind::Escape));
                self.swallow_escape
            }
            WM_KEYUP | WM_SYSKEYUP => std::mem::take(&mut self.swallow_escape),
            _ => false,
        }
    }

    fn note(&mut self, text: String) {
        self.notes.push_back(text);
        // SAFETY: plain call to this thread's own queue.
        let _ = unsafe { PostThreadMessageW(self.thread_id, WM_EFFECTS, WPARAM(0), LPARAM(0)) };
    }

    /// Whether the right button must reach the window `root` unchanged: it belongs to a process
    /// with a higher integrity level than this one (or one that cannot be inspected), so the
    /// hook could not replay input into it; or the program is on the exclusion list; or the
    /// window covers its whole monitor.
    fn should_pass(&mut self, root: HWND, config: Option<&WheelConfig>) -> bool {
        let Some(pid) = window_pid(root) else {
            return false;
        };
        let info = self.process_info(pid);
        if info.elevated {
            return true;
        }
        let Some(config) = config else {
            return false;
        };
        if config.options.is_excluded(&info.exe) {
            let name = info.exe.rsplit('\\').next().unwrap_or(&info.exe);
            self.note(format!("pass: excluded {name}"));
            return true;
        }
        if config.options.disable_in_fullscreen && is_fullscreen(root) {
            self.note("pass: fullscreen".to_string());
            return true;
        }
        false
    }

    fn process_info(&mut self, pid: u32) -> ProcessInfo {
        // SAFETY: plain call.
        let now = unsafe { GetTickCount64() };
        if let Some((_, info, at)) = self.process_cache.iter().find(|e| e.0 == pid)
            && now.wrapping_sub(*at) < ELEVATION_CACHE_MS
        {
            return info.clone();
        }
        let info = query_process(pid, self.own_integrity);
        if self.process_cache.len() >= ELEVATION_CACHE_MAX {
            self.process_cache.clear();
        }
        self.process_cache.retain(|e| e.0 != pid);
        self.process_cache.push((pid, info.clone(), now));
        info
    }

    fn start_timer(&mut self, ms: u32) {
        self.stop_hold_timer();
        // SAFETY: a thread timer (no window) of the calling thread.
        self.hold_timer = unsafe { SetTimer(None, 0, ms + HOLD_TIMER_SLACK_MS, None) };
    }

    fn stop_hold_timer(&mut self) {
        if self.hold_timer != 0 {
            // SAFETY: the id came from SetTimer on this thread.
            let _ = unsafe { KillTimer(None, self.hold_timer) };
            self.hold_timer = 0;
        }
    }

    fn install(&mut self) -> Result<()> {
        // SAFETY: the callbacks are `extern "system"` functions living for the whole program.
        unsafe {
            let module = GetModuleHandleW(None)?;
            self.mouse_hook = Some(SetWindowsHookExW(
                WH_MOUSE_LL,
                Some(mouse_proc),
                Some(module.into()),
                0,
            )?);
            self.keyboard_hook = Some(SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(keyboard_proc),
                Some(module.into()),
                0,
            )?);
        }
        // SAFETY: plain call.
        self.last_event = unsafe { GetTickCount() };
        Ok(())
    }

    fn uninstall(&mut self) {
        for hook in [self.mouse_hook.take(), self.keyboard_hook.take()]
            .into_iter()
            .flatten()
        {
            // SAFETY: the handle came from SetWindowsHookExW on this thread.
            let _ = unsafe { UnhookWindowsHookEx(hook) };
        }
    }

    /// The watchdog found the hooks dead (or missing): install them again from scratch.
    fn reinstall(&mut self) {
        let was_open = self.trigger.is_wheel_open();
        self.uninstall();
        self.stop_hold_timer();
        self.trigger.reset();
        self.effects.clear();
        self.passthrough_right = false;
        self.swallow_escape = false;
        if let Err(e) = self.install() {
            eprintln!("wheel: cannot reinstall the hooks: {e}");
            return;
        }
        if was_open {
            post_to_window(self.window, WM_WHEEL_CLOSE, 0, 0);
        }
        eprintln!("hook reinstalled");
    }

    fn check_watchdog(&mut self) {
        let dead = self.mouse_hook.is_none() || self.keyboard_hook.is_none();
        let stale = watchdog::last_input_tick()
            .is_some_and(|input| watchdog::is_stale(input, self.last_event));
        if dead || stale {
            self.reinstall();
        }
    }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: for HC_ACTION `lparam` points to an MSLLHOOKSTRUCT valid during the call.
        let info = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        if wparam.0 as u32 == WM_RBUTTONDOWN && STALL_PENDING.swap(false, Ordering::Relaxed) {
            eprintln!("test: stalling the mouse hook for {STALL_MS} ms");
            thread::sleep(Duration::from_millis(STALL_MS));
            // The event is not fed to the trigger: Windows has already passed it on.
            // SAFETY: passing the arguments on unchanged.
            return unsafe { CallNextHookEx(None, code, wparam, lparam) };
        }
        if with_state(|s| s.on_mouse(wparam.0 as u32, info)) == Some(true) {
            return LRESULT(1);
        }
    }
    // SAFETY: passing the arguments on unchanged.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: for HC_ACTION `lparam` points to a KBDLLHOOKSTRUCT valid during the call.
        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        if with_state(|s| s.on_key(wparam.0 as u32, info)) == Some(true) {
            return LRESULT(1);
        }
    }
    // SAFETY: passing the arguments on unchanged.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn thread_main(
    config: HookOptions,
    window: isize,
    ready: mpsc::Sender<std::result::Result<u32, i32>>,
) {
    let mut msg = MSG::default();
    // SAFETY: the first call creates this thread's message queue, so that
    // PostThreadMessage works as soon as the thread id is announced.
    let thread_id = unsafe {
        let _ = PeekMessageW(&mut msg, None, WM_USER, WM_USER, PM_NOREMOVE);
        GetCurrentThreadId()
    };
    let mut state = HookState {
        trigger: Trigger::new(current().map(|c| c.options.trigger()).unwrap_or_default()),
        config,
        window: HWND(window as *mut _),
        thread_id,
        mouse_hook: None,
        keyboard_hook: None,
        effects: VecDeque::new(),
        notes: VecDeque::new(),
        last_event: 0,
        hold_timer: 0,
        watchdog_timer: 0,
        passthrough_right: false,
        swallow_escape: false,
        own_integrity: own_integrity(),
        target: 0,
        process_cache: Vec::new(),
    };
    if let Err(e) = state.install() {
        let _ = ready.send(Err(e.code().0));
        return;
    }
    // SAFETY: a thread timer of this thread.
    state.watchdog_timer = unsafe { SetTimer(None, 0, watchdog::INTERVAL_MS, None) };
    STATE.with(|cell| *cell.borrow_mut() = Some(state));
    let _ = ready.send(Ok(thread_id));

    loop {
        // SAFETY: `msg` is a valid out buffer.
        let status = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if status.0 <= 0 {
            break;
        }
        match msg.message {
            WM_EFFECTS => run_effects(),
            WM_TIMER => on_timer(msg.wParam.0),
            _ => {}
        }
    }
    with_state(|s| {
        s.uninstall();
        s.stop_hold_timer();
    });
}

fn on_timer(id: usize) {
    with_state(|s| {
        if id == s.watchdog_timer {
            s.check_watchdog();
        } else if id == s.hold_timer {
            s.stop_hold_timer();
            // SAFETY: plain call.
            let time = unsafe { GetTickCount() };
            s.feed(Event::physical(EventKind::HoldElapsed { time }));
        }
    });
}

/// Runs the queued effects. The state is borrowed only briefly for each one: SendInput calls
/// the hook callbacks of this very thread.
fn run_effects() {
    while let Some(effect) = with_state(|s| s.effects.pop_front()).flatten() {
        run_effect(effect);
    }
    while let Some(note) = with_state(|s| s.notes.pop_front()).flatten() {
        eprintln!("{note}");
    }
}

fn run_effect(effect: Effect) {
    match effect {
        Effect::StartHoldTimer(ms) => {
            with_state(|s| s.start_timer(ms));
        }
        Effect::ReplayClick { down_pos, up_pos } => send_mouse(&[
            mouse_input(down_pos, MOUSEEVENTF_RIGHTDOWN),
            mouse_input(up_pos, MOUSEEVENTF_RIGHTUP),
        ]),
        Effect::ReplayDragStart {
            down_pos,
            current_pos,
        } => send_mouse(&[
            mouse_input(down_pos, MOUSEEVENTF_RIGHTDOWN),
            mouse_input(current_pos, MOUSE_EVENT_FLAGS(0)),
        ]),
        Effect::OpenWheel { center } => {
            let (Some(window), Some(config)) = (with_state(|s| s.window), current()) else {
                return;
            };
            let at = POINT {
                x: center.x,
                y: center.y,
            };
            let o = &config.options;
            let layout = layout_at(at, o.scale_pct, o.inner_sectors, o.outer_sectors);
            with_state(|s| s.trigger.set_layout(layout));
            post_to_window(
                window,
                WM_WHEEL_OPEN,
                center.x as isize as usize,
                center.y as isize,
            );
        }
        // The window stays open and follows the cursor by itself; nothing to do.
        Effect::EnterClickMode => {}
        Effect::Select(hit) => {
            let (Some((window, target)), Some(config)) =
                (with_state(|s| (s.window, s.target)), current())
            else {
                return;
            };
            print_line(&selection_text(hit, &config));
            post_to_window(window, WM_WHEEL_CLOSE, 0, 0);
            if let Some(item) = wheel_core::item_for_hit(hit, &config) {
                actions::run(&item.action, target, window.0 as isize);
            }
        }
        Effect::Cancel => {
            let Some(window) = with_state(|s| s.window) else {
                return;
            };
            print_line("cancelled");
            post_to_window(window, WM_WHEEL_CLOSE, 0, 0);
        }
    }
}

fn post_to_window(window: HWND, msg: u32, wparam: usize, lparam: isize) {
    // SAFETY: plain call; failure (window gone) only happens while quitting.
    let _ = unsafe { PostMessageW(Some(window), msg, WPARAM(wparam), LPARAM(lparam)) };
}

fn print_line(text: &str) {
    // Not `println!`: it panics when stdout is closed, and this runs in a background process.
    // Flushed explicitly: redirected to a file, the line would otherwise wait until exit.
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{text}");
    let _ = out.flush();
}

fn selection_text(hit: Hit, config: &WheelConfig) -> String {
    let name = |item: Option<&wheel_core::Item>| item.map_or("?", |i| i.name.as_str()).to_string();
    match hit {
        Hit::Outer(i) => format!(
            "selected: outer {} \"{}\"",
            i + 1,
            name(config.outer_item(usize::from(i)))
        ),
        Hit::Inner(i) => format!(
            "selected: inner {} \"{}\"",
            i + 1,
            name(config.inner_item(usize::from(i)))
        ),
        Hit::Sub(p, k) => format!(
            "selected: outer {} submenu {} \"{}\"",
            p + 1,
            k + 1,
            name(wheel_core::item_for_hit(hit, config))
        ),
        Hit::Center => "selected: center".to_string(),
    }
}

/// Position on the virtual screen as 0..=65535 (`MOUSEEVENTF_VIRTUALDESK`): the center of the
/// pixel `pos` of the span `origin..origin + extent`, which Windows maps back to that pixel.
fn normalize(pos: i32, origin: i32, extent: i32) -> i32 {
    let extent = i64::from(extent.max(1));
    let rel = i64::from(pos) - i64::from(origin);
    ((rel * 65536 + 32768) / extent).clamp(0, 65535) as i32
}

fn mouse_input(pos: Point, button: MOUSE_EVENT_FLAGS) -> INPUT {
    // SAFETY: plain metric queries.
    let (x, y, w, h) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: normalize(pos.x, x, w),
                dy: normalize(pos.y, y, h),
                mouseData: 0,
                dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK | button,
                time: 0,
                dwExtraInfo: MARKER,
            },
        },
    }
}

fn send_mouse(inputs: &[INPUT]) {
    // SAFETY: `inputs` is a valid slice of fully initialised INPUT structures.
    let sent = unsafe { SendInput(inputs, size_of::<INPUT>() as i32) };
    if sent as usize != inputs.len() {
        eprintln!(
            "wheel: SendInput injected {sent} of {} events",
            inputs.len()
        );
    }
}

fn own_integrity() -> u32 {
    // SAFETY: the pseudo handle of the current process needs no closing.
    process_integrity(unsafe { GetCurrentProcess() }).unwrap_or(MEDIUM_INTEGRITY)
}

/// What is known about the process of a window.
#[derive(Debug, Clone)]
struct ProcessInfo {
    /// Higher integrity than this process, or unreadable.
    elevated: bool,
    /// Full path of the image, empty if unreadable.
    exe: String,
}

fn query_process(pid: u32, own_integrity: u32) -> ProcessInfo {
    // SAFETY: the handle comes from OpenProcess and is closed here; the buffer outlives the call.
    unsafe {
        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return ProcessInfo {
                elevated: true,
                exe: String::new(),
            };
        };
        let level = process_integrity(process);
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let exe = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
        .map(|()| String::from_utf16_lossy(&buf[..len as usize]))
        .unwrap_or_default();
        let _ = CloseHandle(process);
        ProcessInfo {
            elevated: level.is_none_or(|l| l > own_integrity),
            exe,
        }
    }
}

/// The top-level window under `pt` (null if there is none).
fn root_window_at(pt: POINT) -> HWND {
    // SAFETY: plain window queries.
    unsafe {
        let hwnd = WindowFromPoint(pt);
        if hwnd.0.is_null() {
            return hwnd;
        }
        let root = GetAncestor(hwnd, GA_ROOT);
        if root.0.is_null() { hwnd } else { root }
    }
}

fn window_pid(hwnd: HWND) -> Option<u32> {
    if hwnd.0.is_null() {
        return None;
    }
    let mut pid = 0u32;
    // SAFETY: `pid` is a valid out buffer.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    (pid != 0).then_some(pid)
}

/// (left, top, right, bottom)
type Edges = (i32, i32, i32, i32);

/// Classes of the desktop and the taskbars.
pub fn is_shell_class(class: &str) -> bool {
    matches!(
        class,
        "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
    )
}

/// Class name of a window (empty if unreadable).
pub fn window_class(hwnd: HWND) -> String {
    let mut class = [0u16; 64];
    // SAFETY: the buffer is valid.
    let len = unsafe { GetClassNameW(hwnd, &mut class) }.max(0) as usize;
    String::from_utf16_lossy(&class[..len])
}

/// Whether a window with these edges and class counts as full screen on a monitor with these
/// edges: it covers the monitor (taskbar included), is not the desktop or a taskbar, and is not
/// a maximized window with a title bar (that is only maximized, even if the taskbar hides).
fn covers_monitor(
    window: Edges,
    monitor: Edges,
    class: &str,
    maximized: bool,
    framed: bool,
) -> bool {
    !is_shell_class(class)
        && !(maximized && framed)
        && window.0 <= monitor.0
        && window.1 <= monitor.1
        && window.2 >= monitor.2
        && window.3 >= monitor.3
}

fn is_fullscreen(root: HWND) -> bool {
    let mut rect = RECT::default();
    let mut monitor = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: the out buffers are valid; the monitor handle comes from MonitorFromWindow.
    let (ok, maximized, style) = unsafe {
        let ok = GetWindowRect(root, &mut rect).is_ok()
            && GetMonitorInfoW(
                MonitorFromWindow(root, MONITOR_DEFAULTTONEAREST),
                &mut monitor,
            )
            .as_bool();
        (
            ok,
            IsZoomed(root).as_bool(),
            GetWindowLongPtrW(root, GWL_STYLE) as u32,
        )
    };
    let m = monitor.rcMonitor;
    ok && covers_monitor(
        (rect.left, rect.top, rect.right, rect.bottom),
        (m.left, m.top, m.right, m.bottom),
        &window_class(root),
        maximized,
        style & WS_CAPTION.0 == WS_CAPTION.0,
    )
}

/// Integrity level (RID, e.g. 0x2000 medium, 0x3000 high) of a process, if it can be read.
fn process_integrity(process: HANDLE) -> Option<u32> {
    // SAFETY: the token handle comes from OpenProcessToken and is closed here; the buffer is
    // sized by the first GetTokenInformation call, 8-byte aligned for the structure inside it,
    // and the SID it points to lives in the same buffer.
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(process, TOKEN_QUERY, &mut token).ok()?;
        let mut len = 0u32;
        let _ = GetTokenInformation(token, TokenIntegrityLevel, None, 0, &mut len);
        let mut buf = vec![0u64; (len as usize).div_ceil(8)];
        let level = if len > 0
            && GetTokenInformation(
                token,
                TokenIntegrityLevel,
                Some(buf.as_mut_ptr().cast()),
                len,
                &mut len,
            )
            .is_ok()
        {
            let label = &*(buf.as_ptr() as *const TOKEN_MANDATORY_LABEL);
            let sid = label.Label.Sid;
            let count = *GetSidSubAuthorityCount(sid);
            (count > 0).then(|| *GetSidSubAuthority(sid, u32::from(count) - 1))
        } else {
            None
        };
        let _ = CloseHandle(token);
        level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_maps_pixels_to_their_center() {
        // Single 1920 px monitor: pixel p -> floor(norm * 1920 / 65536) must give p back.
        for p in [0, 1, 959, 1000, 1918, 1919] {
            let n = normalize(p, 0, 1920);
            assert!((0..=65535).contains(&n));
            assert_eq!(i64::from(n) * 1920 / 65536, i64::from(p), "p={p}");
        }
    }

    #[test]
    fn normalize_handles_negative_origin_and_clamps() {
        // Monitor left of the primary one: virtual screen -1920..1920.
        let left_edge = normalize(-1920, -1920, 3840);
        assert!(left_edge < 20);
        let right_edge = normalize(1919, -1920, 3840);
        assert!(right_edge > 65500);
        assert_eq!(normalize(-5000, -1920, 3840), 0);
        assert_eq!(normalize(9000, -1920, 3840), 65535);
    }

    #[test]
    fn selection_lines() {
        assert_eq!(
            selection_text(Hit::Outer(3), &WheelConfig::default()),
            "selected: outer 4 \"Zrzut ekranu\""
        );
        assert_eq!(
            selection_text(Hit::Inner(9), &WheelConfig::default()),
            "selected: inner 10 \"Cofnij\""
        );
        assert_eq!(
            selection_text(Hit::Center, &WheelConfig::default()),
            "selected: center"
        );
    }

    const MONITOR: Edges = (0, 0, 1920, 1080);

    #[test]
    fn fullscreen_detection() {
        // Exactly the monitor, and larger than it (borderless games overshoot).
        assert!(covers_monitor(
            MONITOR,
            MONITOR,
            "UnityWndClass",
            false,
            false
        ));
        assert!(covers_monitor(
            (-8, -8, 1928, 1088),
            MONITOR,
            "Chrome_WidgetWin_1",
            false,
            false
        ));
        // A maximized window with a title bar is not full screen, even when the taskbar hides
        // and the window covers the whole monitor; a frameless one (F11, games) is.
        assert!(!covers_monitor(MONITOR, MONITOR, "Notepad", true, true));
        assert!(covers_monitor(MONITOR, MONITOR, "chrome", true, false));
        // Maximized: the taskbar stays visible.
        assert!(!covers_monitor(
            (-8, -8, 1928, 1040),
            MONITOR,
            "Notepad",
            true,
            true
        ));
        // Plain window, and a window on another monitor.
        assert!(!covers_monitor(
            (100, 100, 800, 600),
            MONITOR,
            "Notepad",
            false,
            true
        ));
        assert!(!covers_monitor(
            (1920, 0, 3840, 1080),
            MONITOR,
            "Notepad",
            false,
            true
        ));
        // The desktop and the taskbar cover the monitor but are not full screen windows.
        for class in [
            "Progman",
            "WorkerW",
            "Shell_TrayWnd",
            "Shell_SecondaryTrayWnd",
        ] {
            assert!(
                !covers_monitor(MONITOR, MONITOR, class, false, false),
                "{class}"
            );
        }
    }
}
