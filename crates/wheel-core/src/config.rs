//! The user's configuration (`config.json`): model, validation, defaults, loading and saving.

use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize};

use crate::geometry::{
    DEFAULT_OUTER_SECTORS, INNER_SECTORS, MAX_OUTER_SECTORS, MIN_OUTER_SECTORS, clamp_outer_sectors,
};
use crate::keys::parse_shortcut;
use crate::layout::{DEFAULT_SCALE_PCT, MAX_SCALE_PCT, MIN_SCALE_PCT, clamp_scale_pct};
use crate::trigger::{
    DEFAULT_HOLD_MS, DEFAULT_MOVE_PX, MAX_HOLD_MS, MAX_MOVE_PX, MIN_HOLD_MS, MIN_MOVE_PX,
    TriggerSettings, clamp_hold_ms, clamp_move_px,
};

/// The only schema version this program reads.
pub const SCHEMA_VERSION: u32 = 1;
/// Maximum number of items in a ring.
pub const MAX_RING_ITEMS: usize = 12;
/// Maximum number of items in a submenu.
pub const MAX_SUBMENU_ITEMS: usize = 5;

/// Color theme choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeChoice {
    System,
    Light,
    Dark,
}

/// How items are shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewMode {
    Names,
    Icons,
}

/// An edit command sent to the window the user works in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditCommand {
    Cut,
    Copy,
    Paste,
    PastePlain,
    Delete,
    Undo,
    Redo,
    SelectAll,
    Save,
    Find,
    Rename,
    Properties,
}

impl EditCommand {
    /// The keyboard shortcut that performs the command.
    pub fn shortcut(self) -> &'static str {
        match self {
            EditCommand::Cut => "Ctrl+X",
            EditCommand::Copy => "Ctrl+C",
            EditCommand::Paste => "Ctrl+V",
            EditCommand::PastePlain => "Ctrl+Shift+V",
            EditCommand::Delete => "Delete",
            EditCommand::Undo => "Ctrl+Z",
            EditCommand::Redo => "Ctrl+Y",
            EditCommand::SelectAll => "Ctrl+A",
            EditCommand::Save => "Ctrl+S",
            EditCommand::Find => "Ctrl+F",
            EditCommand::Rename => "F2",
            EditCommand::Properties => "Alt+Enter",
        }
    }
}

/// A system action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemAction {
    Lock,
    CloseWindow,
    MinimizeWindow,
    ShowDesktop,
    VolumeMute,
    VolumeUp,
    VolumeDown,
    Screenshot,
}

/// What a sector does when chosen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    /// Empty sector: choosing it does nothing.
    None,
    Program {
        path: String,
        #[serde(default)]
        args: String,
        #[serde(default)]
        working_dir: Option<String>,
    },
    /// A folder or a file.
    Open {
        path: String,
    },
    Url {
        url: String,
    },
    Shortcut {
        keys: String,
    },
    Edit {
        command: EditCommand,
    },
    PasteText {
        text: String,
    },
    /// A line for `cmd /c`.
    Command {
        line: String,
    },
    System {
        action: SystemAction,
    },
}

/// One sector of a ring.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Item {
    pub name: String,
    pub action: Action,
    /// Icon file (`.ico`, `.exe` or `.dll`, optionally `,index`) for the icon view.
    pub icon: Option<String>,
    /// Up to 5 items shown as an arc beyond an outer sector; they have no submenu of their own.
    pub submenu: Vec<Item>,
}

impl Default for Item {
    fn default() -> Self {
        Item::empty()
    }
}

impl Item {
    /// An empty sector.
    pub fn empty() -> Item {
        Item::new("", Action::None)
    }

    fn new(name: &str, action: Action) -> Item {
        Item {
            name: name.to_string(),
            action,
            icon: None,
            submenu: Vec::new(),
        }
    }

    fn edit(name: &str, command: EditCommand) -> Item {
        Item::new(name, Action::Edit { command })
    }

    fn program(name: &str, path: &str) -> Item {
        Item::new(
            name,
            Action::Program {
                path: path.to_string(),
                args: String::new(),
                working_dir: None,
            },
        )
    }

