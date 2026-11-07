//! Wheel rendering: D3D11 device -> DXGI composition swap chain -> Direct2D, shown through
//! DirectComposition on the window. Text goes through DirectWrite.

use std::cell::RefCell;
use std::collections::HashMap;
use std::mem::ManuallyDrop;
use std::sync::Arc;

use wheel_core::icon::IconSizes;
use wheel_core::{Config, Hit, Layout, SubArc, ViewMode};

use windows::Win32::Foundation::{E_FAIL, HMODULE, HWND};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D_SIZE_F, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_FIGURE_BEGIN_FILLED,
    D2D1_FIGURE_END_CLOSED, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ARC_SEGMENT, D2D1_ARC_SIZE_LARGE, D2D1_ARC_SIZE_SMALL, D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
    D2D1_BITMAP_OPTIONS_TARGET, D2D1_BITMAP_PROPERTIES1, D2D1_DEVICE_CONTEXT_OPTIONS_NONE,
    D2D1_DRAW_TEXT_OPTIONS_CLIP, D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ELLIPSE,
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC,
    D2D1_ROUNDED_RECT, D2D1_SWEEP_DIRECTION_CLOCKWISE, D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE,
    D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE, D2D1CreateFactory, ID2D1Bitmap1, ID2D1Brush,
    ID2D1DeviceContext, ID2D1Factory1, ID2D1Geometry, ID2D1Image, ID2D1PathGeometry,
    ID2D1SolidColorBrush, ID2D1StrokeStyle,
};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device,
};
use windows::Win32::Graphics::DirectComposition::{
    DCompositionCreateDevice, IDCompositionDevice, IDCompositionTarget, IDCompositionVisual,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_BOLD, DWRITE_FONT_WEIGHT_MEDIUM,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_METRICS,
    DWRITE_TEXT_RANGE, DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER,
    DWRITE_WORD_WRAPPING_NO_WRAP, DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat,
    IDWriteTextLayout,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory2, DXGI_CREATE_FACTORY_FLAGS, DXGI_PRESENT, DXGI_SCALING_STRETCH,
    DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL, DXGI_USAGE_RENDER_TARGET_OUTPUT,
    IDXGIDevice, IDXGIFactory2, IDXGISurface, IDXGISwapChain1,
};
use windows::core::{Error, Interface, Result, w};
use windows_numerics::Vector2;

use crate::fit;
use crate::icons::{Icons, TileIcon};
use crate::theme::Palette;

/// Outer radius of the mockup at 100% and 96 DPI; the mockup's other sizes (fonts, gaps,
/// stroke) are expressed relative to it.
const MOCKUP_OUTER_RADIUS: f32 = 210.0;
/// Half of the angular gap between neighbouring sectors, in degrees (mockup values).
const OUTER_HALF_GAP_DEG: f32 = 0.8;
const INNER_HALF_GAP_DEG: f32 = 1.4;
/// Half of the angular width of the submenu notch, in degrees (mockup value).
const NOTCH_HALF_DEG: f32 = 5.0;

pub struct Renderer {
    painter: Painter,
    swap_chain: IDXGISwapChain1,
    dcomp: IDCompositionDevice,
    // Kept alive for as long as the window shows the wheel.
    _target: IDCompositionTarget,
    _visual: IDCompositionVisual,
}

/// Everything needed to paint the wheel on a Direct2D device context. The window-bound part
/// (swap chain, DirectComposition) lives in `Renderer`, which keeps painting testable offscreen.
struct Painter {
    layout: Layout,
    palette: Palette,
    /// Source of the sector names.
    config: Arc<Config>,
    /// Window / swap chain side in pixels.
    size: u32,
    d2d_factory: ID2D1Factory1,
    ctx: ID2D1DeviceContext,
    brush: ID2D1SolidColorBrush,
    outer_font: IDWriteTextFormat,
    inner_font: IDWriteTextFormat,
    sub_font: IDWriteTextFormat,
    hub_font: IDWriteTextFormat,
    /// Bold format of the letter shown instead of an icon.
    letter_font: IDWriteTextFormat,
    dwrite: IDWriteFactory,
    /// Tile icons for the icons view; `None` in the names view.
    icons: Option<Icons>,
    icon_sizes: IconSizes,
    /// Fitted text layouts per label (kind, text), built on first use: the layout never changes
    /// for the lifetime of the renderer.
    layouts: RefCell<HashMap<(Kind, String), FittedLabel>>,
}

