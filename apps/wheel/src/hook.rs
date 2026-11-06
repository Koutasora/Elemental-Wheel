//! Global low-level mouse and keyboard hooks on a dedicated thread with its own message loop.
//!
//! The callbacks only classify the event, step the `Trigger` and queue its effects; the effects
//! (timer, SendInput, window messages, console output) run later in the thread's message loop,
//! because Windows removes a low-level hook whose callback takes too long. The `Trigger` and all
//! other state live in a thread-local that only this thread touches, so there are no locks.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::io::Write;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use wheel_core::{
    Effect, Event, EventKind, Hit, INNER_LABELS, Origin, Point, Trigger, TriggerSettings, Verdict,
    outer_labels,
};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Security::{
    GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TOKEN_MANDATORY_LABEL,
    TOKEN_QUERY, TokenIntegrityLevel,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::SystemInformation::{GetTickCount, GetTickCount64};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentThreadId, OpenProcess, OpenProcessToken,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_MOUSE, MOUSE_EVENT_FLAGS, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_MOVE,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK, MOUSEINPUT, SendInput,
    VK_ESCAPE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GA_ROOT, GetAncestor, GetMessageW, GetSystemMetrics, GetWindowThreadProcessId,
    HC_ACTION, HHOOK, KBDLLHOOKSTRUCT, KillTimer, LLKHF_INJECTED, LLMHF_INJECTED, MSG,
    MSLLHOOKSTRUCT, PM_NOREMOVE, PeekMessageW, PostMessageW, PostThreadMessageW,
    SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SetTimer,
    SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_APP, WM_KEYDOWN,
    WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_QUIT, WM_RBUTTONDOWN, WM_RBUTTONUP,
    WM_SYSKEYDOWN, WM_SYSKEYUP, WM_TIMER, WM_USER, WindowFromPoint,
};
use windows::core::{Error, HRESULT, Result};

use crate::watchdog;
use crate::window::{WM_WHEEL_CLOSE, WM_WHEEL_OPEN, layout_at};

/// `dwExtraInfo` of the input this program injects ("ELWH").
pub const MARKER: usize = 0x454C_5748;

/// Posted to the hook thread: run the queued effects.
const WM_EFFECTS: u32 = WM_APP + 1;
/// The timer fires `HoldElapsed` with `GetTickCount`, which is coarser than the event times, so
/// it is started a little late to make sure the hold time really has elapsed.
const HOLD_TIMER_SLACK_MS: u32 = 16;
/// How long the integrity level of a process is remembered.
const ELEVATION_CACHE_MS: u64 = 5000;
const ELEVATION_CACHE_MAX: usize = 64;
/// Integrity level assumed for this process if it cannot be read (medium).
const MEDIUM_INTEGRITY: u32 = 0x2000;