    fn url(name: &str, url: &str) -> Item {
        Item::new(
            name,
            Action::Url {
                url: url.to_string(),
            },
        )
    }

    fn system(name: &str, action: SystemAction) -> Item {
        Item::new(name, Action::System { action })
    }
}

/// Deserializes a number and saturates it into the field type instead of rejecting it;
/// `validate` then clamps it to the allowed range with a warning.
macro_rules! saturating {
    ($name:ident, $t:ty) => {
        fn $name<'de, D: Deserializer<'de>>(d: D) -> Result<$t, D::Error> {
            let v = i64::deserialize(d)?;
            Ok(v.clamp(i64::from(<$t>::MIN), i64::from(<$t>::MAX)) as $t)
        }
    };
}
saturating!(sat_u8, u8);
saturating!(sat_u16, u16);
saturating!(sat_u32, u32);
saturating!(sat_i32, i32);

/// Program options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub theme: ThemeChoice,
    #[serde(deserialize_with = "sat_u16")]
    pub scale_pct: u16,
    #[serde(deserialize_with = "sat_u8")]
    pub outer_sectors: u8,
    pub view: ViewMode,
    #[serde(deserialize_with = "sat_u32")]
    pub hold_ms: u32,
    #[serde(deserialize_with = "sat_i32")]
    pub move_px: i32,
    pub disable_in_fullscreen: bool,
    /// Names of `.exe` files that get the normal right button.
    pub excluded: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            theme: ThemeChoice::System,
            scale_pct: DEFAULT_SCALE_PCT,
            outer_sectors: DEFAULT_OUTER_SECTORS,
            view: ViewMode::Names,
            hold_ms: DEFAULT_HOLD_MS,
            move_px: DEFAULT_MOVE_PX,
            disable_in_fullscreen: true,
            excluded: Vec::new(),
        }
    }
}

fn file_name_lower(path: &str) -> String {
    path.rsplit(['\\', '/'])
        .next()
        .unwrap_or(path)
        .trim()
        .to_lowercase()
}

impl Options {
    /// Trigger thresholds from the options.
    pub fn trigger(&self) -> TriggerSettings {
        TriggerSettings {
            hold_ms: self.hold_ms,
            move_px: self.move_px,
        }
        .clamped()
    }

    /// Whether the program `exe` (a file name or a full path) is on the exclusion list.
    /// Only the file name counts, case does not matter.
    pub fn is_excluded(&self, exe: &str) -> bool {
        let exe = file_name_lower(exe);
        !exe.is_empty() && self.excluded.iter().any(|e| file_name_lower(e) == exe)
    }
}

fn default_version() -> u32 {
    SCHEMA_VERSION
}

/// The whole configuration file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    #[serde(default = "default_version")]
    pub version: u32,
    pub options: Options,
    /// Outer ring; the index is the sector (0 at the top, clockwise).
    pub outer: Vec<Item>,
    /// Inner ring; the index is the sector (0 at the top, clockwise).
    pub inner: Vec<Item>,
}

impl Default for Config {
    fn default() -> Self {
        use EditCommand as E;
        let inner = [
            ("Kopiuj", E::Copy),
            ("Wklej", E::Paste),
            ("Bez formatu", E::PastePlain),
            ("Ponów", E::Redo),
            ("Zapisz", E::Save),
            ("Znajdź", E::Find),
            ("Usuń", E::Delete),
            ("Właśc.", E::Properties),
            ("Zmień nazwę", E::Rename),
            ("Cofnij", E::Undo),
            ("Zaznacz wszystko", E::SelectAll),
            ("Wytnij", E::Cut),
        ]
        .into_iter()
        .map(|(name, command)| Item::edit(name, command))
        .collect();
        let outer = vec![
            Item::program("Terminal", "powershell.exe"),
            Item::url("Przeglądarka", "https://www.google.com"),
            Item::program("Pliki", "explorer.exe"),
            Item::system("Zrzut ekranu", SystemAction::Screenshot),
            Item::system("Wycisz", SystemAction::VolumeMute),
            Item::new(
                "Schowek",
                Action::Shortcut {
                    keys: "Win+V".to_string(),
                },
            ),
            Item::system("Zamknij okno", SystemAction::CloseWindow),
            Item::program("Notatnik", "notepad.exe"),
            Item::program("Kalkulator", "calc.exe"),
            Item {
                submenu: vec![
                    Item::url("Ekran", "ms-settings:display"),
                    Item::url("Dźwięk", "ms-settings:sound"),
                    Item::url("Bluetooth", "ms-settings:bluetooth"),
                    Item::url("Aktualizacje", "ms-settings:windowsupdate"),
                ],
                ..Item::url("Ustawienia", "ms-settings:")
            },
            Item::program("Menedżer zadań", "taskmgr.exe"),
            Item::system("Zablokuj", SystemAction::Lock),
        ];
        Config {
            version: SCHEMA_VERSION,
            options: Options::default(),
            outer,
            inner,
        }
    }
}

