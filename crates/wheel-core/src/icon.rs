//! What a tile shows in the icons view, decided from the item alone: shared by the wheel and the
//! editor preview so both pick the same icon.

use crate::config::{Action, EditCommand, Item, SystemAction};

/// Icon sizes in px at 100% and 96 DPI, taken from the mockup (`icoSize`).
pub const MOCKUP_ICON_PX: IconSizes = IconSizes {
    inner: 24,
    outer: 36,
    sub: 30,
};

/// Icon size in px for each ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IconSizes {
    pub inner: u32,
    pub outer: u32,
    pub sub: u32,
}

impl IconSizes {
    /// The mockup sizes scaled by `k` (1.0 = mockup), at least 1 px each.
    pub fn scaled(k: f32) -> IconSizes {
        let s = |px: u32| ((px as f32 * k).round() as u32).max(1);
        IconSizes {
            inner: s(MOCKUP_ICON_PX.inner),
            outer: s(MOCKUP_ICON_PX.outer),
            sub: s(MOCKUP_ICON_PX.sub),
        }
    }
}

/// What a tile shows, decided from the item alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IconSpec {
    /// Icon `index` of a file (`.ico`, `.exe`, `.dll`), or the system icon of the target when the
    /// file has none. Environment variables in `path` are not expanded yet.
    File {
        path: String,
        index: i32,
    },
    /// A glyph of the Segoe icon font.
    Glyph(char),
    /// The first letter of the name, upper case.
    Letter(char),
    Blank,
}

/// Chooses what `item` shows: its `icon`, else the target's icon, a glyph or the first letter.
pub fn spec_for(item: &Item) -> IconSpec {
    if let Some(icon) = item
        .icon
        .as_deref()
        .map(str::trim)
        .filter(|i| !i.is_empty())
    {
        let (path, index) = split_icon_index(icon);
        return IconSpec::File {
            path: path.to_string(),
            index,
        };
    }
    match &item.action {
        Action::Program { path, .. } | Action::Open { path } => IconSpec::File {
            path: path.clone(),
            index: 0,
        },
        Action::Edit { command } => IconSpec::Glyph(edit_glyph(*command)),
        Action::System { action } => IconSpec::Glyph(system_glyph(*action)),
        Action::Url { .. } => IconSpec::Glyph('\u{E774}'),
        Action::Shortcut { .. } => IconSpec::Glyph('\u{E765}'),
        Action::PasteText { .. } => IconSpec::Glyph('\u{E8D2}'),
        Action::Command { .. } => IconSpec::Glyph('\u{E756}'),
        Action::None => first_letter(&item.name).map_or(IconSpec::Blank, IconSpec::Letter),
    }
}

/// Splits the Windows `file,N` form (N may be negative: a resource id); without a valid number
/// after the last comma the whole text is the path.
pub fn split_icon_index(icon: &str) -> (&str, i32) {
    match icon.rsplit_once(',') {
        Some((path, n)) => match n.trim().parse() {
            Ok(index) => (path.trim_end(), index),
            Err(_) => (icon, 0),
        },
        None => (icon, 0),
    }
}

/// First alphanumeric character of `name`, upper case (`ź` becomes `Ź`).
pub fn first_letter(name: &str) -> Option<char> {
    let c = name.chars().find(|c| c.is_alphanumeric())?;
    // A few letters upper-case to several characters (`ß` -> `SS`); keep the letter then.
    let mut upper = c.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(u), None) => Some(u),
        _ => Some(c),
    }
}

pub fn edit_glyph(command: EditCommand) -> char {
    match command {
        EditCommand::Cut => '\u{E8C6}',
        EditCommand::Copy => '\u{E8C8}',
        EditCommand::Paste => '\u{E77F}',
        EditCommand::PastePlain => '\u{E8E9}',
        EditCommand::Delete => '\u{E74D}',
        EditCommand::Undo => '\u{E7A7}',
        EditCommand::Redo => '\u{E7A6}',
        EditCommand::SelectAll => '\u{E8B3}',
        EditCommand::Save => '\u{E74E}',
        EditCommand::Find => '\u{E721}',
        EditCommand::Rename => '\u{E8AC}',
        EditCommand::Properties => '\u{E946}',
    }
}