/// What the hook thread needs to know.
#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub trigger: TriggerSettings,
    pub scale_pct: u16,
    pub sectors: u8,
    pub accept_injected: bool,
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
pub fn start(config: Config, window: HWND) -> Result<HookThread> {
    let window = window.0 as isize;
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
    config: Config,
    window: HWND,
    thread_id: u32,
    mouse_hook: Option<HHOOK>,
    keyboard_hook: Option<HHOOK>,
    effects: VecDeque<Effect>,
    /// Tick of the last event either hook saw.
    last_event: u32,
    hold_timer: usize,
    watchdog_timer: usize,
    /// A physical right press went to an elevated window; its release must pass untouched.
    passthrough_right: bool,
    /// A physical Escape press was swallowed; swallow its repeats and release too.
    swallow_escape: bool,
    own_integrity: u32,
    /// (pid, elevated, tick of the check)
    elevation_cache: Vec<(u32, bool, u64)>,
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
            match kind {
                EventKind::RightDown { .. }
                    if !self.trigger.is_wheel_open() && self.is_elevated_target(info.pt) =>
                {
                    self.passthrough_right = true;
                    self.trigger.reset();
                    return false;
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

    /// True if the window under `pt` belongs to a process with a higher integrity level than
    /// this one (or one that cannot be inspected): the hook could not replay input into it.
    fn is_elevated_target(&mut self, pt: POINT) -> bool {
        // SAFETY: plain window queries; the handles come from the calls.
        let pid = unsafe {
            let hwnd = WindowFromPoint(pt);
            if hwnd.0.is_null() {
                return false;
            }
            let root = GetAncestor(hwnd, GA_ROOT);
            let mut pid = 0u32;
            GetWindowThreadProcessId(if root.0.is_null() { hwnd } else { root }, Some(&mut pid));
            pid
        };
        if pid == 0 {
            return false;
        }
        // SAFETY: plain call.
        let now = unsafe { GetTickCount64() };
        if let Some(&(_, elevated, at)) = self.elevation_cache.iter().find(|e| e.0 == pid)
            && now.wrapping_sub(at) < ELEVATION_CACHE_MS
        {
            return elevated;
        }
        let elevated = match process_integrity_by_pid(pid) {
            Some(level) => level > self.own_integrity,
            None => true,
        };
        if self.elevation_cache.len() >= ELEVATION_CACHE_MAX {
            self.elevation_cache.clear();
        }
        self.elevation_cache.retain(|e| e.0 != pid);
        self.elevation_cache.push((pid, elevated, now));
        elevated
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

fn thread_main(config: Config, window: isize, ready: mpsc::Sender<std::result::Result<u32, i32>>) {
    let mut msg = MSG::default();
    // SAFETY: the first call creates this thread's message queue, so that
    // PostThreadMessage works as soon as the thread id is announced.
    let thread_id = unsafe {
        let _ = PeekMessageW(&mut msg, None, WM_USER, WM_USER, PM_NOREMOVE);
        GetCurrentThreadId()
    };
    let mut state = HookState {
        trigger: Trigger::new(config.trigger),
        config,
        window: HWND(window as *mut _),
        thread_id,
        mouse_hook: None,
        keyboard_hook: None,
        effects: VecDeque::new(),
        last_event: 0,
        hold_timer: 0,
        watchdog_timer: 0,
        passthrough_right: false,
        swallow_escape: false,
        own_integrity: own_integrity(),
        elevation_cache: Vec::new(),
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
            let Some((window, config)) = with_state(|s| (s.window, s.config)) else {
                return;
            };
            let at = POINT {
                x: center.x,
                y: center.y,
            };
            let layout = layout_at(at, config.scale_pct, config.sectors);
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
            let Some((window, sectors)) = with_state(|s| (s.window, s.config.sectors)) else {
                return;
            };
            print_line(&selection_text(hit, sectors));
            post_to_window(window, WM_WHEEL_CLOSE, 0, 0);
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
    let _ = writeln!(std::io::stdout(), "{text}");
}

fn selection_text(hit: Hit, sectors: u8) -> String {
    match hit {
        Hit::Outer(i) => format!(
            "selected: outer {i} \"{}\"",
            outer_labels(sectors).get(usize::from(i)).unwrap_or(&"?")
        ),
        Hit::Inner(i) => format!(
            "selected: inner {i} \"{}\"",
            INNER_LABELS.get(usize::from(i)).unwrap_or(&"?")
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

fn process_integrity_by_pid(pid: u32) -> Option<u32> {
    // SAFETY: the handle comes from OpenProcess and is closed here.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let level = process_integrity(process);
        let _ = CloseHandle(process);
        level
    }
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
            selection_text(Hit::Outer(3), 12),
            "selected: outer 3 \"Zrzut ekranu\""
        );
        assert_eq!(
            selection_text(Hit::Inner(9), 12),
            "selected: inner 9 \"Cofnij\""
        );
        assert_eq!(selection_text(Hit::Center, 12), "selected: center");
    }
}
