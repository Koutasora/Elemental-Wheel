//! Editor state and operations: pure logic, no GUI.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use wheel_core::config;
use wheel_core::{
    Action, Config, EditCommand, Item, MAX_INNER_SECTORS, MAX_OUTER_SECTORS, MAX_SUB_TILES,
    MIN_INNER_SECTORS, MIN_OUTER_SECTORS, Source, SystemAction, clamp_inner_sectors,
    clamp_outer_sectors,
};

/// The tile selected in the preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    None,
    Outer(usize),
    Inner(usize),
    /// Submenu tile `k` of the outer sector `parent`.
    Sub(usize, usize),
}

/// The kind of an action, as chosen in the type list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    Program,
    Open,
    Url,
    Shortcut,
    Edit,
    PasteText,
    Command,
    System,
}

impl ActionKind {
    /// Kinds shown in the type list.
    pub const ALL: [ActionKind; 8] = [
        ActionKind::Program,
        ActionKind::Open,
        ActionKind::Url,
        ActionKind::Shortcut,
        ActionKind::Edit,
        ActionKind::PasteText,
        ActionKind::Command,
        ActionKind::System,
    ];

    /// The kind of `action`; `None` for an empty slot.
    pub fn of(action: &Action) -> Option<ActionKind> {
        Some(match action {
            Action::None => return None,
            Action::Program { .. } => ActionKind::Program,
            Action::Open { .. } => ActionKind::Open,
            Action::Url { .. } => ActionKind::Url,
            Action::Shortcut { .. } => ActionKind::Shortcut,
            Action::Edit { .. } => ActionKind::Edit,
            Action::PasteText { .. } => ActionKind::PasteText,
            Action::Command { .. } => ActionKind::Command,
            Action::System { .. } => ActionKind::System,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            ActionKind::Program => "Uruchom program",
            ActionKind::Open => "Otwórz folder lub plik",
            ActionKind::Url => "Otwórz adres",
            ActionKind::Shortcut => "Skrót klawiszowy",
            ActionKind::Edit => "Komenda edycji",
            ActionKind::PasteText => "Wklej tekst",
            ActionKind::Command => "Polecenie",
            ActionKind::System => "Akcja systemowa",
        }
    }

    /// The default action of this kind.
    fn default_action(self) -> Action {
        match self {
            ActionKind::Program => Action::Program {
                path: String::new(),
                args: String::new(),
                working_dir: None,
            },
            ActionKind::Open => Action::Open {
                path: String::new(),
            },
            ActionKind::Url => Action::Url { url: String::new() },
            ActionKind::Shortcut => Action::Shortcut {
                keys: String::new(),
            },
            ActionKind::Edit => Action::Edit {
                command: EditCommand::Copy,
            },
            ActionKind::PasteText => Action::PasteText {
                text: String::new(),
            },
            ActionKind::Command => Action::Command {
                line: String::new(),
            },
            ActionKind::System => Action::System {
                action: SystemAction::Lock,
            },
        }
    }
}

pub fn edit_label(command: EditCommand) -> &'static str {
    match command {
        EditCommand::Cut => "Wytnij",
        EditCommand::Copy => "Kopiuj",
        EditCommand::Paste => "Wklej",
        EditCommand::PastePlain => "Wklej bez formatu",
        EditCommand::Delete => "Usuń",
        EditCommand::Undo => "Cofnij",
        EditCommand::Redo => "Ponów",
        EditCommand::SelectAll => "Zaznacz wszystko",
        EditCommand::Save => "Zapisz",
        EditCommand::Find => "Znajdź",
        EditCommand::Rename => "Zmień nazwę",
        EditCommand::Properties => "Właściwości",
    }
}

pub fn system_label(action: SystemAction) -> &'static str {
    match action {
        SystemAction::Lock => "Zablokuj",
        SystemAction::CloseWindow => "Zamknij okno",
        SystemAction::MinimizeWindow => "Zminimalizuj okno",
        SystemAction::ShowDesktop => "Pokaż pulpit",
        SystemAction::VolumeMute => "Wycisz",
        SystemAction::VolumeUp => "Głośniej",
        SystemAction::VolumeDown => "Ciszej",
        SystemAction::Screenshot => "Zrzut ekranu",
    }
}

