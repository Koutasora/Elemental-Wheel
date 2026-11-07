//! Tile icons for the icons view: a pure choice of what a tile shows (`spec_for`) and the
//! system part that loads it (file icons through the shell and WIC into Direct2D bitmaps, glyphs
//! from the Segoe icon font). The cache is built together with the `Renderer`, so a changed
//! configuration (the renderer is rebuilt) refreshes it.

use std::cell::Cell;
use std::collections::HashMap;

use wheel_core::{Action, Config, EditCommand, Hit, INNER_SECTORS, Item, SystemAction};

use windows::Win32::Foundation::MAX_PATH;
use windows::Win32::Graphics::Direct2D::{ID2D1Bitmap, ID2D1DeviceContext};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_NORMAL,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER, IDWriteFactory,
    IDWriteTextFormat,
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

/// Icon sizes in px at 100% and 96 DPI, taken from the mockup (`icoSize`).
const MOCKUP_ICON_PX: IconSizes = IconSizes {
    inner: 24,
    outer: 36,
    sub: 30,
};

/// Icon size in px for each ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IconSizes {
    pub inner: u32,
    pub outer: u32,
    pub sub: u32,
}

impl IconSizes {
    /// The mockup sizes scaled by `k` (1.0 = mockup), at least 1 px each.
    pub fn scaled(k: f32) -> IconSizes {
        let s = |px: u32| ((px as f32 * k).round() as u32).max(1);
        IconSizes {
            inner: s(MOCKUP_ICON_PX.inner),
            outer: s(MOCKUP_ICON_PX.outer),
            sub: s(MOCKUP_ICON_PX.sub),
        }
    }
}

/// What a tile shows, decided from the item alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IconSpec {
    /// Icon `index` of a file (`.ico`, `.exe`, `.dll`), or the system icon of the target when the
    /// file has none. Environment variables in `path` are not expanded yet.
    File {
        path: String,
        index: i32,
    },
    /// A glyph of the Segoe icon font.
    Glyph(char),
    /// The first letter of the name, upper case.
    Letter(char),
    Blank,
}

/// What a tile draws once loaded.
pub enum TileIcon<'a> {
    Bitmap(&'a ID2D1Bitmap),
    /// Draw with `Icons::glyph_format`.
    Glyph(char),
    Letter(char),
    Blank,
}

/// Chooses what `item` shows: its `icon`, else the target's icon, a glyph or the first letter.
pub fn spec_for(item: &Item) -> IconSpec {
    if let Some(icon) = item
        .icon
        .as_deref()
        .map(str::trim)
        .filter(|i| !i.is_empty())
    {
        let (path, index) = split_icon_index(icon);
        return IconSpec::File {
            path: path.to_string(),
            index,
        };
    }
    match &item.action {
        Action::Program { path, .. } | Action::Open { path } => IconSpec::File {
            path: path.clone(),
            index: 0,
        },
        Action::Edit { command } => IconSpec::Glyph(edit_glyph(*command)),
        Action::System { action } => IconSpec::Glyph(system_glyph(*action)),
        Action::Url { .. } => IconSpec::Glyph('\u{E774}'),
        Action::Shortcut { .. } => IconSpec::Glyph('\u{E765}'),
        Action::PasteText { .. } => IconSpec::Glyph('\u{E8D2}'),
        Action::Command { .. } => IconSpec::Glyph('\u{E756}'),
        Action::None => first_letter(&item.name).map_or(IconSpec::Blank, IconSpec::Letter),
    }
}

/// Splits the Windows `file,N` form (N may be negative: a resource id); without a valid number
/// after the last comma the whole text is the path.
fn split_icon_index(icon: &str) -> (&str, i32) {
    match icon.rsplit_once(',') {
        Some((path, n)) => match n.trim().parse() {
            Ok(index) => (path.trim_end(), index),
            Err(_) => (icon, 0),
        },
        None => (icon, 0),
    }
}

/// First alphanumeric character of `name`, upper case (`ź` becomes `Ź`).
fn first_letter(name: &str) -> Option<char> {
    let c = name.chars().find(|c| c.is_alphanumeric())?;
    // A few letters upper-case to several characters (`ß` -> `SS`); keep the letter then.
    let mut upper = c.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(u), None) => Some(u),
        _ => Some(c),
    }
}

fn edit_glyph(command: EditCommand) -> char {
    match command {
        EditCommand::Cut => '\u{E8C6}',
        EditCommand::Copy => '\u{E8C8}',
        EditCommand::Paste => '\u{E77F}',
        EditCommand::PastePlain => '\u{E8E9}',
        EditCommand::Delete => '\u{E74D}',
        EditCommand::Undo => '\u{E7A7}',
        EditCommand::Redo => '\u{E7A6}',
        EditCommand::SelectAll => '\u{E8B3}',
        EditCommand::Save => '\u{E74E}',
        EditCommand::Find => '\u{E721}',
        EditCommand::Rename => '\u{E8AC}',
        EditCommand::Properties => '\u{E946}',
    }
}

fn system_glyph(action: SystemAction) -> char {
    match action {
        SystemAction::Lock => '\u{E72E}',
        SystemAction::CloseWindow => '\u{E8BB}',
        SystemAction::MinimizeWindow => '\u{E921}',
        SystemAction::ShowDesktop => '\u{E7F4}',
        SystemAction::VolumeMute => '\u{E74F}',
        SystemAction::VolumeUp => '\u{E995}',
        SystemAction::VolumeDown => '\u{E993}',
        SystemAction::Screenshot => '\u{E722}',
    }
}