fn check_item(item: &Item, place: &str, in_submenu: bool) -> Result<(), String> {
    let need = |value: &str, what: &str| {
        if value.trim().is_empty() {
            Err(format!("{place}: empty {what}"))
        } else {
            Ok(())
        }
    };
    match &item.action {
        Action::Program { path, .. } | Action::Open { path } => need(path, "path")?,
        Action::Url { url } => need(url, "url")?,
        Action::Command { line } => need(line, "command line")?,
        Action::Shortcut { keys } => {
            parse_shortcut(keys).map_err(|e| format!("{place}: invalid shortcut: {e}"))?;
        }
        Action::None | Action::Edit { .. } | Action::PasteText { .. } | Action::System { .. } => {}
    }
    if in_submenu && !item.submenu.is_empty() {
        return Err(format!(
            "{place}: a submenu item cannot have its own submenu"
        ));
    }
    if item.submenu.len() > MAX_SUBMENU_ITEMS {
        return Err(format!(
            "{place}: submenu has {} items, the maximum is {MAX_SUBMENU_ITEMS}",
            item.submenu.len()
        ));
    }
    for (i, sub) in item.submenu.iter().enumerate() {
        check_item(sub, &format!("{place}.submenu[{i}]"), true)?;
    }
    Ok(())
}

/// Replaces `value` with `clamped`; adds a warning when that changes it.
fn clamp_warn<T: Copy + PartialEq + std::fmt::Display>(
    warnings: &mut Vec<String>,
    name: &str,
    value: &mut T,
    clamp: fn(T) -> T,
    range: (T, T),
) {
    let clamped = clamp(*value);
    if *value != clamped {
        warnings.push(format!(
            "options.{name} = {value} is outside {}..={}, using {clamped}",
            range.0, range.1
        ));
        *value = clamped;
    }
}

impl Config {
    /// Checks the configuration. Out-of-range numbers are clamped (with a warning) and short
    /// rings are padded with empty sectors; anything else wrong is an error.
    pub fn validate(&mut self) -> Result<Vec<String>, String> {
        if self.version != SCHEMA_VERSION {
            return Err(format!(
                "unsupported config version {} (expected {SCHEMA_VERSION})",
                self.version
            ));
        }
        for (ring, items) in [("outer", &self.outer), ("inner", &self.inner)] {
            if items.len() > MAX_RING_ITEMS {
                return Err(format!(
                    "{ring} has {} items, the maximum is {MAX_RING_ITEMS}",
                    items.len()
                ));
            }
            for (i, item) in items.iter().enumerate() {
                check_item(item, &format!("{ring}[{i}]"), false)?;
            }
        }
        let mut warnings = Vec::new();
        let o = &mut self.options;
        clamp_warn(
            &mut warnings,
            "scale_pct",
            &mut o.scale_pct,
            clamp_scale_pct,
            (MIN_SCALE_PCT, MAX_SCALE_PCT),
        );
        clamp_warn(
            &mut warnings,
            "outer_sectors",
            &mut o.outer_sectors,
            clamp_outer_sectors,
            (MIN_OUTER_SECTORS, MAX_OUTER_SECTORS),
        );
        clamp_warn(
            &mut warnings,
            "hold_ms",
            &mut o.hold_ms,
            clamp_hold_ms,
            (MIN_HOLD_MS, MAX_HOLD_MS),
        );
        clamp_warn(
            &mut warnings,
            "move_px",
            &mut o.move_px,
            clamp_move_px,
            (MIN_MOVE_PX, MAX_MOVE_PX),
        );
        self.outer.resize(MAX_RING_ITEMS, Item::empty());
        self.inner.resize(INNER_SECTORS as usize, Item::empty());
        Ok(warnings)
    }

