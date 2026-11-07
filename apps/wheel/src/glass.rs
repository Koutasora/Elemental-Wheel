//! The glass look: the wheel window is composed with Windows.UI.Composition. A sprite filled with
//! the host backdrop brush (the blurred desktop behind the window) is clipped to the wheel
//! shape, and the swap chain with the Direct2D content lies on top of it.

use std::cell::RefCell;
use std::ffi::c_void;

use windows::Graphics::{IGeometrySource2D, IGeometrySource2D_Impl};
use windows::System::DispatcherQueueController;
use windows::UI::Composition::Desktop::DesktopWindowTarget;
use windows::UI::Composition::{CompositionPath, CompositionPathGeometry, Compositor};
use windows::Win32::Foundation::{E_NOTIMPL, HWND};
use windows::Win32::Graphics::Direct2D::{ID2D1Factory, ID2D1Geometry};
use windows::Win32::Graphics::Dxgi::IDXGISwapChain1;
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows::Win32::System::WinRT::Composition::{ICompositorDesktopInterop, ICompositorInterop};
use windows::Win32::System::WinRT::Graphics::Direct2D::{
    IGeometrySource2DInterop, IGeometrySource2DInterop_Impl,
};
use windows::Win32::System::WinRT::{
    CreateDispatcherQueueController, DQTAT_COM_NONE, DQTYPE_THREAD_CURRENT, DispatcherQueueOptions,
};
use windows::core::{BOOL, Error, Interface, Ref, Result, implement, s, w};
use windows_numerics::Vector2;

thread_local! {
    /// The dispatcher queue Windows.UI.Composition needs on the thread; only one can exist.
    static QUEUE: RefCell<Option<DispatcherQueueController>> = const { RefCell::new(None) };
}

fn ensure_dispatcher_queue() -> Result<()> {
    QUEUE.with(|queue| {
        let mut queue = queue.borrow_mut();
        if queue.is_none() {
            // WinRT needs an initialised apartment; the result only says whether it was.
            // SAFETY: plain calls with valid arguments.
            let controller = unsafe {
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                CreateDispatcherQueueController(DispatcherQueueOptions {
                    dwSize: size_of::<DispatcherQueueOptions>() as u32,
                    threadType: DQTYPE_THREAD_CURRENT,
                    apartmentType: DQTAT_COM_NONE,
                })?
            };
            *queue = Some(controller);
        }
        Ok(())
    })
}

/// A Direct2D geometry offered to the compositor.
#[implement(IGeometrySource2D, IGeometrySource2DInterop)]
struct GeometrySource(ID2D1Geometry);

impl IGeometrySource2D_Impl for GeometrySource_Impl {}

impl IGeometrySource2DInterop_Impl for GeometrySource_Impl {
    fn GetGeometry(&self) -> Result<ID2D1Geometry> {
        Ok(self.0.clone())
    }

    fn TryGetGeometryUsingFactory(&self, _factory: Ref<ID2D1Factory>) -> Result<ID2D1Geometry> {
        Err(E_NOTIMPL.into())
    }
}

fn composition_path(geometry: &ID2D1Geometry) -> Result<CompositionPath> {
    let source: IGeometrySource2D = GeometrySource(geometry.clone()).into();
    CompositionPath::Create(&source)
}

/// `AccentState` of the undocumented `SetWindowCompositionAttribute`, and its parameters.
const ACCENT_DISABLED: u32 = 0;
const ACCENT_ENABLE_HOSTBACKDROP: u32 = 5;
const WCA_ACCENT_POLICY: u32 = 19;

#[repr(C)]
struct AccentPolicy {
    state: u32,
    flags: u32,
    gradient_color: u32,
    animation_id: u32,
}

#[repr(C)]
struct CompositionAttribute {
    attribute: u32,
    data: *mut c_void,
    size: usize,
}