/// Extensions that become a "program" item when a file is dropped.
const PROGRAM_EXTENSIONS: [&str; 4] = ["exe", "lnk", "bat", "cmd"];

/// A new item for a dropped file or folder: a program for `.exe`/`.lnk`/`.bat`/`.cmd`,
/// "open" for anything else. The name is the file name without the extension.
pub fn item_from_path(path: &Path) -> Item {
    let is_program = !path.is_dir()
        && path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| PROGRAM_EXTENSIONS.iter().any(|p| e.eq_ignore_ascii_case(p)));
    let name = if path.is_dir() {
        path.file_name()
    } else {
        path.file_stem()
    }
    .map(|n| n.to_string_lossy().into_owned())
    .unwrap_or_else(|| path.to_string_lossy().into_owned());
    let path = path.to_string_lossy().into_owned();
    let action = if is_program {
        Action::Program {
            path,
            args: String::new(),
            working_dir: None,
        }
    } else {
        Action::Open { path }
    };
    Item {
        name,
        action,
        ..Item::empty()
    }
}

/// An item added with "Dodaj element" or "Dodaj kafelek podmenu".
fn new_item(name: &str) -> Item {
    Item {
        name: name.to_string(),
        action: ActionKind::Program.default_action(),
        ..Item::empty()
    }
}

/// Modified time and length of the config file: enough to notice an edit made elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stamp {
    modified: Option<SystemTime>,
    len: u64,
}

/// The current stamp of `path`; `None` when the file does not exist (or cannot be read).
pub fn read_stamp(path: &Path) -> Option<Stamp> {
    let meta = fs::metadata(path).ok()?;
    Some(Stamp {
        modified: meta.modified().ok(),
        len: meta.len(),
    })
}

/// The result of `Model::check_external`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalChange {
    None,
    /// The file changed and there were no unsaved changes: it was loaded again.
    ReloadedSilently,
    /// The file changed while there are unsaved changes: the user has to choose
    /// (`reload` or `keep_mine`).
    NeedsDecision,
    /// The file was deleted.
    Vanished,
}

/// The edited configuration, its last saved copy and the selection.
pub struct Model {
    pub config: Config,
    saved: Config,
    pub selection: Selection,
    pub path: PathBuf,
    pub source: Source,
    pub load_error: Option<String>,
    pub load_warnings: Vec<String>,
    /// The stamp of the file as the editor knows it (loaded or saved by the editor).
    stamp: Option<Stamp>,
}

impl Model {
    /// Loads `path` without creating it; a missing file gives the defaults, which are written
    /// only on the first save.
    pub fn open(path: PathBuf) -> Model {
        let (config, source, load_error, load_warnings) = match config::load_existing(&path) {
            Some(l) => (l.config, l.source, l.error, l.warnings),
            None => (Config::default(), Source::Defaults, None, Vec::new()),
        };
        Model {
            stamp: read_stamp(&path),
            saved: config.clone(),
            config,
            selection: Selection::None,
            path,
            source,
            load_error,
            load_warnings,
        }
    }

    /// Loads the file again, dropping all unsaved changes.
    pub fn reload(&mut self) {
        *self = Model::open(std::mem::take(&mut self.path));
    }

    pub fn is_dirty(&self) -> bool {
        self.config != self.saved
    }

    /// Whether the file on disk (`now`) differs from the stamp the editor knows.
    pub fn disk_changed(&self, now: Option<Stamp>) -> bool {
        now != self.stamp
    }

    /// Compares the file stamp `now` with the known one. Without unsaved changes a changed
    /// file is loaded again; a deleted file is reported once.
    pub fn check_external(&mut self, now: Option<Stamp>) -> ExternalChange {
        if !self.disk_changed(now) {
            return ExternalChange::None;
        }
        if now.is_none() {
            self.stamp = None;
            return ExternalChange::Vanished;
        }
        if self.is_dirty() {
            ExternalChange::NeedsDecision
        } else {
            self.reload();
            ExternalChange::ReloadedSilently
        }
    }

    /// "Keep my changes": accepts the file as it is now, so it is not reported again.
    pub fn keep_mine(&mut self, now: Option<Stamp>) {
        self.stamp = now;
    }