    /// Parses and validates the text of a config file.
    pub fn parse(text: &str) -> Result<(Config, Vec<String>), String> {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let value: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        // A struct would also be read from a JSON array, so the shape is checked first.
        if !value.is_object() {
            return Err("the top level must be a JSON object".to_string());
        }
        let mut config: Config = serde_json::from_value(value).map_err(|e| e.to_string())?;
        let warnings = config.validate()?;
        Ok((config, warnings))
    }

    /// The outer item of sector `i`.
    pub fn outer_item(&self, i: usize) -> Option<&Item> {
        self.outer.get(i)
    }

    /// The inner item of sector `i`.
    pub fn inner_item(&self, i: usize) -> Option<&Item> {
        self.inner.get(i)
    }

    /// Names of the first `n` outer sectors (`n` is clamped to the allowed range).
    pub fn outer_names(&self, n: u8) -> Vec<&str> {
        self.outer
            .iter()
            .take(usize::from(clamp_outer_sectors(n)))
            .map(|i| i.name.as_str())
            .collect()
    }

    /// Names of the inner sectors.
    pub fn inner_names(&self) -> Vec<&str> {
        self.inner.iter().map(|i| i.name.as_str()).collect()
    }
}

/// `<appdata>\Elemental-Wheel\config.json`.
pub fn default_path(appdata: &Path) -> PathBuf {
    appdata.join("Elemental-Wheel").join("config.json")
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s: OsString = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

/// Saves atomically: writes `<path>.tmp`, copies an existing file to `<path>.bak`, then
/// replaces `path` with the temporary file.
pub fn save(path: &Path, config: &Config) -> io::Result<()> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs::create_dir_all(dir)?;
    }
    let text = serde_json::to_string_pretty(config).map_err(io::Error::other)?;
    let tmp = with_suffix(path, ".tmp");
    let mut file = File::create(&tmp)?;
    file.write_all(text.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    drop(file);
    if path.exists() {
        fs::copy(path, with_suffix(path, ".bak"))?;
    }
    fs::rename(&tmp, path)
}

/// Where the loaded configuration came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    File,
    Backup,
    Defaults,
    /// There was no file; the defaults were written to it.
    Created,
}

/// Result of [`load`]; loading never fails, it falls back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    pub config: Config,
    pub source: Source,
    pub warnings: Vec<String>,
    /// Why the file was not used (also set when only the fallback could be used).
    pub error: Option<String>,
}

enum ReadError {
    /// The file does not exist.
    Missing,
    Invalid(String),
}

/// File text: UTF-8 (with or without BOM) or UTF-16 LE with BOM (what PowerShell 5.1 writes).
fn decode(bytes: &[u8]) -> Result<String, String> {
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        if rest.len() % 2 != 0 {
            return Err("config file is not valid UTF-16".to_string());
        }
        let units: Vec<u16> = rest
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        return String::from_utf16(&units)
            .map_err(|_| "config file is not valid UTF-16".to_string());
    }
    let rest = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    String::from_utf8(rest.to_vec()).map_err(|_| "config file must be UTF-8".to_string())
}

/// The file as read, the configuration in it and its warnings.
type ReadOk = (Vec<u8>, Config, Vec<String>);

fn read_config(path: &Path) -> Result<ReadOk, ReadError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(ReadError::Missing),
        Err(e) => return Err(ReadError::Invalid(format!("{}: {e}", path.display()))),
    };
    let invalid = |e: String| ReadError::Invalid(format!("{}: {e}", path.display()));
    let text = decode(&bytes).map_err(invalid)?;
    let (config, warnings) = Config::parse(&text).map_err(invalid)?;
    Ok((bytes, config, warnings))
}

/// Writes `bytes` to `<path>.bak` through a temporary file, unless it already holds them.
fn update_backup(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let bak = with_suffix(path, ".bak");
    if fs::read(&bak).is_ok_and(|old| old == bytes) {
        return Ok(());
    }
    let tmp = with_suffix(path, ".bak.tmp");
    let mut file = File::create(&tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&tmp, bak)
}