impl Renderer {
    pub fn new(
        hwnd: HWND,
        layout: Layout,
        palette: Palette,
        config: Arc<Config>,
    ) -> Result<Renderer> {
        let size = layout.size_px();
        let device = create_d3d_device()?;
        let dxgi_device: IDXGIDevice = device.cast()?;

        // SAFETY: plain COM calls with valid arguments; every result is checked via `?`.
        unsafe {
            let dxgi_factory: IDXGIFactory2 = CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0))?;
            let desc = DXGI_SWAP_CHAIN_DESC1 {
                Width: size,
                Height: size,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                Stereo: false.into(),
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                Scaling: DXGI_SCALING_STRETCH,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
                AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
                Flags: 0,
            };
            let swap_chain = dxgi_factory.CreateSwapChainForComposition(&device, &desc, None)?;
            let painter = Painter::new(&dxgi_device, layout, palette, config)?;

            let dcomp: IDCompositionDevice = DCompositionCreateDevice(&dxgi_device)?;
            let target = dcomp.CreateTargetForHwnd(hwnd, true)?;
            let visual = dcomp.CreateVisual()?;
            visual.SetContent(&swap_chain)?;
            target.SetRoot(&visual)?;
            dcomp.Commit()?;

            Ok(Renderer {
                painter,
                swap_chain,
                dcomp,
                _target: target,
                _visual: visual,
            })
        }
    }

    /// Draws the whole wheel with `hit` highlighted and the submenu `arc` open (if any), and
    /// presents it.
    pub fn draw(&self, hit: Hit, arc: Option<SubArc>) -> Result<()> {
        // SAFETY: the swap chain and device context are owned by `self` and valid.
        unsafe {
            // With the flip model the back buffer changes after every Present, so the Direct2D
            // target is re-created each frame.
            let surface: IDXGISurface = self.swap_chain.GetBuffer(0)?;
            let props = D2D1_BITMAP_PROPERTIES1 {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96.0,
                dpiY: 96.0,
                bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                colorContext: ManuallyDrop::new(None),
            };
            let bitmap = self
                .painter
                .ctx
                .CreateBitmapFromDxgiSurface(&surface, Some(&props))?;
            self.painter.paint(&bitmap, hit, arc)?;

            self.swap_chain.Present(1, DXGI_PRESENT(0)).ok()?;
            self.dcomp.Commit()
        }
    }
}

impl Painter {
    fn new(
        dxgi_device: &IDXGIDevice,
        layout: Layout,
        palette: Palette,
        config: Arc<Config>,
    ) -> Result<Painter> {
        let size = layout.size_px();
        // SAFETY: plain COM calls with valid arguments; every result is checked via `?`.
        unsafe {
            let d2d_factory: ID2D1Factory1 =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let d2d_device = d2d_factory.CreateDevice(dxgi_device)?;
            let ctx = d2d_device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
            // The target is in physical pixels, so one D2D unit is one pixel.
            ctx.SetDpi(96.0, 96.0);
            ctx.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            let brush = ctx.CreateSolidColorBrush(&palette.fg, None)?;

            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let k = scale_factor(&layout);
            let icon_sizes = IconSizes::scaled(k);
            let icons = (config.options.view == ViewMode::Icons).then(|| {
                Icons::new(
                    &ctx,
                    &dwrite,
                    &config,
                    layout.inner_sectors(),
                    layout.outer_sectors(),
                    icon_sizes,
                )
            });
            Ok(Painter {
                layout,
                palette,
                config,
                size,
                d2d_factory,
                ctx,
                brush,
                outer_font: text_format(&dwrite, 15.0 * k, DWRITE_FONT_WEIGHT_MEDIUM)?,
                inner_font: text_format(&dwrite, 11.5 * k, DWRITE_FONT_WEIGHT_MEDIUM)?,
                sub_font: text_format(&dwrite, 13.5 * k, DWRITE_FONT_WEIGHT_MEDIUM)?,
                hub_font: text_format(&dwrite, 14.0 * k, DWRITE_FONT_WEIGHT_MEDIUM)?,
                letter_font: text_format(&dwrite, 14.0 * k, DWRITE_FONT_WEIGHT_BOLD)?,
                dwrite,
                icons,
                icon_sizes,
                layouts: RefCell::new(HashMap::new()),
            })
        }
    }

