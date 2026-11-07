//! Theme colors (from the mockup in `docs/koncept.html`) and the Windows theme setting.

use std::ffi::c_void;

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::core::{PCWSTR, w};

use wheel_core::ThemeChoice as Theme;

#[derive(Clone, Copy)]
pub struct Palette {
    /// Dark variant; picks the glass tint.
    pub dark: bool,
    /// Solid disc under the rings: shows through the gaps between tiles as thin grooves.
    pub plate: D2D1_COLOR_F,
    /// Soft shadow around the plates (alpha is the strength at the plate edge).
    pub shadow: D2D1_COLOR_F,
    /// Sector fill (`--tile`).
    pub tile: D2D1_COLOR_F,
    /// Submenu arc tile fill (`--surface`).
    pub surface: D2D1_COLOR_F,
    /// Submenu notch (`--muted`).
    pub muted: D2D1_COLOR_F,
    /// Sector outline (`--tile-line`).
    pub tile_line: D2D1_COLOR_F,
    /// Center circle fill (`--center`).
    pub center: D2D1_COLOR_F,
    /// Label text (`--fg`).
    pub fg: D2D1_COLOR_F,
    /// The single accent color: highlighted sector (`--accent`).
    pub accent: D2D1_COLOR_F,
    /// Text on the accent color (`--accent-fg`).
    pub accent_fg: D2D1_COLOR_F,
}

const LIGHT: Palette = Palette {
    dark: false,
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

const DARK: Palette = Palette {
    dark: true,
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

/// Translucent colors for drawing over the blurred snapshot of the desktop. `plate` is a faint
/// darkening over the whole disc: it stays visible only in the grooves, because the tiles lie
/// over it with their own light fill. `tile_line` is the light edge of a tile.
const GLASS_LIGHT: Palette = Palette {
    plate: rgba(0x39405a, 0.10),
    tile: rgba(0xffffff, 0.14),
    surface: rgba(0xffffff, 0.14),
    tile_line: rgba(0xffffff, 0.8),
    center: rgba(0xffffff, 0.3),
    accent: rgba(0x2851d0, 0.85),
    ..LIGHT
};

const GLASS_DARK: Palette = Palette {
    plate: rgba(0x000000, 0.12),
    tile: rgba(0xffffff, 0.05),
    surface: rgba(0xffffff, 0.05),
    tile_line: rgba(0xffffff, 0.35),
    center: rgba(0x11151d, 0.3),
    accent: rgba(0x7f9cff, 0.85),
    ..DARK
};

/// What the glass look needs besides the palette: how the snapshot is toned and the colors of
/// the glass body, its edges, its sheen and the halo behind the labels.
#[derive(Clone, Copy)]
pub struct GlassStyle {
    /// Saturation of the blurred snapshot (1 = unchanged), like the macOS vibrancy boost.
    pub saturation: f32,
    /// Brightness of the blurred snapshot: multiplied by `gain`, then `lift` is added.
    pub gain: f32,
    pub lift: f32,
    /// Tint laid over the blurred snapshot.
    pub tint: D2D1_COLOR_F,
    /// Light edge on the inside of the glass outline and the dark line just outside it.
    pub rim: D2D1_COLOR_F,
    pub edge: D2D1_COLOR_F,
    /// Alpha of the white sheen at the top of the disc.
    pub sheen: f32,
    /// Halo behind label text (the opposite brightness of the text).
    pub halo: D2D1_COLOR_F,
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

impl Palette {
    /// The translucent variant of this palette, for the glass look.
    pub fn glass(self, level: f32) -> Palette {
        let p = if self.dark { GLASS_DARK } else { GLASS_LIGHT };
        let see = see_through(level);
        let scale = |c: D2D1_COLOR_F| D2D1_COLOR_F {
            a: (c.a * see).min(1.0),
            ..c
        };
        Palette {
            plate: scale(p.plate),
            tile: scale(p.tile),
            surface: scale(p.surface),
            center: scale(p.center),
            ..p
        }
    }

    /// Glass look parameters matching this palette's theme.
    pub fn glass_style(&self, level: f32) -> GlassStyle {
        let s = if self.dark { STYLE_DARK } else { STYLE_LIGHT };
        GlassStyle {
            tint: D2D1_COLOR_F {
                a: (s.tint.a * see_through(level)).min(1.0),
                ..s.tint
            },
            ..s
        }
    }
}

/// Factor for the alphas of the glass colors: 1 at the default level 0.5 (the values above),
/// more opaque below it and almost clear at 1.
fn see_through(level: f32) -> f32 {
    1.9 - 1.8 * level.clamp(0.0, 1.0)
}

/// Whether Windows draws transparency effects (Settings > Personalization > Colors).
pub fn transparency_enabled() -> bool {
    registry_dword(w!("EnableTransparency")).is_none_or(|v| v != 0)
}

const fn rgb(hex: u32) -> D2D1_COLOR_F {
    rgba(hex, 1.0)
}

const fn rgba(hex: u32, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: ((hex >> 16) & 0xff) as f32 / 255.0,
        g: ((hex >> 8) & 0xff) as f32 / 255.0,
        b: (hex & 0xff) as f32 / 255.0,
        a,
    }
}

/// `a` with `t` (0..=1) of `b` mixed in; alpha is taken from `a`.
pub fn mix(a: D2D1_COLOR_F, b: D2D1_COLOR_F, t: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a,
    }
}

/// Palette for the requested theme; `System` reads the Windows app theme.
pub fn palette(theme: Theme) -> Palette {
    let dark = match theme {
        Theme::Light => false,
        Theme::Dark => true,
        Theme::System => system_uses_dark(),
    };
    if dark { DARK } else { LIGHT }
}

/// `AppsUseLightTheme` = 0 means dark; a missing or other value means light.
fn system_uses_dark() -> bool {
    registry_dword(w!("AppsUseLightTheme")) == Some(0)
}

/// A DWORD value of the Personalize key; `None` if it is missing.
fn registry_dword(name: PCWSTR) -> Option<u32> {
    let mut data: u32 = 0;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: `data` and `size` outlive the call and `size` matches the buffer.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            name,
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut c_void),
            Some(&mut size),
        )
    };
    (status == ERROR_SUCCESS).then_some(data)
}
