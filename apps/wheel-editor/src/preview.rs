//! The wheel preview: both rings and the submenu arc of the selected tile, drawn from the
//! geometry in `wheel-core`. Clicking selects a tile; dropping a file on a tile fills it.

use std::path::PathBuf;

use eframe::egui::epaint::{Mesh, PathStroke};
use eframe::egui::text::LayoutJob;
use eframe::egui::{
    Align2, Color32, Context, CornerRadius, CursorIcon, FontFamily, FontId, Painter, Pos2, Rect,
    Sense, Shape, Stroke, StrokeKind, Ui, Vec2, pos2, vec2,
};
use wheel_core::icon::IconSizes;
use wheel_core::{
    Action, Hit, Item, Layout, MAX_SCALE_PCT, SubArc, ViewMode, arc_for_hit, hit_test_arc,
};

use crate::ICON_FAMILY;
use crate::icons::{Drawn, Icons};
use crate::model::{Model, Selection};
use crate::theme;

/// What the user did in the preview during this frame.
#[derive(Default)]
pub struct Outcome {
    pub select: Option<Selection>,
    pub drops: Vec<(Selection, PathBuf)>,
}

/// Corner radius of a tile at wheel scale 1 (100%), in layout pixels.
const CORNER_PX: f32 = 6.0;
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

/// Points of a quarter-ish arc of radius `rc` around `centre`, from direction `from` to `to`
/// (degrees, 0 = up, clockwise).
fn corner(centre: Pos2, rc: f32, (from, to): (f32, f32)) -> impl Iterator<Item = Pos2> {
    const STEPS: usize = 5;
    (0..=STEPS).map(move |i| polar(centre, rc, from + (to - from) * i as f32 / STEPS as f32))
}

/// Outline of an annular sector between radii `r0..r1` and angles `a0..a1` (degrees) with
/// corners rounded by at most `round`, as two chains of equal length running from the `a0` side
/// to the `a1` side: the outer edge and the inner edge.
fn sector_chains(
    c: Pos2,
    (r0, r1): (f32, f32),
    (a0, a1): (f32, f32),
    round: f32,
) -> (Vec<Pos2>, Vec<Pos2>) {
    let width = r0 * (a1 - a0).to_radians();
    let rc = round.min((r1 - r0) * 0.3).min(width * 0.3).max(0.0);
    // Corner circles touch the arc and the radial edge: their centres lie `rc` inside the arc,
    // turned away from the edge by the angle `asin(rc / radius)`.
    let (ro, ri) = (r1 - rc, r0 + rc);
    let turn = |rho: f32| (rc / rho).clamp(0.0, 1.0).asin().to_degrees();
    let (to, ti) = (turn(ro), turn(ri));
    let steps = (((a1 - a0 - 2.0 * to) / 3.0).ceil() as usize).max(1);
    let along = |r: f32, from: f32, upto: f32| {
        (1..steps).map(move |i| polar(c, r, from + (upto - from) * i as f32 / steps as f32))
    };

    let mut outer: Vec<Pos2> = corner(polar(c, ro, a0 + to), rc, (a0 - 90.0, a0 + to)).collect();
    outer.extend(along(r1, a0 + to, a1 - to));
    outer.extend(corner(polar(c, ro, a1 - to), rc, (a1 - to, a1 + 90.0)));

    let mut inner: Vec<Pos2> =
        corner(polar(c, ri, a0 + ti), rc, (a0 + 270.0, a0 + ti + 180.0)).collect();
    inner.extend(along(r0, a0 + ti, a1 - ti));
    inner.extend(corner(
        polar(c, ri, a1 - ti),
        rc,
        (a1 - ti + 180.0, a1 + 90.0),
    ));
    (outer, inner)
}

