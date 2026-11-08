//! Wheel rendering: D3D11 device -> DXGI composition swap chain -> Direct2D, shown through
//! DirectComposition on the window. Text goes through DirectWrite.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::mem::ManuallyDrop;
use std::rc::Rc;
use std::sync::Arc;

use wheel_core::fit;
use wheel_core::icon::IconSizes;
use wheel_core::{Config, Hit, Layout, SubArc, ViewMode, arc_for_hit};

use std::time::{Duration, Instant};

use windows::Win32::Foundation::{E_FAIL, HMODULE, HWND, S_FALSE};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D_SIZE_F, D2D_SIZE_U, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_BORDER_MODE_HARD,
    D2D1_COLOR_F, D2D1_COMPOSITE_MODE_SOURCE_COPY, D2D1_COMPOSITE_MODE_SOURCE_OVER,
    D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_END_CLOSED, D2D1_GRADIENT_STOP, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    CLSID_D2D1ColorMatrix, CLSID_D2D1GaussianBlur, CLSID_D2D1Shadow,
    D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_ARC_SEGMENT, D2D1_ARC_SIZE_LARGE, D2D1_ARC_SIZE_SMALL,
    D2D1_BITMAP_OPTIONS, D2D1_BITMAP_OPTIONS_CANNOT_DRAW, D2D1_BITMAP_OPTIONS_NONE,
    D2D1_BITMAP_OPTIONS_TARGET, D2D1_BITMAP_PROPERTIES1, D2D1_BUFFER_PRECISION_8BPC_UNORM,
    D2D1_CAP_STYLE_FLAT, D2D1_COLOR_INTERPOLATION_MODE_PREMULTIPLIED, D2D1_COLOR_SPACE_SRGB,
    D2D1_COLORMATRIX_PROP_COLOR_MATRIX, D2D1_COMBINE_MODE_INTERSECT, D2D1_COMBINE_MODE_UNION,
    D2D1_DASH_STYLE_SOLID, D2D1_DEVICE_CONTEXT_OPTIONS_NONE, D2D1_DRAW_TEXT_OPTIONS_CLIP,
    D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ELLIPSE, D2D1_EXTEND_MODE_CLAMP,
    D2D1_FACTORY_TYPE_MULTI_THREADED, D2D1_GAUSSIANBLUR_PROP_BORDER_MODE,
    D2D1_GAUSSIANBLUR_PROP_STANDARD_DEVIATION, D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC,
    D2D1_INTERPOLATION_MODE_LINEAR, D2D1_LAYER_OPTIONS1_NONE, D2D1_LAYER_PARAMETERS1,
    D2D1_LINE_JOIN_ROUND, D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES, D2D1_PRIMITIVE_BLEND_COPY,
    D2D1_PRIMITIVE_BLEND_SOURCE_OVER, D2D1_PROPERTY_TYPE, D2D1_PROPERTY_TYPE_ENUM,
    D2D1_PROPERTY_TYPE_FLOAT, D2D1_PROPERTY_TYPE_MATRIX_5X4, D2D1_PROPERTY_TYPE_VECTOR4,
    D2D1_ROUNDED_RECT, D2D1_SHADOW_PROP_BLUR_STANDARD_DEVIATION, D2D1_SHADOW_PROP_COLOR,
    D2D1_STROKE_STYLE_PROPERTIES1, D2D1_STROKE_TRANSFORM_TYPE_NORMAL,
    D2D1_SWEEP_DIRECTION_CLOCKWISE, D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE,
    D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE, D2D1CreateFactory, ID2D1Bitmap1, ID2D1BitmapBrush1,
    ID2D1Brush, ID2D1Device, ID2D1DeviceContext, ID2D1Effect, ID2D1Factory1, ID2D1Geometry,
    ID2D1Image, ID2D1Layer, ID2D1LinearGradientBrush, ID2D1PathGeometry, ID2D1SolidColorBrush,
    ID2D1StrokeStyle,
};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_QUERY_DESC, D3D11_QUERY_EVENT, D3D11_SDK_VERSION,
    D3D11CreateDevice, ID3D11Device,
};
use windows::Win32::Graphics::DirectComposition::{
    DCompositionCreateDevice, IDCompositionAnimation, IDCompositionDevice,
    IDCompositionEffectGroup, IDCompositionScaleTransform, IDCompositionTarget,
    IDCompositionVisual,
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
    IDXGIDevice, IDXGIDevice3, IDXGIFactory2, IDXGISurface, IDXGISwapChain1,
};
use windows::core::{Error, Interface, Result, w};
use windows_numerics::{Matrix3x2, Vector2};

use crate::anim::{self, Anim};
use crate::backdrop;
use crate::icons::{Icons, TileIcon};
use crate::theme::{self, GlassStyle, Palette};

/// Outer radius of the mockup at 100% and 96 DPI; the mockup's other sizes (fonts, gaps,
/// stroke) are expressed relative to it.
const MOCKUP_OUTER_RADIUS: f32 = 210.0;
/// Half of the angular gap between neighbouring sectors, in degrees (mockup values).
const OUTER_HALF_GAP_DEG: f32 = 0.8;
const INNER_HALF_GAP_DEG: f32 = 1.4;
/// Half of the angular width of the submenu notch, in degrees (mockup value).
const NOTCH_HALF_DEG: f32 = 5.0;
/// Radial extent of the submenu notch below the outer edge, and the free space kept between it
/// and the tile content, in mockup px (the notch spans 5..9 px from the edge).
const NOTCH_FAR: f32 = 9.0;
const NOTCH_NEAR: f32 = 5.0;
const NOTCH_CLEARANCE: f32 = 2.0;
/// Absolute minimum label font size at 96 DPI, in px.
const MIN_FONT_PX: f32 = 8.0;
/// How far the soft shadow reaches beyond a plate (mockup px; the window leaves room for it)
/// and into how many translucent strokes it is drawn.
const SHADOW_REACH: f32 = 10.0;
const SHADOW_PASSES: u32 = 8;
/// Corner radius of a tile at 100% scale and 96 DPI, in px (the editor preview uses the same).
const CORNER_PX: f32 = 6.0;
/// A corner never takes more than this share of the ring thickness or of the tile width.
const CORNER_MAX_SHARE: f32 = 0.3;
/// Share of the accent mixed into the tile that opened the submenu arc.
const PARENT_TINT: f32 = 0.22;
/// Opacity of the accent on that tile in the glass look.
const PARENT_GLASS_ALPHA: f32 = 0.3;
/// Standard deviation of the blur of the desktop snapshot at 100% scale and 96 DPI, in px.
const BLUR_SIGMA: f32 = 9.0;
/// Longest wait for the GPU between the steps of `Painter::warm`, in milliseconds.
const GPU_WAIT_MS: u64 = 300;
/// Standard deviation of the halo behind label text, in px at the same scale.
const HALO_SIGMA: f32 = 1.8;
/// The sheen of the disc is its part inside an ellipse of this height (share of the radius) whose
/// centre lies this much above the disc centre.
const SHEEN_HEIGHT: f32 = 0.85;
const SHEEN_LIFT: f32 = 0.55;
/// Opacity of the sheen on a highlighted tile.
const TILE_SHEEN: f32 = 0.3;

pub struct Renderer {
    painter: Painter,
    swap_chain: IDXGISwapChain1,
    dcomp: IDCompositionDevice,
    // Kept alive for as long as the window shows the wheel.
    _target: IDCompositionTarget,
    _visual: IDCompositionVisual,
    /// Opacity and scale of the visual; the fades and the scale-up are animations of these.
    effect: IDCompositionEffectGroup,
    scale: IDCompositionScaleTransform,
    /// Created once, reset for every fade.
    fade: IDCompositionAnimation,
    grow: IDCompositionAnimation,
    dxgi_device: IDXGIDevice3,
}

/// Everything needed to paint the wheel on a Direct2D device context. The window-bound part
/// (swap chain, DirectComposition) lives in `Renderer`, which keeps painting testable offscreen.
struct Painter {
    layout: Layout,
    /// Opaque palette, used when there is no snapshot of the desktop to show through.
    palette: Palette,
    /// Translucent palette over the blurred snapshot.
    glass_palette: Palette,
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
    hint_font: IDWriteTextFormat,
    /// Disc under the rings (centre to the outer edge of the outer ring); built once.
    wheel_plate: ID2D1Geometry,
    /// Tile shapes of the inner and the outer ring; built once.
    inner_tiles: Vec<ID2D1Geometry>,
    outer_tiles: Vec<ID2D1Geometry>,
    /// Notch at the outer edge of each outer tile that has a submenu; built once.
    notches: Vec<Option<ID2D1Geometry>>,
    /// Shapes of the submenu arcs drawn so far, by (parent, tile count).
    arcs: RefCell<HashMap<(u8, u8), Rc<ArcShapes>>>,
    /// GPU resources of the glass look; `None` when it is off or could not be set up.
    frost: Option<Frost>,
    /// The glass look is shown: `frost` holds the blurred snapshot of the current open.
    glass: Cell<bool>,
    /// Round-joined stroke style of the shadow passes.
    round_join: ID2D1StrokeStyle,
    /// Bold format of the letter shown instead of an icon.
    letter_font: IDWriteTextFormat,
    dwrite: IDWriteFactory,
    /// Tile icons for the icons view; `None` in the names view.
    icons: Option<Icons>,
    icon_sizes: IconSizes,
    /// Fitted text layouts per label (kind, text), built on first use: the layout never changes
    /// for the lifetime of the renderer.
    layouts: RefCell<HashMap<(Kind, String), FittedLabel>>,
    /// Layouts of the letters and glyphs shown as icons, by (glyph font, character, box side).
    icon_layouts: RefCell<HashMap<(bool, char, u32), IDWriteTextLayout>>,
}

/// Direct2D objects of the glass look. The snapshot of the desktop is blurred once per open into
/// `result`; every frame then only paints that bitmap.
struct Frost {
    style: GlassStyle,
    d2d_device: ID2D1Device,
    /// Standard deviation of the snapshot blur, in px of the shrunk snapshot.
    sigma: f32,
    /// The blur pipeline of the snapshot; it exists from the snapshot to its release, because
    /// the effects keep a lot of GPU memory.
    blur: RefCell<Option<Blur>>,
    /// What bakes label halos; it exists only while halos are being baked (`Painter::warm`).
    halo_tools: RefCell<Option<HaloTools>>,
    /// The blurred snapshot of the current open (or press); it exists only while the wheel is
    /// shown or about to be.
    snap: RefCell<Option<Snap>>,
    /// White to transparent, top to bottom; the points are set per use.
    sheen: ID2D1LinearGradientBrush,
    /// Where the sheen lies on the disc.
    sheen_shape: ID2D1Geometry,
    /// Extra snapshot pixels on every side, so that the blur has real pixels to draw from.
    pad: u32,
    /// Extra pixels around the window that a press-time snapshot covers, for the distance the
    /// pointer can travel before the wheel opens.
    margin: i32,
    /// Standard deviation of the label halo, in px.
    halo_sigma: f32,
}

