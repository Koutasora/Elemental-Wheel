//! Panic guard for the places where a panic must not unwind: `extern "system"` callbacks (it
//! would abort the process) and the worker threads. The panic message and location are already
//! logged by the panic hook (`wheel_core::log::install_panic_hook`); the guard adds one line
//! saying where it was caught and how the program recovers.

use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicU8, Ordering};

use crate::args::PanicTest;

/// Runs `f`. If it panics, logs `"<place>: panic caught, <recovery>"` and returns `None`.
pub fn try_run<R>(place: &str, recovery: &str, f: impl FnOnce() -> R) -> Option<R> {
    match panic::catch_unwind(AssertUnwindSafe(f)) {
        Ok(value) => Some(value),
        Err(_) => {
            wheel_core::log_line!("{place}: panic caught, {recovery}");
            None
        }
    }
}

/// Like [`try_run`], but returns `fallback` after a panic.
pub fn run<R>(place: &str, recovery: &str, fallback: R, f: impl FnOnce() -> R) -> R {
    try_run(place, recovery, f).unwrap_or(fallback)
}

/// The armed hidden test panic (`--test-panic`): 0 = none, otherwise `PanicTest as u8 + 1`.
static ARMED: AtomicU8 = AtomicU8::new(0);

/// Arms the test panic for the first call of [`fire`] with the same `kind`.
pub fn arm(kind: Option<PanicTest>) {
    ARMED.store(kind.map_or(0, |k| k as u8 + 1), Ordering::Relaxed);
}

/// Panics once if `kind` is the armed test panic; the flag is cleared by that first use.
pub fn fire(kind: PanicTest) {
    let armed = kind as u8 + 1;
    if ARMED
        .compare_exchange(armed, 0, Ordering::Relaxed, Ordering::Relaxed)
        .is_ok()
    {
        panic!("test panic ({kind:?})");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Runs `f` with the panic output silenced. The panic hook is global, so tests that
    /// swap it take turns.
    fn silenced<R>(f: impl FnOnce() -> R) -> R {
        static HOOK: Mutex<()> = Mutex::new(());
        let _turn = HOOK.lock().unwrap_or_else(|e| e.into_inner());
        let previous = panic::take_hook();
        panic::set_hook(Box::new(|_| {}));
        let result = f();
        panic::set_hook(previous);
        result
    }

    #[test]
    fn returns_the_value_without_panic() {
        assert_eq!(try_run("t", "r", || 7), Some(7));
        assert_eq!(run("t", "r", 0, || 7), 7);
    }

    #[test]
    fn returns_the_fallback_on_panic() {
        silenced(|| {
            assert_eq!(try_run::<i32>("t", "r", || panic!("boom")), None);
            assert_eq!(run("t", "r", -1, || -> i32 { panic!("boom") }), -1);
        });
    }

    #[test]
    fn does_not_propagate_the_panic() {
        let survived =
            silenced(|| panic::catch_unwind(|| run("t", "r", (), || panic!("boom"))).is_ok());
        assert!(survived);
    }

    #[test]
    fn test_panic_fires_once_for_the_armed_kind() {
        // The only test touching ARMED, so parallel tests cannot interfere.
        arm(Some(PanicTest::Window));
        fire(PanicTest::Hook);
        fire(PanicTest::Action);
        let first = silenced(|| try_run("t", "r", || fire(PanicTest::Window)));
        assert_eq!(first, None);
        assert_eq!(try_run("t", "r", || fire(PanicTest::Window)), Some(()));
        arm(None);
    }
}