    /// Draws the wheel into `target`, a Direct2D target bitmap of `size` x `size` pixels.
    fn paint(&self, target: &ID2D1Bitmap1, hit: Hit, arc: Option<SubArc>) -> Result<()> {
        // SAFETY: the device context is owned by `self`; EndDraw closes the BeginDraw below.
        unsafe {
            self.ctx.SetTarget(target);
            self.ctx.BeginDraw();
            let drawn = self.draw_wheel(hit, arc);
            // EndDraw must be called even if drawing failed.
            let ended = self.ctx.EndDraw(None, None);
            self.ctx.SetTarget(None::<&ID2D1Image>);
            drawn?;
            ended
        }
    }

    /// # Safety
    /// Must be called between `BeginDraw` and `EndDraw` of `self.ctx`.
    unsafe fn draw_wheel(&self, hit: Hit, arc: Option<SubArc>) -> Result<()> {
        let p = &self.palette;
        let k = scale_factor(&self.layout);
        let c = self.size as f32 / 2.0;
        let radii = self.layout.radii();
        let stroke = 1.2 * k;
        // Keeps label text off the sector edges.
        let pad = k;
        // Keep the outer stroke inside the window.
        let outer_edge = self.layout.outer_radius() - stroke / 2.0;
        let inner_ring = (radii.center + 4.0 * k, radii.inner - 2.0 * k);
        let outer_ring = (radii.inner + 2.0 * k, outer_edge);
        let sub_ring = {
            let (r1, r2) = self.layout.sub_radii();
            (r1, r2 - stroke / 2.0)
        };
        let n = self.layout.outer_sectors();

        // SAFETY: guaranteed by the caller.
        unsafe {
            self.ctx.Clear(Some(&D2D1_COLOR_F {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            }));

            for i in 0..self.layout.inner_sectors() {
                let (a1, a2) = self.layout.inner_span(i);
                let geometry = self.sector_path(
                    c,
                    inner_ring,
                    a1 + INNER_HALF_GAP_DEG,
                    a2 - INNER_HALF_GAP_DEG,
                )?;
                let on = hit == Hit::Inner(i);
                self.fill_tile(&geometry, p.tile, on, stroke);
                if self.icons.is_some() {
                    // The mockup puts inner icons 2 px outwards of the ring's middle.
                    let r = (inner_ring.0 + inner_ring.1) / 2.0 + 2.0 * k;
                    let at = polar(c, r, (a1 + a2) / 2.0);
                    self.icon(Hit::Inner(i), at, self.icon_sizes.inner, on, k)?;
                    continue;
                }
                let text_ring = (inner_ring.0 + pad, inner_ring.1 - pad);
                self.label(
                    Kind::Inner(i),
                    self.config.inner[i as usize].name.as_str(),
                    &self.inner_font,
                    &|h, need| {
                        let (w, at) = fit::sector_slot(
                            text_ring,
                            a1 + INNER_HALF_GAP_DEG,
                            a2 - INNER_HALF_GAP_DEG,
                            h,
                            need + pad,
                        );
                        (w - pad, (c + at.0, c + at.1))
                    },
                    if on { p.accent_fg } else { p.fg },
                )?;
            }

            for (i, name) in (0..n).zip(self.config.outer_names(n)) {
                let (a1, a2) = self.layout.outer_span(i);
                let geometry = self.sector_path(
                    c,
                    outer_ring,
                    a1 + OUTER_HALF_GAP_DEG,
                    a2 - OUTER_HALF_GAP_DEG,
                )?;
                let on = hit == Hit::Outer(i);
                self.fill_tile(&geometry, p.tile, on, stroke);
                if self.icons.is_some() {
                    let r = (outer_ring.0 + outer_ring.1) / 2.0;
                    let at = polar(c, r, (a1 + a2) / 2.0);
                    self.icon(Hit::Outer(i), at, self.icon_sizes.outer, on, k)?;
                } else {
                    let text_ring = (outer_ring.0 + pad, outer_ring.1 - pad);
                    self.label(
                        Kind::Outer(i),
                        name,
                        &self.outer_font,
                        &|h, need| {
                            let (w, at) = fit::sector_slot(
                                text_ring,
                                a1 + OUTER_HALF_GAP_DEG,
                                a2 - OUTER_HALF_GAP_DEG,
                                h,
                                need + pad,
                            );
                            (w - pad, (c + at.0, c + at.1))
                        },
                        if on { p.accent_fg } else { p.fg },
                    )?;
                }
                // Notch at the outer edge of a tile with a submenu.
                let has_submenu = self
                    .config
                    .outer_item(usize::from(i))
                    .is_some_and(|item| !item.submenu.is_empty());
                if has_submenu {
                    let mid = (a1 + a2) / 2.0;
                    let notch_ring = (
                        self.layout.outer_radius() - 9.0 * k,
                        self.layout.outer_radius() - 5.0 * k,
                    );
                    let notch = self.sector_path(
                        c,
                        notch_ring,
                        mid - NOTCH_HALF_DEG,
                        mid + NOTCH_HALF_DEG,
                    )?;
                    // While only its arc is open the tile itself is not highlighted, so the notch
                    // takes the accent colour instead of the text colour on accent.
                    let color = if on {
                        &p.accent_fg
                    } else if arc.is_some_and(|a| a.parent == i) {
                        &p.accent
                    } else {
                        &p.muted
                    };
                    self.brush.SetColor(color);
                    self.ctx
                        .FillGeometry(&notch, &self.brush, None::<&ID2D1Brush>);
                }
            }

            if let Some(arc) = arc.filter(|a| a.parent < n) {
                let subs = self
                    .config
                    .outer_item(usize::from(arc.parent))
                    .map_or(&[][..], |item| item.submenu.as_slice());
                for (j, item) in subs.iter().take(usize::from(arc.count)).enumerate() {
                    // `count` is at most MAX_SUB_TILES (5), so `j` fits in u8.
                    let j = j as u8;
                    let (a1, a2) = self.layout.sub_span(arc, j);
                    let geometry = self.sector_path(
                        c,
                        sub_ring,
                        a1 + OUTER_HALF_GAP_DEG,
                        a2 - OUTER_HALF_GAP_DEG,
                    )?;
                    let tile = Hit::Sub(arc.parent, j);
                    let on = hit == tile;
                    self.fill_tile(&geometry, p.surface, on, stroke);
                    if self.icons.is_some() {
                        let at = polar(c, (sub_ring.0 + sub_ring.1) / 2.0, (a1 + a2) / 2.0);
                        self.icon(tile, at, self.icon_sizes.sub, on, k)?;
                        continue;
                    }
                    let text_ring = (sub_ring.0 + pad, sub_ring.1 - pad);
                    self.label(
                        Kind::Sub(arc.parent, j),
                        item.name.as_str(),
                        &self.sub_font,
                        &|h, need| {
                            let (w, at) = fit::sector_slot(
                                text_ring,
                                a1 + OUTER_HALF_GAP_DEG,
                                a2 - OUTER_HALF_GAP_DEG,
                                h,
                                need + pad,
                            );
                            (w - pad, (c + at.0, c + at.1))
                        },
                        if on { p.accent_fg } else { p.fg },
                    )?;
                }
            }

            // Center: cancel; while a sector is highlighted it shows that sector's name.
            let hub = D2D1_ELLIPSE {
                point: Vector2 { X: c, Y: c },
                radiusX: radii.center,
                radiusY: radii.center,
            };
            let hub_on = hit == Hit::Center;
            self.brush.SetColor(&p.center);
            self.ctx.FillEllipse(&hub, &self.brush);
            self.brush
                .SetColor(if hub_on { &p.accent } else { &p.tile_line });
            self.ctx.DrawEllipse(
                &hub,
                &self.brush,
                if hub_on { 2.5 * k } else { stroke },
                None::<&ID2D1StrokeStyle>,
            );
            let text = match hit {
                Hit::Center => "Anuluj",
                Hit::Inner(i) => self.config.inner[i as usize].name.as_str(),
                Hit::Outer(i) => self
                    .config
                    .outer_names(n)
                    .get(i as usize)
                    .copied()
                    .unwrap_or(""),
                Hit::Sub(..) => wheel_core::item_for_hit(hit, &self.config)
                    .map_or("", |item| item.name.as_str()),
            };
            self.label(
                Kind::Hub,
                text,
                &self.hub_font,
                &|h, _| (fit::circle_width(radii.center - pad, h) - pad, (c, c)),
                p.fg,
            )?;
        }
        Ok(())
    }