    /// Validates a copy and writes it. On an error nothing is written and the message is
    /// returned; on success the warnings (such as clamped values) are returned.
    pub fn save(&mut self) -> Result<Vec<String>, String> {
        let mut copy = self.config.clone();
        let warnings = copy.validate()?;
        config::save(&self.path, &copy).map_err(|e| format!("{}: {e}", self.path.display()))?;
        self.config = copy.clone();
        self.saved = copy;
        self.stamp = read_stamp(&self.path);
        self.source = Source::File;
        self.load_error = None;
        self.load_warnings.clear();
        self.fix_selection();
        Ok(warnings)
    }

    pub fn item_at(&self, selection: Selection) -> Option<&Item> {
        match selection {
            Selection::None => None,
            Selection::Outer(i) => self.config.outer.get(i),
            Selection::Inner(i) => self.config.inner.get(i),
            Selection::Sub(p, k) => self.config.outer.get(p)?.submenu.get(k),
        }
    }

    fn item_at_mut(&mut self, selection: Selection) -> Option<&mut Item> {
        match selection {
            Selection::None => None,
            Selection::Outer(i) => self.config.outer.get_mut(i),
            Selection::Inner(i) => self.config.inner.get_mut(i),
            Selection::Sub(p, k) => self.config.outer.get_mut(p)?.submenu.get_mut(k),
        }
    }

    pub fn selected(&self) -> Option<&Item> {
        self.item_at(self.selection)
    }

    pub fn selected_mut(&mut self) -> Option<&mut Item> {
        self.item_at_mut(self.selection)
    }

    /// Clears a selection that points outside the active sectors or submenu.
    fn fix_selection(&mut self) {
        let n = usize::from(clamp_outer_sectors(self.config.options.outer_sectors));
        let n_inner = usize::from(clamp_inner_sectors(self.config.options.inner_sectors));
        let valid = match self.selection {
            Selection::None => true,
            Selection::Outer(i) => i < n && i < self.config.outer.len(),
            Selection::Inner(i) => i < n_inner && i < self.config.inner.len(),
            Selection::Sub(p, k) => {
                p < n
                    && self
                        .config
                        .outer
                        .get(p)
                        .is_some_and(|o| k < o.submenu.len())
            }
        };
        if !valid {
            self.selection = Selection::None;
        }
    }

    pub fn set_selection(&mut self, selection: Selection) {
        self.selection = selection;
        self.fix_selection();
    }

    pub fn set_name(&mut self, name: &str) {
        if let Some(item) = self.selected_mut() {
            item.name = name.to_string();
        }
    }

    /// Changes the action type to the default action of `kind`. The path is carried between
    /// "program" and "open"; choosing the current kind changes nothing.
    pub fn set_action_kind(&mut self, kind: ActionKind) {
        let Some(item) = self.selected_mut() else {
            return;
        };
        if ActionKind::of(&item.action) == Some(kind) {
            return;
        }
        let carried = match &item.action {
            Action::Program { path, .. } | Action::Open { path } => Some(path.clone()),
            _ => None,
        };
        item.action = match (kind, carried) {
            (ActionKind::Program, Some(path)) => Action::Program {
                path,
                args: String::new(),
                working_dir: None,
            },
            (ActionKind::Open, Some(path)) => Action::Open { path },
            (kind, _) => kind.default_action(),
        };
    }

    /// Sets the icon; an empty text means no icon.
    pub fn set_icon(&mut self, icon: &str) {
        if let Some(item) = self.selected_mut() {
            item.icon = (!icon.trim().is_empty()).then(|| icon.to_string());
        }
    }

    /// Empties the selected ring slot, with its submenu. Submenu tiles are removed instead.
    pub fn clear_slot(&mut self) {
        match self.selection {
            Selection::Outer(_) | Selection::Inner(_) => {
                if let Some(item) = self.selected_mut() {
                    *item = Item::empty();
                }
            }
            Selection::Sub(p, k) => self.remove_sub(p, k),
            Selection::None => {}
        }
    }

    /// Puts a new item into the selected empty ring slot.
    pub fn add_item(&mut self) {
        if let Selection::Outer(_) | Selection::Inner(_) = self.selection
            && let Some(item) = self.selected_mut()
            && item.action == Action::None
        {
            *item = new_item("Nowy element");
        }
    }

