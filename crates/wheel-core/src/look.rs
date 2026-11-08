//! The look of the wheel shared by the wheel and the editor preview: palettes, the glass colors
//! and how the transparency level scales their alphas. No dependency on any graphics API.

/// Lowest, highest and default glass transparency, in percent (0 = opaque look).
pub const MIN_GLASS_PCT: u8 = 0;
pub const MAX_GLASS_PCT: u8 = 100;
pub const DEFAULT_GLASS_PCT: u8 = 50;

/// Clamps the glass transparency to `MIN_GLASS_PCT..=MAX_GLASS_PCT`.
pub fn clamp_glass_pct(pct: u8) -> u8 {
    pct.clamp(MIN_GLASS_PCT, MAX_GLASS_PCT)
}

/// A color: 0xRRGGBB and alpha 0..=1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub rgb: u32,
    pub a: f32,
}

const fn rgb(hex: u32) -> Rgba {
    Rgba { rgb: hex, a: 1.0 }
}

const fn rgba(hex: u32, a: f32) -> Rgba {
    Rgba { rgb: hex, a }
}

impl Rgba {
    /// Channels as 0..=255.
    pub fn bytes(self) -> [u8; 3] {
        [
            (self.rgb >> 16) as u8,
            (self.rgb >> 8) as u8,
            self.rgb as u8,
        ]
    }

    /// The same color with the alpha multiplied by `factor` (at most 1).
    fn scaled(self, factor: f32) -> Rgba {
        Rgba {
            a: (self.a * factor).min(1.0),
            ..self
        }
    }
}

/// Colors of the wheel (from the mockup in `docs/koncept.html`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Colors {
    /// Solid disc under the rings: shows through the gaps between tiles as thin grooves.
    pub plate: Rgba,
    /// Soft shadow around the plates (alpha is the strength at the plate edge).
    pub shadow: Rgba,
    /// Sector fill.
    pub tile: Rgba,
    /// Submenu arc tile fill.
    pub surface: Rgba,
    /// Submenu notch.
    pub muted: Rgba,
    /// Sector outline.
    pub tile_line: Rgba,
    /// Center circle fill.
    pub center: Rgba,
    /// Label text.
    pub fg: Rgba,
    /// The single accent color: highlighted sector.
    pub accent: Rgba,
    /// Text on the accent color.
    pub accent_fg: Rgba,
}

pub const LIGHT: Colors = Colors {
    plate: rgb(0xd5dbe6),
    shadow: rgba(0x141821, 0.15),
    tile: rgb(0xffffff),
    surface: rgb(0xffffff),
    muted: rgb(0x586074),
    tile_line: rgb(0xc3cad8),
    center: rgb(0xf6f7fa),
    fg: rgb(0x141821),
    accent: rgb(0x2851d0),
    accent_fg: rgb(0xffffff),
};

pub const DARK: Colors = Colors {
    plate: rgb(0x0b0e14),
    shadow: rgba(0x000000, 0.5),
    tile: rgb(0x1b202b),
    surface: rgb(0x151922),
    muted: rgb(0x9aa3b6),
    tile_line: rgb(0x333b4e),
    center: rgb(0x11151d),
    fg: rgb(0xe7eaf1),
    accent: rgb(0x7f9cff),
    accent_fg: rgb(0x0c0f15),
};

/// Translucent colors for drawing over the blurred snapshot of the desktop (at the default
/// level). `plate` is a faint darkening over the whole disc: it stays visible only in the
/// grooves, because the tiles lie over it with their own light fill. `tile_line` is the light
/// edge of a tile.
const GLASS_LIGHT: Colors = Colors {
    plate: rgba(0x39405a, 0.10),
    tile: rgba(0xffffff, 0.14),
    surface: rgba(0xffffff, 0.14),
    tile_line: rgba(0xffffff, 0.8),
    center: rgba(0xffffff, 0.3),
    accent: rgba(0x2851d0, 0.85),
    ..LIGHT
};

