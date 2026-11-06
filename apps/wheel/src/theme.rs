//! Theme colors (from the mockup in `docs/koncept.html`) and the Windows theme setting.

use std::ffi::c_void;

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::core::w;

use wheel_core::ThemeChoice as Theme;

#[derive(Clone, Copy)]
pub struct Palette {
    /// Sector fill (`--tile`).
    pub tile: D2D1_COLOR_F,
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
    tile: rgb(0xffffff),
    tile_line: rgb(0xc3cad8),
    center: rgb(0xf6f7fa),
    fg: rgb(0x141821),
    accent: rgb(0x2851d0),
    accent_fg: rgb(0xffffff),
};

const DARK: Palette = Palette {
    tile: rgb(0x1b202b),
    tile_line: rgb(0x333b4e),
    center: rgb(0x11151d),
    fg: rgb(0xe7eaf1),
    accent: rgb(0x7f9cff),
    accent_fg: rgb(0x0c0f15),
};

const fn rgb(hex: u32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: ((hex >> 16) & 0xff) as f32 / 255.0,
        g: ((hex >> 8) & 0xff) as f32 / 255.0,
        b: (hex & 0xff) as f32 / 255.0,
        a: 1.0,
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