    /// Adds a submenu tile to the outer item `parent` and selects it.
    pub fn add_sub(&mut self, parent: usize) {
        let Some(item) = self.config.outer.get_mut(parent) else {
            return;
        };
        if item.action == Action::None || item.submenu.len() >= usize::from(MAX_SUB_TILES) {
            return;
        }
        item.submenu.push(new_item("Nowy"));
        self.selection = Selection::Sub(parent, item.submenu.len() - 1);
    }

    pub fn remove_sub(&mut self, parent: usize, k: usize) {
        let Some(item) = self.config.outer.get_mut(parent) else {
            return;
        };
        if k >= item.submenu.len() {
            return;
        }
        item.submenu.remove(k);
        self.selection = match self.selection {
            Selection::Sub(p, s) if p == parent && s == k => Selection::Outer(parent),
            Selection::Sub(p, s) if p == parent && s > k => Selection::Sub(p, s - 1),
            other => other,
        };
    }

    /// Moves submenu tile `k` one place up (`up`) or down; the selection follows the tile.
    pub fn move_sub(&mut self, parent: usize, k: usize, up: bool) {
        let Some(item) = self.config.outer.get_mut(parent) else {
            return;
        };
        let other = if up { k.checked_sub(1) } else { Some(k + 1) };
        let Some(other) = other.filter(|&o| k < item.submenu.len() && o < item.submenu.len())
        else {
            return;
        };
        item.submenu.swap(k, other);
        if let Selection::Sub(p, s) = self.selection
            && p == parent
        {
            if s == k {
                self.selection = Selection::Sub(p, other);
            } else if s == other {
                self.selection = Selection::Sub(p, k);
            }
        }
    }

    /// Sets the number of inner sectors (clamped). Items are kept, only hidden.
    pub fn set_inner_sectors(&mut self, n: u8) {
        self.config.options.inner_sectors = n.clamp(MIN_INNER_SECTORS, MAX_INNER_SECTORS);
        self.fix_selection();
    }

    /// Sets the number of outer sectors (clamped). Items are kept, only hidden.
    pub fn set_outer_sectors(&mut self, n: u8) {
        self.config.options.outer_sectors = n.clamp(MIN_OUTER_SECTORS, MAX_OUTER_SECTORS);
        self.fix_selection();
    }