/// What one tile resolved to.
enum Tile {
    Bitmap(ID2D1Bitmap),
    Glyph(char),
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
    /// Loads the icons of the inner ring, the first `outer_sectors` outer sectors and their
    /// submenus; sizes are in px of the device context (96 DPI). A file that cannot be loaded
    /// becomes a letter, and so does a glyph the font lacks (or any glyph without an icon font).
    pub fn new(
        ctx: &ID2D1DeviceContext,
        dwrite: &IDWriteFactory,
        config: &Config,
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
            wic: wic_factory().ok(),
            has_glyph: font.map(|(_, has)| has),
            bitmaps: HashMap::new(),
        };
        let mut tiles = |items: &[Item], size: u32| -> Vec<Tile> {
            items.iter().map(|item| loader.tile(item, size)).collect()
        };
        let outer_items = &config.outer[..config.outer.len().min(usize::from(outer_sectors))];
        let inner_items = &config.inner[..config.inner.len().min(INNER_SECTORS as usize)];
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
            Some(Tile::Glyph(c)) => TileIcon::Glyph(*c),
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
                Some(has) if has(c) => Tile::Glyph(c),
                _ => letter(),
            },
            IconSpec::Letter(c) => Tile::Letter(c),
            IconSpec::Blank => Tile::Blank,
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
                        eprintln!("wheel: no icon for \"{path}\": {e}");
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
    use std::collections::HashSet;

    use windows::Win32::Graphics::DirectWrite::{DWRITE_FACTORY_TYPE_SHARED, DWriteCreateFactory};

    use super::*;

    fn item(name: &str, action: Action) -> Item {
        Item {
            name: name.to_string(),
            action,
            icon: None,
            submenu: Vec::new(),
        }
    }

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

    const SYSTEM: [SystemAction; 8] = [
        SystemAction::Lock,
        SystemAction::CloseWindow,
        SystemAction::MinimizeWindow,
        SystemAction::ShowDesktop,
        SystemAction::VolumeMute,
        SystemAction::VolumeUp,
        SystemAction::VolumeDown,
        SystemAction::Screenshot,
    ];

    fn glyph(spec: IconSpec) -> char {
        match spec {
            IconSpec::Glyph(c) => c,
            other => panic!("not a glyph: {other:?}"),
        }
    }

    #[test]
    fn program_and_open_use_the_target_icon() {
        let program = item(
            "Notepad",
            Action::Program {
                path: "notepad.exe".into(),
                args: String::new(),
                working_dir: None,
            },
        );
        assert_eq!(
            spec_for(&program),
            IconSpec::File {
                path: "notepad.exe".into(),
                index: 0
            }
        );
        let open = item(
            "Docs",
            Action::Open {
                path: "%USERPROFILE%".into(),
            },
        );
        assert_eq!(
            spec_for(&open),
            IconSpec::File {
                path: "%USERPROFILE%".into(),
                index: 0
            }
        );
    }

    #[test]
    fn commands_and_system_actions_have_distinct_glyphs() {
        let edit: HashSet<char> = EDIT
            .iter()
            .map(|c| glyph(spec_for(&item("x", Action::Edit { command: *c }))))
            .collect();
        assert_eq!(edit.len(), 12);
        let system: HashSet<char> = SYSTEM
            .iter()
            .map(|a| glyph(spec_for(&item("x", Action::System { action: *a }))))
            .collect();
        assert_eq!(system.len(), 8);
        assert!(edit.is_disjoint(&system));
    }

    #[test]
    fn other_actions_have_glyphs() {
        let actions = [
            Action::Url {
                url: "https://example.com".into(),
            },
            Action::Shortcut {
                keys: "Ctrl+K".into(),
            },
            Action::PasteText { text: "t".into() },
            Action::Command { line: "dir".into() },
        ];
        let glyphs: HashSet<char> = actions
            .into_iter()
            .map(|a| glyph(spec_for(&item("x", a))))
            .collect();
        assert_eq!(glyphs.len(), 4);
    }

    #[test]
    fn icon_field_wins_and_parses_the_index() {
        let with = |icon: &str| {
            let mut i = item(
                "x",
                Action::Edit {
                    command: EditCommand::Copy,
                },
            );
            i.icon = Some(icon.into());
            spec_for(&i)
        };
        let file = |path: &str, index| IconSpec::File {
            path: path.into(),
            index,
        };
        assert_eq!(with("C:\\a\\b.ico"), file("C:\\a\\b.ico", 0));
        assert_eq!(with("shell32.dll,3"), file("shell32.dll", 3));
        assert_eq!(
            with("%SystemRoot%\\x.dll , -101"),
            file("%SystemRoot%\\x.dll", -101)
        );
        assert_eq!(with("C:\\a,b\\c.ico"), file("C:\\a,b\\c.ico", 0));
        // An empty icon falls back to the action's glyph.
        assert!(matches!(with("  "), IconSpec::Glyph(_)));
    }

    #[test]
    fn letter_is_the_first_alphanumeric_character_upper_case() {
        let letter = |name: &str| spec_for(&item(name, Action::None));
        assert_eq!(letter("zebra"), IconSpec::Letter('Z'));
        assert_eq!(letter("źrebię"), IconSpec::Letter('Ź'));
        assert_eq!(letter("  łódź"), IconSpec::Letter('Ł'));
        assert_eq!(letter("- (2nd)"), IconSpec::Letter('2'));
        assert_eq!(letter("ßeta"), IconSpec::Letter('ß'));
        assert_eq!(letter(""), IconSpec::Blank);
        assert_eq!(letter(" -- "), IconSpec::Blank);
    }

    #[test]
    fn scaled_sizes() {
        assert_eq!(
            IconSizes::scaled(1.0),
            IconSizes {
                inner: 24,
                outer: 36,
                sub: 30
            }
        );
        assert_eq!(IconSizes::scaled(0.0).inner, 1);
    }

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
