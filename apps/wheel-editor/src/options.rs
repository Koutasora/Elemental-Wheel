//! The options panel.

use eframe::egui::{DragValue, RichText, Slider, TextEdit, Ui};
use wheel_core::{
    MAX_HOLD_MS, MAX_MOVE_PX, MAX_OUTER_SECTORS, MAX_SCALE_PCT, MIN_HOLD_MS, MIN_MOVE_PX,
    MIN_OUTER_SECTORS, MIN_SCALE_PCT, ThemeChoice, ViewMode,
};

use crate::model::Model;

fn caption(ui: &mut Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(text).strong());
}

pub fn show(ui: &mut Ui, model: &mut Model) {
    ui.heading("Opcje koła");

    caption(ui, "Motyw");
    let o = &mut model.config.options;
    ui.horizontal(|ui| {
        ui.selectable_value(&mut o.theme, ThemeChoice::System, "Systemowy");
        ui.selectable_value(&mut o.theme, ThemeChoice::Light, "Jasny");
        ui.selectable_value(&mut o.theme, ThemeChoice::Dark, "Ciemny");
    });

    caption(ui, "Widok kafelków");
    ui.horizontal(|ui| {
        ui.selectable_value(&mut o.view, ViewMode::Names, "Nazwy");
        ui.selectable_value(&mut o.view, ViewMode::Icons, "Ikony");
    });

    caption(ui, "Rozmiar koła");
    ui.add(
        Slider::new(&mut o.scale_pct, MIN_SCALE_PCT..=MAX_SCALE_PCT)
            .step_by(10.0)
            .suffix("%"),
    );

    caption(ui, "Liczba sektorów zewnętrznych");
    let mut sectors = o.outer_sectors;
    if ui
        .add(Slider::new(
            &mut sectors,
            MIN_OUTER_SECTORS..=MAX_OUTER_SECTORS,
        ))
        .changed()
    {
        model.set_outer_sectors(sectors);
    }
    let o = &mut model.config.options;

    caption(ui, "Czas przytrzymania prawego przycisku");
    ui.add(
        DragValue::new(&mut o.hold_ms)
            .range(MIN_HOLD_MS..=MAX_HOLD_MS)
            .suffix(" ms"),
    );

    caption(ui, "Ruch uznawany za przeciąganie");
    ui.add(
        DragValue::new(&mut o.move_px)
            .range(MIN_MOVE_PX..=MAX_MOVE_PX)
            .suffix(" px"),
    );

    ui.add_space(8.0);
    ui.checkbox(
        &mut o.disable_in_fullscreen,
        "Wyłącz koło w trybie pełnoekranowym",
    );

    caption(ui, "Wykluczone programy");
    let mut remove = None;
    for (i, exe) in o.excluded.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 70.0).max(60.0);
            ui.add(
                TextEdit::singleline(exe)
                    .hint_text("np. blender.exe")
                    .desired_width(width),
            );
            if ui.button("Usuń").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        o.excluded.remove(i);
    }
    if ui.button("Dodaj program").clicked() {
        o.excluded.push(String::new());
    }
}
