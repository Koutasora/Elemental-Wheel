//! Snapshot of the screen area under the wheel window, taken with GDI while the window is still
//! hidden. The renderer blurs it on the GPU; the pixels are not kept on the CPU afterwards.

use std::ffi::c_void;

use windows::Win32::Foundation::{E_FAIL, HWND};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, CreateCompatibleDC, CreateDIBSection,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, HBITMAP, HGDIOBJ, ReleaseDC, SRCCOPY,
    SelectObject,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};
use windows::core::{Error, Result};

/// A 32-bit top-down DIB section; the memory is released when it is dropped.
pub struct Snapshot {
    bitmap: HBITMAP,
    bits: *const u8,
    side: u32,
}

impl Snapshot {
    /// Bytes per row.
    pub fn pitch(&self) -> u32 {
        self.side * 4
    }

    /// BGRA pixels, alpha 255 everywhere.
    pub fn pixels(&self) -> &[u8] {
        // SAFETY: `bits` points to the `side` x `side` x 4 bytes of the DIB section, which lives
        // as long as `self`.
        unsafe { std::slice::from_raw_parts(self.bits, (self.side * self.side * 4) as usize) }
    }
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        // SAFETY: the bitmap was created in `capture` and is not selected into any DC anymore.
        let _ = unsafe { DeleteObject(HGDIOBJ(self.bitmap.0)) };
    }
}

/// Copies the `side` x `side` pixel square of the desktop whose top-left corner is (`left`,
/// `top`) in physical screen pixels. The part outside the virtual screen repeats the nearest
/// edge pixels, so no black borders appear.
pub fn capture(left: i32, top: i32, side: u32) -> Result<Snapshot> {
    // SAFETY: plain GDI calls with valid arguments; every handle is released on all paths.
    unsafe {
        let screen = GetDC(None);
        if screen.is_invalid() {
            return Err(Error::from(E_FAIL));
        }
        let mem = CreateCompatibleDC(Some(screen));
        let result = (|| {
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: side as i32,
                    biHeight: -(side as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits: *mut c_void = std::ptr::null_mut();
            let bitmap = CreateDIBSection(Some(mem), &info, DIB_RGB_COLORS, &mut bits, None, 0)?;
            let snapshot = Snapshot {
                bitmap,
                bits: bits.cast(),
                side,
            };
            // The rectangle that really lies on the virtual screen, in screen coordinates.
            let (vx, vy) = (
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
            );
            let (vw, vh) = (
                GetSystemMetrics(SM_CXVIRTUALSCREEN),
                GetSystemMetrics(SM_CYVIRTUALSCREEN),
            );
            let x0 = left.max(vx);
            let y0 = top.max(vy);
            let x1 = (left + side as i32).min(vx + vw);
            let y1 = (top + side as i32).min(vy + vh);
            if x1 <= x0 || y1 <= y0 {
                return Err(Error::from(E_FAIL));
            }
            let old = SelectObject(mem, HGDIOBJ(bitmap.0));
            let blit = BitBlt(
                mem,
                x0 - left,
                y0 - top,
                x1 - x0,
                y1 - y0,
                Some(screen),
                x0,
                y0,
                SRCCOPY | CAPTUREBLT,
            );
            SelectObject(mem, old);
            blit?;
            // SAFETY of the slice: the DIB section is exclusively ours and `side`² pixels long.
            let pixels = std::slice::from_raw_parts_mut(bits.cast::<u32>(), (side * side) as usize);
            finish(
                pixels,
                side as usize,
                (x0 - left) as usize..(x1 - left) as usize,
                (y0 - top) as usize..(y1 - top) as usize,
            );
            Ok(snapshot)
        })();
        let _ = DeleteDC(mem);
        ReleaseDC(None::<HWND>, screen);
        result
    }
}

/// Makes the pixels of the `valid` area opaque and spreads its edge pixels over the rest of the
/// `side` x `side` image.
fn finish(
    pixels: &mut [u32],
    side: usize,
    valid_x: std::ops::Range<usize>,
    valid_y: std::ops::Range<usize>,
) {
    for y in valid_y.clone() {
        let row = &mut pixels[y * side..(y + 1) * side];
        for px in &mut row[valid_x.clone()] {
            *px |= 0xff00_0000;
        }
        let (first, last) = (row[valid_x.start], row[valid_x.end - 1]);
        row[..valid_x.start].fill(first);
        row[valid_x.end..].fill(last);
    }
    for y in 0..valid_y.start {
        pixels.copy_within(valid_y.start * side..(valid_y.start + 1) * side, y * side);
    }
    for y in valid_y.end..side {
        pixels.copy_within((valid_y.end - 1) * side..valid_y.end * side, y * side);
    }
}

#[cfg(test)]
mod tests {
    use super::{capture, finish};

    /// `cargo test capture_timing -- --ignored --nocapture`: reads the screen, nothing else.
    #[test]
    #[ignore = "needs a desktop; prints timings"]
    fn capture_timing() {
        let mut ms: Vec<f64> = (0..30)
            .map(|_| {
                let t = std::time::Instant::now();
                let shot = capture(600, 200, 676).expect("capture");
                assert_eq!(shot.pixels().len(), 676 * 676 * 4);

                t.elapsed().as_secs_f64() * 1e3
            })
            .collect();
        println!("first {:.2} ms", ms[0]);
        ms.sort_by(f64::total_cmp);
        println!("median {:.2} ms, max {:.2} ms", ms[15], ms[29]);
    }

    #[test]
    fn edges_are_repeated_outside_the_valid_area() {
        let mut px = vec![0u32; 16];
        // The valid area is the 2x2 block at (1,1); its pixels are 1..=4 (without alpha).
        for (i, (x, y)) in [(1, 1), (2, 1), (1, 2), (2, 2)].into_iter().enumerate() {
            px[y * 4 + x] = i as u32 + 1;
        }
        finish(&mut px, 4, 1..3, 1..3);
        let a = 0xff00_0000;
        assert_eq!(&px[0..4], &[1 | a, 1 | a, 2 | a, 2 | a]);
        assert_eq!(&px[4..8], &[1 | a, 1 | a, 2 | a, 2 | a]);
        assert_eq!(&px[8..12], &[3 | a, 3 | a, 4 | a, 4 | a]);
        assert_eq!(&px[12..16], &[3 | a, 3 | a, 4 | a, 4 | a]);
    }
}