    /// # Safety
    /// Must be called between `BeginDraw` and `EndDraw` of `self.ctx`.
    unsafe fn fill_tile(
        &self,
        geometry: &ID2D1Geometry,
        fill: D2D1_COLOR_F,
        on: bool,
        stroke: f32,
    ) {
        let p = &self.palette;
        // SAFETY: guaranteed by the caller.
        unsafe {
            self.brush.SetColor(if on { &p.accent } else { &fill });
            self.ctx
                .FillGeometry(geometry, &self.brush, None::<&ID2D1Brush>);
            self.brush
                .SetColor(if on { &p.accent } else { &p.tile_line });
            self.ctx
                .DrawGeometry(geometry, &self.brush, stroke, None::<&ID2D1StrokeStyle>);
        }
    }

    /// Annular sector between radii `ring.0..ring.1` and angles `a1..a2` (degrees, 0 = up,
    /// clockwise), around the point (`c`, `c`).
    fn sector_path(&self, c: f32, ring: (f32, f32), a1: f32, a2: f32) -> Result<ID2D1Geometry> {
        let (r1, r2) = ring;
        let arc_size = if a2 - a1 > 180.0 {
            D2D1_ARC_SIZE_LARGE
        } else {
            D2D1_ARC_SIZE_SMALL
        };
        let point = |r: f32, a: f32| {
            let (x, y) = polar(c, r, a);
            Vector2 { X: x, Y: y }
        };
        // SAFETY: plain COM calls; the sink is closed before the geometry is used.
        unsafe {
            let path: ID2D1PathGeometry = self.d2d_factory.CreatePathGeometry()?.into();
            let sink = path.Open()?;
            sink.BeginFigure(point(r2, a1), D2D1_FIGURE_BEGIN_FILLED);
            sink.AddArc(&D2D1_ARC_SEGMENT {
                point: point(r2, a2),
                size: D2D_SIZE_F {
                    width: r2,
                    height: r2,
                },
                rotationAngle: 0.0,
                sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                arcSize: arc_size,
            });
            sink.AddLine(point(r1, a2));
            sink.AddArc(&D2D1_ARC_SEGMENT {
                point: point(r1, a1),
                size: D2D_SIZE_F {
                    width: r1,
                    height: r1,
                },
                rotationAngle: 0.0,
                sweepDirection: D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE,
                arcSize: arc_size,
            });
            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
            sink.Close()?;
            path.cast()
        }
    }