/// Writes the defaults to `path` only if nothing is there yet (a file that appeared in the
/// meantime is never replaced).
fn create_default(path: &Path, config: &Config) -> io::Result<()> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs::create_dir_all(dir)?;
    }
    let text = serde_json::to_string_pretty(config).map_err(io::Error::other)?;
    let mut file = File::create_new(path)?;
    file.write_all(text.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()
}

/// Like [`load`], but never creates the file: `None` if it does not exist. A valid file is
/// copied to `<path>.bak` as the last good version; an invalid one leaves the backup alone.
pub fn load_existing(path: &Path) -> Option<Loaded> {
    let error = match read_config(path) {
        Ok((bytes, config, mut warnings)) => {
            if let Err(e) = update_backup(path, &bytes) {
                warnings.push(format!("cannot update the backup: {e}"));
            }
            return Some(Loaded {
                config,
                source: Source::File,
                warnings,
                error: None,
            });
        }
        Err(ReadError::Missing) => return None,
        Err(ReadError::Invalid(e)) => e,
    };
    Some(match read_config(&with_suffix(path, ".bak")) {
        Ok((_, config, warnings)) => Loaded {
            config,
            source: Source::Backup,
            warnings,
            error: Some(error),
        },
        Err(_) => Loaded {
            config: Config::default(),
            source: Source::Defaults,
            warnings: Vec::new(),
            error: Some(error),
        },
    })
}

