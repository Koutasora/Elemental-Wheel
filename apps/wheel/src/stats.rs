//! Hidden `--stats` measurements: wheel open timing and process memory/handle counts, written
//! to the log. Everything is a cheap no-op until [`enable`] is called.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX};
use windows::Win32::System::Threading::{
    GR_GDIOBJECTS, GR_USEROBJECTS, GetCurrentProcess, GetGuiResources, GetProcessHandleCount,
};

static ENABLED: AtomicBool = AtomicBool::new(false);
/// Nanoseconds (plus one) since `base()` of the last `mark_open_request`; 0 when there is none.
static OPEN_REQUEST_NS: AtomicU64 = AtomicU64::new(0);
static OPEN_COUNT: AtomicU64 = AtomicU64::new(0);

fn base() -> Instant {
    static BASE: OnceLock<Instant> = OnceLock::new();
    *BASE.get_or_init(Instant::now)
}

/// Why an open took the time it did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenKind {
    /// The renderer was kept.
    Same,
    /// The renderer was rebuilt after a configuration change.
    Reload,
    /// The renderer was rebuilt because the layout (monitor DPI) changed.
    Dpi,
}

/// Turns the measurements on; called once at the start of `--run --stats`.
pub fn enable() {
    base();
    ENABLED.store(true, Ordering::Relaxed);
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Called on the hook thread just before the open request is posted.
pub fn mark_open_request() {
    if enabled() {
        let ns = base().elapsed().as_nanos() as u64 + 1;
        OPEN_REQUEST_NS.store(ns, Ordering::Relaxed);
    }
}

/// The start of handling an open request on the window thread.
pub struct OpenStart {
    at: Instant,
    queue_ms: Option<f64>,
}

/// Call first thing when `WM_WHEEL_OPEN` arrives. `None` when stats are off.
pub fn open_started() -> Option<OpenStart> {
    if !enabled() {
        return None;
    }
    let at = Instant::now();
    let marked = OPEN_REQUEST_NS.swap(0, Ordering::Relaxed);
    let queue_ms = (marked != 0).then(|| {
        let now_ns = at.duration_since(base()).as_nanos() as u64 + 1;
        now_ns.saturating_sub(marked) as f64 / 1e6
    });
    Some(OpenStart { at, queue_ms })
}

/// Logs one line after a successful open.
pub fn open_finished(start: OpenStart, kind: OpenKind) {
    let work_ms = start.at.elapsed().as_secs_f64() * 1e3;
    let n = OPEN_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    let kind = match kind {
        _ if n == 1 => "first",
        OpenKind::Same => "next",
        OpenKind::Reload => "reload",
        OpenKind::Dpi => "dpi",
    };
    match start.queue_ms {
        Some(queue_ms) => wheel_core::log_line!(
            "stats open n={n} kind={kind} total_ms={:.3} queue_ms={queue_ms:.3} work_ms={work_ms:.3}",
            queue_ms + work_ms
        ),
        None => wheel_core::log_line!("stats open n={n} kind={kind} work_ms={work_ms:.3}"),
    }
}

/// Logs memory, handle and GDI/USER object counts of this process.
pub fn log_memory() {
    let mut counters = PROCESS_MEMORY_COUNTERS_EX {
        cb: size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        ..Default::default()
    };
    let mut handles = 0u32;
    // SAFETY: the pseudo handle is always valid; the buffers are valid out buffers and `cb`
    // matches the structure passed (EX starts with the plain PROCESS_MEMORY_COUNTERS layout).
    let (memory, handle_count, gdi, user) = unsafe {
        let process = GetCurrentProcess();
        (
            GetProcessMemoryInfo(
                process,
                (&mut counters as *mut PROCESS_MEMORY_COUNTERS_EX).cast(),
                counters.cb,
            ),
            GetProcessHandleCount(process, &mut handles),
            GetGuiResources(process, GR_GDIOBJECTS),
            GetGuiResources(process, GR_USEROBJECTS),
        )
    };
    if let Err(e) = memory {
        wheel_core::log_line!("stats mem: cannot read memory info: {e}");
        return;
    }
    if let Err(e) = handle_count {
        wheel_core::log_line!("stats mem: cannot read handle count: {e}");
        return;
    }
    wheel_core::log_line!(
        "stats mem private_kb={} working_set_kb={} peak_working_set_kb={} handles={handles} gdi={gdi} user={user}",
        counters.PrivateUsage / 1024,
        counters.WorkingSetSize / 1024,
        counters.PeakWorkingSetSize / 1024
    );
}
