//! The wheel preview: both rings and the submenu arc of the selected tile, drawn from the
//! geometry in `wheel-core`. Clicking selects a tile; dropping a file on a tile fills it.

use std::path::PathBuf;

use eframe::egui::epaint::{Mesh, PathStroke};
use eframe::egui::text::LayoutJob;
use eframe::egui::{
    Color32, Context, CursorIcon, FontId, Painter, Pos2, Sense, Shape, Stroke, Ui, Vec2, vec2,
};
use wheel_core::{Action, Hit, Layout, MAX_SCALE_PCT, SubArc, arc_for_hit, hit_test_arc};

use crate::model::{Model, Selection};
use crate::theme;

/// What the user did in the preview during this frame.
#[derive(Default)]
pub struct Outcome {
    pub select: Option<Selection>,
    pub drops: Vec<(Selection, PathBuf)>,
}

/// Angular gap between neighbouring tiles, in degrees.
const GAP_DEG: f32 = 1.5;

fn polar(center: Pos2, radius: f32, deg: f32) -> Pos2 {
    let a = deg.to_radians();
    center + vec2(a.sin(), -a.cos()) * radius
}

/// Colors of one tile.
struct Style {
    fill: Color32,
    stroke: Stroke,
    text: Color32,
}

/// Draws an annular sector between radii `r0..r1` and angles `a0..a1` (degrees).
fn annular(painter: &Painter, c: Pos2, (r0, r1): (f32, f32), (a0, a1): (f32, f32), s: &Style) {
    let steps = (((a1 - a0) / 3.0).ceil() as usize).max(1);
    let angle = |i: usize| a0 + (a1 - a0) * i as f32 / steps as f32;
    let outer: Vec<Pos2> = (0..=steps).map(|i| polar(c, r1, angle(i))).collect();
    let inner: Vec<Pos2> = (0..=steps).map(|i| polar(c, r0, angle(i))).collect();
    // The sector is concave, so it is filled as a strip of triangles.
    let mut mesh = Mesh::default();
    for p in outer.iter().chain(&inner) {
        mesh.colored_vertex(*p, s.fill);
    }
    let n = steps as u32 + 1;
    for i in 0..steps as u32 {
        mesh.add_triangle(i, i + 1, n + i);
        mesh.add_triangle(i + 1, n + i + 1, n + i);
    }
    painter.add(Shape::mesh(mesh));
    let outline: Vec<Pos2> = outer.into_iter().chain(inner.into_iter().rev()).collect();
    painter.add(Shape::closed_line(outline, PathStroke::from(s.stroke)));
}

fn label(painter: &Painter, at: Pos2, text: &str, size: f32, width: f32, color: Color32) {
    if text.trim().is_empty() {
        return;
    }
    let mut job = LayoutJob::simple(text.to_string(), FontId::proportional(size), color, width);
    job.wrap.max_rows = 2;
    let galley = painter.layout_job(job);
    let pos = at - galley.size() / 2.0;
    painter.galley(pos, galley, color);
}

/// Cursor position inside the window, in points. winit sends no cursor moves while a file is
/// dragged over the window on Windows, so the system cursor is read directly.
fn cursor_in_window(ctx: &Context) -> Option<Pos2> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut p = POINT::default();
    // SAFETY: `p` is a valid, writable POINT.
    unsafe { GetCursorPos(&mut p) }.ok()?;
    let (inner, ppp) = ctx.input(|i| (i.viewport().inner_rect, i.pixels_per_point()));
    let inner = inner?;
    Some(Pos2::new(p.x as f32 / ppp, p.y as f32 / ppp) - inner.min.to_vec2())
}

