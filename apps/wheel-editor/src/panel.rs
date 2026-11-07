//! The tile panel: edits the item selected in the preview.

use std::path::PathBuf;

use eframe::egui::{ComboBox, RichText, TextEdit, Ui};
use rfd::FileDialog;
use wheel_core::{
    Action, EditCommand, MAX_SUB_TILES, SystemAction, clamp_outer_sectors, parse_shortcut,
};

use crate::model::{ActionKind, Model, Selection, edit_label, system_label};

/// A structural change chosen in the panel, applied after drawing.
enum Op {
    Select(Selection),
    Clear,
    AddItem,
    AddSub(usize),
    RemoveSub(usize, usize),
    MoveSub(usize, usize, bool),
}

fn caption(ui: &mut Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(text).strong());
}

/// A text field filling the row.
fn text(ui: &mut Ui, value: &mut String, hint: &str) -> bool {
    ui.add(
        TextEdit::singleline(value)
            .hint_text(hint)
            .desired_width(f32::INFINITY),
    )
    .changed()
}

/// A text field with a button to its right; returns whether the text changed and whether the
/// button was clicked.
fn text_with_button(ui: &mut Ui, value: &mut String, hint: &str, button: &str) -> (bool, bool) {
    ui.horizontal(|ui| {
        let width = (ui.available_width() - 100.0).max(60.0);
        let changed = ui
            .add(
                TextEdit::singleline(value)
                    .hint_text(hint)
                    .desired_width(width),
            )
            .changed();
        (changed, ui.button(button).clicked())
    })
    .inner
}

fn path_text(path: Option<PathBuf>) -> Option<String> {
    path.map(|p| p.to_string_lossy().into_owned())
}

fn pick_program() -> Option<String> {
    path_text(
        FileDialog::new()
            .add_filter("Programy i skróty", &["exe", "lnk", "bat", "cmd"])
            .add_filter("Wszystkie pliki", &["*"])
            .pick_file(),
    )
}

fn pick_file() -> Option<String> {
    path_text(FileDialog::new().pick_file())
}

fn pick_folder() -> Option<String> {
    path_text(FileDialog::new().pick_folder())
}

fn pick_icon() -> Option<String> {
    path_text(
        FileDialog::new()
            .add_filter("Ikony i programy", &["ico", "exe", "dll"])
            .pick_file(),
    )
}

fn location(model: &Model) -> String {
    let n = usize::from(clamp_outer_sectors(model.config.options.outer_sectors));
    match model.selection {
        Selection::Outer(i) => format!(
            "pierścień zewnętrzny, sektor {}/{n}, {:.0}°",
            i + 1,
            i as f32 * 360.0 / n as f32
        ),
        Selection::Inner(i) => format!("pierścień wewnętrzny, sektor {}/12, {}°", i + 1, i * 30),
        Selection::Sub(p, k) => format!(
            "podmenu, sektor {}, kafelek {}/{}",
            p + 1,
            k + 1,
            model.config.outer.get(p).map_or(0, |o| o.submenu.len())
        ),
        Selection::None => String::new(),
    }
}

