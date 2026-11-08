//! The wheel preview: both rings and the submenu arc of the selected tile, drawn from the
//! geometry in `wheel-core` and looking like the wheel (see `apps/wheel/src/render.rs`): rounded
//! tiles on a grooved plate with a shadow, and the glass look over a sample backdrop.
//! Clicking selects a tile; dropping a file on a tile fills it.

use std::path::PathBuf;

use eframe::egui::epaint::{Mesh, PathStroke, Vertex, WHITE_UV};
use eframe::egui::text::LayoutJob;
use eframe::egui::{
    Align2, Color32, ColorImage, Context, CornerRadius, CursorIcon, FontFamily, FontId, Id,
    Painter, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, TextureHandle, TextureId,
    TextureOptions, Ui, Vec2, pos2, vec2,
};
use wheel_core::fit;
use wheel_core::icon::IconSizes;
use wheel_core::look::{self, Rgba};
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
/// Half of the angular gap between neighbouring tiles, in degrees (as in the wheel).
const OUTER_HALF_GAP_DEG: f32 = 0.8;
const INNER_HALF_GAP_DEG: f32 = 1.4;
/// Smallest label font size at wheel scale 1, in points (as in the wheel).
const MIN_FONT: f32 = 8.0;
/// How far the shadow reaches beyond the plate, in layout pixels, and into how many strokes it
/// is drawn.
const SHADOW_REACH: f32 = 10.0;
const SHADOW_PASSES: u32 = 8;
/// Share of the accent mixed into the tile that opened the submenu arc, and its opacity in the
/// glass look.
const PARENT_TINT: f32 = 0.22;
const PARENT_GLASS_ALPHA: f32 = 0.3;
/// Opacity of the sheen on a highlighted tile in the glass look.
const TILE_SHEEN: f32 = 0.3;
/// The sheen of the disc is its part inside an ellipse of this height (share of the radius) whose
/// centre lies this much above the disc centre.
const SHEEN_HEIGHT: f32 = 0.85;
const SHEEN_LIFT: f32 = 0.55;
/// The submenu notch at the outer edge of a tile: radial extent below the edge (layout px) and
/// half of its angular width; the content of the tile ends `NOTCH_CLEARANCE` before it.
const NOTCH_FAR: f32 = 9.0;
const NOTCH_NEAR: f32 = 5.0;
const NOTCH_HALF_DEG: f32 = 5.0;
const NOTCH_CLEARANCE: f32 = 2.0;
/// How deep the submenu band reaches into the wheel disc, in layout pixels.
const BAND_OVERLAP: f32 = 5.0;

/// `fit::sector_slot` in the wheel's terms: the label width (minus `pad`) and the block centre.
fn sector_slot(ring: (f32, f32), a1: f32, a2: f32, h: f32, need: f32, pad: f32) -> (f32, Vec2) {
    let (w, at) = fit::sector_slot(ring, a1, a2, h, need);
    (w - pad, vec2(at.0, at.1))
}

fn polar(center: Pos2, radius: f32, deg: f32) -> Pos2 {
    let a = deg.to_radians();
    center + vec2(a.sin(), -a.cos()) * radius
}

