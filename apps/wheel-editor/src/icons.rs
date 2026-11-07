//! Tile icons for the preview's icons view. What a tile shows is chosen by `wheel_core::icon`;
//! this module loads file icons through the shell (on a worker thread, so a slow path never
//! blocks the window) into egui textures, cached by (path, index, size).

use std::collections::{HashMap, VecDeque};
use std::ffi::c_void;
use std::sync::mpsc::{Receiver, Sender, channel};

use eframe::egui::{ColorImage, Context, TextureHandle, TextureId, TextureOptions};
use wheel_core::Item;
use wheel_core::icon::{IconSpec, first_letter, spec_for};

use windows::Win32::Foundation::{E_FAIL, MAX_PATH};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits,
    GetObjectW, HGDIOBJ, ReleaseDC,
};
use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_NORMAL, SearchPathW};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
use windows::Win32::UI::Controls::{IImageList, ILD_TRANSPARENT};
use windows::Win32::UI::Shell::{
    SHDefExtractIconW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGFI_SYSICONINDEX,
    SHGetFileInfoW, SHGetImageList, SHIL_EXTRALARGE, SHIL_JUMBO,
};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};
use windows::core::{PCWSTR, Result, w};

/// What a tile draws.
pub enum Drawn {
    Texture(TextureId),
    /// A glyph of the icon font.
    Glyph(char),
    Letter(char),
    Blank,
}

type Key = (String, i32, u32);

/// Loader threads running at the same time; further loads wait in a queue.
const MAX_LOADERS: usize = 4;

enum Entry {
    /// Being loaded on a worker thread.
    Pending,
    Failed,
    Ready(TextureHandle),
}

struct Slot {
    entry: Entry,
    /// Frame in which the icon was last asked for.
    used: u64,
}

/// Loaded file icons and whether the icon font is available.
pub struct Icons {
    glyph_font: bool,
    entries: HashMap<Key, Slot>,
    /// Keys waiting for a loader thread.
    queue: VecDeque<Key>,
    in_flight: usize,
    /// Frame counter, advanced by `poll`.
    frame: u64,
    tx: Sender<(Key, Option<ColorImage>)>,
    rx: Receiver<(Key, Option<ColorImage>)>,
}

impl Icons {
    pub fn new(glyph_font: bool) -> Icons {
        let (tx, rx) = channel();
        Icons {
            glyph_font,
            entries: HashMap::new(),
            queue: VecDeque::new(),
            in_flight: 0,
            frame: 0,
            tx,
            rx,
        }
    }

    /// Call once per frame, before drawing: turns finished loads into textures, drops icons not
    /// asked for in the previous frame (and their textures) and starts queued loads.
    pub fn poll(&mut self, ctx: &Context) {
        self.frame += 1;
        while let Ok((key, image)) = self.rx.try_recv() {
            self.in_flight = self.in_flight.saturating_sub(1);
            let entry = match image {
                Some(image) => Entry::Ready(ctx.load_texture(
                    format!("icon {} {}", key.0, key.1),
                    image,
                    TextureOptions::LINEAR,
                )),
                None => Entry::Failed,
            };
            // The slot may be gone only if it was never created; a loaded icon nobody asks for
            // any more is dropped by `evict` below.
            let used = self.entries.get(&key).map_or(self.frame, |s| s.used);
            self.entries.insert(key, Slot { entry, used });
        }
        self.evict();
        while self.in_flight < MAX_LOADERS {
            let Some(key) = self.queue.pop_front() else {
                break;
            };
            self.in_flight += 1;
            let (tx, ctx) = (self.tx.clone(), ctx.clone());
            std::thread::spawn(move || {
                let image = load_image(&key.0, key.1, key.2);
                let _ = tx.send((key, image));
                ctx.request_repaint();
            });
        }
    }

    /// Removes entries not used in the previous frame; loads in progress stay.
    fn evict(&mut self) {
        let frame = self.frame;
        self.entries
            .retain(|_, s| matches!(s.entry, Entry::Pending) || s.used + 1 >= frame);
    }