    /// Draws the icon of tile `hit` centred on `at`, `size` px square; text and strokes use the
    /// tile text colour (accent text colour while the tile is highlighted).
    ///
    /// # Safety
    /// Must be called between `BeginDraw` and `EndDraw` of `self.ctx`.
    unsafe fn icon(&self, hit: Hit, at: (f32, f32), size: u32, on: bool, k: f32) -> Result<()> {
        let Some(icons) = &self.icons else {
            return Ok(());
        };
        let color = if on {
            self.palette.accent_fg
        } else {
            self.palette.fg
        };
        let s = size as f32;
        let half = s / 2.0;
        // Text layouts are the size of the icon box and centred in it.
        let draw_text = |text: char, font: &IDWriteTextFormat, em: f32| -> Result<()> {
            let mut buf = [0u8; 4];
            let layout = self.text_layout(text.encode_utf8(&mut buf), font, em, s, s)?;
            // SAFETY: guaranteed by the caller of `icon`.
            unsafe {
                self.brush.SetColor(&color);
                self.ctx.DrawTextLayout(
                    Vector2 {
                        X: at.0 - half,
                        Y: at.1 - half,
                    },
                    &layout,
                    &self.brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                );
            }
            Ok(())
        };
        // SAFETY: guaranteed by the caller.
        unsafe {
            match icons.icon_for(hit) {
                TileIcon::Bitmap(bitmap) => {
                    let rect = D2D_RECT_F {
                        left: at.0 - half,
                        top: at.1 - half,
                        right: at.0 + half,
                        bottom: at.1 + half,
                    };
                    self.ctx.DrawBitmap(
                        bitmap,
                        Some(&rect),
                        1.0,
                        D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC,
                        None,
                        None,
                    );
                }
                TileIcon::Glyph(glyph) => {
                    if let Some(font) = icons.glyph_format() {
                        draw_text(glyph, font, s)?;
                    }
                }
                TileIcon::Letter(letter) => {
                    let outline = D2D1_ROUNDED_RECT {
                        rect: D2D_RECT_F {
                            left: at.0 - half,
                            top: at.1 - half,
                            right: at.0 + half,
                            bottom: at.1 + half,
                        },
                        radiusX: s * 0.22,
                        radiusY: s * 0.22,
                    };
                    self.brush.SetColor(&color);
                    self.ctx.DrawRoundedRectangle(
                        &outline,
                        &self.brush,
                        1.6 * k,
                        None::<&ID2D1StrokeStyle>,
                    );
                    draw_text(letter, &self.letter_font, s * 0.55)?;
                }
                TileIcon::Blank => {}
            }
        }
        Ok(())
    }

