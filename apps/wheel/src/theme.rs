//! Theme colors (from the mockup in `docs/koncept.html`) and the Windows theme setting.

use std::ffi::c_void;

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::core::w;

use wheel_core::ThemeChoice as Theme;

#[derive(Clone, Copy)]
pub struct Palette {
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
    plate: rgb(0xd5dbe6),
    shadow: rgba(0x141821, 0.3),
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
    let mut data: u32 = 0;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: `data` and `size` outlive the call and `size` matches the buffer.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut c_void),
            Some(&mut size),
        )
    };
    status == ERROR_SUCCESS && data == 0
}
