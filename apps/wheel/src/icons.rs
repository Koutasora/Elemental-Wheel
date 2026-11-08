//! Tile icons for the icons view: the system part that loads what `wheel_core::icon::spec_for`
//! chose (file icons through the shell and WIC into Direct2D bitmaps, glyphs
//! from the Segoe icon font). The cache is built together with the `Renderer`, so a changed
//! configuration (the renderer is rebuilt) refreshes it.

use std::cell::Cell;
use std::collections::HashMap;

use wheel_core::icon::{IconSizes, IconSpec, first_letter, spec_for};
use wheel_core::{Config, Hit, Item};

use windows::Win32::Foundation::MAX_PATH;
use windows::Win32::Graphics::Direct2D::{ID2D1Bitmap, ID2D1DeviceContext};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_NORMAL,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_RANGE,
    IDWriteFactory, IDWriteTextFormat,
};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICBitmapSource, IWICImagingFactory,
    WICBitmapDitherTypeNone, WICBitmapPaletteTypeCustom,
};
use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_NORMAL, SearchPathW};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
use windows::Win32::UI::Controls::{IImageList, ILD_TRANSPARENT};
use windows::Win32::UI::Shell::{
    SHDefExtractIconW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGFI_SYSICONINDEX,
    SHGetFileInfoW, SHGetImageList, SHIL_EXTRALARGE, SHIL_JUMBO,
};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, HICON};
use windows::core::{Interface, PCWSTR, Result, w};

/// Largest side of a glyph's visible outline relative to its icon box, so every glyph looks as
/// big as a file icon regardless of how much of the em square the font uses.
const GLYPH_INK_FRACTION: f32 = 0.72;
/// Limits of the correction, so a tiny or huge outline cannot blow a glyph up or shrink it away.
const GLYPH_SCALE_RANGE: (f32, f32) = (0.5, 1.5);

/// How to draw a glyph so its outline is `GLYPH_INK_FRACTION` of the icon box and centred in it;
/// measured once when the cache is built.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphFit {
    /// Factor for the font size (the icon box side is the base size).
    pub scale: f32,
    /// Offset of the outline centre from the box centre at the base size; multiply by `scale`.
    pub dx: f32,
    pub dy: f32,
}

impl GlyphFit {
    const IDENTITY: GlyphFit = GlyphFit {
        scale: 1.0,
        dx: 0.0,
        dy: 0.0,
    };
}

/// What a tile draws once loaded.
pub enum TileIcon<'a> {
    Bitmap(&'a ID2D1Bitmap),
    /// Draw with `Icons::glyph_format`.
    Glyph(char, GlyphFit),
    Letter(char),
    Blank,
}

/// What one tile resolved to.
enum Tile {
    Bitmap(ID2D1Bitmap),
    Glyph(char, GlyphFit),
    Letter(char),
    Blank,
}

/// Icons of every tile of a configuration, loaded once.
pub struct Icons {
    inner: Vec<Tile>,
    outer: Vec<Tile>,
    /// Per outer sector, its submenu tiles.
    sub: Vec<Vec<Tile>>,
    glyph_format: Option<IDWriteTextFormat>,
}

impl Icons {
    /// Loads the icons of the first `inner_sectors` inner sectors, the first `outer_sectors` outer sectors and their
    /// submenus; sizes are in px of the device context (96 DPI). A file that cannot be loaded
    /// becomes a letter, and so does a glyph the font lacks (or any glyph without an icon font).
    pub fn new(
        ctx: &ID2D1DeviceContext,
        dwrite: &IDWriteFactory,
        config: &Config,
        inner_sectors: u8,
        outer_sectors: u8,
        sizes: IconSizes,
    ) -> Icons {
        ensure_com();
        let font = glyph_font(dwrite);
        let glyph_format = font
            .as_ref()
            .and_then(|(name, _)| glyph_text_format(dwrite, name, sizes.outer as f32).ok());
        let mut loader = Loader {
            ctx,
            dwrite,
            glyph_format: glyph_format.as_ref(),
            wic: wic_factory().ok(),
            has_glyph: font.map(|(_, has)| has),
            bitmaps: HashMap::new(),
        };
        let mut tiles = |items: &[Item], size: u32| -> Vec<Tile> {
            items.iter().map(|item| loader.tile(item, size)).collect()
        };
        let outer_items = &config.outer[..config.outer.len().min(usize::from(outer_sectors))];
        let inner_items = &config.inner[..config.inner.len().min(usize::from(inner_sectors))];
        Icons {
            inner: tiles(inner_items, sizes.inner),
            outer: tiles(outer_items, sizes.outer),
            sub: outer_items
                .iter()
                .map(|item| tiles(&item.submenu, sizes.sub))
                .collect(),
            glyph_format,
        }
    }

