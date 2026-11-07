//! Log file shared by `wheel.exe` and `wheel-editor.exe` (std only).
//!
//! Lines look like `2026-11-07 12:00:00.123Z [1234] text` (UTC). The file is capped: when it
//! grows over [`MAX_LOG_BYTES`] it is renamed to `<stem>.old.log` (replacing an older one) and
//! a fresh file is started. Logging never panics and never fails the caller.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Size limit of the log file (1 MB).
pub const MAX_LOG_BYTES: u64 = 1_048_576;

/// Appends lines to a file with a size limit.
pub struct Logger {
    path: PathBuf,
    file: Option<File>,
    /// Bytes in the current file, tracked in memory.
    size: u64,
    limit: u64,
    /// A rotation failed (e.g. another process holds the file open): the current file keeps
    /// growing past the limit and rotation is not tried again until the next start.
    rotation_failed: bool,
}

impl Logger {
    /// Opens (creating the parent directory if needed) the file for appending, rotating it
    /// first when it is already over [`MAX_LOG_BYTES`].
    pub fn open(path: &Path) -> std::io::Result<Logger> {
        Self::open_with_limit(path, MAX_LOG_BYTES)
    }

    fn open_with_limit(path: &Path, limit: u64) -> std::io::Result<Logger> {
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            fs::create_dir_all(dir)?;
        }
        let mut logger = Logger {
            path: path.to_path_buf(),
            file: None,
            size: 0,
            limit,
            rotation_failed: false,
        };
        let existing = fs::metadata(path).map_or(0, |m| m.len());
        if existing > limit {
            logger.rotate()?;
        } else {
            logger.file = Some(OpenOptions::new().create(true).append(true).open(path)?);
            logger.size = existing;
        }
        Ok(logger)
    }

    /// Appends one line (a newline is added) and flushes it. Rotates first when the line
    /// would make the file exceed the limit.
    pub fn write_line(&mut self, line: &str) -> std::io::Result<()> {
        let bytes = line.len() as u64 + 1;
        if !self.rotation_failed
            && (self.file.is_none() || (self.size > 0 && self.size + bytes > self.limit))
        {
            self.rotate()?;
        }
        let Some(file) = self.file.as_mut() else {
            return Err(std::io::Error::other("log file is not open"));
        };
        writeln!(file, "{line}")?;
        file.flush()?;
        self.size += bytes;
        Ok(())
    }

    fn rotate(&mut self) -> std::io::Result<()> {
        // The handle must be closed before the rename on Windows.
        self.file = None;
        // The rename replaces an older file; nothing is removed beforehand, so a failure loses
        // nothing and the current file stays in use.
        if self.path.exists() && fs::rename(&self.path, old_path(&self.path)).is_err() {
            self.rotation_failed = true;
            self.file = Some(
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&self.path)?,
            );
            self.size = fs::metadata(&self.path).map_or(0, |m| m.len());
            return Ok(());
        }
        self.file = Some(File::create(&self.path)?);
        self.size = 0;
        Ok(())
    }
}

fn old_path(path: &Path) -> PathBuf {
    let stem = path
        .file_stem()
        .map_or_else(|| "log".to_string(), |s| s.to_string_lossy().into_owned());
    path.with_file_name(format!("{stem}.old.log"))
}

/// `YYYY-MM-DD HH:MM:SS.mmmZ` (UTC) for a time given in milliseconds since the Unix epoch.
pub fn format_timestamp(since_epoch_ms: u64) -> String {
    let secs = since_epoch_ms / 1000;
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}.{:03}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60,
        since_epoch_ms % 1000
    )
}

/// Date for a day count since 1970-01-01 (proleptic Gregorian calendar).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// Full log line: timestamp, process id and text.
pub fn format_line(since_epoch_ms: u64, pid: u32, text: &str) -> String {
    format!("{} [{pid}] {text}", format_timestamp(since_epoch_ms))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

static LOGGER: Mutex<Option<Logger>> = Mutex::new(None);

/// Opens the process-wide log file. Only the first successful call has an effect; later
/// calls do nothing and return `Ok`. Until it succeeds, [`line`] writes to stderr only.
pub fn init(path: &Path) -> std::io::Result<()> {
    let mut guard = LOGGER.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        *guard = Some(Logger::open(path)?);
    }
    Ok(())
}

/// `%LOCALAPPDATA%\Elemental-Wheel\<file_name>`; `None` when `LOCALAPPDATA` is not set.
pub fn default_path(file_name: &str) -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    Some(Path::new(&base).join("Elemental-Wheel").join(file_name))
}

/// Logs one line to the global file (if initialised) and to stderr. Never panics, ignores
/// I/O errors.
pub fn line(text: &str) {
    let stamped = format_line(now_ms(), std::process::id(), text);
    if let Some(logger) = LOGGER.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        let _ = logger.write_line(&stamped);
    }
    // Not `eprintln!`: it panics when stderr is closed.
    let _ = writeln!(std::io::stderr().lock(), "{text}");
}

/// `log_line!("format {}", args)` formats and calls [`line`].
#[macro_export]
macro_rules! log_line {
    ($($arg:tt)*) => {
        $crate::log::line(&::std::format!($($arg)*))
    };
}

/// Text of a panic report.
pub fn format_panic(
    thread: &str,
    location: Option<(&str, u32, u32)>,
    payload: &(dyn std::any::Any + Send),
) -> String {
    let message = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("Box<dyn Any>");
    match location {
        Some((file, line, col)) => {
            format!("panic in thread '{thread}' at {file}:{line}:{col}: {message}")
        }
        None => format!("panic in thread '{thread}': {message}"),
    }
}