/// Effects that turn a snapshot into the blurred backdrop.
struct Blur {
    /// Gaussian blur of the snapshot (input 0, set per snapshot).
    blur: ID2D1Effect,
    /// Saturation and brightness of the blur.
    tone: ID2D1Effect,
}

/// Direct2D objects that render label halos.
struct HaloTools {
    /// Second context: bakes label halos while `Painter::ctx` is in the middle of a frame.
    aux: ID2D1DeviceContext,
    aux_brush: ID2D1SolidColorBrush,
    /// Soft shadow of a label, for its halo.
    shadow: ID2D1Effect,
}

/// The blurred desktop for one open, `SCALE` times smaller than the screen.
struct Snap {
    /// Paints the blurred snapshot of the window area plus `margin` px on every side at full
    /// size, positioned for the window; see `Painter::place_snapshot`.
    brush: ID2D1BitmapBrush1,
    /// Top-left corner of the window the snapshot was taken for.
    taken: (i32, i32),
    margin: i32,
}

impl Renderer {
    /// `glass` (0..=1) asks for the translucent look, 0 for the opaque one; it is given up (with a line in the log) when
    /// Windows transparency is off or the effects cannot be set up.
    pub fn new(
        hwnd: HWND,
        layout: Layout,
        palette: Palette,
        config: Arc<Config>,
        glass: f32,
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
            let glass = if theme::transparency_enabled() {
                glass
            } else {
                0.0
            };
            let painter = Painter::new(&dxgi_device, layout, palette, config, glass)?;

            let dcomp: IDCompositionDevice = DCompositionCreateDevice(&dxgi_device)?;
            let target = dcomp.CreateTargetForHwnd(hwnd, true)?;
            let visual = dcomp.CreateVisual()?;
            visual.SetContent(&swap_chain)?;
            let effect = dcomp.CreateEffectGroup()?;
            let scale = dcomp.CreateScaleTransform()?;
            scale.SetCenterX2(size as f32 / 2.0)?;
            scale.SetCenterY2(size as f32 / 2.0)?;
            visual.SetEffect(&effect)?;
            visual.SetTransform(&scale)?;
            let fade = dcomp.CreateAnimation()?;
            let grow = dcomp.CreateAnimation()?;
            target.SetRoot(&visual)?;
            dcomp.Commit()?;

            Ok(Renderer {
                painter,
                swap_chain,
                dcomp,
                _target: target,
                _visual: visual,
                effect,
                scale,
                fade,
                grow,
                dxgi_device: dxgi_device.cast()?,
            })
        }
    }

    /// Makes the blurred desktop for the glass look ready for `draw`: call it with the window
    /// still hidden, giving the window's top-left corner in screen pixels. It uses the snapshot
    /// taken by `snapshot` when that covers the window, otherwise takes one now. Without it, or
    /// when it fails (logged), `draw` paints the opaque look. Returns the milliseconds the
    /// capture and the upload plus blur took (0 for a snapshot taken earlier), `None` when there
    /// is no glass look.
    pub fn prepare(&self, left: i32, top: i32) -> Option<(f64, f64)> {
        let frost = self.painter.frost.as_ref()?;
        self.painter.glass.set(false);
        let mut times = (0.0, 0.0);
        if !self.painter.place_snapshot(left, top) {
            times = self.take_snapshot(left, top, 0)?;
            self.painter.place_snapshot(left, top);
        }
        self.painter.glass.set(frost.snap.borrow().is_some());
        Some(times)
    }

    /// Takes the snapshot early, when the button goes down and the window (which opens with its
    /// top-left corner at (`left`, `top`) unless the pointer moves) is still hidden, so that
    /// `prepare` finds it ready. Returns the milliseconds as `prepare` does.
    pub fn snapshot(&self, left: i32, top: i32) -> Option<(f64, f64)> {
        let margin = self.painter.frost.as_ref()?.margin;
        self.take_snapshot(left, top, margin)
    }

    /// Hands memory that Direct2D and the graphics driver keep for reuse back to the system; call
    /// it when the wheel is hidden. The next draw rebuilds what it needs, a little slower.
    pub fn trim(&self) {
        // SAFETY: plain COM calls on objects owned by `self`.
        unsafe {
            if let Ok(device) = self.painter.ctx.GetDevice() {
                device.ClearResources(0);
            }
            self.dxgi_device.Trim();
        }
    }

    /// Frees the snapshot (the wheel closed, or the press did not open it).
    pub fn release_snapshot(&self) {
        self.painter.release_snapshot();
    }

    fn take_snapshot(&self, left: i32, top: i32, margin: i32) -> Option<(f64, f64)> {
        self.painter
            .take_snapshot(left, top, margin)
            .map_err(|e| wheel_core::log_line!("glass: cannot take the snapshot: {e}; opaque look"))
            .ok()
    }

    /// Builds everything that would otherwise be made on first use (label layouts and halos,
    /// submenu shapes, effect pipelines), so that the first open is as fast as the later ones.
    /// Failures are logged; the wheel still draws, only slower the first time. With `wait_gpu`
    /// the GPU is let to catch up between the drawings (see `Painter::warm`); the open path
    /// passes false, as it must not be delayed.
    pub fn warm(&self, wait_gpu: bool) {
        if let Err(e) = self.painter.warm(wait_gpu) {
            wheel_core::log_line!("cannot warm the renderer up: {e}");
        }
    }

    /// Sets what the compositor shows of the window before the next `draw`: invisible when the
    /// open will fade in (`animate`), plain otherwise. Call it while the window is hidden.
    pub fn prepare_open(&self, animate: bool) -> Result<()> {
        let (opacity, scale) = if animate {
            (0.0, anim::OPEN_SCALE_FROM)
        } else {
            (1.0, 1.0)
        };
        // SAFETY: plain COM calls on objects owned by `self`; the next Commit applies them.
        unsafe {
            self.effect.SetOpacity2(opacity)?;
            self.scale.SetScaleX2(scale)?;
            self.scale.SetScaleY2(scale)
        }
    }

    /// Starts the opening animation (fade in, scale up around the centre) in the compositor; call
    /// it right after the window is shown. Without `animate` the wheel just stays as drawn.
    pub fn start_open(&self, animate: bool) -> Result<()> {
        if animate {
            // SAFETY: plain COM calls on objects owned by `self`.
            unsafe {
                self.run(&self.fade, 0.0, 1.0, anim::OPEN_MS)?;
                self.run(&self.grow, anim::OPEN_SCALE_FROM, 1.0, anim::OPEN_MS)?;
                self.effect.SetOpacity(&self.fade)?;
                self.scale.SetScaleX(&self.grow)?;
                self.scale.SetScaleY(&self.grow)?;
            }
        }
        // SAFETY: plain COM call.
        unsafe { self.dcomp.Commit() }
    }

    /// Starts the closing fade in the compositor; the caller hides the window after
    /// `anim::CLOSE_MS`.
    pub fn start_close(&self) -> Result<()> {
        // SAFETY: plain COM calls on objects owned by `self`.
        unsafe {
            self.run(&self.fade, 1.0, 0.0, anim::CLOSE_MS)?;
            self.effect.SetOpacity(&self.fade)?;
            self.scale.SetScaleX2(1.0)?;
            self.scale.SetScaleY2(1.0)?;
            self.dcomp.Commit()
        }
    }

    /// Makes `animation` go from `from` to `to` in `ms` along an ease-out curve, starting at the
    /// next Commit.
    ///
    /// # Safety
    /// Plain COM calls; `animation` must come from `self.dcomp`.
    unsafe fn run(
        &self,
        animation: &IDCompositionAnimation,
        from: f32,
        to: f32,
        ms: u32,
    ) -> Result<()> {
        let seconds = ms as f32 / 1000.0;
        let [c0, c1, c2, c3] = anim::ease_out_cubic(from, to, seconds);
        // SAFETY: guaranteed by the caller.
        unsafe {
            animation.Reset()?;
            animation.AddCubic(0.0, c0, c1, c2, c3)?;
            animation.End(f64::from(seconds), to)
        }
    }

    /// Draws the whole wheel with `hit` highlighted and the submenu `arc` open (if any), and
    /// presents it. `anim` says how far the highlight and the arc have got.
    pub fn draw(&self, hit: Hit, arc: Option<SubArc>, anim: &Anim) -> Result<()> {
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
            self.painter.paint(&bitmap, hit, arc, anim)?;
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
        glass: f32,
    ) -> Result<Painter> {
        let size = layout.size_px();
        // SAFETY: plain COM calls with valid arguments; every result is checked via `?`.
        unsafe {
            let (d2d_factory, d2d_device) = d2d_for(dxgi_device)?;
            let ctx = d2d_device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
            // The target is in physical pixels, so one D2D unit is one pixel.
            ctx.SetDpi(96.0, 96.0);
            ctx.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            let brush = ctx.CreateSolidColorBrush(&palette.fg, None)?;
            let plate_radius = layout.outer_radius();
            let wheel_plate: ID2D1Geometry = d2d_factory
                .CreateEllipseGeometry(&D2D1_ELLIPSE {
                    point: Vector2 {
                        X: size as f32 / 2.0,
                        Y: size as f32 / 2.0,
                    },
                    radiusX: plate_radius,
                    radiusY: plate_radius,
                })?
                .cast()?;
            let round_join = d2d_factory.CreateStrokeStyle(
                &D2D1_STROKE_STYLE_PROPERTIES1 {
                    startCap: D2D1_CAP_STYLE_FLAT,
                    endCap: D2D1_CAP_STYLE_FLAT,
                    dashCap: D2D1_CAP_STYLE_FLAT,
                    lineJoin: D2D1_LINE_JOIN_ROUND,
                    miterLimit: 1.0,
                    dashStyle: D2D1_DASH_STYLE_SOLID,
                    dashOffset: 0.0,
                    transformType: D2D1_STROKE_TRANSFORM_TYPE_NORMAL,
                },
                None,
            )?;

            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let k = scale_factor(&layout);
            let rings = rings(&layout);
            let round = CORNER_PX * k;
            let c = size as f32 / 2.0;
            let inner_tiles = (0..layout.inner_sectors())
                .map(|i| {
                    let (a1, a2) = layout.inner_span(i);
                    let span = (a1 + INNER_HALF_GAP_DEG, a2 - INNER_HALF_GAP_DEG);
                    sector_path(&d2d_factory, c, rings.inner, span, round)
                })
                .collect::<Result<Vec<_>>>()?;
            let outer_tiles = (0..layout.outer_sectors())
                .map(|i| {
                    let (a1, a2) = layout.outer_span(i);
                    let span = (a1 + OUTER_HALF_GAP_DEG, a2 - OUTER_HALF_GAP_DEG);
                    sector_path(&d2d_factory, c, rings.outer, span, round)
                })
                .collect::<Result<Vec<_>>>()?;
            let notch_ring = (
                layout.outer_radius() - NOTCH_FAR * k,
                layout.outer_radius() - NOTCH_NEAR * k,
            );
            let notches = (0..layout.outer_sectors())
                .map(|i| {
                    let has_submenu = config
                        .outer_item(usize::from(i))
                        .is_some_and(|item| !item.submenu.is_empty());
                    if !has_submenu {
                        return Ok(None);
                    }
                    let (a1, a2) = layout.outer_span(i);
                    let mid = (a1 + a2) / 2.0;
                    sector_path(
                        &d2d_factory,
                        c,
                        notch_ring,
                        (mid - NOTCH_HALF_DEG, mid + NOTCH_HALF_DEG),
                        0.0,
                    )
                    .map(Some)
                })
                .collect::<Result<Vec<_>>>()?;
            let frost = if glass > 0.0 {
                Frost::new(
                    &d2d_device,
                    &ctx,
                    &d2d_factory,
                    size,
                    k,
                    &palette,
                    plate_radius,
                    &wheel_plate,
                    glass,
                    (config.options.move_px as f32 * layout.dpi_factor()).ceil() as i32,
                )
                .map_err(|e| wheel_core::log_line!("glass: {e}; using the opaque look"))
                .ok()
            } else {
                None
            };
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
                glass_palette: palette.glass(glass),
                palette,
                config,
                size,
                d2d_factory,
                ctx,
                brush,
                wheel_plate,
                inner_tiles,
                outer_tiles,
                notches,
                arcs: RefCell::new(HashMap::new()),
                frost,
                glass: Cell::new(false),
                round_join: round_join.into(),
                outer_font: text_format(&dwrite, 15.0 * k, DWRITE_FONT_WEIGHT_MEDIUM)?,
                inner_font: text_format(&dwrite, 11.5 * k, DWRITE_FONT_WEIGHT_MEDIUM)?,
                sub_font: text_format(&dwrite, 13.5 * k, DWRITE_FONT_WEIGHT_MEDIUM)?,
                hub_font: text_format(&dwrite, 14.0 * k, DWRITE_FONT_WEIGHT_MEDIUM)?,
                hint_font: text_format(
                    &dwrite,
                    wheel_core::look::OPTIONS_HINT_PX * k,
                    DWRITE_FONT_WEIGHT_MEDIUM,
                )?,
                letter_font: text_format(&dwrite, 14.0 * k, DWRITE_FONT_WEIGHT_BOLD)?,
                dwrite,
                icons,
                icon_sizes,
                layouts: RefCell::new(HashMap::new()),
                icon_layouts: RefCell::new(HashMap::new()),
            })
        }
    }

    /// The halo of a label for the glass look; `None` without it.
    fn bake_halo(&self, layout: &IDWriteTextLayout, width: f32, height: f32) -> Option<Halo> {
        let frost = self.frost.as_ref()?;
        frost
            .halo(layout, width, height)
            .map_err(|e| wheel_core::log_line!("glass: cannot render a label halo: {e}"))
            .ok()
    }

    /// Palette of the look in use.
    fn pal(&self) -> &Palette {
        if self.glass.get() {
            &self.glass_palette
        } else {
            &self.palette
        }
    }

    /// Sides, in shrunk pixels, of the blurred result for a window plus `margin` px on every
    /// side, of the blur input, and the padding of the input.
    fn snapshot_sides(&self, margin: i32) -> (i32, i32, i32) {
        let frost = self.frost.as_ref().expect("glass look");
        let q = backdrop::SCALE as i32;
        let result = (self.size as i32 + 2 * margin + q - 1) / q;
        let pad = (frost.pad as i32 + q - 1) / q;
        (result, result + 2 * pad, pad)
    }

    /// Captures the desktop around the window whose top-left corner will be at (`left`, `top`),
    /// `margin` px more on every side, shrinks and blurs it into `frost.snap`. Returns the
    /// milliseconds of the capture (with the shrinking) and of the upload plus blur.
    fn take_snapshot(&self, left: i32, top: i32, margin: i32) -> Result<(f64, f64)> {
        if self.frost.is_none() {
            return Err(Error::from(E_FAIL));
        }
        let q = backdrop::SCALE as i32;
        let (_, source_side, pad) = self.snapshot_sides(margin);
        let started = Instant::now();
        // The result starts exactly at the window's top-left corner minus `margin`; the blur
        // input extends it by the padding.
        let origin = (left - margin - pad * q, top - margin - pad * q);
        let shot = backdrop::capture(origin.0, origin.1, (source_side * q) as u32)?;
        let small = backdrop::downscale(shot.pixels(), (source_side * q) as usize);
        drop(shot);
        let captured = Instant::now();
        self.blur_snapshot(&small, (left, top), margin)?;
        Ok((
            (captured - started).as_secs_f64() * 1e3,
            captured.elapsed().as_secs_f64() * 1e3,
        ))
    }

    /// Uploads the shrunk snapshot `small` (BGRA, `snapshot_sides(margin).1` px square) taken for
    /// the window at `taken` and blurs it into `frost.snap`.
    fn blur_snapshot(&self, small: &[u8], taken: (i32, i32), margin: i32) -> Result<()> {
        let frost = self.frost.as_ref().ok_or_else(|| Error::from(E_FAIL))?;
        let (result_side, source_side, pad) = self.snapshot_sides(margin);
        let (source_side, result_side) = (source_side as u32, result_side as u32);
        let source = new_bitmap(
            &self.ctx,
            source_side,
            source_side,
            D2D1_BITMAP_OPTIONS_NONE,
        )?;
        let result = new_bitmap(
            &self.ctx,
            result_side,
            result_side,
            D2D1_BITMAP_OPTIONS_TARGET,
        )?;
        let mut effects = frost.blur.borrow_mut();
        if effects.is_none() {
            *effects = Some(frost.new_blur(&self.ctx)?);
        }
        let Blur { blur, tone } = effects.as_ref().expect("just made");
        // SAFETY: `small` holds the whole bitmap at `source_side` * 4 bytes per row.
        unsafe { source.CopyFromMemory(None, small.as_ptr().cast(), source_side * 4)? };
        // SAFETY: the device context and the effect chain are owned by `self` and valid; EndDraw
        // closes the BeginDraw.
        unsafe {
            blur.SetInput(0, &source, true);
            self.ctx.SetTarget(&result);
            self.ctx.BeginDraw();
            let offset = Vector2 {
                X: -(pad as f32),
                Y: -(pad as f32),
            };
            let drawn = tone.GetOutput().map(|output| {
                self.ctx.DrawImage(
                    &output,
                    Some(&offset),
                    None,
                    D2D1_INTERPOLATION_MODE_LINEAR,
                    D2D1_COMPOSITE_MODE_SOURCE_COPY,
                );
            });
            let ended = self.ctx.EndDraw(None, None);
            self.ctx.SetTarget(None::<&ID2D1Image>);
            // Lets go of the input, so that the effect does not keep the snapshot alive.
            blur.SetInput(0, None::<&ID2D1Image>, true);
            drawn?;
            ended?;
        }
        // SAFETY: plain COM call.
        let brush = unsafe { self.ctx.CreateBitmapBrush(&result, None, None)? };
        frost.snap.replace(Some(Snap {
            brush,
            taken,
            margin,
        }));
        Ok(())
    }

    /// Positions the snapshot for the window at (`left`, `top`) and returns true, or returns
    /// false when there is none or it does not cover that window.
    fn place_snapshot(&self, left: i32, top: i32) -> bool {
        let Some(frost) = &self.frost else {
            return false;
        };
        let snap = frost.snap.borrow();
        let Some(snap) = snap.as_ref() else {
            return false;
        };
        let Some(d) = backdrop::shift(snap.taken, (left, top), snap.margin) else {
            return false;
        };
        // The brush maps a shrunk pixel to `SCALE` window pixels; the result starts `margin`
        // pixels (and the shift of the window) before the window's corner.
        let q = backdrop::SCALE as f32;
        let matrix = Matrix3x2 {
            M11: q,
            M12: 0.0,
            M21: 0.0,
            M22: q,
            M31: -((d.0 + snap.margin) as f32),
            M32: -((d.1 + snap.margin) as f32),
        };
        // SAFETY: plain COM call.
        unsafe { snap.brush.SetTransform(&matrix) };
        true
    }

    fn release_snapshot(&self) {
        self.glass.set(false);
        if let Some(frost) = &self.frost {
            frost.snap.replace(None);
            frost.blur.replace(None);
        }
    }

    /// Draws every label and submenu once into a scratch bitmap, and runs the glass pipeline
    /// once, so that their caches and GPU pipelines exist before the first open. With `wait_gpu`
    /// the GPU is let to catch up after every drawing: with work queued up the driver keeps
    /// allocating new buffers, and the memory it grew to stays with the process after the trim.
    /// All the waits together take at most `GPU_WAIT_MS`; the rest of the drawings run unwaited.
    fn warm(&self, wait_gpu: bool) -> Result<()> {
        let deadline = wait_gpu.then(|| Instant::now() + Duration::from_millis(GPU_WAIT_MS));
        let settle = || {
            if let Some(deadline) = deadline {
                wait_for_gpu(deadline);
            }
        };
        let target = new_bitmap(&self.ctx, self.size, self.size, D2D1_BITMAP_OPTIONS_TARGET)?;
        let n = self.layout.outer_sectors();
        let hits = std::iter::once(Hit::Center)
            .chain((0..self.layout.inner_sectors()).map(Hit::Inner))
            .chain((0..n).map(Hit::Outer));
        for hit in hits {
            self.paint(&target, hit, None, &Anim::settled(hit))?;
            settle();
        }
        let mut first_arc = None;
        for parent in 0..n {
            let Some(arc) = self.valid_arc(arc_for_hit(Hit::Outer(parent), &self.config)) else {
                continue;
            };
            first_arc.get_or_insert(arc);
            // The hub shows the name of the highlighted submenu tile.
            for j in 0..arc.count {
                let hit = Hit::Sub(parent, j);
                self.paint(&target, hit, Some(arc), &Anim::settled(hit))?;
                settle();
            }
        }
        if let Some(frost) = &self.frost {
            // The effects run and the glass fills are drawn once, over a corner of the desktop.
            self.take_snapshot(0, 0, 0)?;
            if self.place_snapshot(0, 0) {
                self.glass.set(true);
                // The arc is drawn as when it starts to slide out, which also warms the layers.
                let sliding = Anim::new(true, Hit::Center, Some(0.0), Instant::now());
                let drawn = self.paint(&target, Hit::Center, first_arc, &sliding);
                self.release_snapshot();
                drawn?;
            }
            frost.halo_tools.replace(None);
        }
        Ok(())
    }

    /// The arc as it is drawn: its parent is valid and it has tiles.
    fn valid_arc(&self, arc: Option<SubArc>) -> Option<SubArc> {
        arc.filter(|a| a.parent < self.layout.outer_sectors() && a.count > 0)
    }

    /// Shapes of the open `arc` (valid, see `valid_arc`); built on first use.
    fn arc_shapes(&self, arc: SubArc) -> Result<Rc<ArcShapes>> {
        let key = (arc.parent, arc.count);
        if let Some(shapes) = self.arcs.borrow().get(&key) {
            return Ok(shapes.clone());
        }
        let k = scale_factor(&self.layout);
        let c = self.size as f32 / 2.0;
        let round = CORNER_PX * k;
        let first = self.layout.sub_span(arc, 0).0;
        let last = self.layout.sub_span(arc, arc.count - 1).1;
        // Starts inside the wheel disc so the two overlap without a seam.
        let ring = (
            self.layout.outer_radius() - 5.0 * k,
            self.layout.sub_radii().1,
        );
        let plate = sector_path(&self.d2d_factory, c, ring, (first, last), round)?;
        let sub_ring = rings(&self.layout).sub;
        let tiles = (0..arc.count)
            .map(|j| {
                let (a1, a2) = self.layout.sub_span(arc, j);
                let span = (a1 + OUTER_HALF_GAP_DEG, a2 - OUTER_HALF_GAP_DEG);
                sector_path(&self.d2d_factory, c, sub_ring, span, round)
            })
            .collect::<Result<Vec<_>>>()?;
        // The outline of the wheel disc joined with the band.
        // SAFETY: plain COM calls; the sink is closed before the geometry is used.
        let outline: ID2D1Geometry = unsafe {
            let path: ID2D1PathGeometry = self.d2d_factory.CreatePathGeometry()?.into();
            let sink = path.Open()?;
            self.wheel_plate.CombineWithGeometry(
                &plate,
                D2D1_COMBINE_MODE_UNION,
                None,
                0.1,
                &sink,
            )?;
            sink.Close()?;
            path.cast()?
        };
        let shapes = Rc::new(ArcShapes {
            plate,
            tiles,
            outline,
        });
        self.arcs.borrow_mut().insert(key, shapes.clone());
        Ok(shapes)
    }

    /// Draws the wheel into `target`, a Direct2D target bitmap of `size` x `size` pixels.
    fn paint(
        &self,
        target: &ID2D1Bitmap1,
        hit: Hit,
        arc: Option<SubArc>,
        anim: &Anim,
    ) -> Result<()> {
        // SAFETY: the device context is owned by `self`; EndDraw closes the BeginDraw below.
        unsafe {
            self.ctx.SetTarget(target);
            self.ctx.BeginDraw();
            let drawn = self.draw_wheel(hit, arc, anim);
            // EndDraw must be called even if drawing failed.
            let ended = self.ctx.EndDraw(None, None);
            self.ctx.SetTarget(None::<&ID2D1Image>);
            drawn?;
            ended
        }
    }

    /// # Safety
    /// Must be called between `BeginDraw` and `EndDraw` of `self.ctx`.
    unsafe fn draw_wheel(&self, hit: Hit, arc: Option<SubArc>, anim: &Anim) -> Result<()> {
        let p = self.pal();
        let glass = self.glass.get();
        let k = scale_factor(&self.layout);
        let c = self.size as f32 / 2.0;
        let radii = self.layout.radii();
        let stroke = 1.2 * k;
        // Keeps label text off the sector edges.
        let pad = k;
        let Rings {
            inner: inner_ring,
            outer: outer_ring,
            sub: sub_ring,
        } = rings(&self.layout);
        let n = self.layout.outer_sectors();
        let arc = self.valid_arc(arc);
        let arc_shapes = arc.map(|a| self.arc_shapes(a)).transpose()?;
        // Where tile content must end in a sector with a submenu, so it stays clear of the notch.
        let notch_ring = (
            self.layout.outer_radius() - NOTCH_FAR * k,
            self.layout.outer_radius() - NOTCH_NEAR * k,
        );
        let notch_clear = notch_ring.0 - NOTCH_CLEARANCE * k;

        // SAFETY: guaranteed by the caller.
        unsafe {
            self.ctx.Clear(Some(&D2D1_COLOR_F {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            }));

            // Plates under the tiles: the wheel disc and, while an arc is open, a band joined to
            // it; the gaps between tiles show them as grooves. Shadows first, so neither plate is
            // darkened by the other's shadow.
            let arc_plate = arc_shapes.as_ref().map(|shapes| &shapes.plate);
            // While the arc slides out, its band and tiles come from the wheel and fade in.
            let arc_t = if arc.is_some() { anim.arc() } else { 1.0 };
            let plate_shift = arc.map_or((0.0, 0.0), |a| {
                let mid =
                    (self.layout.sub_span(a, 0).0 + self.layout.sub_span(a, a.count - 1).1) / 2.0;
                arc_shift(mid, k, arc_t)
            });
            self.shadow(&self.wheel_plate, k);
            if let Some(plate) = arc_plate {
                self.faded(arc_t, plate_shift, || self.shadow(plate, k));
            }
            let plates = [&self.wheel_plate]
                .into_iter()
                .chain(arc_plate)
                .collect::<Vec<_>>();
            // Runs `paint` (given the shift applied) on the wheel disc and on the arc band.
            let each_plate = |paint: &dyn Fn(&ID2D1Geometry, (f32, f32))| {
                paint(&self.wheel_plate, (0.0, 0.0));
                if let Some(plate) = arc_plate {
                    self.faded(arc_t, plate_shift, || paint(plate, plate_shift));
                }
            };
            let frost = self.frost.as_ref().filter(|_| glass);
            let backdrop = frost.and_then(|f| f.snap.borrow().as_ref().map(|s| s.brush.clone()));
            if let (Some(frost), Some(backdrop)) = (frost, &backdrop) {
                let outline = arc_shapes
                    .as_ref()
                    .map_or(&self.wheel_plate, |shapes| &shapes.outline);
                // Strokes the edge of the disc and band together; while the arc slides out, the
                // disc's own edge fades into that.
                let stroke_outline = |color: &D2D1_COLOR_F, width: f32| {
                    self.brush.SetColor(color);
                    if arc_t < 1.0 && arc_shapes.is_some() {
                        self.faded(1.0 - arc_t, (0.0, 0.0), || {
                            self.ctx.DrawGeometry(
                                &self.wheel_plate,
                                &self.brush,
                                width,
                                None::<&ID2D1StrokeStyle>,
                            )
                        });
                    }
                    self.faded(arc_t, (0.0, 0.0), || {
                        self.ctx.DrawGeometry(
                            outline,
                            &self.brush,
                            width,
                            None::<&ID2D1StrokeStyle>,
                        )
                    });
                };
                // The glass is translucent: the shadow must not show through it. The dark line
                // outside its edge is half hidden by the glass drawn over it.
                self.cut(&plates);
                stroke_outline(&frost.style.edge, 2.0);
                // The blurred desktop (kept in place when the band shifts), a tint over it, and
                // the faint darkening of the grooves.
                let mut home = Matrix3x2::default();
                backdrop.GetTransform(&mut home);
                each_plate(&|plate, shift| {
                    let mut at = home;
                    at.M31 -= shift.0;
                    at.M32 -= shift.1;
                    backdrop.SetTransform(&at);
                    self.ctx.FillGeometry(plate, backdrop, None::<&ID2D1Brush>);
                });
                backdrop.SetTransform(&home);
                for color in [&frost.style.tint, &p.plate] {
                    self.brush.SetColor(color);
                    each_plate(&|plate, _| {
                        self.ctx
                            .FillGeometry(plate, &self.brush, None::<&ID2D1Brush>)
                    });
                }
                // Sheen on the upper part of the disc, and the light edge of the glass.
                let r = self.layout.outer_radius();
                frost.sheen.SetStartPoint(Vector2 { X: c, Y: c - r });
                frost.sheen.SetEndPoint(Vector2 {
                    X: c,
                    Y: c + 0.2 * r,
                });
                frost.sheen.SetOpacity(frost.style.sheen);
                self.ctx
                    .FillGeometry(&frost.sheen_shape, &frost.sheen, None::<&ID2D1Brush>);
                stroke_outline(&frost.style.rim, 1.0);
            } else {
                self.brush.SetColor(&p.plate);
                each_plate(&|plate, _| {
                    self.ctx
                        .FillGeometry(plate, &self.brush, None::<&ID2D1Brush>)
                });
            }

            for i in 0..self.layout.inner_sectors() {
                let (a1, a2) = self.layout.inner_span(i);
                let geometry = &self.inner_tiles[usize::from(i)];
                let on = anim.level(Hit::Inner(i));
                self.fill_tile(geometry, p.tile, on, stroke);
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
                    theme::blend(p.fg, p.accent_fg, on),
                    1.0 - on,
                )?;
            }

            for (i, name) in (0..n).zip(self.config.outer_names(n)) {
                let (a1, a2) = self.layout.outer_span(i);
                let geometry = &self.outer_tiles[usize::from(i)];
                let on = anim.level(Hit::Outer(i));
                let has_submenu = self
                    .config
                    .outer_item(usize::from(i))
                    .is_some_and(|item| !item.submenu.is_empty());
                // The tile that opened the open arc is tinted so the arc visibly comes from it.
                let fill = if arc.is_some_and(|a| a.parent == i) {
                    if glass {
                        // Tiles are nearly clear, so a mix would not show.
                        D2D1_COLOR_F {
                            a: PARENT_GLASS_ALPHA,
                            ..p.accent
                        }
                    } else {
                        theme::mix(p.tile, p.accent, PARENT_TINT)
                    }
                } else {
                    p.tile
                };
                self.fill_tile(geometry, fill, on, stroke);
                let content_end = if has_submenu {
                    outer_ring.1.min(notch_clear)
                } else {
                    outer_ring.1
                };
                if self.icons.is_some() {
                    let size = self.icon_sizes.outer;
                    let mid_r = (outer_ring.0 + outer_ring.1) / 2.0;
                    let r = mid_r.min(content_end - size as f32 / 2.0);
                    let at = polar(c, r, (a1 + a2) / 2.0);
                    self.icon(Hit::Outer(i), at, size, on, k)?;
                } else {
                    let text_ring = (outer_ring.0 + pad, content_end - pad);
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
                        theme::blend(p.fg, p.accent_fg, on),
                        1.0 - on,
                    )?;
                }
                // Notch at the outer edge of a tile with a submenu.
                if let Some(notch) = &self.notches[usize::from(i)] {
                    // While only its arc is open the tile itself is not highlighted, so the notch
                    // takes the accent colour instead of the text colour on accent.
                    let base = if arc.is_some_and(|a| a.parent == i) {
                        p.accent
                    } else {
                        p.muted
                    };
                    self.brush.SetColor(&theme::blend(base, p.accent_fg, on));
                    self.ctx
                        .FillGeometry(notch, &self.brush, None::<&ID2D1Brush>);
                }
            }

            if let (Some(arc), Some(shapes)) = (arc, &arc_shapes) {
                let subs = self
                    .config
                    .outer_item(usize::from(arc.parent))
                    .map_or(&[][..], |item| item.submenu.as_slice());
                self.faded(arc_t, (0.0, 0.0), || -> Result<()> {
                    for (j, item) in subs.iter().take(usize::from(arc.count)).enumerate() {
                        // `count` is at most MAX_SUB_TILES (5), so `j` fits in u8.
                        let j = j as u8;
                        let (a1, a2) = self.layout.sub_span(arc, j);
                        // Each tile slides out along its own radius.
                        let shift = arc_shift((a1 + a2) / 2.0, k, arc_t);
                        self.ctx
                            .SetTransform(&Matrix3x2::translation(shift.0, shift.1));
                        let geometry = &shapes.tiles[usize::from(j)];
                        let tile = Hit::Sub(arc.parent, j);
                        let on = anim.level(tile);
                        self.fill_tile(geometry, p.surface, on, stroke);
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
                            theme::blend(p.fg, p.accent_fg, on),
                            1.0 - on,
                        )?;
                    }
                    self.ctx.SetTransform(&Matrix3x2::identity());
                    Ok(())
                })?;
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
                1.0,
            )?;
            // The options hint sits under "Anuluj" (the text stays where it is) and gives way to
            // a tile name.
            let hint = anim.hint();
            if hit == Hit::Center && hint > 0.0 {
                let below = self
                    .layouts
                    .borrow()
                    .get(&(Kind::Hub, text.to_string()))
                    .map_or(0.0, |l| l.height / 2.0 + k);
                self.faded(hint, (0.0, 0.0), || {
                    self.label(
                        Kind::Hint,
                        wheel_core::look::OPTIONS_HINT,
                        &self.hint_font,
                        &|h, _| {
                            let reach = below + h / 2.0;
                            let width =
                                fit::circle_width(radii.center - pad, 2.0 * (reach + h / 2.0));
                            (width - pad, (c, c + reach))
                        },
                        p.muted,
                        1.0,
                    )
                })?;
            }
        }
        Ok(())
    }

    /// Soft shadow around `geometry`: a few wide translucent strokes of falling width, whose
    /// inner halves end up under the (opaque) plate drawn afterwards.
    ///
    /// # Safety
    /// Must be called between `BeginDraw` and `EndDraw` of `self.ctx`.
    unsafe fn shadow(&self, geometry: &ID2D1Geometry, k: f32) {
        let mut color = self.palette.shadow;
        color.a /= SHADOW_PASSES as f32;
        // SAFETY: guaranteed by the caller.
        unsafe {
            self.brush.SetColor(&color);
            for pass in 0..SHADOW_PASSES {
                let left = (SHADOW_PASSES - pass) as f32 / SHADOW_PASSES as f32;
                self.ctx.DrawGeometry(
                    geometry,
                    &self.brush,
                    2.0 * SHADOW_REACH * k * left,
                    &self.round_join,
                );
            }
        }
    }

    /// Runs `f`, whose drawing is shifted by `shift` and faded to `opacity` as one group; just
    /// `f` when there is nothing to apply.
    ///
    /// # Safety
    /// Must be called between `BeginDraw` and `EndDraw` of `self.ctx`.
    unsafe fn faded<R>(&self, opacity: f32, shift: (f32, f32), f: impl FnOnce() -> R) -> R {
        if opacity >= 1.0 && shift == (0.0, 0.0) {
            return f();
        }
        let everywhere = D2D_RECT_F {
            left: f32::MIN,
            top: f32::MIN,
            right: f32::MAX,
            bottom: f32::MAX,
        };
        let params = D2D1_LAYER_PARAMETERS1 {
            contentBounds: everywhere,
            geometricMask: ManuallyDrop::new(None),
            maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
            maskTransform: Matrix3x2::identity(),
            opacity,
            opacityBrush: ManuallyDrop::new(None),
            layerOptions: D2D1_LAYER_OPTIONS1_NONE,
        };
        // SAFETY: guaranteed by the caller; the layer is popped before returning.
        unsafe {
            self.ctx.PushLayer(&params, None::<&ID2D1Layer>);
            self.ctx
                .SetTransform(&Matrix3x2::translation(shift.0, shift.1));
            let result = f();
            self.ctx.SetTransform(&Matrix3x2::identity());
            self.ctx.PopLayer();
            result
        }
    }

    /// Makes the area of `shapes` fully transparent.
    ///
    /// # Safety
    /// Must be called between `BeginDraw` and `EndDraw` of `self.ctx`.
    unsafe fn cut(&self, shapes: &[&ID2D1Geometry]) {
        // SAFETY: guaranteed by the caller.
        unsafe {
            self.ctx.SetPrimitiveBlend(D2D1_PRIMITIVE_BLEND_COPY);
            self.brush.SetColor(&D2D1_COLOR_F {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            });
            for shape in shapes {
                self.ctx
                    .FillGeometry(*shape, &self.brush, None::<&ID2D1Brush>);
            }
            self.ctx.SetPrimitiveBlend(D2D1_PRIMITIVE_BLEND_SOURCE_OVER);
        }
    }

    /// # Safety
    /// Must be called between `BeginDraw` and `EndDraw` of `self.ctx`.
    unsafe fn fill_tile(&self, geometry: &ID2D1Geometry, fill: D2D1_COLOR_F, on: f32, stroke: f32) {
        let p = self.pal();
        // SAFETY: guaranteed by the caller.
        unsafe {
            self.brush.SetColor(&theme::blend(fill, p.accent, on));
            self.ctx
                .FillGeometry(geometry, &self.brush, None::<&ID2D1Brush>);
            let frost = self.frost.as_ref().filter(|_| self.glass.get());
            if let (true, Some(frost), Ok(bounds)) = (on > 0.0, frost, geometry.GetBounds(None)) {
                // Sheen on the upper part of the highlighted tile.
                let mid = (bounds.left + bounds.right) / 2.0;
                let height = bounds.bottom - bounds.top;
                frost.sheen.SetStartPoint(Vector2 {
                    X: mid,
                    Y: bounds.top,
                });
                frost.sheen.SetEndPoint(Vector2 {
                    X: mid,
                    Y: bounds.top + 0.7 * height,
                });
                frost.sheen.SetOpacity(TILE_SHEEN * on);
                self.ctx
                    .FillGeometry(geometry, &frost.sheen, None::<&ID2D1Brush>);
            }
            self.brush
                .SetColor(&theme::blend(p.tile_line, p.accent, on));
            self.ctx
                .DrawGeometry(geometry, &self.brush, stroke, None::<&ID2D1StrokeStyle>);
        }
    }

    /// Draws the icon of tile `hit` centred on `at`, `size` px square; text and strokes use the
    /// tile text colour (accent text colour while the tile is highlighted).
    ///
    /// # Safety
    /// Must be called between `BeginDraw` and `EndDraw` of `self.ctx`.
    unsafe fn icon(&self, hit: Hit, at: (f32, f32), size: u32, on: f32, k: f32) -> Result<()> {
        let Some(icons) = &self.icons else {
            return Ok(());
        };
        let color = theme::blend(self.pal().fg, self.pal().accent_fg, on);
        let s = size as f32;
        let half = s / 2.0;
        // Text layouts are the size of the icon box and centred in it.
        // `shift` moves the layout so the glyph outline, not its em box, is centred.
        let draw_text = |text: char,
                         glyph: bool,
                         font: &IDWriteTextFormat,
                         em: f32,
                         shift: (f32, f32)|
         -> Result<()> {
            let mut layouts = self.icon_layouts.borrow_mut();
            let key = (glyph, text, size);
            let layout = match layouts.entry(key) {
                Entry::Occupied(e) => e.into_mut(),
                Entry::Vacant(e) => {
                    let mut buf = [0u8; 4];
                    e.insert(self.text_layout(text.encode_utf8(&mut buf), font, em, s, s)?)
                }
            };
            // SAFETY: guaranteed by the caller of `icon`.
            unsafe {
                self.brush.SetColor(&color);
                self.ctx.DrawTextLayout(
                    Vector2 {
                        X: at.0 - half - shift.0,
                        Y: at.1 - half - shift.1,
                    },
                    &*layout,
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
                TileIcon::Glyph(glyph, fit) => {
                    if let Some(font) = icons.glyph_format() {
                        let shift = (fit.dx * fit.scale, fit.dy * fit.scale);
                        draw_text(glyph, true, font, s * fit.scale, shift)?;
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
                    draw_text(letter, false, &self.letter_font, s * 0.55, (0.0, 0.0))?;
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
        halo_alpha: f32,
    ) -> Result<()> {
        let mut layouts = self.layouts.borrow_mut();
        let key = (kind, text.to_string());
        if !layouts.contains_key(&key) {
            layouts.insert(key.clone(), self.fit_label(text, font, slot)?);
        }
        let fitted = &layouts[&key];
        // SAFETY: guaranteed by the caller.
        unsafe {
            if let Some(halo) = fitted
                .halo
                .as_ref()
                .filter(|_| halo_alpha > 0.0 && self.glass.get())
            {
                let left = fitted.at.0 - fitted.width / 2.0 - halo.margin;
                let top = fitted.at.1 - fitted.height / 2.0 - halo.margin;
                let rect = D2D_RECT_F {
                    left,
                    top,
                    right: left + halo.size.0,
                    bottom: top + halo.size.1,
                };
                self.ctx.DrawBitmap(
                    &halo.bitmap,
                    Some(&rect),
                    halo_alpha,
                    D2D1_INTERPOLATION_MODE_LINEAR,
                    None,
                    None,
                );
            }
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
        let min_scale = MIN_FONT_PX * self.layout.dpi_factor() / base;
        let fitted = fit::fit(text, measure, |h| slot(h, f32::INFINITY).0, min_scale);
        let (text_width, height) = measure(&fitted.text, fitted.scale);
        let (width, at) = slot(height, text_width);
        let width = width.max(1.0);
        let layout = self.text_layout(&fitted.text, font, base * fitted.scale, width, height)?;
        let halo = self.bake_halo(&layout, width, height);
        Ok(FittedLabel {
            layout,
            halo,
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

impl Frost {
    /// Builds the effect chain and the bitmaps once; `size` is the window side and `radius` the
    /// radius of `wheel_plate`.
    #[allow(clippy::too_many_arguments)]
    fn new(
        d2d_device: &ID2D1Device,
        ctx: &ID2D1DeviceContext,
        factory: &ID2D1Factory1,
        size: u32,
        k: f32,
        palette: &Palette,
        radius: f32,
        wheel_plate: &ID2D1Geometry,
        level: f32,
        margin: i32,
    ) -> Result<Frost> {
        let style = palette.glass_style(level);
        // The snapshot is blurred after it is shrunk.
        let sigma = BLUR_SIGMA * k;
        let halo_sigma = HALO_SIGMA * k;
        // Three standard deviations reach almost all of a Gaussian.
        let pad = (3.0 * sigma).ceil() as u32;
        let c = size as f32 / 2.0;
        // SAFETY: plain COM calls with valid arguments; every result is checked via `?`.
        unsafe {
            let white = |a| D2D1_COLOR_F {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a,
            };
            let stops = ctx.CreateGradientStopCollection(
                &[
                    D2D1_GRADIENT_STOP {
                        position: 0.0,
                        color: white(1.0),
                    },
                    D2D1_GRADIENT_STOP {
                        position: 1.0,
                        color: white(0.0),
                    },
                ],
                D2D1_COLOR_SPACE_SRGB,
                D2D1_COLOR_SPACE_SRGB,
                D2D1_BUFFER_PRECISION_8BPC_UNORM,
                D2D1_EXTEND_MODE_CLAMP,
                D2D1_COLOR_INTERPOLATION_MODE_PREMULTIPLIED,
            )?;
            let sheen = ctx.CreateLinearGradientBrush(
                &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                    startPoint: Vector2 { X: 0.0, Y: 0.0 },
                    endPoint: Vector2 { X: 0.0, Y: 1.0 },
                },
                None,
                &stops,
            )?;
            // The sheen is the part of the disc inside a large ellipse sitting high on it.
            let lens = factory.CreateEllipseGeometry(&D2D1_ELLIPSE {
                point: Vector2 {
                    X: c,
                    Y: c - SHEEN_LIFT * radius,
                },
                radiusX: radius,
                radiusY: SHEEN_HEIGHT * radius,
            })?;
            let path: ID2D1PathGeometry = factory.CreatePathGeometry()?.into();
            let sink = path.Open()?;
            wheel_plate.CombineWithGeometry(
                &lens,
                D2D1_COMBINE_MODE_INTERSECT,
                None,
                0.1,
                &sink,
            )?;
            sink.Close()?;

            Ok(Frost {
                style,
                d2d_device: d2d_device.clone(),
                sigma,
                blur: RefCell::new(None),
                halo_tools: RefCell::new(None),
                snap: RefCell::new(None),
                sheen,
                sheen_shape: path.cast()?,
                pad,
                margin,
                halo_sigma,
            })
        }
    }

    /// The blur pipeline for `ctx`, the context the snapshot is drawn with.
    fn new_blur(&self, ctx: &ID2D1DeviceContext) -> Result<Blur> {
        let style = &self.style;
        // SAFETY: plain COM calls with valid arguments; every result is checked via `?`.
        unsafe {
            let blur = ctx.CreateEffect(&CLSID_D2D1GaussianBlur)?;
            set_effect(
                &blur,
                D2D1_GAUSSIANBLUR_PROP_STANDARD_DEVIATION.0,
                D2D1_PROPERTY_TYPE_FLOAT,
                &[self.sigma / backdrop::SCALE as f32],
            )?;
            // Hard borders: the padding keeps them outside the window.
            set_effect(
                &blur,
                D2D1_GAUSSIANBLUR_PROP_BORDER_MODE.0,
                D2D1_PROPERTY_TYPE_ENUM,
                &[f32::from_bits(D2D1_BORDER_MODE_HARD.0 as u32)],
            )?;

            // Saturation and brightness in one color matrix (rows: input R, G, B, A, offset).
            let tone = ctx.CreateEffect(&CLSID_D2D1ColorMatrix)?;
            tone.SetInput(0, &blur.GetOutput()?, true);
            let luma = [0.2126, 0.7152, 0.0722];
            let mut matrix = [0.0f32; 20];
            for (input, weight) in luma.into_iter().enumerate() {
                for output in 0..3 {
                    let own = if input == output {
                        style.saturation
                    } else {
                        0.0
                    };
                    matrix[input * 4 + output] =
                        style.gain * ((1.0 - style.saturation) * weight + own);
                }
            }
            matrix[3 * 4 + 3] = 1.0;
            matrix[16..19].fill(style.lift);
            set_effect(
                &tone,
                D2D1_COLORMATRIX_PROP_COLOR_MATRIX.0,
                D2D1_PROPERTY_TYPE_MATRIX_5X4,
                &matrix,
            )?;
            Ok(Blur { blur, tone })
        }
    }

    fn new_halo_tools(&self) -> Result<HaloTools> {
        // SAFETY: plain COM calls with valid arguments; every result is checked via `?`.
        unsafe {
            let aux = self
                .d2d_device
                .CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
            aux.SetDpi(96.0, 96.0);
            aux.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            let aux_brush = aux.CreateSolidColorBrush(&self.style.halo, None)?;
            let shadow = aux.CreateEffect(&CLSID_D2D1Shadow)?;
            set_effect(
                &shadow,
                D2D1_SHADOW_PROP_BLUR_STANDARD_DEVIATION.0,
                D2D1_PROPERTY_TYPE_FLOAT,
                &[self.halo_sigma],
            )?;
            let h = self.style.halo;
            set_effect(
                &shadow,
                D2D1_SHADOW_PROP_COLOR.0,
                D2D1_PROPERTY_TYPE_VECTOR4,
                &[h.r, h.g, h.b, h.a],
            )?;
            Ok(HaloTools {
                aux,
                aux_brush,
                shadow,
            })
        }
    }

    /// Renders the halo of a label: its text blurred into a soft shadow, drawn twice for strength.
    fn halo(&self, layout: &IDWriteTextLayout, width: f32, height: f32) -> Result<Halo> {
        let mut tools = self.halo_tools.borrow_mut();
        if tools.is_none() {
            *tools = Some(self.new_halo_tools()?);
        }
        let HaloTools {
            aux,
            aux_brush,
            shadow,
        } = tools.as_ref().expect("just made");
        let margin = (3.0 * self.halo_sigma).ceil();
        let (w, h) = (
            (width + 2.0 * margin).ceil(),
            (height + 2.0 * margin).ceil(),
        );
        // SAFETY: plain COM calls on the aux context, which is not drawing anything else; each
        // BeginDraw is closed by its EndDraw before the next call.
        unsafe {
            let text = new_bitmap(aux, w as u32, h as u32, D2D1_BITMAP_OPTIONS_TARGET)?;
            let bitmap = new_bitmap(aux, w as u32, h as u32, D2D1_BITMAP_OPTIONS_TARGET)?;
            let clear = D2D1_COLOR_F {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            };
            aux.SetTarget(&text);
            aux.BeginDraw();
            aux.Clear(Some(&clear));
            aux.DrawTextLayout(
                Vector2 {
                    X: margin,
                    Y: margin,
                },
                layout,
                aux_brush,
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
            );
            aux.EndDraw(None, None)?;

            shadow.SetInput(0, &text, true);
            let output = shadow.GetOutput()?;
            aux.SetTarget(&bitmap);
            aux.BeginDraw();
            aux.Clear(Some(&clear));
            for _ in 0..2 {
                aux.DrawImage(
                    &output,
                    None,
                    None,
                    D2D1_INTERPOLATION_MODE_LINEAR,
                    D2D1_COMPOSITE_MODE_SOURCE_OVER,
                );
            }
            let ended = aux.EndDraw(None, None);
            aux.SetTarget(None::<&ID2D1Image>);
            ended?;
            Ok(Halo {
                bitmap,
                margin,
                size: (w, h),
            })
        }
    }
}

/// An empty B8G8R8A8 premultiplied bitmap of `width` x `height` px.
fn new_bitmap(
    ctx: &ID2D1DeviceContext,
    width: u32,
    height: u32,
    options: D2D1_BITMAP_OPTIONS,
) -> Result<ID2D1Bitmap1> {
    let props = D2D1_BITMAP_PROPERTIES1 {
        pixelFormat: D2D1_PIXEL_FORMAT {
            format: DXGI_FORMAT_B8G8R8A8_UNORM,
            alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
        },
        dpiX: 96.0,
        dpiY: 96.0,
        bitmapOptions: options,
        colorContext: ManuallyDrop::new(None),
    };
    // SAFETY: plain COM call with valid arguments.
    unsafe { ctx.CreateBitmap(D2D_SIZE_U { width, height }, None, 0, &props) }
}

/// Sets effect property `index`; `values` are the 32-bit words of its value (a float, an enum, a
/// vector or a matrix).
fn set_effect(
    effect: &ID2D1Effect,
    index: i32,
    kind: D2D1_PROPERTY_TYPE,
    values: &[f32],
) -> Result<()> {
    let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_ne_bytes()).collect();
    // SAFETY: plain COM call; `bytes` outlives it.
    unsafe { effect.SetValue(index as u32, kind, &bytes) }
}

/// Shapes of one submenu arc: the band under the tiles, the tiles and the outline of the glass.
struct ArcShapes {
    plate: ID2D1Geometry,
    tiles: Vec<ID2D1Geometry>,
    /// The wheel disc and `plate` as one shape.
    outline: ID2D1Geometry,
}

/// Radii of the tile rings, in px from the wheel centre.
struct Rings {
    inner: (f32, f32),
    outer: (f32, f32),
    sub: (f32, f32),
}

fn rings(layout: &Layout) -> Rings {
    let k = scale_factor(layout);
    let stroke = 1.2 * k;
    let radii = layout.radii();
    // Keep the outer stroke inside the window.
    let outer_edge = layout.outer_radius() - stroke / 2.0;
    let (r1, r2) = layout.sub_radii();
    Rings {
        inner: (radii.center + 4.0 * k, radii.inner - 2.0 * k),
        outer: (radii.inner + 2.0 * k, outer_edge),
        sub: (r1, r2 - stroke / 2.0),
    }
}

/// A label's halo, drawn `margin` px outside the label box.
struct Halo {
    bitmap: ID2D1Bitmap1,
    margin: f32,
    size: (f32, f32),
}

/// Which label a cached layout belongs to; the index tells sectors with equal text apart.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Kind {
    Inner(u8),
    Outer(u8),
    /// Tile of the submenu arc: parent sector and index in its submenu.
    Sub(u8, u8),
    Hub,
    Hint,
}

struct FittedLabel {
    layout: IDWriteTextLayout,
    /// Soft glow behind the text for the glass look; `None` without it.
    halo: Option<Halo>,
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

/// Point at distance `r` and angle `deg` from the point `p`.
/// Offset of an arc part whose middle lies at `deg`: it starts `ARC_SLIDE_PX` closer to the wheel's
/// centre and moves out to 0 as `t` (0..=1) goes up.
fn arc_shift(deg: f32, k: f32, t: f32) -> (f32, f32) {
    let dir = polar(0.0, 1.0, deg);
    let back = -anim::ARC_SLIDE_PX * k * (1.0 - t);
    (dir.0 * back, dir.1 * back)
}

fn around(p: (f32, f32), r: f32, deg: f32) -> Vector2 {
    let a = deg.to_radians();
    Vector2 {
        X: p.0 + r * a.sin(),
        Y: p.1 - r * a.cos(),
    }
}

/// Annular sector between radii `ring.0..ring.1` and angles `span.0..span.1` (degrees, 0 = up,
/// clockwise), around the point (`c`, `c`), with corners rounded by at most `round` px (never
/// more than `CORNER_MAX_SHARE` of the ring thickness or of the inner arc width). Each corner is
/// a circle touching the arc and the radial edge, as in the editor preview.
fn sector_path(
    factory: &ID2D1Factory1,
    c: f32,
    ring: (f32, f32),
    span: (f32, f32),
    round: f32,
) -> Result<ID2D1Geometry> {
    let (r0, r1) = ring;
    let (s, e) = span;
    let width = r0 * (e - s).to_radians();
    let rc = round
        .min((r1 - r0) * CORNER_MAX_SHARE)
        .min(width * CORNER_MAX_SHARE)
        .max(0.0);
    let centre = (c, c);
    // Corner circle centres lie `rc` inside the arc, turned away from the edge by an angle.
    let (ro, ri) = (r1 - rc, r0 + rc);
    let turn = |rho: f32| (rc / rho).clamp(0.0, 1.0).asin().to_degrees();
    let (to, ti) = (turn(ro), turn(ri));
    let arc_size = |from: f32, upto: f32| {
        if upto - from > 180.0 {
            D2D1_ARC_SIZE_LARGE
        } else {
            D2D1_ARC_SIZE_SMALL
        }
    };
    // SAFETY: plain COM calls; the sink is closed before the geometry is used.
    unsafe {
        let path: ID2D1PathGeometry = factory.CreatePathGeometry()?.into();
        let sink = path.Open()?;
        let arc = |point: Vector2, r: f32, sweep, size| {
            sink.AddArc(&D2D1_ARC_SEGMENT {
                point,
                size: D2D_SIZE_F {
                    width: r,
                    height: r,
                },
                rotationAngle: 0.0,
                sweepDirection: sweep,
                arcSize: size,
            });
        };
        let clockwise = D2D1_SWEEP_DIRECTION_CLOCKWISE;
        let counter = D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE;
        if rc < 0.05 {
            let point = |r: f32, a: f32| around(centre, r, a);
            sink.BeginFigure(point(r1, s), D2D1_FIGURE_BEGIN_FILLED);
            arc(point(r1, e), r1, clockwise, arc_size(s, e));
            sink.AddLine(point(r0, e));
            arc(point(r0, s), r0, counter, arc_size(s, e));
        } else {
            let at = |r: f32, a: f32| {
                let v = around(centre, r, a);
                (v.X, v.Y)
            };
            let (outer_s, outer_e) = (at(ro, s + to), at(ro, e - to));
            let (inner_s, inner_e) = (at(ri, s + ti), at(ri, e - ti));
            let small = D2D1_ARC_SIZE_SMALL;
            sink.BeginFigure(around(outer_s, rc, s - 90.0), D2D1_FIGURE_BEGIN_FILLED);
            arc(around(outer_s, rc, s + to), rc, clockwise, small);
            arc(
                around(outer_e, rc, e - to),
                r1,
                clockwise,
                arc_size(s + to, e - to),
            );
            arc(around(outer_e, rc, e + 90.0), rc, clockwise, small);
            sink.AddLine(around(inner_e, rc, e + 90.0));
            arc(around(inner_e, rc, e - ti + 180.0), rc, clockwise, small);
            arc(
                around(inner_s, rc, s + ti + 180.0),
                r0,
                counter,
                arc_size(s + ti, e - ti),
            );
            arc(around(inner_s, rc, s + 270.0), rc, clockwise, small);
        }
        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
        sink.Close()?;
        path.cast()
    }
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

thread_local! {
    /// Direct2D factory and device made for a Direct3D device (identified by its pointer), kept
    /// for the renderers that follow, like the Direct3D device itself.
    static D2D_SHARED: RefCell<Option<(usize, ID2D1Factory1, ID2D1Device)>> =
        const { RefCell::new(None) };
}

/// The Direct2D factory and device for `dxgi_device`, shared by the renderers on this thread.
fn d2d_for(dxgi_device: &IDXGIDevice) -> Result<(ID2D1Factory1, ID2D1Device)> {
    let key = dxgi_device.as_raw() as usize;
    D2D_SHARED.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((_, factory, device)) = cache.as_ref().filter(|(k, ..)| *k == key) {
            return Ok((factory.clone(), device.clone()));
        }
        // SAFETY: plain COM calls with valid arguments.
        let (factory, device) = unsafe {
            let factory: ID2D1Factory1 = D2D1CreateFactory(D2D1_FACTORY_TYPE_MULTI_THREADED, None)?;
            let device = factory.CreateDevice(dxgi_device)?;
            (factory, device)
        };
        *cache = Some((key, factory.clone(), device.clone()));
        Ok((factory, device))
    })
}

/// Drops the shared Direct3D and Direct2D devices, so the next renderer gets fresh ones. For
/// when the glass look is switched off: the blur effects it made stay with the old devices.
/// Call it after the renderers that use them are gone.
pub fn release_shared_devices() {
    D2D_SHARED.with(|cache| *cache.borrow_mut() = None);
    D3D_DEVICE.with(|cache| *cache.borrow_mut() = None);
}

/// Whether a wait that ends at `deadline` has run out at `now`.
fn wait_over(deadline: Instant, now: Instant) -> bool {
    now >= deadline
}

/// Waits (until `deadline` at most) until the GPU has run everything queued on the shared
/// device; does nothing if the wait cannot be set up.
fn wait_for_gpu(deadline: Instant) {
    let Some(device) = D3D_DEVICE.with(|cache| cache.borrow().clone()) else {
        return;
    };
    let desc = D3D11_QUERY_DESC {
        Query: D3D11_QUERY_EVENT,
        MiscFlags: 0,
    };
    let mut query = None;
    // SAFETY: plain COM calls; `query` is a valid out pointer and outlives the polling.
    unsafe {
        let Ok(context) = device.GetImmediateContext() else {
            return;
        };
        if device.CreateQuery(&desc, Some(&mut query)).is_err() {
            return;
        }
        let Some(query) = query else { return };
        context.End(&query);
        context.Flush();
        // GetData answers S_FALSE while the work is still running, which the safe wrapper
        // reports as success; so the raw call.
        let finished = || {
            (Interface::vtable(&context).GetData)(
                Interface::as_raw(&context),
                Interface::as_raw(&query),
                std::ptr::null_mut(),
                0,
                0,
            ) != S_FALSE
        };
        while !finished() && !wait_over(deadline, Instant::now()) {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

thread_local! {
    /// The device of this thread, kept across renderers (see `create_d3d_device`).
    static D3D_DEVICE: RefCell<Option<ID3D11Device>> = const { RefCell::new(None) };
}

/// The Direct3D device of the calling thread, made on first use and shared by the renderers that
/// follow (a renderer is rebuilt on every configuration change). A device made anew each time
/// leaves a few MB of driver and shader memory behind in the process; a lost device is replaced.
fn create_d3d_device() -> Result<ID3D11Device> {
    D3D_DEVICE.with(|cache| {
        let mut cache = cache.borrow_mut();
        // SAFETY: plain COM call.
        if let Some(device) = cache
            .as_ref()
            .filter(|d| unsafe { d.GetDeviceRemovedReason() }.is_ok())
        {
            return Ok(device.clone());
        }
        let device = new_d3d_device()?;
        *cache = Some(device.clone());
        Ok(device)
    })
}

/// Hardware device, or the WARP software rasterizer when there is no usable GPU.
fn new_d3d_device() -> Result<ID3D11Device> {
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

    #[test]
    fn gpu_wait_runs_out_at_the_deadline() {
        let now = Instant::now();
        let deadline = now + Duration::from_millis(GPU_WAIT_MS);
        assert!(!super::wait_over(deadline, now));
        assert!(super::wait_over(deadline, deadline));
        assert!(super::wait_over(
            deadline,
            deadline + Duration::from_millis(1)
        ));
    }

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
        scale_pct: u16,
        /// Translucent palette; `save_png` then shows it over stripes instead of a plain colour.
        glass: bool,
    }

    /// Renders the case offscreen and returns the side and BGRA premultiplied pixels.
    fn render(case: &Case) -> (u32, Vec<u8>) {
        let mut config = Config::default();
        config.options.view = case.view;
        config.options.outer_sectors = case.sectors;
        config.outer[usize::from(case.parent)].submenu = submenu(case.tiles);
        let arc = arc_for_hit(case.hit, &config);
        let layout = Layout::new(96, case.scale_pct, 12, case.sectors);
        let device = create_d3d_device().expect("D3D device");
        let dxgi: IDXGIDevice = device.cast().unwrap();
        let level = if case.glass { 0.5 } else { 0.0 };
        let painter = Painter::new(
            &dxgi,
            layout,
            theme::palette(case.theme),
            Arc::new(config),
            level,
        )
        .expect("painter");
        if let Some(frost) = &painter.frost {
            // A stand-in for the desktop: vertical stripes.
            let _ = frost;
            let side = painter.snapshot_sides(0).1 as usize;
            let q = backdrop::SCALE as usize;
            let bgra: Vec<u8> = (0..side * side)
                .flat_map(|i| {
                    if (i % side * q / 24).is_multiple_of(2) {
                        [0x30; 4]
                    } else {
                        [0xc0; 4]
                    }
                })
                .collect();
            painter.blur_snapshot(&bgra, (0, 0), 0).expect("blur");
            assert!(painter.place_snapshot(0, 0));
            painter.glass.set(true);
        }
        paint_to_pixels(&painter, case.hit, arc)
    }

    /// Paints one frame offscreen and returns the side and BGRA premultiplied pixels.
    fn paint_to_pixels(painter: &Painter, hit: Hit, arc: Option<SubArc>) -> (u32, Vec<u8>) {
        let size = painter.size;
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
            painter
                .paint(&target, hit, arc, &Anim::settled(hit))
                .expect("paint");
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

    /// Writes the pixels as a 24-bit PNG composited over a plain background, or over stripes of
    /// mid grey (no blur) when `stripes` is set.
    fn save_png(path: &Path, size: u32, bgra: &[u8], dark: bool, stripes: bool) {
        let plain: [u32; 3] = if dark {
            [0x1c, 0x14, 0x10]
        } else {
            [0xf0, 0xe8, 0xe4]
        };
        let mut bgr = Vec::with_capacity((size * size * 3) as usize);
        for (i, px) in bgra.as_chunks::<4>().0.iter().enumerate() {
            let bg = if stripes && (i as u32 % size / 24).is_multiple_of(2) {
                [0x30, 0x30, 0x30]
            } else if stripes {
                [0xc0, 0xc0, 0xc0]
            } else {
                plain
            };
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

    /// A snapshot taken at press time with a margin and moved by the pointer's travel must look
    /// like one taken at the final position (up to the shrinking grid).
    #[test]
    #[ignore = "needs a D3D device"]
    fn shifted_snapshot_matches_direct() {
        let config = Config::default();
        let layout = Layout::new(96, 100, 12, 12);
        let device = create_d3d_device().expect("D3D device");
        let dxgi: IDXGIDevice = device.cast().unwrap();
        let painter = Painter::new(
            &dxgi,
            layout,
            theme::palette(ThemeChoice::Dark),
            Arc::new(config),
            0.5,
        )
        .expect("painter");
        let q = backdrop::SCALE as i32;
        // A fake desktop: diagonal stripes, coloured by position.
        let desktop = |x: i32, y: i32| -> [u8; 4] {
            if ((x + y).div_euclid(37)).rem_euclid(2) == 0 {
                [0x30, 0x60, 0xa0, 255]
            } else {
                [0xd0, 0xc0, 0x40, 255]
            }
        };
        let blur_at = |left: i32, top: i32, margin: i32| {
            let (_, side, pad) = painter.snapshot_sides(margin);
            let (origin_x, origin_y) = (left - margin - pad * q, top - margin - pad * q);
            let full = (side * q) as usize;
            let bgra: Vec<u8> = (0..full * full)
                .flat_map(|i| desktop(origin_x + (i % full) as i32, origin_y + (i / full) as i32))
                .collect();
            painter
                .blur_snapshot(&backdrop::downscale(&bgra, full), (left, top), margin)
                .unwrap();
        };
        let paint = |left: i32, top: i32| {
            assert!(painter.place_snapshot(left, top));
            painter.glass.set(true);
            paint_to_pixels(&painter, Hit::Center, None).1
        };
        blur_at(1000, 500, 6);
        let shifted = paint(1003, 497);
        blur_at(1003, 497, 0);
        let direct = paint(1003, 497);
        let total: u64 = shifted
            .iter()
            .zip(&direct)
            .map(|(a, b)| u64::from(a.abs_diff(*b)))
            .sum();
        let mean = total as f64 / shifted.len() as f64;
        println!("mean abs difference per channel: {mean:.3}");
        assert!(mean < 2.0, "{mean}");
    }

    /// `cargo test startup_and_memory -- --ignored --nocapture`: time to build and warm a
    /// renderer, and the private memory in each state.
    #[test]
    #[ignore = "needs a D3D device and a desktop; prints timings and memory"]
    fn startup_and_memory() {
        use windows::Win32::System::ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX,
        };
        use windows::Win32::System::Threading::GetCurrentProcess;
        let private_mb = || {
            let mut c = PROCESS_MEMORY_COUNTERS_EX {
                cb: size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
                ..Default::default()
            };
            // SAFETY: valid out buffer and matching `cb`.
            unsafe {
                GetProcessMemoryInfo(
                    GetCurrentProcess(),
                    (&mut c as *mut PROCESS_MEMORY_COUNTERS_EX).cast(),
                    c.cb,
                )
                .unwrap();
            }
            c.PrivateUsage as f64 / 1048576.0
        };
        // SAFETY: plain call; the result is ignored (the thread may already have a model).
        let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        let device = create_d3d_device().expect("D3D device");
        let dxgi: IDXGIDevice = device.cast().unwrap();
        for view in [ViewMode::Names, ViewMode::Icons] {
            let mut config = Config::default();
            config.options.view = view;
            let layout = Layout::new(96, 100, 12, 12);
            println!("view {view:?}: start {:.1} MB", private_mb());
            let t = Instant::now();
            let painter = Painter::new(
                &dxgi,
                layout,
                theme::palette(ThemeChoice::Dark),
                Arc::new(config),
                0.5,
            )
            .expect("painter");
            println!("  new {:.1} ms, {:.1} MB", ms(t), private_mb());
            let t = Instant::now();
            painter.warm(true).expect("warm");
            println!("  warm {:.1} ms, {:.1} MB", ms(t), private_mb());
            let halos: Vec<(f32, f32)> = painter
                .layouts
                .borrow()
                .values()
                .filter_map(|l| l.halo.as_ref().map(|h| h.size))
                .collect();
            let bytes: f32 = halos.iter().map(|(w, h)| w * h * 4.0).sum();
            println!("  {} halos, {:.2} MB", halos.len(), bytes / 1048576.0);
            let target = new_bitmap(
                &painter.ctx,
                painter.size,
                painter.size,
                D2D1_BITMAP_OPTIONS_TARGET,
            )
            .unwrap();
            let t = Instant::now();
            painter.take_snapshot(600, 200, 6).unwrap();
            println!("  snapshot (press) {:.1} ms, {:.1} MB", ms(t), private_mb());
            let t = Instant::now();
            assert!(painter.place_snapshot(602, 199));
            painter.glass.set(true);
            painter
                .paint(&target, Hit::Center, None, &Anim::settled(Hit::Center))
                .unwrap();
            println!(
                "  place + first paint {:.2} ms, {:.1} MB",
                ms(t),
                private_mb()
            );
            painter.release_snapshot();
            println!("  released {:.1} MB", private_mb());
        }
    }

    fn ms(t: Instant) -> f64 {
        t.elapsed().as_secs_f64() * 1e3
    }

    /// `cargo test blur_timing -- --ignored --nocapture`: upload plus blur of a 676 px snapshot,
    /// with the GPU work waited for.
    #[test]
    #[ignore = "needs a D3D device; prints timings"]
    fn blur_timing() {
        let case = Case {
            name: "t",
            theme: ThemeChoice::Dark,
            view: ViewMode::Names,
            sectors: 12,
            parent: 0,
            tiles: 0,
            hit: Hit::Center,
            scale_pct: 100,
            glass: true,
        };
        let config = Config::default();
        let layout = Layout::new(96, 100, 12, 12);
        let device = create_d3d_device().expect("D3D device");
        let dxgi: IDXGIDevice = device.cast().unwrap();
        let painter = Painter::new(
            &dxgi,
            layout,
            theme::palette(case.theme),
            Arc::new(config),
            0.5,
        )
        .expect("painter");
        let frost = painter.frost.as_ref().expect("frost");
        let side = painter.snapshot_sides(0).1 as usize;
        let bgra = vec![0x80u8; side * side * 4];
        let mut ms = Vec::new();
        for _ in 0..30 {
            let t = std::time::Instant::now();
            painter.blur_snapshot(&bgra, (0, 0), 0).unwrap();
            // Reading a pixel back waits for the GPU.
            let staging = unsafe {
                painter
                    .ctx
                    .CreateBitmap(
                        D2D_SIZE_U {
                            width: 1,
                            height: 1,
                        },
                        None,
                        0,
                        &D2D1_BITMAP_PROPERTIES1 {
                            pixelFormat: D2D1_PIXEL_FORMAT {
                                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                            },
                            dpiX: 96.0,
                            dpiY: 96.0,
                            bitmapOptions: D2D1_BITMAP_OPTIONS_CPU_READ
                                | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                            colorContext: ManuallyDrop::new(None),
                        },
                    )
                    .unwrap()
            };
            unsafe {
                let result = frost
                    .snap
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .brush
                    .GetBitmap()
                    .unwrap();
                staging.CopyFromBitmap(None, &result, None).unwrap();
                let _ = staging.Map(D2D1_MAP_OPTIONS_READ).unwrap();
                staging.Unmap().unwrap();
            }
            ms.push(t.elapsed().as_secs_f64() * 1e3);
        }
        println!("first {:.2} ms", ms[0]);
        ms.sort_by(f64::total_cmp);
        println!("median {:.2} ms, max {:.2} ms", ms[15], ms[29]);

        // One frame of the wheel over the blurred snapshot (CPU time of the draw calls).
        painter.glass.set(true);
        let target = new_bitmap(
            &painter.ctx,
            painter.size,
            painter.size,
            D2D1_BITMAP_OPTIONS_TARGET,
        )
        .unwrap();
        let mut ms = Vec::new();
        for i in 0..30 {
            let t = std::time::Instant::now();
            painter
                .paint(
                    &target,
                    Hit::Outer(i % 12),
                    None,
                    &Anim::settled(Hit::Outer(i % 12)),
                )
                .unwrap();
            ms.push(t.elapsed().as_secs_f64() * 1e3);
        }
        println!("frame first {:.2} ms", ms[0]);
        ms.sort_by(f64::total_cmp);
        println!("frame median {:.2} ms, max {:.2} ms", ms[15], ms[29]);
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
            scale_pct: 100,
            glass: false,
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
            // Submenu notch next to a name, arc closed.
            case(
                "names-light-notch-no-arc",
                Light,
                N,
                12,
                9,
                3,
                Hit::Inner(0),
            ),
            case("icons-dark-notch-no-arc", Dark, I, 12, 9, 3, Hit::Inner(0)),
            // Translucent palette over stripes (the live blur is not part of the offscreen draw).
            Case {
                glass: true,
                ..case("glass-dark-arc3", Dark, N, 12, 2, 3, Hit::Sub(2, 1))
            },
            Case {
                glass: true,
                ..case("glass-light-arc3", Light, N, 12, 2, 3, Hit::Sub(2, 1))
            },
            Case {
                glass: true,
                ..case("glass-dark-icons-hover", Dark, I, 12, 0, 5, Hit::Outer(4))
            },
            // Smallest scale: labels stay at the minimum font size (trimmed when needed).
            Case {
                scale_pct: 60,
                ..case("names-light-60pct", Light, N, 12, 9, 3, Hit::Inner(0))
            },
        ];
        for case in &cases {
            let (size, pixels) = render(case);
            assert!(pixels.iter().any(|&b| b != 0), "{} is empty", case.name);
            if let Some(dir) = &out {
                let path = dir.join(format!("{}.png", case.name));
                save_png(&path, size, &pixels, case.theme == Dark, case.glass);
            }
        }
    }
}