    /// What the tile `hit` shows; `Blank` for the centre and unknown tiles.
    pub fn icon_for(&self, hit: Hit) -> TileIcon<'_> {
        let tile = match hit {
            Hit::Inner(i) => self.inner.get(usize::from(i)),
            Hit::Outer(i) => self.outer.get(usize::from(i)),
            Hit::Sub(p, k) => self
                .sub
                .get(usize::from(p))
                .and_then(|s| s.get(usize::from(k))),
            Hit::Center => None,
        };
        match tile {
            Some(Tile::Bitmap(b)) => TileIcon::Bitmap(b),
            Some(Tile::Glyph(c, fit)) => TileIcon::Glyph(*c, *fit),
            Some(Tile::Letter(c)) => TileIcon::Letter(*c),
            Some(Tile::Blank) | None => TileIcon::Blank,
        }
    }

    /// Centred text format of the glyph font at the outer-ring icon size (resize per use with
    /// `SetFontSize` on a layout); `None` when no icon font exists, in which case no tile is a
    /// `Glyph`.
    pub fn glyph_format(&self) -> Option<&IDWriteTextFormat> {
        self.glyph_format.as_ref()
    }
}

type GlyphCheck = Box<dyn Fn(char) -> bool>;

struct Loader<'a> {
    ctx: &'a ID2D1DeviceContext,
    dwrite: &'a IDWriteFactory,
    /// Format the glyphs are measured (and later drawn) with.
    glyph_format: Option<&'a IDWriteTextFormat>,
    wic: Option<IWICImagingFactory>,
    /// Whether the glyph font has a character; `None` without a glyph font.
    has_glyph: Option<GlyphCheck>,
    /// (expanded path, index, size) -> bitmap, so a file shared by tiles is loaded once.
    bitmaps: HashMap<(String, i32, u32), Option<ID2D1Bitmap>>,
}