/// Loads the configuration: the file, else its `.bak`, else the defaults. A missing file is
/// created with the defaults. A broken file is never overwritten.
pub fn load(path: &Path) -> Loaded {
    if let Some(loaded) = load_existing(path) {
        return loaded;
    }
    let config = Config::default();
    match create_default(path, &config) {
        Ok(()) => Loaded {
            config,
            source: Source::Created,
            warnings: Vec::new(),
            error: None,
        },
        // The file appeared after the check above: use it.
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            load_existing(path).unwrap_or(Loaded {
                config,
                source: Source::Defaults,
                warnings: Vec::new(),
                error: Some(format!("cannot read {}", path.display())),
            })
        }
        Err(e) => Loaded {
            config,
            source: Source::Defaults,
            warnings: Vec::new(),
            error: Some(format!("cannot create {}: {e}", path.display())),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A clean folder `<repo>\target\test-tmp\<name>`; never the user's `%APPDATA%`.
    fn dir(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("target")
            .join("test-tmp")
            .join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn err(json: &str) -> String {
        Config::parse(json).expect_err(json)
    }

    #[test]
    fn default_is_valid() {
        let mut c = Config::default();
        assert_eq!(c.validate().unwrap(), Vec::<String>::new());
        assert_eq!((c.outer.len(), c.inner.len()), (12, 12));
        assert_eq!(c.inner_item(0).unwrap().name, "Kopiuj");
        assert_eq!(
            c.outer_names(4),
            ["Terminal", "Przeglądarka", "Pliki", "Zrzut ekranu"]
        );
        assert_eq!(c.outer_names(255).len(), 12);
        assert_eq!(c.inner_names()[9], "Cofnij");
        assert_eq!(
            c.outer_item(5).unwrap().action,
            Action::Shortcut {
                keys: "Win+V".into()
            }
        );
        assert!(c.outer_item(12).is_none());
        let settings = c.outer_item(9).unwrap();
        assert_eq!(settings.name, "Ustawienia");
        let sub: Vec<&str> = settings.submenu.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(sub, ["Ekran", "Dźwięk", "Bluetooth", "Aktualizacje"]);
        assert!(c.outer.iter().filter(|i| !i.submenu.is_empty()).count() == 1);
    }

    #[test]
    fn save_and_reload() {
        let path = dir("save_and_reload").join("config.json");
        let mut config = Config::default();
        config.options.theme = ThemeChoice::Dark;
        config.options.excluded = vec!["blender.exe".into()];
        save(&path, &config).unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.source, Source::File);
        assert_eq!(loaded.config, config);
        assert!(loaded.warnings.is_empty() && loaded.error.is_none());
    }

    #[test]
    fn second_save_makes_backup() {
        let path = dir("second_save_makes_backup").join("config.json");
        save(&path, &Config::default()).unwrap();
        let first_text = fs::read(&path).unwrap();
        assert!(!with_suffix(&path, ".bak").exists());
        let mut second = Config::default();
        second.options.scale_pct = 80;
        save(&path, &second).unwrap();
        assert_eq!(fs::read(with_suffix(&path, ".bak")).unwrap(), first_text);
        assert_eq!(load(&path).config, second);
        assert!(!with_suffix(&path, ".tmp").exists());
    }

    #[test]
    fn missing_file_is_created() {
        let path = dir("missing_file_is_created")
            .join("sub")
            .join("config.json");
        let loaded = load(&path);
        assert_eq!(loaded.source, Source::Created);
        assert_eq!(loaded.config, Config::default());
        assert!(path.exists());
        assert_eq!(load(&path).source, Source::File);
    }

    #[test]
    fn good_file_becomes_the_backup_and_survives_a_bad_one() {
        let path = dir("good_file_becomes_backup").join("config.json");
        let good = br#"{"options":{"scale_pct":90}}"#;
        fs::write(&path, good).unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.source, Source::File);
        let bak = with_suffix(&path, ".bak");
        assert_eq!(fs::read(&bak).unwrap(), good);
        assert_eq!(fs::read(&path).unwrap(), good);

        let bad = br#"{"outer":[{"action":{"type":"fly"}}]}"#;
        fs::write(&path, bad).unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.source, Source::Backup);
        assert_eq!(loaded.config.options.scale_pct, 90);
        assert!(loaded.error.is_some());
        assert_eq!(fs::read(&bak).unwrap(), good);
        assert_eq!(fs::read(&path).unwrap(), bad);
    }

    #[test]
    fn load_existing_never_creates_the_file() {
        let path = dir("load_existing_missing").join("config.json");
        assert!(load_existing(&path).is_none());
        assert!(!path.exists());
        assert!(!with_suffix(&path, ".bak").exists());
    }

    #[test]
    fn default_file_never_replaces_an_existing_one() {
        let path = dir("create_default_race").join("config.json");
        fs::write(&path, b"mine").unwrap();
        let e = create_default(&path, &Config::default()).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(&path).unwrap(), b"mine");
    }

    #[test]
    fn utf8_and_utf16_files() {
        let json = r#"{"outer":[{"name":"Menedżer zadań","action":{"type":"none"}}]}"#;
        let d = dir("encodings");
        let mut utf8_bom = vec![0xEF, 0xBB, 0xBF];
        utf8_bom.extend_from_slice(json.as_bytes());
        let mut utf16 = vec![0xFF, 0xFE];
        utf16.extend(json.encode_utf16().flat_map(u16::to_le_bytes));
        for (name, bytes) in [
            ("plain", json.as_bytes().to_vec()),
            ("bom", utf8_bom),
            ("utf16", utf16),
        ] {
            let path = d.join(format!("{name}.json"));
            fs::write(&path, bytes).unwrap();
            let loaded = load(&path);
            assert_eq!(loaded.source, Source::File, "{name}: {:?}", loaded.error);
            assert_eq!(loaded.config.outer[0].name, "Menedżer zadań", "{name}");
        }
        let path = d.join("latin1.json");
        fs::write(&path, b"{\"outer\":[{\"name\":\"Men\xe8d\"}]}").unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.source, Source::Defaults);
        assert!(loaded.error.unwrap().contains("config file must be UTF-8"));
    }

    #[test]
    fn broken_file_uses_backup() {
        let path = dir("broken_file_uses_backup").join("config.json");
        let mut good = Config::default();
        good.options.scale_pct = 90;
        save(&path, &good).unwrap();
        save(&path, &good).unwrap(); // now there is a .bak
        let broken = b"{ not json";
        fs::write(&path, broken).unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.source, Source::Backup);
        assert_eq!(loaded.config, good);
        assert!(loaded.error.is_some());
        assert_eq!(fs::read(&path).unwrap(), broken);
    }

    #[test]
    fn broken_file_without_good_backup_uses_defaults() {
        let path = dir("broken_file_defaults").join("config.json");
        fs::write(&path, b"{ not json").unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.source, Source::Defaults);
        assert_eq!(loaded.config, Config::default());
        assert!(loaded.error.is_some());
        assert_eq!(fs::read(&path).unwrap(), b"{ not json");
        // A bad backup is no better than none.
        fs::write(with_suffix(&path, ".bak"), b"[]").unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.source, Source::Defaults);
        assert_eq!(fs::read(&path).unwrap(), b"{ not json");
        assert_eq!(fs::read(with_suffix(&path, ".bak")).unwrap(), b"[]");
    }

    #[test]
    fn out_of_range_numbers_are_clamped() {
        let (c, w) = Config::parse(
            r#"{"options":{"scale_pct":500,"outer_sectors":1,"hold_ms":5,"move_px":-3}}"#,
        )
        .unwrap();
        assert_eq!(w.len(), 4, "{w:?}");
        assert_eq!(c.options.scale_pct, 140);
        assert_eq!(c.options.outer_sectors, 4);
        assert_eq!(c.options.hold_ms, 50);
        assert_eq!(c.options.move_px, 1);
        // Far outside the field type as well.
        let (c, w) =
            Config::parse(r#"{"options":{"outer_sectors":300,"hold_ms":99999999999}}"#).unwrap();
        assert_eq!((c.options.outer_sectors, c.options.hold_ms), (12, 2000));
        assert_eq!(w.len(), 2);
        assert_eq!(
            c.options.trigger(),
            TriggerSettings {
                hold_ms: 2000,
                move_px: 6
            }
        );
    }

    #[test]
    fn errors_reject_the_file() {
        let item = r#"{"name":"x","action":{"type":"none"}}"#;
        let too_many = format!("[{}]", [item; 13].join(","));
        let sub6 = format!(r#"[{{"submenu":[{}]}}]"#, [item; 6].join(","));
        let nested = format!(r#"{{"outer":[{{"submenu":[{{"submenu":[{item}]}}]}}]}}"#);
        let cases: Vec<(String, &str)> = vec![
            ("{".into(), "EOF"),
            ("[]".into(), "JSON object"),
            (r#"{"version":2}"#.into(), "version"),
            (r#"{"outer":[{"action":{"type":"fly"}}]}"#.into(), "fly"),
            (
                r#"{"inner":[{"action":{"type":"edit","command":"boom"}}]}"#.into(),
                "boom",
            ),
            (
                r#"{"outer":[{"action":{"type":"system","action":"reboot"}}]}"#.into(),
                "reboot",
            ),
            (r#"{"options":{"theme":"pink"}}"#.into(), "pink"),
            (r#"{"options":{"view":"grid"}}"#.into(), "grid"),
            (format!(r#"{{"outer":{too_many}}}"#), "outer has 13"),
            (format!(r#"{{"inner":{too_many}}}"#), "inner has 13"),
            (format!(r#"{{"outer":{sub6}}}"#), "submenu has 6"),
            (nested, "own submenu"),
            (
                r#"{"outer":[{"action":{"type":"shortcut","keys":"Ctrl+"}}]}"#.into(),
                "shortcut",
            ),
            (
                r#"{"outer":[{"action":{"type":"shortcut","keys":"Ctrl"}}]}"#.into(),
                "shortcut",
            ),
            (
                r#"{"outer":[{"action":{"type":"program","path":" "}}]}"#.into(),
                "empty path",
            ),
            (
                r#"{"outer":[{"action":{"type":"open","path":""}}]}"#.into(),
                "empty path",
            ),
            (
                r#"{"outer":[{"action":{"type":"url","url":""}}]}"#.into(),
                "empty url",
            ),
            (
                r#"{"outer":[{"action":{"type":"command","line":""}}]}"#.into(),
                "empty command",
            ),
        ];
        for (json, want) in cases {
            let e = err(&json);
            assert!(e.contains(want), "{json} -> {e}");
        }
    }

    #[test]
    fn short_ring_is_padded() {
        let (c, _) = Config::parse(
            r#"{"outer":[{"name":"A","action":{"type":"url","url":"https://a"}}],"inner":[]}"#,
        )
        .unwrap();
        assert_eq!((c.outer.len(), c.inner.len()), (12, 12));
        assert_eq!(c.outer[0].name, "A");
        assert_eq!(c.outer[1], Item::empty());
        assert_eq!(c.inner[11].action, Action::None);
        assert_eq!(c.options, Options::default());
    }

    #[test]
    fn exclusions() {
        let o = Options {
            excluded: vec!["Blender.exe".into(), r"C:\Tools\Game.EXE".into()],
            ..Options::default()
        };
        assert!(o.is_excluded("blender.exe"));
        assert!(o.is_excluded(r"D:\Apps\BLENDER.EXE"));
        assert!(o.is_excluded("game.exe"));
        assert!(!o.is_excluded("notepad.exe"));
        assert!(!o.is_excluded("blender.exe.bak"));
        assert!(!o.is_excluded(""));
        assert!(!Options::default().is_excluded("blender.exe"));
    }

    #[test]
    fn edit_command_shortcuts() {
        use EditCommand::*;
        let all = [
            (Cut, "Ctrl+X"),
            (Copy, "Ctrl+C"),
            (Paste, "Ctrl+V"),
            (PastePlain, "Ctrl+Shift+V"),
            (Delete, "Delete"),
            (Undo, "Ctrl+Z"),
            (Redo, "Ctrl+Y"),
            (SelectAll, "Ctrl+A"),
            (Save, "Ctrl+S"),
            (Find, "Ctrl+F"),
            (Rename, "F2"),
            (Properties, "Alt+Enter"),
        ];
        for (command, want) in all {
            assert_eq!(command.shortcut(), want);
            assert!(parse_shortcut(command.shortcut()).is_ok());
        }
    }

    #[test]
    fn handwritten_file_with_every_action() {
        let json = r#"{
            "options": {"theme": "dark", "view": "icons", "excluded": ["a.exe"], "extra": 1},
            "outer": [
                {"name": "P", "action": {"type": "program", "path": "notepad.exe"}},
                {"name": "P2", "action": {"type": "program", "path": "x.exe", "args": "-a", "working_dir": "C:\\"}},
                {"name": "O", "action": {"type": "open", "path": "C:\\Windows"}},
                {"name": "U", "action": {"type": "url", "url": "https://example.com"}},
                {"name": "S", "action": {"type": "shortcut", "keys": "ctrl + alt + Del"}},
                {"name": "E", "action": {"type": "edit", "command": "paste_plain"}},
                {"name": "T", "action": {"type": "paste_text", "text": "hi"}},
                {"name": "C", "action": {"type": "command", "line": "echo hi"}},
                {"name": "Y", "action": {"type": "system", "action": "show_desktop"}},
                {"name": "N", "action": {"type": "none"}},
                {"name": "M", "icon": "m.png", "submenu": [{"name": "s", "action": {"type": "edit", "command": "undo"}}]}
            ]
        }"#;
        let (c, w) = Config::parse(json).unwrap();
        assert!(w.is_empty());
        assert_eq!(c.options.theme, ThemeChoice::Dark);
        assert_eq!(c.options.view, ViewMode::Icons);
        assert_eq!(c.options.hold_ms, DEFAULT_HOLD_MS);
        assert_eq!(
            c.outer[1].action,
            Action::Program {
                path: "x.exe".into(),
                args: "-a".into(),
                working_dir: Some("C:\\".into())
            }
        );
        assert_eq!(
            c.outer[5].action,
            Action::Edit {
                command: EditCommand::PastePlain
            }
        );
        assert_eq!(
            c.outer[8].action,
            Action::System {
                action: SystemAction::ShowDesktop
            }
        );
        assert_eq!(c.outer[10].submenu.len(), 1);
        assert_eq!(c.outer[10].icon.as_deref(), Some("m.png"));
        assert_eq!(c.outer.len(), 12);
        assert_eq!(c.version, 1);
    }

    #[test]
    fn default_path_layout() {
        let appdata = Path::new(r"C:\Users\x\AppData\Roaming");
        assert_eq!(
            default_path(appdata),
            appdata.join("Elemental-Wheel").join("config.json")
        );
    }
}