    /// Draws `text` fitted by `fit::fit`: `slot(height, need)` gives the width available for a block of
    /// that height and the point it is centred on. The fitted layout is built once per label and
    /// reused on later frames.
    ///
    /// # Safety
    /// Must be called between `BeginDraw` and `EndDraw` of `self.ctx`.
    unsafe fn label(
        &self,
        kind: Kind,
        text: &str,
        font: &IDWriteTextFormat,
        slot: &dyn Fn(f32, f32) -> (f32, (f32, f32)),
        color: D2D1_COLOR_F,
    ) -> Result<()> {
        let mut layouts = self.layouts.borrow_mut();
        let key = (kind, text.to_string());
        if !layouts.contains_key(&key) {
            layouts.insert(key.clone(), self.fit_label(text, font, slot)?);
        }
        let fitted = &layouts[&key];
        // SAFETY: guaranteed by the caller.
        unsafe {
            self.brush.SetColor(&color);
            self.ctx.DrawTextLayout(
                Vector2 {
                    X: fitted.at.0 - fitted.width / 2.0,
                    Y: fitted.at.1 - fitted.height / 2.0,
                },
                &fitted.layout,
                &self.brush,
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
            );
        }
        Ok(())
    }

    /// Measures `text` with DirectWrite and builds the layout `fit::fit` chose for it.
    fn fit_label(
        &self,
        text: &str,
        font: &IDWriteTextFormat,
        slot: &dyn Fn(f32, f32) -> (f32, (f32, f32)),
    ) -> Result<FittedLabel> {
        // SAFETY: plain COM call.
        let base = unsafe { font.GetFontSize() };
        let measure = |t: &str, scale: f32| {
            self.text_layout(t, font, base * scale, 10_000.0, 10_000.0)
                .and_then(|l| {
                    let mut m = DWRITE_TEXT_METRICS::default();
                    // SAFETY: `m` is a valid out pointer for the call.
                    unsafe { l.GetMetrics(&mut m)? };
                    Ok((m.width, m.height))
                })
                .unwrap_or((f32::INFINITY, f32::INFINITY))
        };
        let fitted = fit::fit(text, measure, |h| slot(h, f32::INFINITY).0);
        let (text_width, height) = measure(&fitted.text, fitted.scale);
        let (width, at) = slot(height, text_width);
        let width = width.max(1.0);
        let layout = self.text_layout(&fitted.text, font, base * fitted.scale, width, height)?;
        Ok(FittedLabel {
            layout,
            width,
            height,
            at,
        })
    }

    fn text_layout(
        &self,
        text: &str,
        font: &IDWriteTextFormat,
        size: f32,
        width: f32,
        height: f32,
    ) -> Result<IDWriteTextLayout> {
        let wide: Vec<u16> = text.encode_utf16().collect();
        // SAFETY: plain COM calls with valid arguments.
        unsafe {
            let layout = self.dwrite.CreateTextLayout(&wide, font, width, height)?;
            layout.SetFontSize(
                size,
                DWRITE_TEXT_RANGE {
                    startPosition: 0,
                    length: wide.len() as u32,
                },
            )?;
            Ok(layout)
        }
    }
}

/// Which label a cached layout belongs to; the index tells sectors with equal text apart.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Kind {
    Inner(u8),
    Outer(u8),
    /// Tile of the submenu arc: parent sector and index in its submenu.
    Sub(u8, u8),
    Hub,
}

struct FittedLabel {
    layout: IDWriteTextLayout,
    /// Layout box size and the point it is drawn centred on.
    width: f32,
    height: f32,
    at: (f32, f32),
}

/// Size relative to the mockup (100% scale at 96 DPI is 1.0).
fn scale_factor(layout: &Layout) -> f32 {
    layout.outer_radius() / MOCKUP_OUTER_RADIUS
}

/// Point at distance `r` and angle `deg` (0 = up, clockwise) from (`c`, `c`), in screen
/// coordinates (y down).
fn polar(c: f32, r: f32, deg: f32) -> (f32, f32) {
    let a = deg.to_radians();
    (c + r * a.sin(), c - r * a.cos())
}