impl Loader<'_> {
    fn tile(&mut self, item: &Item, size: u32) -> Tile {
        let letter = || first_letter(&item.name).map_or(Tile::Blank, Tile::Letter);
        match spec_for(item) {
            IconSpec::File { path, index } => match self.bitmap(&path, index, size) {
                Some(b) => Tile::Bitmap(b),
                None => letter(),
            },
            IconSpec::Glyph(c) => match &self.has_glyph {
                Some(has) if has(c) => Tile::Glyph(c, self.glyph_fit(c, size)),
                _ => letter(),
            },
            IconSpec::Letter(c) => Tile::Letter(c),
            IconSpec::Blank => Tile::Blank,
        }
    }

    /// Measures the outline of `c` in a `size` px box (DirectWrite overhang metrics of a large
    /// layout, so nothing is cut off) and derives how to scale and shift it.
    fn glyph_fit(&self, c: char, size: u32) -> GlyphFit {
        let Some(format) = self.glyph_format else {
            return GlyphFit::IDENTITY;
        };
        let em = size as f32;
        let side = em * 4.0;
        let mut buf = [0u16; 2];
        let text = c.encode_utf16(&mut buf);
        // SAFETY: plain COM calls with valid arguments.
        let overhang = unsafe {
            let layout = self.dwrite.CreateTextLayout(text, format, side, side);
            layout
                .and_then(|l| {
                    l.SetFontSize(
                        em,
                        DWRITE_TEXT_RANGE {
                            startPosition: 0,
                            length: text.len() as u32,
                        },
                    )?;
                    l.GetOverhangMetrics()
                })
                .ok()
        };
        let Some(o) = overhang else {
            return GlyphFit::IDENTITY;
        };
        // Overhang is how far the outline reaches beyond each box edge (negative: inside it).
        let (w, h) = (side + o.left + o.right, side + o.top + o.bottom);
        let ink = w.max(h);
        if ink < 1.0 {
            return GlyphFit::IDENTITY;
        }
        GlyphFit {
            scale: (em * GLYPH_INK_FRACTION / ink).clamp(GLYPH_SCALE_RANGE.0, GLYPH_SCALE_RANGE.1),
            dx: (o.right - o.left) / 2.0,
            dy: (o.bottom - o.top) / 2.0,
        }
    }

    fn bitmap(&mut self, path: &str, index: i32, size: u32) -> Option<ID2D1Bitmap> {
        let path = resolve_path(&expand(path));
        let (ctx, wic) = (self.ctx, self.wic.as_ref());
        self.bitmaps
            .entry((path.clone(), index, size))
            .or_insert_with(|| {
                let loaded = load_wic(wic?, &path, index, size)
                    // SAFETY: plain COM call on a valid source.
                    .and_then(|src| unsafe { ctx.CreateBitmapFromWicBitmap(&src, None) })
                    .and_then(|b| b.cast::<ID2D1Bitmap>());
                match loaded {
                    Ok(bitmap) => Some(bitmap),
                    Err(e) => {
                        wheel_core::log_line!("wheel: no icon for \"{path}\": {e}");
                        None
                    }
                }
            })
            .clone()
    }
}

thread_local! {
    static COM_READY: Cell<bool> = const { Cell::new(false) };
}

/// WIC needs COM on the calling thread. The UI thread lives as long as the process, so the
/// apartment is never released; a thread that already chose another model is fine too.
fn ensure_com() {
    COM_READY.with(|ready| {
        if !ready.get() {
            // SAFETY: plain call; the result is deliberately ignored (see above).
            let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            ready.set(true);
        }
    });
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// Expands `%VARIABLE%` references; the text itself if that fails.
fn expand(text: &str) -> String {
    let src = wide(text);
    // SAFETY: `src` is NUL-terminated; the buffer is sized by the first call.
    unsafe {
        let needed = ExpandEnvironmentStringsW(PCWSTR(src.as_ptr()), None) as usize;
        if needed == 0 {
            return text.to_string();
        }
        let mut buf = vec![0u16; needed];
        let written = ExpandEnvironmentStringsW(PCWSTR(src.as_ptr()), Some(&mut buf)) as usize;
        if written == 0 || written > buf.len() {
            return text.to_string();
        }
        String::from_utf16_lossy(&buf[..written - 1])
    }
}

/// A bare file name (`notepad.exe`) is looked up on the search path, as when it is run; the
/// text itself if it is not found or is not bare.
fn resolve_path(path: &str) -> String {
    if path.trim().is_empty() || path.contains(['\\', '/', ':']) {
        return path.to_string();
    }
    let name = wide(path);
    let mut buf = vec![0u16; MAX_PATH as usize];
    // A longer result returns the needed size including the NUL; one retry with that size.
    for _ in 0..2 {
        // SAFETY: `name` is NUL-terminated and `buf` is a valid output buffer.
        let len = unsafe {
            SearchPathW(
                None,
                PCWSTR(name.as_ptr()),
                w!(".exe"),
                Some(&mut buf),
                None,
            )
        } as usize;
        if len == 0 {
            break;
        }
        if len < buf.len() {
            return String::from_utf16_lossy(&buf[..len]);
        }
        buf = vec![0u16; len];
    }
    path.to_string()
}

fn wic_factory() -> Result<IWICImagingFactory> {
    // SAFETY: COM is initialised on this thread by `ensure_com`.
    unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER) }
}