const GLASS_DARK: Colors = Colors {
    plate: rgba(0x000000, 0.12),
    tile: rgba(0xffffff, 0.05),
    surface: rgba(0xffffff, 0.05),
    tile_line: rgba(0xffffff, 0.35),
    center: rgba(0x11151d, 0.3),
    accent: rgba(0x7f9cff, 0.85),
    ..DARK
};

/// What the glass look needs besides the colors: how the snapshot is toned and the colors of
/// the glass body, its edges, its sheen and the halo behind the labels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlassStyle {
    /// Saturation of the blurred snapshot (1 = unchanged), like the macOS vibrancy boost.
    pub saturation: f32,
    /// Brightness of the blurred snapshot: multiplied by `gain`, then `lift` is added.
    pub gain: f32,
    pub lift: f32,
    /// Tint laid over the blurred snapshot.
    pub tint: Rgba,
    /// Light edge on the inside of the glass outline and the dark line just outside it.
    pub rim: Rgba,
    pub edge: Rgba,
    /// Alpha of the white sheen at the top of the disc.
    pub sheen: f32,
    /// Halo behind label text (the opposite brightness of the text).
    pub halo: Rgba,
}

const STYLE_LIGHT: GlassStyle = GlassStyle {
    saturation: 1.4,
    gain: 1.0,
    lift: 0.02,
    tint: rgba(0xffffff, 0.42),
    rim: rgba(0xffffff, 0.9),
    edge: rgba(0x141821, 0.35),
    sheen: 0.3,
    halo: rgba(0xffffff, 0.9),
};

const STYLE_DARK: GlassStyle = GlassStyle {
    saturation: 1.4,
    gain: 0.9,
    lift: 0.0,
    tint: rgba(0x1b202b, 0.45),
    rim: rgba(0xffffff, 0.35),
    edge: rgba(0x000000, 0.45),
    sheen: 0.18,
    halo: rgba(0x000000, 0.85),
};

/// The opaque colors of a theme.
pub fn colors(dark: bool) -> Colors {
    if dark { DARK } else { LIGHT }
}

/// Factor for the alphas of the glass colors: 1 at the default level 0.5, more opaque below it
/// and almost clear at 1.
pub fn see_through(level: f32) -> f32 {
    1.9 - 1.8 * level.clamp(0.0, 1.0)
}

/// The translucent colors of a theme for the glass look; `level` is 0..=1.
pub fn glass_colors(dark: bool, level: f32) -> Colors {
    let p = if dark { GLASS_DARK } else { GLASS_LIGHT };
    let see = see_through(level);
    Colors {
        plate: p.plate.scaled(see),
        tile: p.tile.scaled(see),
        surface: p.surface.scaled(see),
        center: p.center.scaled(see),
        ..p
    }
}

/// Glass look parameters of a theme; `level` is 0..=1.
pub fn glass_style(dark: bool, level: f32) -> GlassStyle {
    let s = if dark { STYLE_DARK } else { STYLE_LIGHT };
    GlassStyle {
        tint: s.tint.scaled(see_through(level)),
        ..s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glass_pct_is_clamped() {
        assert_eq!(clamp_glass_pct(0), 0);
        assert_eq!(clamp_glass_pct(100), 100);
        assert_eq!(clamp_glass_pct(101), 100);
    }

    #[test]
    fn transparency_scales_the_alphas() {
        // The default level keeps the base values; a higher level is more see-through.
        assert!((see_through(0.5) - 1.0).abs() < 1e-6);
        let (mid, high) = (glass_colors(false, 0.5), glass_colors(false, 1.0));
        assert!((mid.tile.a - 0.14).abs() < 1e-6);
        assert!(high.tile.a < mid.tile.a);
        assert!(glass_colors(false, 0.0).tile.a > mid.tile.a);
        assert!(glass_style(true, 1.0).tint.a < glass_style(true, 0.5).tint.a);
        // Colors that do not depend on the level stay as they are.
        assert_eq!(mid.accent, GLASS_LIGHT.accent);
        assert_eq!(mid.tile.bytes(), [255, 255, 255]);
    }
}