/// Asks DWM to feed the window's host backdrop brush with the blurred desktop behind the window
/// (`on`), or stops doing so. Without it the brush paints nothing useful.
fn set_host_backdrop(hwnd: HWND, on: bool) -> Result<()> {
    type SetAttribute = unsafe extern "system" fn(HWND, *mut CompositionAttribute) -> BOOL;
    // SAFETY: user32 stays loaded; the function is looked up by name and called with the
    // signature it has had since Windows 10.
    unsafe {
        let user32 = GetModuleHandleW(w!("user32.dll"))?;
        let Some(proc) = GetProcAddress(user32, s!("SetWindowCompositionAttribute")) else {
            return Err(Error::from(E_NOTIMPL));
        };
        let set: SetAttribute = std::mem::transmute(proc);
        let mut policy = AccentPolicy {
            state: if on {
                ACCENT_ENABLE_HOSTBACKDROP
            } else {
                ACCENT_DISABLED
            },
            flags: 0,
            gradient_color: 0,
            animation_id: 0,
        };
        let mut data = CompositionAttribute {
            attribute: WCA_ACCENT_POLICY,
            data: (&raw mut policy).cast(),
            size: size_of::<AccentPolicy>(),
        };
        if set(hwnd, &mut data).as_bool() {
            Ok(())
        } else {
            Err(Error::from_thread())
        }
    }
}

pub struct Glass {
    hwnd: HWND,
    compositor: Compositor,
    target: DesktopWindowTarget,
    /// The blur shape.
    shape: CompositionPathGeometry,
}

impl Glass {
    /// Puts `swap_chain` (`size` x `size` px) on `hwnd` over the blurred backdrop clipped to
    /// `clip`. Fails when Windows.UI.Composition or the host backdrop is not available.
    pub fn new(
        hwnd: HWND,
        swap_chain: &IDXGISwapChain1,
        size: u32,
        clip: &ID2D1Geometry,
    ) -> Result<Glass> {
        ensure_dispatcher_queue()?;
        set_host_backdrop(hwnd, true)?;
        let glass = Self::compose(hwnd, swap_chain, size, clip);
        if glass.is_err() {
            let _ = set_host_backdrop(hwnd, false);
        }
        glass
    }

    fn compose(
        hwnd: HWND,
        swap_chain: &IDXGISwapChain1,
        size: u32,
        clip: &ID2D1Geometry,
    ) -> Result<Glass> {
        let compositor = Compositor::new()?;
        let desktop: ICompositorDesktopInterop = compositor.cast()?;
        // SAFETY: `hwnd` is a live window of this thread.
        let target = unsafe { desktop.CreateDesktopWindowTarget(hwnd, true)? };
        match Self::build(&compositor, &target, swap_chain, size, clip) {
            Ok(shape) => Ok(Glass {
                hwnd,
                compositor,
                target,
                shape,
            }),
            Err(e) => {
                // A window can have only one composition target.
                let _ = target.Close();
                let _ = compositor.Close();
                Err(e)
            }
        }
    }

    fn build(
        compositor: &Compositor,
        target: &DesktopWindowTarget,
        swap_chain: &IDXGISwapChain1,
        size: u32,
        clip: &ID2D1Geometry,
    ) -> Result<CompositionPathGeometry> {
        let side = Vector2 {
            X: size as f32,
            Y: size as f32,
        };
        let root = compositor.CreateContainerVisual()?;
        root.SetSize(side)?;
        target.SetRoot(&root)?;
        let children = root.Children()?;

        let shape = compositor.CreatePathGeometryWithPath(&composition_path(clip)?)?;
        let blur = compositor.CreateSpriteVisual()?;
        blur.SetSize(side)?;
        blur.SetBrush(&compositor.CreateHostBackdropBrush()?)?;
        blur.SetClip(&compositor.CreateGeometricClipWithGeometry(&shape)?)?;
        children.InsertAtTop(&blur)?;

        let interop: ICompositorInterop = compositor.cast()?;
        // SAFETY: `swap_chain` is a composition swap chain.
        let surface = unsafe { interop.CreateCompositionSurfaceForSwapChain(swap_chain)? };
        let content = compositor.CreateSpriteVisual()?;
        content.SetSize(side)?;
        content.SetBrush(&compositor.CreateSurfaceBrushWithSurface(&surface)?)?;
        children.InsertAtTop(&content)?;
        Ok(shape)
    }

    /// Changes the blur shape.
    pub fn set_clip(&self, clip: &ID2D1Geometry) -> Result<()> {
        self.shape.SetPath(&composition_path(clip)?)
    }
}

impl Drop for Glass {
    fn drop(&mut self) {
        // The window is shown by another target next time; release this one now.
        let _ = self.target.Close();
        let _ = self.compositor.Close();
        let _ = set_host_backdrop(self.hwnd, false);
    }
}
