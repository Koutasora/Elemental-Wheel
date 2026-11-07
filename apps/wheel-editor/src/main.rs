// In release builds there is no console window; `--version` attaches to the parent console.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod model;
mod options;
mod panel;
mod preview;
mod theme;

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use eframe::egui::{
    self, Align, CentralPanel, Context, FontData, FontDefinitions, FontFamily, Frame, Id, Key,
    Layout, Modal, Modifiers, Panel, RichText, ScrollArea, Ui, ViewportCommand,
};
use wheel_core::{Source, ThemeChoice, config};

use model::{Model, Selection};

const TITLE: &str = "Elemental Wheel – edytor";
const FONT_PATH: &str = r"C:\Windows\Fonts\segoeui.ttf";

enum Command {
    Version,
    Run(Option<PathBuf>),
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut config = None;
    let mut args = args;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--version" => return Ok(Command::Version),
            "--config" => {
                let value = args.next().ok_or("--config needs a path")?;
                config = Some(PathBuf::from(value));
            }
            other => return Err(format!("Unknown argument: {other}")),
        }
    }
    Ok(Command::Run(config))
}

/// Writes to the console of the parent process: a program built for the `windows` subsystem has
/// no standard handles of its own, unless the caller redirected them.
fn console_print(text: &str, to_stderr: bool) {
    use windows::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE,
    };
    // SAFETY: plain Win32 calls without pointers; failure only means there is no console.
    let redirected = unsafe {
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
        GetStdHandle(if to_stderr {
            STD_ERROR_HANDLE
        } else {
            STD_OUTPUT_HANDLE
        })
    }
    .is_ok_and(|h| !h.is_invalid());
    if redirected {
        if to_stderr {
            let _ = writeln!(std::io::stderr(), "{text}");
        } else {
            let _ = writeln!(std::io::stdout(), "{text}");
        }
    } else if let Ok(mut console) = File::options().write(true).open("CONOUT$") {
        let _ = writeln!(console, "{text}");
    }
}

fn config_path(arg: Option<PathBuf>) -> Result<PathBuf, String> {
    if let Some(path) = arg {
        return Ok(path);
    }
    std::env::var_os("APPDATA")
        .map(|appdata| config::default_path(Path::new(&appdata)))
        .ok_or_else(|| "The APPDATA variable is not set; use --config <path>".to_string())
}

fn main() -> ExitCode {
    let path = match parse_args(std::env::args().skip(1)) {
        Ok(Command::Version) => {
            console_print(
                &format!("Elemental Wheel Editor {}", env!("CARGO_PKG_VERSION")),
                false,
            );
            return ExitCode::SUCCESS;
        }
        Ok(Command::Run(arg)) => match config_path(arg) {
            Ok(path) => path,
            Err(e) => {
                console_print(&format!("wheel-editor: {e}"), true);
                return ExitCode::from(1);
            }
        },
        Err(e) => {
            console_print(&e, true);
            return ExitCode::from(2);
        }
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(TITLE)
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([820.0, 520.0]),
        ..Default::default()
    };
    let result = eframe::run_native(
        TITLE,
        options,
        Box::new(move |cc| {
            install_font(&cc.egui_ctx);
            let model = Model::open(path);
            theme::install(&cc.egui_ctx, model.config.options.theme);
            Ok(Box::new(App::new(model)))
        }),
    );
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            console_print(&format!("wheel-editor: {e}"), true);
            ExitCode::from(1)
        }
    }
}

/// Segoe UI from the system, so that Polish letters look native; the built-in font stays as
/// the fallback.
fn install_font(ctx: &Context) {
    let Ok(bytes) = std::fs::read(FONT_PATH) else {
        return;
    };
    let mut fonts = FontDefinitions::default();
    fonts
        .font_data
        .insert("segoeui".to_string(), Arc::new(FontData::from_owned(bytes)));
    if let Some(list) = fonts.families.get_mut(&FontFamily::Proportional) {
        list.insert(0, "segoeui".to_string());
    }
    ctx.set_fonts(fonts);
}

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Tile,
    Options,
}

enum Dialog {
    /// The window was asked to close with unsaved changes.
    Close,
    /// "Wczytaj ponownie" with unsaved changes.
    Reload,
}

/// A message in the bottom bar.
struct Status {
    error: bool,
    lines: Vec<String>,
}

impl Status {
    fn info(line: String) -> Status {
        Status {
            error: false,
            lines: vec![line],
        }
    }

    fn error(line: String) -> Status {
        Status {
            error: true,
            lines: vec![line],
        }
    }
}

/// What to tell the user right after loading.
fn load_status(model: &Model) -> Option<Status> {
    let mut lines = Vec::new();
    if let Some(e) = &model.load_error {
        lines.push(format!("Nie wczytano pliku: {e}"));
    }
    lines.extend(
        model
            .load_warnings
            .iter()
            .map(|w| format!("Ostrzeżenie: {w}")),
    );
    (!lines.is_empty()).then_some(Status {
        error: model.load_error.is_some(),
        lines,
    })
}

fn source_label(model: &Model) -> &'static str {
    match model.source {
        Source::File | Source::Created => "plik",
        Source::Backup => "kopia .bak",
        Source::Defaults => "domyślna konfiguracja",
    }
}