    /// What `item` shows at `px` physical pixels. A file icon that is still loading is blank,
    /// one that cannot be loaded becomes the letter.
    pub fn resolve(&mut self, ctx: &Context, item: &Item, px: u32) -> Drawn {
        match spec_for(item) {
            IconSpec::File { path, index } => {
                let key = (path, index, px);
                let frame = self.frame;
                if let Some(slot) = self.entries.get_mut(&key) {
                    slot.used = frame;
                    return match &slot.entry {
                        Entry::Ready(texture) => Drawn::Texture(texture.id()),
                        Entry::Pending => Drawn::Blank,
                        Entry::Failed => letter(&item.name),
                    };
                }
                self.queue.push_back(key.clone());
                self.entries.insert(
                    key,
                    Slot {
                        entry: Entry::Pending,
                        used: frame,
                    },
                );
                // Started by the next `poll`.
                ctx.request_repaint();
                Drawn::Blank
            }
            IconSpec::Glyph(c) => glyph_or_letter(c, self.glyph_font, &item.name),
            IconSpec::Letter(c) => Drawn::Letter(c),
            IconSpec::Blank => Drawn::Blank,
        }
    }
}

fn letter(name: &str) -> Drawn {
    first_letter(name).map_or(Drawn::Blank, Drawn::Letter)
}

/// The glyph, or the first letter of the name when there is no icon font.
fn glyph_or_letter(glyph: char, font: bool, name: &str) -> Drawn {
    if font {
        Drawn::Glyph(glyph)
    } else {
        letter(name)
    }
}

/// Icon of `path` (environment variables expanded) as RGBA, `None` when it cannot be loaded.
fn load_image(path: &str, index: i32, size: u32) -> Option<ColorImage> {
    // SAFETY: plain call; balanced by `CoUninitialize` below when it succeeded.
    let com = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
    let image = load_rgba(&resolve_path(&expand(path)), index, size);
    if com {
        // SAFETY: balances the successful `CoInitializeEx` on this thread.
        unsafe { CoUninitialize() };
    }
    match image {
        Ok(image) => Some(image),
        Err(e) => {
            eprintln!("wheel-editor: no icon for \"{path}\": {e}");
            None
        }
    }
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

/// A bare file name (`notepad.exe`) is looked up on the search path, as the wheel does; the
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

/// Icon `index` of `path` at `size` px; falls back to the system icon associated with the path
/// (folders, documents, files without icons).
fn load_rgba(path: &str, index: i32, size: u32) -> Result<ColorImage> {
    let file = wide(path);
    let hicon = extract_icon(&file, index, size)
        .or_else(|| shell_icon(&file, size))
        .ok_or_else(|| windows::core::Error::from(E_FAIL))?;
    let image = icon_to_image(hicon);
    // SAFETY: `hicon` is a valid icon owned here.
    let _ = unsafe { DestroyIcon(hicon) };
    image
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
/// system image list in a size of at least `size`, as the wheel does.
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

/// Reads the pixels of a colour bitmap as top-down 32-bit BGRA rows.
fn bitmap_bgra(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    bitmap: windows::Win32::Graphics::Gdi::HBITMAP,
    w: i32,
    h: i32,
) -> Option<Vec<u8>> {
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut buf = vec![0u8; w as usize * h as usize * 4];
    // SAFETY: `buf` holds `h` rows of `w` 32-bit pixels, as requested in `info`.
    let rows = unsafe {
        GetDIBits(
            hdc,
            bitmap,
            0,
            h as u32,
            Some(buf.as_mut_ptr() as *mut c_void),
            &mut info,
            DIB_RGB_COLORS,
        )
    };
    (rows == h).then_some(buf)
}

fn icon_to_image(hicon: HICON) -> Result<ColorImage> {
    let mut info = ICONINFO::default();
    // SAFETY: `hicon` is valid and `info` is a valid out structure.
    unsafe { GetIconInfo(hicon, &mut info) }?;
    let result = (|| {
        // A monochrome icon has no colour bitmap; such icons are not supported.
        if info.hbmColor.is_invalid() {
            return Err(windows::core::Error::from(E_FAIL));
        }
        let mut bm = BITMAP::default();
        // SAFETY: `bm` is a valid out buffer of the size given.
        let got = unsafe {
            GetObjectW(
                HGDIOBJ(info.hbmColor.0),
                size_of::<BITMAP>() as i32,
                Some(&mut bm as *mut BITMAP as *mut c_void),
            )
        };
        if got == 0 || bm.bmWidth <= 0 || bm.bmHeight <= 0 {
            return Err(windows::core::Error::from(E_FAIL));
        }
        // SAFETY: the DC is released below.
        let hdc = unsafe { GetDC(None) };
        let color = bitmap_bgra(hdc, info.hbmColor, bm.bmWidth, bm.bmHeight);
        let mask = (!info.hbmMask.is_invalid())
            .then(|| bitmap_bgra(hdc, info.hbmMask, bm.bmWidth, bm.bmHeight))
            .flatten();
        // SAFETY: releases the DC taken above.
        unsafe { ReleaseDC(None, hdc) };
        let color = color.ok_or_else(|| windows::core::Error::from(E_FAIL))?;
        let rgba = bgra_to_rgba(&color, mask.as_deref());
        Ok(ColorImage::from_rgba_unmultiplied(
            [bm.bmWidth as usize, bm.bmHeight as usize],
            &rgba,
        ))
    })();
    // SAFETY: the bitmaps of `GetIconInfo` belong to the caller.
    unsafe {
        if !info.hbmColor.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(info.hbmColor.0));
        }
        if !info.hbmMask.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(info.hbmMask.0));
        }
    }
    result
}

