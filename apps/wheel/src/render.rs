//! Wheel rendering: D3D11 device -> DXGI composition swap chain -> Direct2D, shown through
//! DirectComposition on the window. Text goes through DirectWrite.

use std::cell::RefCell;
use std::collections::HashMap;
use std::mem::ManuallyDrop;
use std::sync::Arc;

use wheel_core::{Config, Hit, INNER_SECTORS, Layout, ViewMode};

use windows::Win32::Foundation::{E_FAIL, HMODULE, HWND};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_SIZE_F, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_FIGURE_BEGIN_FILLED,
    D2D1_FIGURE_END_CLOSED, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ARC_SEGMENT, D2D1_ARC_SIZE_LARGE, D2D1_ARC_SIZE_SMALL, D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
    D2D1_BITMAP_OPTIONS_TARGET, D2D1_BITMAP_PROPERTIES1, D2D1_DEVICE_CONTEXT_OPTIONS_NONE,
    D2D1_DRAW_TEXT_OPTIONS_CLIP, D2D1_ELLIPSE, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_SWEEP_DIRECTION_CLOCKWISE, D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE,
    D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE, D2D1CreateFactory, ID2D1Brush, ID2D1DeviceContext,
    ID2D1Factory1, ID2D1Geometry, ID2D1Image, ID2D1PathGeometry, ID2D1SolidColorBrush,
    ID2D1StrokeStyle,
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
    DWRITE_FONT_WEIGHT_MEDIUM, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER,
    DWRITE_TEXT_METRICS, DWRITE_TEXT_RANGE, DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER,
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
use crate::icons::{IconSizes, Icons};
use crate::theme::Palette;

/// Outer radius of the mockup at 100% and 96 DPI; the mockup's other sizes (fonts, gaps,
/// stroke) are expressed relative to it.
const MOCKUP_OUTER_RADIUS: f32 = 210.0;
/// Half of the angular gap between neighbouring sectors, in degrees (mockup values).
const OUTER_HALF_GAP_DEG: f32 = 0.8;
const INNER_HALF_GAP_DEG: f32 = 1.4;

pub struct Renderer {
    layout: Layout,
    palette: Palette,
    /// Source of the sector names.
    config: Arc<Config>,
    /// Window / swap chain side in pixels.
    size: u32,
    d2d_factory: ID2D1Factory1,
    ctx: ID2D1DeviceContext,
    brush: ID2D1SolidColorBrush,
    swap_chain: IDXGISwapChain1,
    dcomp: IDCompositionDevice,
    // Kept alive for as long as the window shows the wheel.
    _target: IDCompositionTarget,
    _visual: IDCompositionVisual,
    outer_font: IDWriteTextFormat,
    inner_font: IDWriteTextFormat,
    hub_font: IDWriteTextFormat,
    dwrite: IDWriteFactory,
    /// Tile icons for the icons view; `None` in the names view. Used in step 4.
    #[allow(dead_code)]
    icons: Option<Icons>,
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

            let d2d_factory: ID2D1Factory1 =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let d2d_device = d2d_factory.CreateDevice(&dxgi_device)?;
            let ctx = d2d_device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
            // The swap chain is in physical pixels, so one D2D unit is one pixel.
            ctx.SetDpi(96.0, 96.0);
            ctx.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            let brush = ctx.CreateSolidColorBrush(&palette.fg, None)?;

            let dcomp: IDCompositionDevice = DCompositionCreateDevice(&dxgi_device)?;
            let target = dcomp.CreateTargetForHwnd(hwnd, true)?;
            let visual = dcomp.CreateVisual()?;
            visual.SetContent(&swap_chain)?;
            target.SetRoot(&visual)?;
            dcomp.Commit()?;

            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let k = scale_factor(&layout);
            let icons = (config.options.view == ViewMode::Icons).then(|| {
                Icons::new(
                    &ctx,
                    &dwrite,
                    &config,
                    layout.outer_sectors(),
                    IconSizes::scaled(k),
                )
            });
            Ok(Renderer {
                layout,
                palette,
                config,
                size,
                d2d_factory,
                ctx,
                brush,
                swap_chain,
                dcomp,
                _target: target,
                _visual: visual,
                outer_font: text_format(&dwrite, 15.0 * k)?,
                inner_font: text_format(&dwrite, 11.5 * k)?,
                hub_font: text_format(&dwrite, 14.0 * k)?,
                dwrite,
                icons,
                layouts: RefCell::new(HashMap::new()),
            })
        }
    }

    /// Draws the whole wheel with `hit` highlighted and presents it.
    pub fn draw(&self, hit: Hit) -> Result<()> {
        // SAFETY: the swap chain, device context and brush are owned by `self` and valid.
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
                .ctx
                .CreateBitmapFromDxgiSurface(&surface, Some(&props))?;
            self.ctx.SetTarget(&bitmap);

            self.ctx.BeginDraw();
            let drawn = self.draw_wheel(hit);
            // EndDraw must be called even if drawing failed.
            let ended = self.ctx.EndDraw(None, None);
            self.ctx.SetTarget(None::<&ID2D1Image>);
            drawn?;
            ended?;

            self.swap_chain.Present(1, DXGI_PRESENT(0)).ok()?;
            self.dcomp.Commit()
        }
    }

    /// # Safety
    /// Must be called between `BeginDraw` and `EndDraw` of `self.ctx`.
    unsafe fn draw_wheel(&self, hit: Hit) -> Result<()> {
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
        let n = self.layout.outer_sectors();

        // SAFETY: guaranteed by the caller.
        unsafe {
            self.ctx.Clear(Some(&D2D1_COLOR_F {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            }));

            for i in 0..INNER_SECTORS {
                let (a1, a2) = self.layout.inner_span(i);
                let geometry = self.sector_path(
                    c,
                    inner_ring,
                    a1 + INNER_HALF_GAP_DEG,
                    a2 - INNER_HALF_GAP_DEG,
                )?;
                let on = hit == Hit::Inner(i);
                self.fill_tile(&geometry, on, stroke);
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
                self.fill_tile(&geometry, on, stroke);
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
    unsafe fn fill_tile(&self, geometry: &ID2D1Geometry, on: bool, stroke: f32) {
        let p = &self.palette;
        // SAFETY: guaranteed by the caller.
        unsafe {
            self.brush.SetColor(if on { &p.accent } else { &p.tile });
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

fn text_format(factory: &IDWriteFactory, size: f32) -> Result<IDWriteTextFormat> {
    // SAFETY: plain COM calls with valid arguments.
    unsafe {
        let format = factory.CreateTextFormat(
            w!("Segoe UI"),
            None,
            DWRITE_FONT_WEIGHT_MEDIUM,
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