/// Icon `index` of `path` at `size` px as a 32bpp premultiplied BGRA WIC source. Falls back to
/// the system icon associated with the path (folders, documents, files without icons).
fn load_wic(
    wic: &IWICImagingFactory,
    path: &str,
    index: i32,
    size: u32,
) -> Result<IWICBitmapSource> {
    let file = wide(path);
    let hicon = extract_icon(&file, index, size)
        .or_else(|| shell_icon(&file, size))
        .ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))?;
    // SAFETY: `hicon` is a valid icon owned here; it is destroyed once WIC has copied it.
    let converter = unsafe {
        let bitmap = wic.CreateBitmapFromHICON(hicon);
        let _ = DestroyIcon(hicon);
        let converter = wic.CreateFormatConverter()?;
        converter.Initialize(
            &bitmap?,
            &GUID_WICPixelFormat32bppPBGRA,
            WICBitmapDitherTypeNone,
            None,
            0.0,
            WICBitmapPaletteTypeCustom,
        )?;
        converter
    };
    converter.cast()
}

/// `file` is NUL-terminated.
fn extract_icon(file: &[u16], index: i32, size: u32) -> Option<HICON> {
    let mut large = HICON::default();
    // SAFETY: `file` is NUL-terminated and `large` is a valid out pointer.
    let result = unsafe {
        SHDefExtractIconW(
            PCWSTR(file.as_ptr()),
            index,
            0,
            Some(&mut large),
            None,
            size,
        )
    };
    if large.is_invalid() {
        return None;
    }
    if result.is_err() {
        // SAFETY: a handle set despite the error is still ours to destroy.
        let _ = unsafe { DestroyIcon(large) };
        return None;
    }
    Some(large)
}

/// The shell icon for `file` (NUL-terminated); works for folders and documents. Taken from the
/// system image list in a size of at least `size` (48 px, or 256 px beyond that): the plain large
/// icon is 32 px with an empty margin and would look small once scaled to the tile's icon box.
fn shell_icon(file: &[u16], size: u32) -> Option<HICON> {
    let mut info = SHFILEINFOW::default();
    let list = if size <= 48 {
        SHIL_EXTRALARGE
    } else {
        SHIL_JUMBO
    };
    // SAFETY: `file` is NUL-terminated; `info` is a valid out structure of the size given.
    let from_list = unsafe {
        let ok = SHGetFileInfoW(
            PCWSTR(file.as_ptr()),
            FILE_ATTRIBUTE_NORMAL,
            Some(&mut info),
            size_of::<SHFILEINFOW>() as u32,
            SHGFI_SYSICONINDEX,
        );
        (ok != 0)
            .then(|| SHGetImageList::<IImageList>(list as i32).ok())
            .flatten()
            .and_then(|images| images.GetIcon(info.iIcon, ILD_TRANSPARENT.0).ok())
    };
    if from_list.is_some() {
        return from_list;
    }
    // SAFETY: as above.
    let ok = unsafe {
        SHGetFileInfoW(
            PCWSTR(file.as_ptr()),
            FILE_ATTRIBUTE_NORMAL,
            Some(&mut info),
            size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        )
    };
    (ok != 0 && !info.hIcon.is_invalid()).then_some(info.hIcon)
}

/// Family name of the icon font and a character check for it: Segoe Fluent Icons, else Segoe MDL2
/// Assets (Windows 10), else none.
fn glyph_font(dwrite: &IDWriteFactory) -> Option<(&'static str, GlyphCheck)> {
    ["Segoe Fluent Icons", "Segoe MDL2 Assets"]
        .into_iter()
        .find_map(|name| font_check(dwrite, name).map(|check| (name, check)))
}