fn text_format(
    factory: &IDWriteFactory,
    size: f32,
    weight: DWRITE_FONT_WEIGHT,
) -> Result<IDWriteTextFormat> {
    // SAFETY: plain COM calls with valid arguments.
    unsafe {
        let format = factory.CreateTextFormat(
            w!("Segoe UI"),
            None,
            weight,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            size,
            w!(""),
        )?;
        format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
        format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        let trimming = DWRITE_TRIMMING {
            granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
            delimiter: 0,
            delimiterCount: 0,
        };
        let sign = factory.CreateEllipsisTrimmingSign(&format)?;
        format.SetTrimming(&trimming, &sign)?;
        Ok(format)
    }
}

/// Hardware device, or the WARP software rasterizer when there is no usable GPU.
fn create_d3d_device() -> Result<ID3D11Device> {
    let mut last = Error::from(E_FAIL);
    for driver in [D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP] {
        let mut device = None;
        // SAFETY: `device` is a valid out pointer for the duration of the call.
        let result = unsafe {
            D3D11CreateDevice(
                None,
                driver,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                None,
            )
        };
        match (result, device) {
            (Ok(()), Some(device)) => return Ok(device),
            (Err(e), _) => last = e,
            (Ok(()), None) => {}
        }
    }
    Err(last)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use wheel_core::{Action, EditCommand, Item, SystemAction, ThemeChoice, arc_for_hit};

    use windows::Win32::Graphics::Direct2D::Common::D2D_SIZE_U;
    use windows::Win32::Graphics::Direct2D::{D2D1_BITMAP_OPTIONS_CPU_READ, D2D1_MAP_OPTIONS_READ};
    use windows::Win32::Graphics::Imaging::{
        CLSID_WICImagingFactory, GUID_ContainerFormatPng, GUID_WICPixelFormat24bppBGR,
        IWICImagingFactory, WICBitmapEncoderNoCache,
    };
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    };
    use windows::core::{GUID, PCWSTR};

    use super::*;
    use crate::theme;

    fn item(name: &str, action: Action) -> Item {
        Item {
            name: name.to_string(),
            action,
            icon: None,
            submenu: Vec::new(),
        }
    }

    /// Submenu of `count` tiles (at most 5) covering every kind of icon.
    fn submenu(count: usize) -> Vec<Item> {
        vec![
            item(
                "Notatnik",
                Action::Program {
                    path: "notepad.exe".to_string(),
                    args: String::new(),
                    working_dir: None,
                },
            ),
            item(
                "Windows",
                Action::Open {
                    path: r"C:\Windows".to_string(),
                },
            ),
            item(
                "Kopiuj",
                Action::Edit {
                    command: EditCommand::Copy,
                },
            ),
            item(
                "Zablokuj",
                Action::System {
                    action: SystemAction::Lock,
                },
            ),
            item("Źródło", Action::None),
        ]
        .into_iter()
        .take(count)
        .collect()
    }

    struct Case {
        name: &'static str,
        theme: ThemeChoice,
        view: ViewMode,
        sectors: u8,
        parent: u8,
        tiles: usize,
        hit: Hit,
    }

    /// Renders the case offscreen and returns the side and BGRA premultiplied pixels.
    fn render(case: &Case) -> (u32, Vec<u8>) {
        let mut config = Config::default();
        config.options.view = case.view;
        config.options.outer_sectors = case.sectors;
        config.outer[usize::from(case.parent)].submenu = submenu(case.tiles);
        let arc = arc_for_hit(case.hit, &config);
        let layout = Layout::new(96, 100, 12, case.sectors);
        let size = layout.size_px();
        let device = create_d3d_device().expect("D3D device");
        let dxgi: IDXGIDevice = device.cast().unwrap();
        let painter = Painter::new(&dxgi, layout, theme::palette(case.theme), Arc::new(config))
            .expect("painter");
        let props = |options| D2D1_BITMAP_PROPERTIES1 {
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 96.0,
            dpiY: 96.0,
            bitmapOptions: options,
            colorContext: ManuallyDrop::new(None),
        };
        let dims = D2D_SIZE_U {
            width: size,
            height: size,
        };
        // SAFETY: plain COM calls with valid arguments; the mapped memory is read before Unmap.
        unsafe {
            let target = painter
                .ctx
                .CreateBitmap(dims, None, 0, &props(D2D1_BITMAP_OPTIONS_TARGET))
                .unwrap();
            painter.paint(&target, case.hit, arc).expect("paint");
            let staging = painter
                .ctx
                .CreateBitmap(
                    dims,
                    None,
                    0,
                    &props(D2D1_BITMAP_OPTIONS_CPU_READ | D2D1_BITMAP_OPTIONS_CANNOT_DRAW),
                )
                .unwrap();
            staging.CopyFromBitmap(None, &target, None).unwrap();
            let mapped = staging.Map(D2D1_MAP_OPTIONS_READ).unwrap();
            let mut pixels = Vec::with_capacity((size * size * 4) as usize);
            for y in 0..size {
                let row = mapped.bits.add((y * mapped.pitch) as usize);
                pixels.extend_from_slice(std::slice::from_raw_parts(row, (size * 4) as usize));
            }
            staging.Unmap().unwrap();
            (size, pixels)
        }
    }

    /// Writes the pixels as a 24-bit PNG composited over a plain background.
    fn save_png(path: &Path, size: u32, bgra: &[u8], dark: bool) {
        let bg: [u32; 3] = if dark {
            [0x1c, 0x14, 0x10]
        } else {
            [0xf0, 0xe8, 0xe4]
        };
        let mut bgr = Vec::with_capacity((size * size * 3) as usize);
        for px in bgra.as_chunks::<4>().0 {
            let inv = 255 - u32::from(px[3]);
            for (ch, bg) in bg.iter().enumerate() {
                bgr.push((u32::from(px[ch]) + bg * inv / 255).min(255) as u8);
            }
        }
        let wide: Vec<u16> = path
            .to_str()
            .unwrap()
            .encode_utf16()
            .chain(Some(0))
            .collect();
        // SAFETY: plain COM calls with valid arguments; COM is initialised by the caller.
        unsafe {
            let wic: IWICImagingFactory =
                CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).unwrap();
            let stream = wic.CreateStream().unwrap();
            // GENERIC_WRITE
            stream
                .InitializeFromFilename(PCWSTR(wide.as_ptr()), 0x4000_0000)
                .unwrap();
            let encoder = wic
                .CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null())
                .unwrap();
            encoder
                .Initialize(&stream, WICBitmapEncoderNoCache)
                .unwrap();
            let mut frame = None;
            encoder
                .CreateNewFrame(&mut frame, std::ptr::null_mut())
                .unwrap();
            let frame = frame.unwrap();
            frame.Initialize(None).unwrap();
            frame.SetSize(size, size).unwrap();
            let mut format: GUID = GUID_WICPixelFormat24bppBGR;
            frame.SetPixelFormat(&mut format).unwrap();
            frame.WritePixels(size, size * 3, &bgr).unwrap();
            frame.Commit().unwrap();
            encoder.Commit().unwrap();
        }
    }

    /// Visual check without a window: `WHEEL_RENDER_OUT=<dir> cargo test render_cases -- --ignored`
    /// writes one PNG per case; without the variable the cases are only drawn.
    #[test]
    #[ignore = "needs a D3D device and system fonts; writes PNGs when WHEEL_RENDER_OUT is set"]
    fn render_cases() {
        // SAFETY: plain call; the result is ignored (the thread may already have a model).
        let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        let out = std::env::var_os("WHEEL_RENDER_OUT").map(std::path::PathBuf::from);
        let case = |name, theme, view, sectors, parent, tiles, hit| Case {
            name,
            theme,
            view,
            sectors,
            parent,
            tiles,
            hit,
        };
        use ThemeChoice::{Dark, Light};
        use ViewMode::{Icons as I, Names as N};
        let cases = [
            case("names-light-arc3", Light, N, 12, 2, 3, Hit::Sub(2, 1)),
            case("names-dark-parent0-arc3", Dark, N, 12, 0, 3, Hit::Sub(0, 0)),
            case(
                "names-light-4sectors-arc5",
                Light,
                N,
                4,
                3,
                5,
                Hit::Outer(3),
            ),
            case("icons-light-arc5", Light, I, 12, 2, 5, Hit::Sub(2, 4)),
            case("icons-dark-parent0-arc5", Dark, I, 12, 0, 5, Hit::Sub(0, 2)),
            case("icons-dark-4sectors-arc5", Dark, I, 4, 3, 5, Hit::Sub(3, 3)),
        ];
        for case in &cases {
            let (size, pixels) = render(case);
            assert!(pixels.iter().any(|&b| b != 0), "{} is empty", case.name);
            if let Some(dir) = &out {
                let path = dir.join(format!("{}.png", case.name));
                save_png(&path, size, &pixels, case.theme == Dark);
            }
        }
    }
}
