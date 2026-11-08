//! Theme colors (from the mockup in `docs/koncept.html`) and the Windows theme setting.

use std::ffi::c_void;

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::core::{PCWSTR, w};

use wheel_core::ThemeChoice as Theme;
use wheel_core::look;

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

const fn from_colors(dark: bool, c: look::Colors) -> Palette {
    Palette {
        dark,
        plate: color(c.plate),
        shadow: color(c.shadow),
        tile: color(c.tile),
        surface: color(c.surface),
        muted: color(c.muted),
        tile_line: color(c.tile_line),
        center: color(c.center),
        fg: color(c.fg),
        accent: color(c.accent),
        accent_fg: color(c.accent_fg),
    }
}

const LIGHT: Palette = from_colors(false, look::LIGHT);
const DARK: Palette = from_colors(true, look::DARK);

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

impl Palette {
    /// The translucent variant of this palette, for the glass look.
    pub fn glass(self, level: f32) -> Palette {
        from_colors(self.dark, look::glass_colors(self.dark, level))
    }

    /// Glass look parameters matching this palette's theme.
    pub fn glass_style(&self, level: f32) -> GlassStyle {
        let s = look::glass_style(self.dark, level);
        GlassStyle {
            saturation: s.saturation,
            gain: s.gain,
            lift: s.lift,
            tint: color(s.tint),
            rim: color(s.rim),
            edge: color(s.edge),
            sheen: s.sheen,
            halo: color(s.halo),
        }
    }
}

/// Whether Windows draws transparency effects (Settings > Personalization > Colors).
pub fn transparency_enabled() -> bool {
    registry_dword(w!("EnableTransparency")).is_none_or(|v| v != 0)
}

const fn color(c: look::Rgba) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: ((c.rgb >> 16) & 0xff) as f32 / 255.0,
        g: ((c.rgb >> 8) & 0xff) as f32 / 255.0,
        b: (c.rgb & 0xff) as f32 / 255.0,
        a: c.a,
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

/// `a` blended with `b` by `t` (0..=1), alpha included.
pub fn blend(a: D2D1_COLOR_F, b: D2D1_COLOR_F, t: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        a: a.a + (b.a - a.a) * t,
        ..mix(a, b, t)
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