/// Draws a rounded annular sector between radii `r0..r1` and angles `a0..a1` (degrees).
fn annular(painter: &Painter, c: Pos2, ring: (f32, f32), span: (f32, f32), round: f32, s: &Style) {
    let (outer, inner) = sector_chains(c, ring, span, round);
    // The sector is concave, so it is filled as a strip of triangles.
    let mut mesh = Mesh::default();
    for p in outer.iter().chain(&inner) {
        mesh.colored_vertex(*p, s.fill);
    }
    let n = outer.len() as u32;
    for i in 0..n - 1 {
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

/// Draws what `item` shows in the icons view, centred on `at`; `base_px` is the icon size at
/// preview scale 1, `fit` the points per layout pixel and `k` the wheel scale (1.0 = 100%).
#[allow(clippy::too_many_arguments)]
fn tile_icon(
    painter: &Painter,
    icons: &mut Icons,
    item: &Item,
    at: Pos2,
    base_px: u32,
    fit: f32,
    k: f32,
    color: Color32,
) {
    let ctx = painter.ctx();
    let size = base_px as f32 * fit;
    // Rounded up to a multiple of 8 px, so that resizing the window does not reload every icon.
    let px = ((size * ctx.pixels_per_point()).ceil() as u32)
        .div_ceil(8)
        .max(2)
        * 8;
    match icons.resolve(ctx, item, px) {
        Drawn::Texture(id) => {
            let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
            painter.image(
                id,
                Rect::from_center_size(at, vec2(size, size)),
                uv,
                Color32::WHITE,
            );
        }
        Drawn::Glyph(c) => {
            let font = FontId::new(size, FontFamily::Name(ICON_FAMILY.into()));
            painter.text(at, Align2::CENTER_CENTER, c, font, color);
        }
        Drawn::Letter(c) => {
            let outline = Rect::from_center_size(at, vec2(size, size));
            painter.rect_stroke(
                outline,
                CornerRadius::same((size * 0.22).round() as u8),
                Stroke::new((1.6 * k * fit).max(1.0), color),
                StrokeKind::Inside,
            );
            let font = FontId::proportional(size * 0.55);
            painter.text(at, Align2::CENTER_CENTER, c, font, color);
        }
        Drawn::Blank => {}
    }
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

pub fn show(ui: &mut Ui, model: &Model, icons: &mut Icons) -> Outcome {
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
    let round = CORNER_PX * u * fit;
    let icons_view = config.options.view == ViewMode::Icons;
    let sizes = IconSizes::scaled(u);

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
    let hovered = response.hover_pos().and_then(tile_at);
    if hovered.is_some() {
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
        let hover = hovered == Some(sel);
        let target = dragging && drop_target == Some(sel);
        let parent = matches!((sel, parent_selected), (Selection::Outer(i), Some(p)) if i == p);
        // Hover fills the tile with the accent, as in the wheel; the selection is an outline.
        Style {
            fill: if hover {
                accent
            } else if filled {
                base
            } else {
                empty_fill
            },
            stroke: if selected || target {
                Stroke::new(3.0, accent)
            } else if parent {
                Stroke::new(2.0, accent)
            } else {
                Stroke::new(1.0, line)
            },
            text: if hover { on_accent } else { text_color },
        }
    };

    for (i, item) in config.inner.iter().enumerate() {
        let (a0, a1) = layout.inner_span(i as u8);
        let st = tile_style(Selection::Inner(i), item.action != Action::None, inner_fill);
        let r = scale(inner_ring);
        annular(
            &painter,
            center,
            r,
            (a0 + GAP_DEG, a1 - GAP_DEG),
            round,
            &st,
        );
        let mid_r = (r.0 + r.1) / 2.0;
        if icons_view {
            // As in the wheel, inner icons sit 2 px outwards of the ring's middle.
            let at = polar(center, mid_r + 2.0 * u * fit, (a0 + a1) / 2.0);
            tile_icon(&painter, icons, item, at, sizes.inner, fit, u, st.text);
        } else {
            label(
                &painter,
                polar(center, mid_r, (a0 + a1) / 2.0),
                &item.name,
                (11.0 * fit).clamp(8.0, 15.0),
                (r.1 - r.0) * 0.8,
                st.text,
            );
        }
    }

    for (i, item) in config.outer.iter().enumerate().take(usize::from(n)) {
        let (a0, a1) = layout.outer_span(i as u8);
        let st = tile_style(Selection::Outer(i), item.action != Action::None, outer_fill);
        let r = scale(outer_ring);
        annular(
            &painter,
            center,
            r,
            (a0 + GAP_DEG, a1 - GAP_DEG),
            round,
            &st,
        );
        let mid = (a0 + a1) / 2.0;
        let at = polar(center, (r.0 + r.1) / 2.0, mid);
        if icons_view {
            tile_icon(&painter, icons, item, at, sizes.outer, fit, u, st.text);
        } else {
            label(
                &painter,
                at,
                &item.name,
                (13.0 * fit).clamp(9.0, 17.0),
                (r.1 - r.0) * 0.85,
                st.text,
            );
        }
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
            annular(
                &painter,
                center,
                r,
                (a0 + GAP_DEG, a1 - GAP_DEG),
                round,
                &st,
            );
            let at = polar(center, (r.0 + r.1) / 2.0, (a0 + a1) / 2.0);
            if icons_view {
                tile_icon(&painter, icons, item, at, sizes.sub, fit, u, st.text);
            } else {
                label(
                    &painter,
                    at,
                    &item.name,
                    (12.0 * fit).clamp(9.0, 16.0),
                    (r.1 - r.0) * 0.9,
                    st.text,
                );
            }
        }
    }

    // The centre shows the name of the tile under the cursor, else of the selected one.
    let shown = hovered
        .filter(|s| *s != Selection::None)
        .unwrap_or(model.selection);
    if let Some(item) = model.item_at(shown) {
        let r = radii.center * fit;
        label(
            &painter,
            center,
            &item.name,
            (14.0 * fit).clamp(9.0, 18.0),
            r * 1.5,
            text_color,
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounded_sector_stays_inside_its_rings() {
        let c = pos2(10.0, -5.0);
        for round in [0.0, 4.0, 100.0] {
            let (outer, inner) = sector_chains(c, (100.0, 160.0), (-15.0, 15.0), round);
            assert_eq!(outer.len(), inner.len());
            for p in outer.iter().chain(&inner) {
                let d = (*p - c).length();
                assert!((100.0 - 1e-3..=160.0 + 1e-3).contains(&d), "{round}: {d}");
            }
        }
    }

    #[test]
    fn rounding_cuts_the_corners() {
        let c = Pos2::ZERO;
        let (sharp, _) = sector_chains(c, (100.0, 160.0), (-15.0, 15.0), 0.0);
        let (round, _) = sector_chains(c, (100.0, 160.0), (-15.0, 15.0), 8.0);
        // The first outer point lies on the radial edge: farther out when sharp.
        assert!(sharp[0].to_vec2().length() > round[0].to_vec2().length());
    }
}
