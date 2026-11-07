// In release builds there is no console window; `--version` attaches to the parent console.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod icons;
mod messages;
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
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align, CentralPanel, Context, FontData, FontDefinitions, FontFamily, Frame, Id, Key,
    Layout, Modal, Modifiers, Panel, RichText, ScrollArea, Ui, ViewportCommand,
};
use wheel_core::{Source, ThemeChoice, config};

use model::{ExternalChange, Model, Selection, read_stamp};

const TITLE: &str = "Elemental Wheel – edytor";
/// How often the config file is checked for a change made outside the editor.
const CHECK_INTERVAL: Duration = Duration::from_secs(2);
const FONT_PATH: &str = r"C:\Windows\Fonts\segoeui.ttf";
/// Icon fonts in order of preference: Segoe Fluent Icons, else Segoe MDL2 Assets (Windows 10).
const ICON_FONT_PATHS: [&str; 2] = [
    r"C:\Windows\Fonts\SegoeIcons.ttf",
    r"C:\Windows\Fonts\segmdl2.ttf",
];
/// Name of the font family with the icon glyphs.
pub const ICON_FAMILY: &str = "icons";

enum Command {
    Version,
    /// Config path and log path from the command line.
    Run(Option<PathBuf>, Option<PathBuf>),
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut config = None;
    let mut log = None;
    let mut args = args;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--version" => return Ok(Command::Version),
            "--config" => {
                let value = args.next().ok_or("--config needs a path")?;
                config = Some(PathBuf::from(value));
            }
            "--log" => {
                let value = args.next().ok_or("--log needs a path")?;
                log = Some(PathBuf::from(value));
            }
            other => return Err(format!("Unknown argument: {other}")),
        }
    }
    Ok(Command::Run(config, log))
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

/// Opens the log file and routes panics to it. A log that cannot be opened is not fatal.
fn start_log(arg: Option<PathBuf>) {
    wheel_core::log::install_panic_hook();
    let Some(path) = arg.or_else(|| wheel_core::log::default_path("editor.log")) else {
        wheel_core::log_line!("wheel-editor: LOCALAPPDATA is not set; no log file");
        return;
    };
    if let Err(e) = wheel_core::log::init(&path) {
        wheel_core::log_line!("wheel-editor: cannot open the log {}: {e}", path.display());
    }
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
        Ok(Command::Run(arg, log)) => {
            start_log(log);
            match config_path(arg) {
                Ok(path) => path,
                Err(e) => {
                    console_print(&format!("wheel-editor: {e}"), true);
                    return ExitCode::from(1);
                }
            }
        }
        Err(e) => {
            console_print(&e, true);
            return ExitCode::from(2);
        }
    };
    wheel_core::log_line!("editor {} started", env!("CARGO_PKG_VERSION"));
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
            let icon_font = install_font(&cc.egui_ctx);
            let model = Model::open(path);
            theme::install(&cc.egui_ctx, model.config.options.theme);
            Ok(Box::new(App::new(model, icon_font)))
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

/// Segoe UI from the system, so that Polish letters look native (the built-in font stays as
/// the fallback), and the Segoe icon font as the `ICON_FAMILY` family. Returns whether the icon
/// font was found.
fn install_font(ctx: &Context) -> bool {
    let mut fonts = FontDefinitions::default();
    if let Ok(bytes) = std::fs::read(FONT_PATH) {
        fonts
            .font_data
            .insert("segoeui".to_string(), Arc::new(FontData::from_owned(bytes)));
        if let Some(list) = fonts.families.get_mut(&FontFamily::Proportional) {
            list.insert(0, "segoeui".to_string());
        }
    }
    let icon_font = ICON_FONT_PATHS
        .iter()
        .find_map(|path| std::fs::read(path).ok());
    let found = icon_font.is_some();
    if let Some(bytes) = icon_font {
        fonts.font_data.insert(
            "segoeicons".to_string(),
            Arc::new(FontData::from_owned(bytes)),
        );
        fonts.families.insert(
            FontFamily::Name(ICON_FAMILY.into()),
            vec!["segoeicons".to_string()],
        );
    }
    ctx.set_fonts(fonts);
    found
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
    /// The file changed outside the editor while there are unsaved changes.
    External,
    /// Saving would overwrite a file changed outside the editor.
    Overwrite,
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
        lines.push(format!("Nie wczytano pliku: {}", messages::pl(e)));
    }
    lines.extend(
        model
            .load_warnings
            .iter()
            .map(|w| format!("Ostrzeżenie: {}", messages::pl(w))),
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
    icons: icons::Icons,
    last_check: Instant,
    was_focused: bool,
}