fn font_check(dwrite: &IDWriteFactory, family: &str) -> Option<GlyphCheck> {
    let name = wide(family);
    // SAFETY: plain COM calls with valid arguments and out pointers.
    let font = unsafe {
        let mut collection = None;
        dwrite
            .GetSystemFontCollection(&mut collection, false)
            .ok()?;
        let collection = collection?;
        let (mut index, mut exists) = (0u32, false.into());
        collection
            .FindFamilyName(PCWSTR(name.as_ptr()), &mut index, &mut exists)
            .ok()?;
        if !exists.as_bool() {
            return None;
        }
        collection
            .GetFontFamily(index)
            .ok()?
            .GetFirstMatchingFont(
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
            )
            .ok()?
    };
    Some(Box::new(move |c| {
        // SAFETY: plain COM call.
        unsafe { font.HasCharacter(c as u32) }.is_ok_and(|has| has.as_bool())
    }))
}

fn glyph_text_format(
    dwrite: &IDWriteFactory,
    family: &str,
    size: f32,
) -> Result<IDWriteTextFormat> {
    let name = wide(family);
    // SAFETY: plain COM calls with valid arguments.
    unsafe {
        let format = dwrite.CreateTextFormat(
            PCWSTR(name.as_ptr()),
            None,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            size,
            w!(""),
        )?;
        format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
        format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        Ok(format)
    }
}

#[cfg(test)]
mod tests {
    use wheel_core::icon::{edit_glyph, system_glyph};
    use wheel_core::{EditCommand, SystemAction};
    use windows::Win32::Graphics::DirectWrite::{DWRITE_FACTORY_TYPE_SHARED, DWriteCreateFactory};

    use super::*;

    const EDIT: [EditCommand; 12] = [
        EditCommand::Cut,
        EditCommand::Copy,
        EditCommand::Paste,
        EditCommand::PastePlain,
        EditCommand::Delete,
        EditCommand::Undo,
        EditCommand::Redo,
        EditCommand::SelectAll,
        EditCommand::Save,
        EditCommand::Find,
        EditCommand::Rename,
        EditCommand::Properties,
    ];

    const SYSTEM: [SystemAction; 9] = [
        SystemAction::Lock,
        SystemAction::CloseWindow,
        SystemAction::MinimizeWindow,
        SystemAction::ShowDesktop,
        SystemAction::VolumeMute,
        SystemAction::VolumeUp,
        SystemAction::VolumeDown,
        SystemAction::Screenshot,
        SystemAction::ContextMenu,
    ];

    #[test]
    fn system_icons_load_into_wic() {
        ensure_com();
        let wic = wic_factory().expect("WIC factory");
        let loads = |path: &str, index: i32| {
            let src = load_wic(&wic, &expand(path), index, 32).expect(path);
            let (mut w, mut h) = (0, 0);
            // SAFETY: valid out pointers.
            unsafe { src.GetSize(&mut w, &mut h).expect("size") };
            assert!(w > 0 && h > 0, "{path}: {w}x{h}");
        };
        loads("C:\\Windows\\System32\\notepad.exe", 0);
        loads("%SystemRoot%\\System32\\shell32.dll", 3);
        // A folder has no icon resource: the shell icon is used.
        loads("%SystemRoot%", 0);
    }

    #[test]
    fn bare_names_are_searched() {
        let found = resolve_path("notepad.exe");
        assert!(found.contains('\\'), "{found}");
        assert_eq!(resolve_path("no-such-file-xyz.exe"), "no-such-file-xyz.exe");
    }

    #[test]
    fn a_glyph_font_exists_and_has_the_glyphs() {
        // SAFETY: plain COM call.
        let dwrite: IDWriteFactory =
            unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }.expect("DirectWrite");
        let (name, has) = glyph_font(&dwrite).expect("Segoe Fluent Icons or Segoe MDL2 Assets");
        let all = EDIT
            .iter()
            .map(|c| edit_glyph(*c))
            .chain(SYSTEM.iter().map(|a| system_glyph(*a)))
            .chain(['\u{E774}', '\u{E765}', '\u{E8D2}', '\u{E756}']);
        let missing: Vec<String> = all
            .filter(|c| !has(*c))
            .map(|c| format!("{:04X}", c as u32))
            .collect();
        assert!(missing.is_empty(), "{name} lacks {missing:?}");
    }
}