    /// Replaces the item at `target` with one made from a dropped file. A ring item keeps
    /// its submenu. The new item becomes the selection.
    pub fn drop_file(&mut self, target: Selection, path: &Path) {
        let mut new = item_from_path(path);
        let Some(item) = self.item_at_mut(target) else {
            return;
        };
        new.submenu = std::mem::take(&mut item.submenu);
        *item = new;
        self.set_selection(target);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A clean folder `<repo>\target\test-tmp\editor-<name>`; never the user's `%APPDATA%`.
    fn dir(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("target")
            .join("test-tmp")
            .join(format!("editor-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn model() -> Model {
        let mut m = Model::open(dir("none").join("missing.json"));
        m.selection = Selection::Outer(0);
        m
    }

    #[test]
    fn missing_file_gives_defaults_and_is_not_created() {
        let path = dir("missing").join("config.json");
        let m = Model::open(path.clone());
        assert_eq!(m.source, Source::Defaults);
        assert!(m.load_error.is_none() && !m.is_dirty());
        assert!(!path.exists());
    }

    #[test]
    fn changing_kind_carries_path_between_program_and_open() {
        let mut m = model();
        m.selected_mut().unwrap().action = Action::Program {
            path: "C:\\a.exe".into(),
            args: "-x".into(),
            working_dir: None,
        };
        m.set_action_kind(ActionKind::Open);
        assert_eq!(
            m.selected().unwrap().action,
            Action::Open {
                path: "C:\\a.exe".into()
            }
        );
        m.set_action_kind(ActionKind::Program);
        assert_eq!(
            m.selected().unwrap().action,
            Action::Program {
                path: "C:\\a.exe".into(),
                args: String::new(),
                working_dir: None
            }
        );
        m.set_action_kind(ActionKind::Url);
        assert_eq!(
            m.selected().unwrap().action,
            Action::Url { url: String::new() }
        );
        m.set_action_kind(ActionKind::Edit);
        assert_eq!(
            m.selected().unwrap().action,
            Action::Edit {
                command: EditCommand::Copy
            }
        );
        // The same kind keeps the value.
        m.selected_mut().unwrap().action = Action::Edit {
            command: EditCommand::Undo,
        };
        m.set_action_kind(ActionKind::Edit);
        assert_eq!(
            m.selected().unwrap().action,
            Action::Edit {
                command: EditCommand::Undo
            }
        );
    }

    #[test]
    fn name_and_icon_can_be_set_also_in_the_inner_ring() {
        let mut m = model();
        m.set_selection(Selection::Inner(0));
        m.set_name("Skopiuj");
        m.set_icon("C:\\x.ico");
        let item = m.selected().unwrap();
        assert_eq!(item.name, "Skopiuj");
        assert_eq!(item.icon.as_deref(), Some("C:\\x.ico"));
        m.set_icon("  ");
        assert_eq!(m.selected().unwrap().icon, None);
        assert!(m.is_dirty());
    }

    #[test]
    fn clear_slot_empties_item_and_submenu() {
        let mut m = model();
        m.set_selection(Selection::Outer(9));
        assert!(!m.selected().unwrap().submenu.is_empty());
        m.clear_slot();
        assert_eq!(*m.selected().unwrap(), Item::empty());
        assert_eq!(m.selected().unwrap().action, Action::None);
        m.add_item();
        assert_ne!(m.selected().unwrap().action, Action::None);
    }

    #[test]
    fn submenu_add_remove_move_and_limit() {
        let mut m = model();
        m.add_sub(0);
        assert_eq!(m.selection, Selection::Sub(0, 0));
        for _ in 0..10 {
            m.add_sub(0);
        }
        assert_eq!(m.config.outer[0].submenu.len(), 5);

        m.config.outer[0]
            .submenu
            .iter_mut()
            .enumerate()
            .for_each(|(i, s)| s.name = i.to_string());
        m.set_selection(Selection::Sub(0, 1));
        m.move_sub(0, 1, true);
        assert_eq!(m.config.outer[0].submenu[0].name, "1");
        assert_eq!(m.selection, Selection::Sub(0, 0));
        m.move_sub(0, 0, true);
        m.move_sub(0, 4, false);
        let names: Vec<_> = m.config.outer[0].submenu.iter().map(|s| &s.name).collect();
        assert_eq!(names, ["1", "0", "2", "3", "4"]);

        m.remove_sub(0, 0);
        assert_eq!(m.selection, Selection::Outer(0));
        m.set_selection(Selection::Sub(0, 3));
        m.remove_sub(0, 1);
        assert_eq!(m.selection, Selection::Sub(0, 2));
        assert_eq!(m.config.outer[0].submenu.len(), 3);
    }

    #[test]
    fn submenu_is_not_added_to_an_empty_slot() {
        let mut m = model();
        m.config.outer[2] = Item::empty();
        m.add_sub(2);
        assert!(m.config.outer[2].submenu.is_empty());
    }

    #[test]
    fn changing_sector_count_keeps_items_and_fixes_selection() {
        let mut m = model();
        m.set_selection(Selection::Outer(9));
        m.set_outer_sectors(4);
        assert_eq!(m.selection, Selection::None);
        assert_eq!(m.config.outer.len(), 12);
        assert_eq!(m.config.outer[9].name, "Ustawienia");
        assert_eq!(m.config.outer[9].submenu.len(), 4);
        m.set_selection(Selection::Outer(9));
        assert_eq!(m.selection, Selection::None);
        m.set_selection(Selection::Outer(3));
        m.set_outer_sectors(0);
        assert_eq!(m.config.options.outer_sectors, 4);
        m.set_outer_sectors(200);
        assert_eq!(m.config.options.outer_sectors, 12);
        m.set_selection(Selection::Outer(9));
        assert_eq!(m.selection, Selection::Outer(9));
    }

    #[test]
    fn changing_inner_sector_count_keeps_items_and_fixes_selection() {
        let mut m = model();
        let name = m.config.inner[9].name.clone();
        m.set_selection(Selection::Inner(9));
        assert_eq!(m.selection, Selection::Inner(9));
        m.set_inner_sectors(6);
        assert_eq!(m.selection, Selection::None);
        assert_eq!(m.config.inner.len(), 12);
        assert_eq!(m.config.inner[9].name, name);
        m.set_selection(Selection::Inner(9));
        assert_eq!(m.selection, Selection::None);
        m.set_selection(Selection::Inner(5));
        assert_eq!(m.selection, Selection::Inner(5));
        m.set_inner_sectors(0);
        assert_eq!(m.config.options.inner_sectors, 4);
        assert_eq!(m.selection, Selection::None);
        m.set_inner_sectors(200);
        assert_eq!(m.config.options.inner_sectors, 12);
        m.set_selection(Selection::Inner(9));
        assert_eq!(m.selection, Selection::Inner(9));
        // The outer ring is independent.
        assert_eq!(m.config.options.outer_sectors, 12);
    }

    #[test]
    fn items_from_files() {
        let d = dir("files");
        let exe = d.join("Notatnik.EXE");
        let txt = d.join("plan.v2.txt");
        let folder = d.join("Dokumenty.stare");
        fs::write(&exe, "").unwrap();
        fs::write(&txt, "").unwrap();
        fs::create_dir(&folder).unwrap();
        let program = item_from_path(&exe);
        assert_eq!(program.name, "Notatnik");
        assert_eq!(
            program.action,
            Action::Program {
                path: exe.to_string_lossy().into_owned(),
                args: String::new(),
                working_dir: None
            }
        );
        let file = item_from_path(&txt);
        assert_eq!(file.name, "plan.v2");
        assert!(matches!(file.action, Action::Open { .. }));
        let dir_item = item_from_path(&folder);
        assert_eq!(dir_item.name, "Dokumenty.stare");
        assert!(matches!(dir_item.action, Action::Open { .. }));
        for ext in ["lnk", "bat", "cmd"] {
            let p = d.join(format!("a.{ext}"));
            assert!(matches!(item_from_path(&p).action, Action::Program { .. }));
        }
    }

    #[test]
    fn dropped_file_replaces_slot_and_keeps_submenu() {
        let mut m = model();
        let d = dir("drop");
        let exe = d.join("gra.exe");
        fs::write(&exe, "").unwrap();
        m.drop_file(Selection::Outer(9), &exe);
        let item = &m.config.outer[9];
        assert_eq!(item.name, "gra");
        assert_eq!(item.submenu.len(), 4);
        assert_eq!(m.selection, Selection::Outer(9));
        m.drop_file(Selection::Sub(9, 1), &d);
        assert!(matches!(
            m.config.outer[9].submenu[1].action,
            Action::Open { .. }
        ));
        assert_eq!(m.selection, Selection::Sub(9, 1));
    }

    #[test]
    fn save_validates_then_writes_and_clears_dirty() {
        let path = dir("save").join("config.json");
        let mut m = Model::open(path.clone());
        m.add_item_to_empty_for_test();
        // An empty program path is an error: nothing is written.
        let err = m.save().unwrap_err();
        assert!(err.contains("path"), "{err}");
        assert!(!path.exists() && m.is_dirty());

        m.set_selection(Selection::Outer(11));
        m.selected_mut().unwrap().action = Action::Url {
            url: "https://example.org".into(),
        };
        m.config.options.hold_ms = 5000;
        let warnings = m.save().unwrap();
        assert_eq!(warnings.len(), 1);
        assert!(!m.is_dirty());
        assert_eq!(m.source, Source::File);
        assert_eq!(m.config.options.hold_ms, 2000);
        assert_eq!(config::load(&path).config, m.config);
    }

    #[test]
    fn broken_file_falls_back_and_save_keeps_backup() {
        let path = dir("broken").join("config.json");
        let mut m = Model::open(path.clone());
        m.save().unwrap();
        m.config.options.scale_pct = 80;
        m.save().unwrap();
        m.config.options.scale_pct = 90;
        m.save().unwrap();
        fs::write(&path, "{").unwrap();
        let reopened = Model::open(path.clone());
        assert_eq!(reopened.source, Source::Backup);
        assert!(reopened.load_error.is_some());
        assert_eq!(reopened.config.options.scale_pct, 80);
    }

    #[test]
    fn reload_drops_changes() {
        let path = dir("reload").join("config.json");
        let mut m = Model::open(path);
        m.set_selection(Selection::Outer(0));
        m.set_name("Zmieniony");
        assert!(m.is_dirty());
        m.reload();
        assert!(!m.is_dirty());
        assert_eq!(m.config.outer[0].name, "Terminal");
    }

    /// Writes `config` to `path`, padded with `pad` spaces, so that the stamp differs from any earlier one (the length
    /// differs, as the modified time may be too coarse).
    fn write_external(path: &Path, config: &Config, pad: usize) {
        config::save(path, config).unwrap();
        let mut text = fs::read_to_string(path).unwrap();
        text.push_str(&" ".repeat(pad));
        fs::write(path, text).unwrap();
    }

    #[test]
    fn own_save_is_not_an_external_change() {
        let path = dir("ext-own").join("config.json");
        let mut m = Model::open(path.clone());
        m.set_selection(Selection::Outer(0));
        m.set_name("Moje");
        m.save().unwrap();
        assert!(!m.disk_changed(read_stamp(&path)));
        assert_eq!(m.check_external(read_stamp(&path)), ExternalChange::None);
    }

    #[test]
    fn external_change_without_unsaved_changes_reloads() {
        let path = dir("ext-clean").join("config.json");
        let mut m = Model::open(path.clone());
        m.save().unwrap();
        let mut other = m.config.clone();
        other.outer[0].name = "Obce".into();
        write_external(&path, &other, 3);
        assert_eq!(
            m.check_external(read_stamp(&path)),
            ExternalChange::ReloadedSilently
        );
        assert_eq!(m.config.outer[0].name, "Obce");
        assert!(!m.is_dirty());
        assert_eq!(m.check_external(read_stamp(&path)), ExternalChange::None);
    }

    #[test]
    fn external_change_with_unsaved_changes_needs_a_decision() {
        let path = dir("ext-dirty").join("config.json");
        let mut m = Model::open(path.clone());
        m.save().unwrap();
        m.set_selection(Selection::Outer(0));
        m.set_name("Moje");
        let mut other = m.config.clone();
        other.outer[0].name = "Obce".into();
        write_external(&path, &other, 3);
        assert_eq!(
            m.check_external(read_stamp(&path)),
            ExternalChange::NeedsDecision
        );
        assert_eq!(m.config.outer[0].name, "Moje");
        // "Wczytaj plik".
        m.reload();
        assert_eq!(m.config.outer[0].name, "Obce");
        assert_eq!(m.check_external(read_stamp(&path)), ExternalChange::None);
    }

    #[test]
    fn keep_mine_acknowledges_the_change() {
        let path = dir("ext-keep").join("config.json");
        let mut m = Model::open(path.clone());
        m.save().unwrap();
        m.set_selection(Selection::Outer(0));
        m.set_name("Moje");
        write_external(&path, &Config::default(), 5);
        assert_eq!(
            m.check_external(read_stamp(&path)),
            ExternalChange::NeedsDecision
        );
        m.keep_mine(read_stamp(&path));
        assert_eq!(m.check_external(read_stamp(&path)), ExternalChange::None);
        assert!(m.is_dirty());
        // A further change is reported again.
        write_external(&path, &Config::default(), 9);
        assert_eq!(
            m.check_external(read_stamp(&path)),
            ExternalChange::NeedsDecision
        );
    }

    #[test]
    fn vanished_file_is_reported_once_and_save_recreates_it() {
        let path = dir("ext-gone").join("config.json");
        let mut m = Model::open(path.clone());
        m.save().unwrap();
        fs::remove_file(&path).unwrap();
        assert_eq!(
            m.check_external(read_stamp(&path)),
            ExternalChange::Vanished
        );
        assert_eq!(m.check_external(read_stamp(&path)), ExternalChange::None);
        assert!(!m.disk_changed(read_stamp(&path)));
        m.save().unwrap();
        assert!(path.exists());
        assert_eq!(m.check_external(read_stamp(&path)), ExternalChange::None);
    }

    #[test]
    fn save_over_a_newer_file_is_detected() {
        let path = dir("ext-over").join("config.json");
        let mut m = Model::open(path.clone());
        m.save().unwrap();
        assert!(!m.disk_changed(read_stamp(&path)));
        write_external(&path, &Config::default(), 7);
        assert!(m.disk_changed(read_stamp(&path)));
        // Saving anyway updates the stamp.
        m.save().unwrap();
        assert!(!m.disk_changed(read_stamp(&path)));
    }

    impl Model {
        fn add_item_to_empty_for_test(&mut self) {
            self.set_selection(Selection::Outer(11));
            self.clear_slot();
            self.add_item();
        }
    }
}