pub fn system_glyph(action: SystemAction) -> char {
    match action {
        SystemAction::Lock => '\u{E72E}',
        SystemAction::CloseWindow => '\u{E8BB}',
        SystemAction::MinimizeWindow => '\u{E921}',
        SystemAction::ShowDesktop => '\u{E7F4}',
        SystemAction::VolumeMute => '\u{E74F}',
        SystemAction::VolumeUp => '\u{E995}',
        SystemAction::VolumeDown => '\u{E993}',
        SystemAction::Screenshot => '\u{E722}',
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn item(name: &str, action: Action) -> Item {
        Item {
            name: name.to_string(),
            action,
            icon: None,
            submenu: Vec::new(),
        }
    }

    const EDIT: [EditCommand; 12] = [
        EditCommand::Cut,
        EditCommand::Copy,
        EditCommand::Paste,
        EditCommand::PastePlain,
        EditCommand::Delete,
        EditCommand::Undo,
        EditCommand::Redo,
        EditCommand::SelectAll,
        EditCommand::Save,
        EditCommand::Find,
        EditCommand::Rename,
        EditCommand::Properties,
    ];

    const SYSTEM: [SystemAction; 8] = [
        SystemAction::Lock,
        SystemAction::CloseWindow,
        SystemAction::MinimizeWindow,
        SystemAction::ShowDesktop,
        SystemAction::VolumeMute,
        SystemAction::VolumeUp,
        SystemAction::VolumeDown,
        SystemAction::Screenshot,
    ];

    fn glyph(spec: IconSpec) -> char {
        match spec {
            IconSpec::Glyph(c) => c,
            other => panic!("not a glyph: {other:?}"),
        }
    }

    #[test]
    fn program_and_open_use_the_target_icon() {
        let program = item(
            "Notepad",
            Action::Program {
                path: "notepad.exe".into(),
                args: String::new(),
                working_dir: None,
            },
        );
        assert_eq!(
            spec_for(&program),
            IconSpec::File {
                path: "notepad.exe".into(),
                index: 0
            }
        );
        let open = item(
            "Docs",
            Action::Open {
                path: "%USERPROFILE%".into(),
            },
        );
        assert_eq!(
            spec_for(&open),
            IconSpec::File {
                path: "%USERPROFILE%".into(),
                index: 0
            }
        );
    }

    #[test]
    fn commands_and_system_actions_have_distinct_glyphs() {
        let edit: HashSet<char> = EDIT
            .iter()
            .map(|c| glyph(spec_for(&item("x", Action::Edit { command: *c }))))
            .collect();
        assert_eq!(edit.len(), 12);
        let system: HashSet<char> = SYSTEM
            .iter()
            .map(|a| glyph(spec_for(&item("x", Action::System { action: *a }))))
            .collect();
        assert_eq!(system.len(), 8);
        assert!(edit.is_disjoint(&system));
    }

    #[test]
    fn other_actions_have_glyphs() {
        let actions = [
            Action::Url {
                url: "https://example.com".into(),
            },
            Action::Shortcut {
                keys: "Ctrl+K".into(),
            },
            Action::PasteText { text: "t".into() },
            Action::Command { line: "dir".into() },
        ];
        let glyphs: HashSet<char> = actions
            .into_iter()
            .map(|a| glyph(spec_for(&item("x", a))))
            .collect();
        assert_eq!(glyphs.len(), 4);
    }

    #[test]
    fn icon_field_wins_and_parses_the_index() {
        let with = |icon: &str| {
            let mut i = item(
                "x",
                Action::Edit {
                    command: EditCommand::Copy,
                },
            );
            i.icon = Some(icon.into());
            spec_for(&i)
        };
        let file = |path: &str, index| IconSpec::File {
            path: path.into(),
            index,
        };
        assert_eq!(with("C:\\a\\b.ico"), file("C:\\a\\b.ico", 0));
        assert_eq!(with("shell32.dll,3"), file("shell32.dll", 3));
        assert_eq!(
            with("%SystemRoot%\\x.dll , -101"),
            file("%SystemRoot%\\x.dll", -101)
        );
        assert_eq!(with("C:\\a,b\\c.ico"), file("C:\\a,b\\c.ico", 0));
        // An empty icon falls back to the action's glyph.
        assert!(matches!(with("  "), IconSpec::Glyph(_)));
    }

    #[test]
    fn letter_is_the_first_alphanumeric_character_upper_case() {
        let letter = |name: &str| spec_for(&item(name, Action::None));
        assert_eq!(letter("zebra"), IconSpec::Letter('Z'));
        assert_eq!(letter("źrebię"), IconSpec::Letter('Ź'));
        assert_eq!(letter("  łódź"), IconSpec::Letter('Ł'));
        assert_eq!(letter("- (2nd)"), IconSpec::Letter('2'));
        assert_eq!(letter("ßeta"), IconSpec::Letter('ß'));
        assert_eq!(letter(""), IconSpec::Blank);
        assert_eq!(letter(" -- "), IconSpec::Blank);
    }

    #[test]
    fn scaled_sizes() {
        assert_eq!(
            IconSizes::scaled(1.0),
            IconSizes {
                inner: 24,
                outer: 36,
                sub: 30
            }
        );
        assert_eq!(IconSizes::scaled(0.0).inner, 1);
    }
}