struct App {
    model: Model,
    tab: Tab,
    status: Option<Status>,
    dialog: Option<Dialog>,
    allow_close: bool,
    applied_theme: ThemeChoice,
    title: String,
}

impl App {
    fn new(model: Model) -> App {
        App {
            status: load_status(&model),
            applied_theme: model.config.options.theme,
            model,
            tab: Tab::Tile,
            dialog: None,
            allow_close: false,
            title: String::new(),
        }
    }

    fn save(&mut self) -> bool {
        match self.model.save() {
            Ok(warnings) => {
                let mut lines = vec!["Zapisano.".to_string()];
                lines.extend(warnings.into_iter().map(|w| format!("Ostrzeżenie: {w}")));
                self.status = Some(Status {
                    error: false,
                    lines,
                });
                true
            }
            Err(e) => {
                self.status = Some(Status::error(format!("Nie zapisano: {e}")));
                false
            }
        }
    }

    fn reload(&mut self) {
        self.model.reload();
        self.status = load_status(&self.model)
            .or_else(|| Some(Status::info("Wczytano ponownie.".to_string())));
    }

    fn bottom_bar(&mut self, ui: &mut Ui) {
        Panel::bottom("status").show(ui, |ui| {
            if let Some(status) = &self.status {
                ScrollArea::vertical().max_height(64.0).show(ui, |ui| {
                    for line in &status.lines {
                        if status.error {
                            let color = ui.visuals().error_fg_color;
                            ui.label(RichText::new(line).color(color));
                        } else {
                            ui.label(line);
                        }
                    }
                });
            }
            ui.horizontal(|ui| {
                let dirty = self.model.is_dirty();
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Zapisz").on_hover_text("Ctrl+S").clicked() {
                        self.save();
                    }
                    if ui.button("Wczytaj ponownie").clicked() {
                        if dirty {
                            self.dialog = Some(Dialog::Reload);
                        } else {
                            self.reload();
                        }
                    }
                    if dirty {
                        ui.label(RichText::new("Niezapisane zmiany").strong());
                    }
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        let text = format!(
                            "{} ({})",
                            self.model.path.display(),
                            source_label(&self.model)
                        );
                        ui.add(egui::Label::new(text).truncate());
                    });
                });
            });
        });
    }

    fn side_panel(&mut self, ui: &mut Ui) {
        Panel::right("side")
            .default_size(400.0)
            .min_size(340.0)
            .resizable(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.tab, Tab::Tile, "Kafelek");
                    ui.selectable_value(&mut self.tab, Tab::Options, "Opcje");
                });
                ui.separator();
                ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| match self.tab {
                        Tab::Tile => panel::show(ui, &mut self.model),
                        Tab::Options => options::show(ui, &mut self.model),
                    });
            });
    }

    fn dialog(&mut self, ctx: &Context) {
        let Some(kind) = &self.dialog else {
            return;
        };
        let closing = matches!(kind, Dialog::Close);
        let mut choice = None;
        let response = Modal::new(Id::unique("dialog")).show(ctx, |ui| {
            ui.set_width(340.0);
            ui.heading("Niezapisane zmiany");
            ui.add_space(6.0);
            ui.label(if closing {
                "Zapisać zmiany przed zamknięciem?"
            } else {
                "Odrzucić zmiany i wczytać plik ponownie?"
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if closing {
                    if ui.button("Zapisz").clicked() {
                        choice = Some(1);
                    }
                    if ui.button("Nie zapisuj").clicked() {
                        choice = Some(2);
                    }
                } else if ui.button("Wczytaj ponownie").clicked() {
                    choice = Some(2);
                }
                if ui.button("Anuluj").clicked() {
                    choice = Some(0);
                }
            });
        });
        if choice.is_none() && response.should_close() {
            choice = Some(0);
        }
        match (choice, closing) {
            (None, _) => return,
            (Some(1), _) => {
                if self.save() {
                    self.allow_close = true;
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
            }
            (Some(2), true) => {
                self.allow_close = true;
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
            (Some(2), false) => self.reload(),
            _ => {}
        }
        self.dialog = None;
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        let choice = self.model.config.options.theme;
        if choice != self.applied_theme {
            theme::apply(&ctx, choice);
            self.applied_theme = choice;
        }

        if ctx.input(|i| i.viewport().close_requested())
            && !self.allow_close
            && self.model.is_dirty()
        {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.dialog = Some(Dialog::Close);
        }
        if ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::S)) {
            self.save();
        }

        let title = if self.model.is_dirty() {
            format!("{TITLE} *")
        } else {
            TITLE.to_string()
        };
        if title != self.title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.title = title;
        }

        self.bottom_bar(ui);
        self.side_panel(ui);
        CentralPanel::default().frame(Frame::NONE).show(ui, |ui| {
            let out = preview::show(ui, &self.model);
            if let Some(selection) = out.select {
                self.model.set_selection(selection);
                if selection != Selection::None {
                    self.tab = Tab::Tile;
                }
            }
            for (target, path) in out.drops {
                self.model.drop_file(target, &path);
                self.tab = Tab::Tile;
            }
        });
        self.dialog(&ctx);
    }
}
