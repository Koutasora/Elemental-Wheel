//! Hook watchdog: Windows silently removes a low-level hook that times out. If the system saw
//! input that neither hook did, the hooks are dead and must be installed again.

use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

/// How often the hook thread runs the check.
pub const INTERVAL_MS: u32 = 5000;
/// The hook is considered dead when system input is newer than its last event by more than this.
const MAX_LAG_MS: i32 = 1000;

/// True when `last_input` (system tick of the last input) is more than `MAX_LAG_MS` newer than
/// `last_event` (tick of the last hook event). Both are `GetTickCount` values and wrap around.
pub fn is_stale(last_input: u32, last_event: u32) -> bool {
    last_input.wrapping_sub(last_event) as i32 > MAX_LAG_MS
}

/// Tick of the last input of the whole system.
pub fn last_input_tick() -> Option<u32> {
    let mut info = LASTINPUTINFO {
        cbSize: size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    // SAFETY: `info` is a valid out buffer with `cbSize` set.
    unsafe { GetLastInputInfo(&mut info) }
        .as_bool()
        .then_some(info.dwTime)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_only_when_input_is_clearly_newer() {
        assert!(!is_stale(1000, 1000));
        assert!(!is_stale(1500, 1000));
        assert!(!is_stale(2000, 1000));
        assert!(is_stale(2001, 1000));
        // The hook saw the event after the input: not stale.
        assert!(!is_stale(1000, 5000));
    }

    #[test]
    fn handles_tick_wraparound() {
        assert!(is_stale(500, u32::MAX - 700));
        assert!(!is_stale(100, u32::MAX - 100));
        assert!(!is_stale(u32::MAX - 100, 100));
    }
}