pub fn show(ui: &mut Ui, model: &Model) -> Outcome {
    let mut out = Outcome::default();
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click());
    let painter = ui.painter_at(rect);
    let dark = ui.visuals().dark_mode;
    let accent = theme::accent(dark);
    let on_accent = theme::on_accent(dark);
    let widgets = &ui.visuals().widgets;
    let text_color = widgets.noninteractive.fg_stroke.color;
    let line = widgets.noninteractive.bg_stroke.color;
    let (outer_fill, inner_fill) = (widgets.inactive.bg_fill, widgets.hovered.bg_fill);
    let empty_fill = ui.visuals().extreme_bg_color;

    let config = &model.config;
    let layout = Layout::new(96, config.options.scale_pct, config.options.outer_sectors);
    let n = layout.outer_sectors();
    let radii = layout.radii();
    // The largest scale fills the area (with its submenu arc), smaller ones are proportionally smaller.
    let largest = Layout::new(96, MAX_SCALE_PCT, config.options.outer_sectors);
    let fit = (rect.width().min(rect.height()) / largest.size_px() as f32).max(0.1);
    let center = rect.center();
    // Ring edges as in the mockup: 4 px around the center, 4 px between the rings.
    let u = layout.outer_radius() / 210.0;
    let inner_ring = (radii.center + 4.0 * u, radii.inner - 2.0 * u);
    let outer_ring = (radii.inner + 2.0 * u, layout.outer_radius());
    let (sub_in, sub_out) = layout.sub_radii();
    let scale = |(a, b): (f32, f32)| (a * fit, b * fit);

    let open_arc: Option<SubArc> = match model.selection {
        Selection::Outer(i) | Selection::Sub(i, _) => u8::try_from(i)
            .ok()
            .and_then(|i| arc_for_hit(Hit::Outer(i), config)),
        _ => None,
    };

    // Tile under a point; `None` outside the tiles.
    let tile_at = |pos: Pos2| -> Option<Selection> {
        let d: Vec2 = (pos - center) / fit;
        let dist = d.length();
        match hit_test_arc(d.x, d.y, &radii, n, open_arc) {
            Hit::Center => Some(Selection::None),
            Hit::Inner(i) => Some(Selection::Inner(usize::from(i))),
            Hit::Outer(i) => {
                (dist <= layout.outer_radius()).then_some(Selection::Outer(usize::from(i)))
            }
            Hit::Sub(p, k) => (sub_in..=sub_out)
                .contains(&dist)
                .then_some(Selection::Sub(usize::from(p), usize::from(k))),
        }
    };

    if response.clicked()
        && let Some(pos) = response.interact_pointer_pos()
        && let Some(s) = tile_at(pos)
    {
        out.select = Some(s);
    }
    if response.hover_pos().and_then(tile_at).is_some() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }

    let dragging = ui.input(|i| !i.raw.hovered_files.is_empty());
    let dropped: Vec<PathBuf> = ui.input(|i| {
        i.raw
            .dropped_files
            .iter()
            .map(|f| f.path().to_path_buf())
            .collect()
    });
    let drop_pos = (dragging || !dropped.is_empty())
        .then(|| cursor_in_window(ui.ctx()).or_else(|| ui.input(|i| i.pointer.hover_pos())))
        .flatten();
    let drop_target = drop_pos.and_then(tile_at).filter(|s| *s != Selection::None);
    if dragging {
        ui.ctx().request_repaint();
    }
    if let (Some(target), Some(path)) = (drop_target, dropped.first()) {
        out.drops.push((target, path.clone()));
    }

    let parent_selected = match model.selection {
        Selection::Sub(p, _) => Some(p),
        _ => None,
    };
    let tile_style = |sel: Selection, filled: bool, base: Color32| -> Style {
        let selected = model.selection == sel;
        let target = dragging && drop_target == Some(sel);
        let parent = matches!((sel, parent_selected), (Selection::Outer(i), Some(p)) if i == p);
        Style {
            fill: if selected {
                accent
            } else if filled {
                base
            } else {
                empty_fill
            },
            stroke: if target || parent {
                Stroke::new(3.0, accent)
            } else {
                Stroke::new(1.0, line)
            },
            text: if selected { on_accent } else { text_color },
        }
    };

    for (i, item) in config.inner.iter().enumerate() {
        let (a0, a1) = layout.inner_span(i as u8);
        let st = tile_style(Selection::Inner(i), item.action != Action::None, inner_fill);
        let r = scale(inner_ring);
        annular(&painter, center, r, (a0 + GAP_DEG, a1 - GAP_DEG), &st);
        label(
            &painter,
            polar(center, (r.0 + r.1) / 2.0, (a0 + a1) / 2.0),
            &item.name,
            (11.0 * fit).clamp(8.0, 15.0),
            (r.1 - r.0) * 0.8,
            st.text,
        );
    }

    for (i, item) in config.outer.iter().enumerate().take(usize::from(n)) {
        let (a0, a1) = layout.outer_span(i as u8);
        let st = tile_style(Selection::Outer(i), item.action != Action::None, outer_fill);
        let r = scale(outer_ring);
        annular(&painter, center, r, (a0 + GAP_DEG, a1 - GAP_DEG), &st);
        let mid = (a0 + a1) / 2.0;
        label(
            &painter,
            polar(center, (r.0 + r.1) / 2.0, mid),
            &item.name,
            (13.0 * fit).clamp(9.0, 17.0),
            (r.1 - r.0) * 0.85,
            st.text,
        );
        if !item.submenu.is_empty() {
            // A short line at the outer edge marks a tile with a submenu.
            let mark_r = r.1 - 7.0 * fit;
            painter.line_segment(
                [
                    polar(center, mark_r, mid - 6.0),
                    polar(center, mark_r, mid + 6.0),
                ],
                Stroke::new(2.0, st.text),
            );
        }
    }

    painter.circle(
        center,
        radii.center * fit,
        empty_fill,
        Stroke::new(1.0, line),
    );

    // Submenu arc of the selected outer tile (or of the parent of the selected submenu tile).
    if let Some(arc) = open_arc {
        let sub = &config.outer[usize::from(arc.parent)].submenu;
        let r = scale((sub_in, sub_out));
        for (k, item) in sub.iter().enumerate().take(usize::from(arc.count)) {
            let (a0, a1) = layout.sub_span(arc, k as u8);
            let st = tile_style(
                Selection::Sub(usize::from(arc.parent), k),
                item.action != Action::None,
                outer_fill,
            );
            annular(&painter, center, r, (a0 + GAP_DEG, a1 - GAP_DEG), &st);
            label(
                &painter,
                polar(center, (r.0 + r.1) / 2.0, (a0 + a1) / 2.0),
                &item.name,
                (12.0 * fit).clamp(9.0, 16.0),
                (r.1 - r.0) * 0.9,
                st.text,
            );
        }
    }
    out
}