/// Makes panics go to the log, then to the previous hook.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let location = info.location().map(|l| (l.file(), l.line(), l.column()));
        line(&format_panic(
            thread.name().unwrap_or("<unnamed>"),
            location,
            info.payload(),
        ));
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unique empty folder under the system temp dir, removed on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> TempDir {
            let dir =
                std::env::temp_dir().join(format!("wheel-core-log-{}-{name}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn timestamps() {
        assert_eq!(format_timestamp(0), "1970-01-01 00:00:00.000Z");
        // 2000-02-29 12:34:56.789 UTC
        assert_eq!(
            format_timestamp(951_827_696_789),
            "2000-02-29 12:34:56.789Z"
        );
        // 2026-11-07 00:00:00 UTC
        assert_eq!(
            format_timestamp(1_794_009_600_000),
            "2026-11-07 00:00:00.000Z"
        );
        // 2026-11-07 23:59:59.999 UTC
        assert_eq!(
            format_timestamp(1_794_095_999_999),
            "2026-11-07 23:59:59.999Z"
        );
    }

    #[test]
    fn line_format() {
        assert_eq!(
            format_line(0, 42, "hello"),
            "1970-01-01 00:00:00.000Z [42] hello"
        );
    }

    #[test]
    fn appends_lines_and_creates_parent_dir() {
        let tmp = TempDir::new("append");
        let path = tmp.0.join("sub").join("dir").join("t.log");
        let mut log = Logger::open(&path).unwrap();
        log.write_line("one").unwrap();
        drop(log);
        let mut log = Logger::open(&path).unwrap();
        log.write_line("two").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "one\ntwo\n");
    }

    #[test]
    fn a_failed_rotation_keeps_appending_to_the_current_file() {
        let dir = TempDir::new("failed-rotation");
        let path = dir.0.join("w.log");
        // A directory in the way makes the rename fail.
        fs::create_dir(dir.0.join("w.old.log")).unwrap();
        fs::write(&path, "x".repeat(30)).unwrap();
        let mut log = Logger::open_with_limit(&path, 20).unwrap();
        for i in 0..5 {
            log.write_line(&format!("line {i}")).unwrap();
        }
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.starts_with(&"x".repeat(30)));
        assert!(
            text.contains(
                "line 0
"
            ) && text.contains(
                "line 4
"
            )
        );
        assert!(dir.0.join("w.old.log").is_dir());
    }

    #[test]
    fn rotates_at_open_when_over_the_limit() {
        let tmp = TempDir::new("open");
        let path = tmp.0.join("t.log");
        fs::write(&path, "x".repeat(21)).unwrap();
        let mut log = Logger::open_with_limit(&path, 20).unwrap();
        log.write_line("fresh").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "fresh\n");
        assert_eq!(
            fs::read_to_string(tmp.0.join("t.old.log")).unwrap(),
            "x".repeat(21)
        );
    }

    #[test]
    fn keeps_a_file_at_the_limit_at_open() {
        let tmp = TempDir::new("open-at");
        let path = tmp.0.join("t.log");
        fs::write(&path, "x".repeat(20)).unwrap();
        Logger::open_with_limit(&path, 20).unwrap();
        assert!(!tmp.0.join("t.old.log").exists());
    }

    #[test]
    fn rotates_while_writing_and_replaces_the_old_file() {
        let tmp = TempDir::new("write");
        let path = tmp.0.join("t.log");
        let old = tmp.0.join("t.old.log");
        fs::write(&old, "ancient\n").unwrap();
        let mut log = Logger::open_with_limit(&path, 20).unwrap();
        log.write_line("aaaaaaaaaa").unwrap(); // 11 bytes
        log.write_line("bbbbbbbbbb").unwrap(); // would reach 22: rotate first
        assert_eq!(fs::read_to_string(&old).unwrap(), "aaaaaaaaaa\n");
        assert_eq!(fs::read_to_string(&path).unwrap(), "bbbbbbbbbb\n");
        log.write_line("c").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "bbbbbbbbbb\nc\n");
        // A second rotation replaces the older file again.
        log.write_line("dddddddddd").unwrap();
        assert_eq!(fs::read_to_string(&old).unwrap(), "bbbbbbbbbb\nc\n");
        assert_eq!(fs::read_to_string(&path).unwrap(), "dddddddddd\n");
    }

    #[test]
    fn a_long_line_in_an_empty_file_is_not_rotated() {
        let tmp = TempDir::new("long");
        let path = tmp.0.join("t.log");
        let mut log = Logger::open_with_limit(&path, 5).unwrap();
        log.write_line("much longer than the limit").unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "much longer than the limit\n"
        );
        assert!(!tmp.0.join("t.old.log").exists());
    }

    #[test]
    fn panic_messages() {
        let loc = Some(("src/a.rs", 3, 9));
        assert_eq!(
            format_panic("main", loc, &"boom"),
            "panic in thread 'main' at src/a.rs:3:9: boom"
        );
        assert_eq!(
            format_panic("action", loc, &String::from("bad 5")),
            "panic in thread 'action' at src/a.rs:3:9: bad 5"
        );
        assert_eq!(
            format_panic("t", loc, &42_u32),
            "panic in thread 't' at src/a.rs:3:9: Box<dyn Any>"
        );
        assert_eq!(format_panic("t", None, &"x"), "panic in thread 't': x");
    }
}