pub fn show(ui: &mut Ui, model: &mut Model) {
    ui.heading("Kafelek");
    let Some(original) = model.selected().cloned() else {
        ui.label("Wybierz kafelek w podglądzie.");
        return;
    };
    ui.label(location(model));
    let selection = model.selection;
    let mut op = None;

    if original.action == Action::None && !matches!(selection, Selection::Sub(..)) {
        ui.add_space(8.0);
        ui.label("Pusty slot.");
        if ui.button("Dodaj element").clicked() {
            op = Some(Op::AddItem);
        }
        apply(model, op);
        return;
    }

    let mut edited = original.clone();

    caption(ui, "Nazwa");
    text(ui, &mut edited.name, "");

    caption(ui, "Typ akcji");
    let current = ActionKind::of(&original.action);
    let mut new_kind = None;
    ComboBox::from_id_salt("action_kind")
        .width(ui.available_width())
        .selected_text(current.map_or("", ActionKind::label))
        .show_ui(ui, |ui| {
            for kind in ActionKind::ALL {
                if ui
                    .selectable_label(current == Some(kind), kind.label())
                    .clicked()
                {
                    new_kind = Some(kind);
                }
            }
        });

    match &mut edited.action {
        Action::Program {
            path,
            args,
            working_dir,
        } => {
            caption(ui, "Ścieżka programu");
            let (_, pick) = text_with_button(ui, path, "np. wt.exe", "Wybierz…");
            if pick && let Some(p) = pick_program() {
                *path = p;
            }
            caption(ui, "Argumenty");
            text(ui, args, "");
            caption(ui, "Folder roboczy");
            let mut dir = working_dir.clone().unwrap_or_default();
            let (changed, pick) = text_with_button(ui, &mut dir, "domyślny", "Wybierz…");
            if pick && let Some(p) = pick_folder() {
                dir = p;
                *working_dir = Some(dir.clone());
            } else if changed {
                *working_dir = (!dir.trim().is_empty()).then_some(dir);
            }
        }
        Action::Open { path } => {
            caption(ui, "Ścieżka pliku lub folderu");
            text(ui, path, "np. %USERPROFILE%\\Downloads");
            ui.horizontal(|ui| {
                if ui.button("Wybierz plik…").clicked()
                    && let Some(p) = pick_file()
                {
                    *path = p;
                }
                if ui.button("Wybierz folder…").clicked()
                    && let Some(p) = pick_folder()
                {
                    *path = p;
                }
            });
        }
        Action::Url { url } => {
            caption(ui, "Adres");
            text(ui, url, "https://…");
        }
        Action::Shortcut { keys } => {
            caption(ui, "Skrót klawiszowy");
            text(ui, keys, "np. Ctrl+Shift+T");
            if let Err(e) = parse_shortcut(keys) {
                let color = ui.visuals().error_fg_color;
                ui.label(RichText::new(e).color(color));
            }
        }
        Action::Edit { command } => {
            caption(ui, "Komenda");
            let old = *command;
            ComboBox::from_id_salt("edit_command")
                .width(ui.available_width())
                .selected_text(format!("{} ({})", edit_label(old), old.shortcut()))
                .show_ui(ui, |ui| {
                    for c in EditCommand::ALL {
                        let label = format!("{} ({})", edit_label(c), c.shortcut());
                        ui.selectable_value(command, c, label);
                    }
                });
            // As in the mockup: a name that was just the command's name follows the command.
            if *command != old && edited.name == edit_label(old) {
                edited.name = edit_label(*command).to_string();
            }
        }
        Action::PasteText { text } => {
            caption(ui, "Tekst do wklejenia");
            ui.add(
                TextEdit::multiline(text)
                    .desired_rows(4)
                    .desired_width(f32::INFINITY),
            );
        }
        Action::Command { line } => {
            caption(ui, "Polecenie");
            text(ui, line, "np. powershell -File skrypt.ps1");
        }
        Action::System { action } => {
            caption(ui, "Akcja");
            ComboBox::from_id_salt("system_action")
                .width(ui.available_width())
                .selected_text(system_label(*action))
                .show_ui(ui, |ui| {
                    for a in SystemAction::ALL {
                        ui.selectable_value(action, a, system_label(a));
                    }
                });
        }
        Action::None => {}
    }

    caption(ui, "Ikona");
    let mut icon = edited.icon.clone().unwrap_or_default();
    let (changed, pick) =
        text_with_button(ui, &mut icon, "z programu, jeśli to możliwe", "Wybierz…");
    if pick && let Some(p) = pick_icon() {
        icon = p;
        edited.icon = Some(icon.clone());
    } else if changed {
        edited.icon = Some(icon);
    }

    ui.add_space(10.0);
    match selection {
        Selection::Outer(parent) => {
            submenu_list(ui, model, parent, &mut op);
            ui.add_space(6.0);
            if ui.button("Opróżnij slot").clicked() {
                op = Some(Op::Clear);
            }
        }
        Selection::Inner(_) => {
            if ui.button("Opróżnij slot").clicked() {
                op = Some(Op::Clear);
            }
        }
        Selection::Sub(parent, _) => {
            ui.horizontal(|ui| {
                if ui.button("Usuń z podmenu").clicked() {
                    op = Some(Op::Clear);
                }
                if ui.button("Kafelek nadrzędny").clicked() {
                    op = Some(Op::Select(Selection::Outer(parent)));
                }
            });
        }
        Selection::None => {}
    }

    if let Some(kind) = new_kind {
        model.set_action_kind(kind);
    } else {
        if edited.name != original.name {
            model.set_name(&edited.name);
        }
        if edited.icon != original.icon {
            model.set_icon(edited.icon.as_deref().unwrap_or_default());
        }
        if edited.action != original.action
            && let Some(item) = model.selected_mut()
        {
            item.action = edited.action;
        }
    }
    apply(model, op);
}

fn submenu_list(ui: &mut Ui, model: &Model, parent: usize, op: &mut Option<Op>) {
    let sub = model
        .config
        .outer
        .get(parent)
        .map(|o| o.submenu.as_slice())
        .unwrap_or_default();
    caption(ui, &format!("Podmenu ({}/{MAX_SUB_TILES})", sub.len()));
    for (k, item) in sub.iter().enumerate() {
        ui.horizontal(|ui| {
            let name = if item.name.trim().is_empty() {
                "(bez nazwy)"
            } else {
                item.name.as_str()
            };
            if ui.button(name).clicked() {
                *op = Some(Op::Select(Selection::Sub(parent, k)));
            }
            ui.add_space(4.0);
            ui.add_enabled_ui(k > 0, |ui| {
                if ui.button("W górę").clicked() {
                    *op = Some(Op::MoveSub(parent, k, true));
                }
            });
            ui.add_enabled_ui(k + 1 < sub.len(), |ui| {
                if ui.button("W dół").clicked() {
                    *op = Some(Op::MoveSub(parent, k, false));
                }
            });
            if ui.button("Usuń").clicked() {
                *op = Some(Op::RemoveSub(parent, k));
            }
        });
    }
    ui.add_enabled_ui(sub.len() < usize::from(MAX_SUB_TILES), |ui| {
        if ui.button("Dodaj kafelek podmenu").clicked() {
            *op = Some(Op::AddSub(parent));
        }
    });
}

fn apply(model: &mut Model, op: Option<Op>) {
    match op {
        None => {}
        Some(Op::Select(s)) => model.set_selection(s),
        Some(Op::Clear) => model.clear_slot(),
        Some(Op::AddItem) => model.add_item(),
        Some(Op::AddSub(p)) => model.add_sub(p),
        Some(Op::RemoveSub(p, k)) => model.remove_sub(p, k),
        Some(Op::MoveSub(p, k, up)) => model.move_sub(p, k, up),
    }
}