/// Converts BGRA pixels to RGBA. An icon without an alpha channel (every alpha 0) takes its
/// transparency from the AND `mask` (a black pixel is opaque); without a mask it is opaque.
fn bgra_to_rgba(bgra: &[u8], mask: Option<&[u8]>) -> Vec<u8> {
    let pixels = bgra.as_chunks::<4>().0;
    let has_alpha = pixels.iter().any(|p| p[3] != 0);
    pixels
        .iter()
        .enumerate()
        .flat_map(|(i, p)| {
            let alpha = if has_alpha {
                p[3]
            } else if mask.is_some_and(|m| m.get(i * 4).is_some_and(|b| *b != 0)) {
                0
            } else {
                255
            };
            [p[2], p[1], p[0], alpha]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swaps_channels_and_keeps_alpha() {
        let bgra = [10, 20, 30, 200, 1, 2, 3, 0];
        assert_eq!(bgra_to_rgba(&bgra, None), [30, 20, 10, 200, 3, 2, 1, 0]);
    }

    #[test]
    fn icons_without_alpha_use_the_mask() {
        let bgra = [10, 20, 30, 0, 1, 2, 3, 0];
        // First pixel opaque (mask black), second transparent (mask white).
        let mask = [0, 0, 0, 0, 255, 255, 255, 0];
        assert_eq!(
            bgra_to_rgba(&bgra, Some(&mask)),
            [30, 20, 10, 255, 3, 2, 1, 0]
        );
        assert_eq!(bgra_to_rgba(&bgra, None), [30, 20, 10, 255, 3, 2, 1, 255]);
    }

    #[test]
    fn glyph_without_font_becomes_the_letter() {
        assert!(matches!(
            glyph_or_letter('\u{E774}', true, "Web"),
            Drawn::Glyph(_)
        ));
        assert!(matches!(
            glyph_or_letter('\u{E774}', false, "web"),
            Drawn::Letter('W')
        ));
        assert!(matches!(
            glyph_or_letter('\u{E774}', false, ""),
            Drawn::Blank
        ));
    }

    #[test]
    fn unused_icons_are_evicted_but_loading_ones_stay() {
        let mut icons = Icons::new(false);
        let key = |n: &str| (n.to_string(), 0, 16);
        for (name, entry, used) in [
            ("recent", Entry::Failed, 9),
            ("previous", Entry::Failed, 8),
            ("old", Entry::Failed, 7),
            ("loading", Entry::Pending, 1),
        ] {
            icons.entries.insert(key(name), Slot { entry, used });
        }
        icons.frame = 9;
        icons.evict();
        let mut left: Vec<_> = icons.entries.keys().map(|k| k.0.as_str()).collect();
        left.sort();
        assert_eq!(left, ["loading", "previous", "recent"]);
    }

    #[test]
    fn bare_names_are_searched() {
        assert!(resolve_path("notepad.exe").contains('\\'));
        assert_eq!(resolve_path("no-such-file-xyz.exe"), "no-such-file-xyz.exe");
    }

    #[test]
    fn system_icons_load_as_rgba() {
        for (path, index) in [
            ("C:\\Windows\\System32\\notepad.exe", 0),
            ("%SystemRoot%\\System32\\shell32.dll", 3),
            // A folder has no icon resource: the shell icon is used.
            ("%SystemRoot%", 0),
        ] {
            let image = load_image(path, index, 32).expect(path);
            assert!(image.size[0] > 0 && image.size[1] > 0, "{path}");
            assert!(image.pixels.iter().any(|p| p.a() > 0), "{path} is empty");
        }
    }
}