/// Colors of one tile.
struct Style {
    fill: Color32,
    stroke: Stroke,
    text: Color32,
    /// Opacity of the white sheen on the upper part of the tile (glass look, highlighted tile).
    sheen: Option<f32>,
    /// Glow behind the label text (glass look).
    halo: Option<Color32>,
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

fn c32(c: Rgba) -> Color32 {
    let [r, g, b] = c.bytes();
    Color32::from_rgba_unmultiplied(r, g, b, (c.a * 255.0).round() as u8)
}

/// White with the given opacity (0..=1; clamped).
fn white(alpha: f32) -> Color32 {
    Color32::from_white_alpha((alpha.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// Fills the strip between two chains of equal length with the vertices made by `vertex`.
fn strip(mesh: &mut Mesh, outer: &[Pos2], inner: &[Pos2], vertex: &dyn Fn(Pos2) -> Vertex) {
    let base = mesh.vertices.len() as u32;
    mesh.vertices
        .extend(outer.iter().chain(inner).map(|p| vertex(*p)));
    let n = outer.len() as u32;
    for i in 0..n - 1 {
        mesh.add_triangle(base + i, base + i + 1, base + n + i);
        mesh.add_triangle(base + i + 1, base + n + i + 1, base + n + i);
    }
}

fn solid(color: Color32) -> impl Fn(Pos2) -> Vertex {
    move |pos| Vertex {
        pos,
        uv: WHITE_UV,
        color,
    }
}

/// Draws a rounded annular sector between radii `r0..r1` and angles `a0..a1` (degrees).
fn annular(painter: &Painter, c: Pos2, ring: (f32, f32), span: (f32, f32), round: f32, s: &Style) {
    let (outer, inner) = sector_chains(c, ring, span, round);
    // The sector is concave, so it is filled as a strip of triangles.
    let mut mesh = Mesh::default();
    strip(&mut mesh, &outer, &inner, &solid(s.fill));
    painter.add(Shape::mesh(mesh));
    if let Some(alpha) = s.sheen {
        // White sheen fading out over the upper part of the tile.
        let (top, bottom) = outer
            .iter()
            .chain(&inner)
            .fold((f32::MAX, f32::MIN), |(t, b), p| (t.min(p.y), b.max(p.y)));
        let reach = 0.7 * (bottom - top);
        let mut mesh = Mesh::default();
        strip(&mut mesh, &outer, &inner, &|pos| Vertex {
            pos,
            uv: WHITE_UV,
            color: white(alpha * (1.0 - (pos.y - top) / reach)),
        });
        painter.add(Shape::mesh(mesh));
    }
    let outline: Vec<Pos2> = outer.into_iter().chain(inner.into_iter().rev()).collect();
    painter.add(Shape::closed_line(outline, PathStroke::from(s.stroke)));
}

/// Where a label may go: for a block of the given height, the width available and the centre of
/// the block, relative to the wheel centre (screen px).
type Slot<'a> = &'a dyn Fn(f32, f32) -> (f32, Vec2);

/// Draws a tile name the way the wheel does (`wheel_core::fit`): one or two lines, the font
/// reduced down to `MIN_FONT` (scaled like the rest of the preview) and then trimmed with an
/// ellipsis; never wider than the slot. `size` is the base font size in screen px.
#[allow(clippy::too_many_arguments)]
fn label(
    painter: &Painter,
    center: Pos2,
    text: &str,
    size: f32,
    min_font: f32,
    slot: Slot,
    color: Color32,
    halo: Option<Color32>,
) -> f32 {
    if text.trim().is_empty() {
        return 0.0;
    }
    let measure = |t: &str, scale: f32| {
        let galley = painter.layout_no_wrap(
            t.to_string(),
            FontId::proportional(size * scale),
            Color32::WHITE,
        );
        (galley.size().x, galley.size().y)
    };
    let fitted = fit::fit(text, measure, |h| slot(h, f32::INFINITY).0, min_font / size);
    let font = FontId::proportional(size * fitted.scale);
    let (text_width, height) = measure(&fitted.text, fitted.scale);
    let (width, offset) = slot(height, text_width);
    let width = width.max(1.0);
    let at = center + offset;
    // Each line is centred on its own and trimmed to the slot width, like the wheel's layout.
    let galleys: Vec<_> = fitted
        .text
        .lines()
        .map(|line| {
            let mut job = LayoutJob::simple_singleline(line.to_string(), font.clone(), color);
            job.wrap.max_width = width;
            job.wrap.max_rows = 1;
            job.wrap.break_anywhere = true;
            job.wrap.overflow_character = Some('…');
            painter.layout_job(job)
        })
        .collect();
    let total: f32 = galleys.iter().map(|g| g.size().y).sum();
    let mut y = at.y - total / 2.0;
    for galley in galleys {
        let pos = pos2(at.x - galley.size().x / 2.0, y);
        y += galley.size().y;
        if let Some(halo) = halo {
            for d in [
                vec2(1.0, 0.0),
                vec2(-1.0, 0.0),
                vec2(0.0, 1.0),
                vec2(0.0, -1.0),
            ] {
                painter.galley_with_override_text_color(pos + d, galley.clone(), halo);
            }
        }
        painter.galley(pos, galley, color);
    }
    total
}

/// The sample desktop under the glass: soft colored patches like a wallpaper, made once.
fn backdrop(ctx: &Context) -> TextureId {
    let key = Id::NULL.with("glass_backdrop");
    if let Some(texture) = ctx.data(|d| d.get_temp::<TextureHandle>(key)) {
        return texture.id();
    }
    const N: usize = 160;
    // Center and radius (share of the side) and color of each patch.
    const PATCHES: [(f32, f32, f32, [f32; 3]); 5] = [
        (0.20, 0.25, 0.40, [40.0, 170.0, 255.0]),
        (0.82, 0.30, 0.35, [215.0, 70.0, 170.0]),
        (0.65, 0.88, 0.35, [255.0, 150.0, 60.0]),
        (0.30, 0.82, 0.40, [95.0, 55.0, 195.0]),
        (0.52, 0.50, 0.20, [235.0, 240.0, 255.0]),
    ];
    let mut rgb = Vec::with_capacity(N * N * 3);
    for y in 0..N {
        for x in 0..N {
            let (u, v) = (x as f32 / (N - 1) as f32, y as f32 / (N - 1) as f32);
            let t = (u + v) / 2.0;
            let mut c = [20.0 + 70.0 * t, 60.0 - 20.0 * t, 140.0 + 10.0 * t];
            for (cx, cy, r, color) in PATCHES {
                let d2 = (u - cx).powi(2) + (v - cy).powi(2);
                let w = 0.9 * (-d2 / (2.0 * (r / 1.6).powi(2))).exp();
                for (channel, target) in c.iter_mut().zip(color) {
                    *channel += (target - *channel) * w;
                }
            }
            rgb.extend(c.map(|v| v.clamp(0.0, 255.0) as u8));
        }
    }
    let texture = ctx.load_texture(
        "glass_backdrop",
        ColorImage::from_rgb([N, N], &rgb),
        TextureOptions::LINEAR,
    );
    let id = texture.id();
    ctx.data_mut(|d| d.insert_temp(key, texture));
    id
}

/// What the plates are filled with.
#[derive(Clone, Copy)]
enum Fill {
    Solid(Color32),
    /// The sample backdrop, stretched over the whole preview area.
    Backdrop(TextureId, Rect),
}

/// The plates under the tiles: the wheel disc and, while an arc is open, the band joined to it.
struct Plates {
    center: Pos2,
    radius: f32,
    /// First and last angle of the band of the open arc, and its outer and inner chain.
    band: Option<Band>,
}

struct Band {
    span: (f32, f32),
    outer: Vec<Pos2>,
    inner: Vec<Pos2>,
}

impl Plates {
    fn rim(&self) -> Vec<Pos2> {
        let steps = 120;
        (0..steps)
            .map(|i| polar(self.center, self.radius, 360.0 * i as f32 / steps as f32))
            .collect()
    }

    /// Outline of the disc joined with the band, as one closed line.
    fn outline(&self) -> Vec<Pos2> {
        let Some(band) = &self.band else {
            return self.rim();
        };
        let (first, last) = band.span;
        let mut points = Vec::new();
        let mut a = last;
        while a < first + 360.0 {
            points.push(polar(self.center, self.radius, a));
            a += 3.0;
        }
        points.push(polar(self.center, self.radius, first + 360.0));
        points.extend(&band.outer);
        points
    }

    fn fill(&self, painter: &Painter, fill: Fill) {
        let vertex = |pos: Pos2| match fill {
            Fill::Solid(color) => Vertex {
                pos,
                uv: WHITE_UV,
                color,
            },
            Fill::Backdrop(_, area) => Vertex {
                pos,
                uv: pos2(
                    (pos.x - area.min.x) / area.width(),
                    (pos.y - area.min.y) / area.height(),
                ),
                color: Color32::WHITE,
            },
        };
        let new_mesh = || match fill {
            Fill::Solid(_) => Mesh::default(),
            Fill::Backdrop(id, _) => Mesh::with_texture(id),
        };
        let mut disc = new_mesh();
        let rim = self.rim();
        disc.vertices.push(vertex(self.center));
        disc.vertices.extend(rim.iter().map(|p| vertex(*p)));
        let n = rim.len() as u32;
        for i in 0..n {
            disc.add_triangle(0, 1 + i, 1 + (i + 1) % n);
        }
        painter.add(Shape::mesh(disc));
        if let Some(band) = &self.band {
            let mut mesh = new_mesh();
            strip(&mut mesh, &band.outer, &band.inner, &vertex);
            painter.add(Shape::mesh(mesh));
        }
    }

    /// Soft shadow outside the outline: strokes of falling width, the same as in the wheel;
    /// their inner halves are covered by the plates drawn afterwards.
    fn shadow(&self, painter: &Painter, color: Rgba, reach: f32) {
        let mut color = color;
        color.a /= SHADOW_PASSES as f32;
        let outline = self.outline();
        for pass in 0..SHADOW_PASSES {
            let left = (SHADOW_PASSES - pass) as f32 / SHADOW_PASSES as f32;
            painter.add(Shape::closed_line(
                outline.clone(),
                PathStroke::new(2.0 * reach * left, c32(color)),
            ));
        }
    }

    /// The white sheen on the upper part of the disc.
    fn sheen(&self, painter: &Painter, alpha: f32) {
        let r = self.radius;
        const ROWS: usize = 24;
        let (top, bottom) = (-r, 0.2 * r);
        let mut mesh = Mesh::default();
        for i in 0..=ROWS {
            let dy = top + (bottom - top) * i as f32 / ROWS as f32;
            let disc = (r * r - dy * dy).max(0.0).sqrt();
            let e = (dy + SHEEN_LIFT * r) / (SHEEN_HEIGHT * r);
            let half = disc.min(r * (1.0 - e * e).max(0.0).sqrt());
            let color = white(alpha * (1.0 - i as f32 / ROWS as f32));
            for dx in [-half, half] {
                mesh.vertices.push(Vertex {
                    pos: self.center + vec2(dx, dy),
                    uv: WHITE_UV,
                    color,
                });
            }
        }
        for i in 0..ROWS as u32 {
            let b = 2 * i;
            mesh.add_triangle(b, b + 1, b + 2);
            mesh.add_triangle(b + 1, b + 3, b + 2);
        }
        painter.add(Shape::mesh(mesh));
    }
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

    let config = &model.config;
    let o = &config.options;
    // The same colors and alphas as in the wheel; 0% is the opaque look.
    let glass = o.glass_pct > 0;
    let level = f32::from(o.glass_pct) / 100.0;
    let p = if glass {
        look::glass_colors(dark, level)
    } else {
        look::colors(dark)
    };
    let glass_style = look::glass_style(dark, level);
    let (tile_color, surface_color, line_color) = (c32(p.tile), c32(p.surface), c32(p.tile_line));
    let text_color = c32(p.fg);
    let accent_fill = accent.gamma_multiply(p.accent.a);
    let halo = glass.then(|| {
        c32(Rgba {
            a: glass_style.halo.a * 0.5,
            ..glass_style.halo
        })
    });

    let layout = Layout::new(96, o.scale_pct, o.inner_sectors, o.outer_sectors);
    let n = layout.outer_sectors();
    let n_inner = layout.inner_sectors();
    let radii = layout.radii();
    // The largest scale fills the area (with its submenu arc), smaller ones are proportionally smaller.
    let largest = Layout::new(96, MAX_SCALE_PCT, o.inner_sectors, o.outer_sectors);
    let fit = (rect.width().min(rect.height()) / largest.size_px() as f32).max(0.1);
    let center = rect.center();
    // Sizes relative to the mockup, like `k` in the wheel.
    let u = layout.outer_radius() / 210.0;
    let stroke_w = (1.2 * u * fit).max(1.0);
    // The tile rings, as in the wheel: the outlines stay inside the plates.
    let inner_ring = (radii.center + 4.0 * u, radii.inner - 2.0 * u);
    let outer_ring = (radii.inner + 2.0 * u, layout.outer_radius() - 0.6 * u);
    let (sub_in, sub_out) = layout.sub_radii();
    let sub_ring = (sub_in, sub_out - 0.6 * u);
    let scale = |(a, b): (f32, f32)| (a * fit, b * fit);
    let round = CORNER_PX * u * fit;
    let icons_view = config.options.view == ViewMode::Icons;
    let sizes = IconSizes::scaled(u);
    // Keeps label text off the sector edges, as in the wheel; labels never get smaller than
    // `MIN_FONT`.
    let pad = u * fit;
    let min_font = MIN_FONT * fit;

    let open_arc: Option<SubArc> = match model.selection {
        Selection::Outer(i) | Selection::Sub(i, _) => u8::try_from(i)
            .ok()
            .and_then(|i| arc_for_hit(Hit::Outer(i), config))
            .filter(|a| a.parent < n && a.count > 0),
        _ => None,
    };

    // Tile under a point; `None` outside the tiles.
    let tile_at = |pos: Pos2| -> Option<Selection> {
        let d: Vec2 = (pos - center) / fit;
        let dist = d.length();
        match hit_test_arc(d.x, d.y, &radii, n_inner, n, open_arc) {
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

    let parent_open = open_arc.map(|a| usize::from(a.parent));
    let tile_style = |sel: Selection, filled: bool, base: Color32| -> Style {
        let selected = model.selection == sel;
        let hover = hovered == Some(sel);
        let target = dragging && drop_target == Some(sel);
        let parent = matches!((sel, parent_open), (Selection::Outer(i), Some(p)) if i == p);
        // The tile that opened the arc is tinted with the accent; hover fills the tile with it,
        // as in the wheel. The selection is an outline.
        let mut fill = if !filled {
            base.gamma_multiply(0.45)
        } else {
            base
        };
        if parent {
            fill = if glass {
                accent.gamma_multiply(PARENT_GLASS_ALPHA)
            } else {
                fill.lerp_to_gamma(accent, PARENT_TINT)
            };
        }
        if hover {
            fill = accent_fill;
        }
        Style {
            fill,
            stroke: if selected || target {
                Stroke::new(3.0, accent)
            } else if hover {
                Stroke::new(stroke_w, accent)
            } else {
                Stroke::new(stroke_w, line_color)
            },
            text: if hover { on_accent } else { text_color },
            sheen: (hover && glass).then_some(TILE_SHEEN),
            halo: if hover { None } else { halo },
        }
    };

    // The sample desktop under the wheel.
    let sample = backdrop(ui.ctx());
    painter.image(
        sample,
        rect,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        Color32::WHITE,
    );

    // Plates under the tiles: the wheel disc and, while an arc is open, a band joined to it. The
    // gaps between tiles show them as grooves.
    let plates = Plates {
        center,
        radius: layout.outer_radius() * fit,
        band: open_arc.map(|arc| {
            let first = layout.sub_span(arc, 0).0;
            let last = layout.sub_span(arc, arc.count - 1).1;
            // Starts inside the wheel disc so the two overlap without a seam.
            let ring = (layout.outer_radius() - BAND_OVERLAP * u, sub_out);
            let (outer, inner) = sector_chains(center, scale(ring), (first, last), round);
            Band {
                span: (first, last),
                outer,
                inner,
            }
        }),
    };
    plates.shadow(&painter, look::colors(dark).shadow, SHADOW_REACH * u * fit);
    if glass {
        // The glass is translucent: the shadow must not show through it. The dark line outside
        // its edge is half hidden by the glass drawn over it.
        let desktop = Fill::Backdrop(sample, rect);
        plates.fill(&painter, desktop);
        let outline = plates.outline();
        painter.add(Shape::closed_line(
            outline.clone(),
            PathStroke::new(2.0, c32(glass_style.edge)),
        ));
        plates.fill(&painter, desktop);
        plates.fill(&painter, Fill::Solid(c32(glass_style.tint)));
        plates.fill(&painter, Fill::Solid(c32(p.plate)));
        plates.sheen(&painter, glass_style.sheen);
        painter.add(Shape::closed_line(
            outline,
            PathStroke::new(1.0, c32(glass_style.rim)),
        ));
    } else {
        plates.fill(&painter, Fill::Solid(c32(p.plate)));
    }

    for (i, item) in config.inner.iter().enumerate().take(usize::from(n_inner)) {
        let (a0, a1) = layout.inner_span(i as u8);
        let st = tile_style(Selection::Inner(i), item.action != Action::None, tile_color);
        let r = scale(inner_ring);
        annular(
            &painter,
            center,
            r,
            (a0 + INNER_HALF_GAP_DEG, a1 - INNER_HALF_GAP_DEG),
            round,
            &st,
        );
        let mid_r = (r.0 + r.1) / 2.0;
        if icons_view {
            // As in the wheel, inner icons sit 2 px outwards of the ring's middle.
            let at = polar(center, mid_r + 2.0 * u * fit, (a0 + a1) / 2.0);
            tile_icon(&painter, icons, item, at, sizes.inner, fit, u, st.text);
        } else {
            let text_ring = (r.0 + pad, r.1 - pad);
            label(
                &painter,
                center,
                &item.name,
                11.5 * u * fit,
                min_font,
                &|h, need| {
                    sector_slot(
                        text_ring,
                        a0 + INNER_HALF_GAP_DEG,
                        a1 - INNER_HALF_GAP_DEG,
                        h,
                        need + pad,
                        pad,
                    )
                },
                st.text,
                st.halo,
            );
        }
    }

    for (i, item) in config.outer.iter().enumerate().take(usize::from(n)) {
        let (a0, a1) = layout.outer_span(i as u8);
        let has_submenu = !item.submenu.is_empty();
        let filled = item.action != Action::None || has_submenu;
        let st = tile_style(Selection::Outer(i), filled, tile_color);
        let r = scale(outer_ring);
        annular(
            &painter,
            center,
            r,
            (a0 + OUTER_HALF_GAP_DEG, a1 - OUTER_HALF_GAP_DEG),
            round,
            &st,
        );
        let mid = (a0 + a1) / 2.0;
        // The content of a tile with a submenu ends before the notch.
        let content_end = if has_submenu {
            r.1.min((layout.outer_radius() - (NOTCH_FAR + NOTCH_CLEARANCE) * u) * fit)
        } else {
            r.1
        };
        if icons_view {
            let size = sizes.outer as f32 * fit;
            let at_r = ((r.0 + r.1) / 2.0).min(content_end - size / 2.0);
            tile_icon(
                &painter,
                icons,
                item,
                polar(center, at_r, mid),
                sizes.outer,
                fit,
                u,
                st.text,
            );
        } else {
            let text_ring = (r.0 + pad, content_end - pad);
            label(
                &painter,
                center,
                &item.name,
                15.0 * u * fit,
                min_font,
                &|h, need| {
                    sector_slot(
                        text_ring,
                        a0 + OUTER_HALF_GAP_DEG,
                        a1 - OUTER_HALF_GAP_DEG,
                        h,
                        need + pad,
                        pad,
                    )
                },
                st.text,
                st.halo,
            );
        }
        if has_submenu {
            // The notch at the outer edge marks a tile with a submenu; it takes the accent while
            // only its arc is open.
            let (near, far) = (
                (layout.outer_radius() - NOTCH_NEAR * u) * fit,
                (layout.outer_radius() - NOTCH_FAR * u) * fit,
            );
            let color = if hovered == Some(Selection::Outer(i)) {
                on_accent
            } else if parent_open == Some(i) {
                accent
            } else {
                c32(p.muted)
            };
            let points = (0..=6)
                .map(|s| {
                    let a = mid - NOTCH_HALF_DEG + 2.0 * NOTCH_HALF_DEG * s as f32 / 6.0;
                    polar(center, (near + far) / 2.0, a)
                })
                .collect();
            painter.add(Shape::line(points, Stroke::new(near - far, color)));
        }
    }

    painter.circle(
        center,
        radii.center * fit,
        c32(p.center),
        Stroke::new(stroke_w, line_color),
    );

    // Submenu arc of the selected outer tile (or of the parent of the selected submenu tile).
    if let Some(arc) = open_arc {
        let sub = &config.outer[usize::from(arc.parent)].submenu;
        let r = scale(sub_ring);
        for (k, item) in sub.iter().enumerate().take(usize::from(arc.count)) {
            let (a0, a1) = layout.sub_span(arc, k as u8);
            let st = tile_style(
                Selection::Sub(usize::from(arc.parent), k),
                item.action != Action::None,
                surface_color,
            );
            annular(
                &painter,
                center,
                r,
                (a0 + OUTER_HALF_GAP_DEG, a1 - OUTER_HALF_GAP_DEG),
                round,
                &st,
            );
            if icons_view {
                let at = polar(center, (r.0 + r.1) / 2.0, (a0 + a1) / 2.0);
                tile_icon(&painter, icons, item, at, sizes.sub, fit, u, st.text);
            } else {
                let text_ring = (r.0 + pad, r.1 - pad);
                label(
                    &painter,
                    center,
                    &item.name,
                    13.5 * u * fit,
                    min_font,
                    &|h, need| {
                        sector_slot(
                            text_ring,
                            a0 + OUTER_HALF_GAP_DEG,
                            a1 - OUTER_HALF_GAP_DEG,
                            h,
                            need + pad,
                            pad,
                        )
                    },
                    st.text,
                    st.halo,
                );
            }
        }
    }

    // The centre shows the name of the tile under the cursor, else of the selected one.
    let shown = hovered
        .filter(|s| *s != Selection::None)
        .unwrap_or(model.selection);
    let r = radii.center * fit;
    let name_height = model.item_at(shown).map_or(0.0, |item| {
        label(
            &painter,
            center,
            &item.name,
            14.0 * u * fit,
            min_font,
            &|h, _| (fit::circle_width(r - pad, h) - pad, Vec2::ZERO),
            text_color,
            halo,
        )
    });
    // The options hint under the name, as the wheel shows it when it opens (without the fade).
    let below = name_height / 2.0 + u * fit;
    label(
        &painter,
        center,
        look::OPTIONS_HINT,
        look::OPTIONS_HINT_PX * u * fit,
        min_font,
        &|h, _| {
            let reach = below + h / 2.0;
            let width = fit::circle_width(r - pad, 2.0 * (reach + h / 2.0));
            (width - pad, vec2(0.0, reach))
        },
        c32(p.muted),
        halo,
    );
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