impl App {
    fn new(model: Model, icon_font: bool) -> App {
        App {
            status: load_status(&model),
            applied_theme: model.config.options.theme,
            model,
            tab: Tab::Tile,
            dialog: None,
            allow_close: false,
            title: String::new(),
            icons: icons::Icons::new(icon_font),
            last_check: Instant::now(),
            was_focused: true,
        }
    }

    fn save(&mut self) -> bool {
        match self.model.save() {
            Ok(warnings) => {
                let mut lines = vec!["Zapisano.".to_string()];
                lines.extend(
                    warnings
                        .iter()
                        .map(|w| format!("Ostrzeżenie: {}", messages::pl(w))),
                );
                self.status = Some(Status {
                    error: false,
                    lines,
                });
                true
            }
            Err(e) => {
                self.status = Some(Status::error(format!("Nie zapisano: {}", messages::pl(&e))));
                false
            }
        }
    }

    fn reload(&mut self) {
        self.model.reload();
        self.status = load_status(&self.model)
            .or_else(|| Some(Status::info("Wczytano ponownie.".to_string())));
    }

    /// Saves, but first asks when the file changed outside the editor since it was loaded.
    fn save_checked(&mut self) {
        if self.model.disk_changed(read_stamp(&self.model.path)) {
            self.dialog = Some(Dialog::Overwrite);
        } else {
            self.save();
        }
    }

    /// Looks for a change of the file made outside the editor.
    fn check_external(&mut self) {
        self.last_check = Instant::now();
        match self.model.check_external(read_stamp(&self.model.path)) {
            ExternalChange::None => {}
            ExternalChange::ReloadedSilently => {
                let note = "Plik zmieniony poza edytorem – wczytano ponownie".to_string();
                self.status = Some(match load_status(&self.model) {
                    Some(mut status) => {
                        status.lines.insert(0, note);
                        status
                    }
                    None => Status::info(note),
                });
            }
            ExternalChange::NeedsDecision => self.dialog = Some(Dialog::External),
            ExternalChange::Vanished => {
                self.status = Some(Status::info(
                    "Plik konfiguracji usunięty poza edytorem".to_string(),
                ));
            }
        }
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
                        self.save_checked();
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

    /// The questions about a file changed outside the editor.
    fn external_dialog(&mut self, ctx: &Context) {
        let external = matches!(self.dialog, Some(Dialog::External));
        let mut choice = None;
        let response = Modal::new(Id::unique("dialog")).show(ctx, |ui| {
            ui.set_width(340.0);
            ui.heading("Plik zmieniony");
            ui.add_space(6.0);
            ui.label(if external {
                "Plik konfiguracji zmienił się poza edytorem."
            } else {
                "Plik zmienił się od wczytania. Nadpisać?"
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let (yes, no) = if external {
                    ("Wczytaj plik", "Zostaw moje zmiany")
                } else {
                    ("Nadpisz", "Anuluj")
                };
                if ui.button(yes).clicked() {
                    choice = Some(true);
                }
                if ui.button(no).clicked() {
                    choice = Some(false);
                }
            });
        });
        if choice.is_none() && response.should_close() {
            choice = Some(false);
        }
        match (choice, external) {
            (None, _) => return,
            (Some(true), true) => self.reload(),
            (Some(false), true) => self.model.keep_mine(read_stamp(&self.model.path)),
            (Some(true), false) => {
                self.save();
            }
            (Some(false), false) => {}
        }
        self.dialog = None;
        self.last_check = Instant::now();
    }

    fn dialog(&mut self, ctx: &Context) {
        let Some(kind) = &self.dialog else {
            return;
        };
        if matches!(kind, Dialog::External | Dialog::Overwrite) {
            self.external_dialog(ctx);
            return;
        }
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
            self.save_checked();
        }

        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        let regained = focused && !self.was_focused;
        self.was_focused = focused;
        if self.dialog.is_none() && (regained || self.last_check.elapsed() >= CHECK_INTERVAL) {
            self.check_external();
        }
        ctx.request_repaint_after(CHECK_INTERVAL);

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
            self.icons.poll(ui.ctx());
            let out = preview::show(ui, &self.model, &mut self.icons);
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
