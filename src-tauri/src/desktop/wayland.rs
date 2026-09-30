//! Wayland (wlroots) 实现：gtk-layer-shell 全屏透明覆盖层 + input region 控制。
//!
//! 核心机制（对应 spec §7）：
//!   - Passive：input region 只保留右缘编辑热区，其余点击落到下层桌面/应用。
//!   - Editing：input region 为整个屏幕，键盘交互性设为 Exclusive。
//!
//! input region 无法通过 gtk-layer-shell 直接设置，这里在 gdk 同一条
//! wayland 连接上用 libwayland 的原始 FFI 调用：
//!   wl_compositor_create_region + wl_region_add + wl_surface_set_input_region。

use crate::desktop::surface::{DesktopSurface, Rect, ScreenSize, SurfaceError, SurfaceMode};
use gtk::gdk::prelude::*;
use gtk::prelude::*;
use gtk_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use libc::c_void;
use std::cell::RefCell;
use std::rc::Rc;

/// 编辑热区尺寸（表面/物理坐标）。
///
/// 前端 handle 用 CSS 相对定位（right:0 / top 由 handle_y 决定）天然跟随 DPR；
/// 热区要覆盖展开后的编辑按钮（约 130 CSS px 宽），同时尽量小以少占用桌面点击。
/// 热区垂直居中于 handle_y 位置。
pub const HOTZONE_WIDTH: i32 = 200;
pub const HOTZONE_HEIGHT: i32 = 190;

/// libwayland / gdk-wayland 原始 FFI。
///
/// gdk-3 与 gtk 使用同一条 wayland 连接，`gdk_wayland_*` 返回的连接对象上的
/// `wl_*` 调用必须发生在 GTK 主线程 —— 这正是本模块的使用前提。
///
/// 注意：`wl_compositor_create_region` / `wl_region_add` / `wl_surface_set_input_region`
/// 等在 libwayland 头文件里是 `static inline`，**不导出**。这里改用真正导出的
/// `wl_proxy_marshal_array_flags` 手动封装请求（opcode 取自 wayland.xml）。
/// 链接 `-lwayland-client` 由 build.rs 放到链接行末尾（避免 --as-needed 提前丢弃）。
#[allow(clippy::missing_safety_doc)]
mod ffi {
    use libc::{c_char, c_void};

    /// libwayland 的 `union wl_argument`。
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub union WlArgument {
        pub i: i32,
        pub u: u32,
        pub f: i32,
        pub s: *const c_char,
        pub o: *mut c_void,
        pub n: u32,
        pub a: *mut c_void,
        pub h: i32,
    }

    extern "C" {
        // gdk-wayland：从 gdk 窗口/显示拿 wl_surface / wl_compositor / wl_display
        pub fn gdk_wayland_window_get_wl_surface(window: *mut c_void) -> *mut c_void;
        pub fn gdk_wayland_display_get_wl_compositor(display: *mut c_void) -> *mut c_void;
        pub fn gdk_wayland_display_get_wl_display(display: *mut c_void) -> *mut c_void;

        // libwayland-client 导出的底层封装函数
        pub fn wl_proxy_marshal_array_flags(
            proxy: *mut c_void,
            opcode: u32,
            interface: *const c_void,
            version: u32,
            flags: u32,
            args: *const WlArgument,
        ) -> *mut c_void;
        pub fn wl_proxy_get_version(proxy: *mut c_void) -> u32;
        pub fn wl_proxy_destroy(proxy: *mut c_void);
        pub fn wl_display_flush(display: *mut c_void) -> i32;
        pub fn wl_display_roundtrip(display: *mut c_void) -> i32;

        // 核心协议接口描述符（数据符号）
        pub static wl_region_interface: c_void;
    }
}

// 核心协议 opcode（源自 /usr/share/wayland/wayland.xml）
const OP_COMPOSITOR_CREATE_REGION: u32 = 1;
const OP_REGION_DESTROY: u32 = 0;
const OP_REGION_ADD: u32 = 1;
const OP_SURFACE_SET_INPUT_REGION: u32 = 5;
const OP_SURFACE_COMMIT: u32 = 6;

pub struct WaylandSurface {
    window: gtk::Window,
    wl_surface: *mut c_void,
    compositor: *mut c_void,
    /// gdk 连接的 wl_display，用于 set_input_region 后显式 flush。
    wl_display: *mut c_void,
    /// 当前 surface 尺寸（表面坐标），随窗口 resize 更新。
    size: Rc<RefCell<ScreenSize>>,
    mode: Rc<RefCell<SurfaceMode>>,
    /// 编辑入口的垂直位置（0..1，占屏高比例），passive 热区与 Done 跟随它。
    handle_y: Rc<RefCell<f32>>,
}

impl WaylandSurface {
    /// 包装一个已 realize 的 gtk-layer-shell 窗口，并把 surface 初始化为 Passive。
    pub fn new(window: gtk::Window) -> Result<Self, SurfaceError> {
        let gdk_window = window.window().ok_or(SurfaceError::NoGdkWindow)?;
        let display = gdk_window.display();

        let wl_surface =
            unsafe { ffi::gdk_wayland_window_get_wl_surface(gdk_window.as_ptr() as *mut c_void) };
        let compositor =
            unsafe { ffi::gdk_wayland_display_get_wl_compositor(display.as_ptr() as *mut c_void) };
        let wl_display =
            unsafe { ffi::gdk_wayland_display_get_wl_display(display.as_ptr() as *mut c_void) };
        if wl_surface.is_null() || compositor.is_null() || wl_display.is_null() {
            return Err(SurfaceError::NotWayland);
        }

        let alloc = window.allocation();
        let size = Rc::new(RefCell::new(ScreenSize {
            width: alloc.width() as u32,
            height: alloc.height() as u32,
        }));
        let mode = Rc::new(RefCell::new(SurfaceMode::Passive));
        let handle_y = Rc::new(RefCell::new(0.5));

        let surface = Self {
            window,
            wl_surface,
            compositor,
            wl_display,
            size,
            mode,
            handle_y,
        };
        surface.connect_resize_handler();
        surface.enter_passive()?;
        Ok(surface)
    }

    /// 窗口尺寸变化时更新 surface 尺寸，并按当前模式重设 input region。
    fn connect_resize_handler(&self) {
        let wl_surface = self.wl_surface;
        let compositor = self.compositor;
        let wl_display = self.wl_display;
        let size = self.size.clone();
        let mode = self.mode.clone();
        let handle_y = self.handle_y.clone();
        self.window.connect_size_allocate(move |_win, alloc| {
            *size.borrow_mut() = ScreenSize {
                width: alloc.width() as u32,
                height: alloc.height() as u32,
            };
            let rect = input_rect_for(*size.borrow(), *mode.borrow(), *handle_y.borrow());
            let _ = set_input_region_ffi(wl_surface, compositor, rect);
            flush_display(wl_display);
            tracing::debug!(?alloc, "input region re-applied on resize");
        });
    }

    fn apply_current_region(&self) -> Result<(), SurfaceError> {
        let rect = input_rect_for(
            *self.size.borrow(),
            *self.mode.borrow(),
            *self.handle_y.borrow(),
        );
        set_input_region_ffi(self.wl_surface, self.compositor, rect)?;
        flush_display(self.wl_display);
        tracing::debug!(?rect, "input region applied");
        Ok(())
    }

    /// 设置编辑入口的垂直位置（0..1），并立即重算 passive 热区。
    ///
    /// 仅 flush 可能在当前 IPC 回调里不生效（连接处于读取/派发中间态），
    /// 因此延后一次 `wl_display_roundtrip`，确保 compositor 真正应用 input region。
    pub fn set_handle_y(&self, y: f32) {
        let y = y.clamp(0.02, 0.98);
        *self.handle_y.borrow_mut() = y;
        if *self.mode.borrow() == SurfaceMode::Passive {
            let _ = self.apply_current_region();
        }
        let wl_display = self.wl_display as usize;
        glib::MainContext::default().invoke(move || unsafe {
            ffi::wl_display_roundtrip(wl_display as *mut c_void);
        });
        tracing::info!(y, "handle position set");
    }

    /// 当前编辑入口垂直位置（0..1）。
    pub fn handle_y(&self) -> f32 {
        *self.handle_y.borrow()
    }

    fn set_keyboard_mode(&self, mode: KeyboardMode) {
        self.window.set_keyboard_mode(mode);
    }

    /// 当前画布所在 layer（"overlay" 顶层 / "background" 壁纸层）。
    pub fn layer(&self) -> &'static str {
        match self.window.layer() {
            Layer::Overlay => "overlay",
            Layer::Background => "background",
            _ => "overlay",
        }
    }

    /// 切换画布 layer：overlay=所有窗口之上，background=壁纸之上/所有窗口之下。
    pub fn set_layer_str(&self, layer: &str) {
        let layer = match layer {
            "background" => Layer::Background,
            _ => Layer::Overlay,
        };
        self.window.set_layer(layer);
        flush_display(self.wl_display);
        tracing::info!(?layer, "canvas layer switched");
    }
}

impl DesktopSurface for WaylandSurface {
    fn enter_passive(&self) -> Result<(), SurfaceError> {
        *self.mode.borrow_mut() = SurfaceMode::Passive;
        self.set_keyboard_mode(KeyboardMode::None);
        self.apply_current_region()?;
        tracing::info!("enter passive");
        Ok(())
    }

    fn enter_editing(&self) -> Result<(), SurfaceError> {
        *self.mode.borrow_mut() = SurfaceMode::Editing;
        self.set_keyboard_mode(KeyboardMode::Exclusive);
        self.apply_current_region()?;
        tracing::info!("enter editing");
        Ok(())
    }

    fn mode(&self) -> SurfaceMode {
        *self.mode.borrow()
    }

    fn screen_size(&self) -> ScreenSize {
        *self.size.borrow()
    }

    fn handle_rect(&self) -> Rect {
        input_rect_for(
            *self.size.borrow(),
            SurfaceMode::Passive,
            *self.handle_y.borrow(),
        )
    }

    fn layer(&self) -> &'static str {
        self.layer()
    }

    fn set_layer_str(&self, layer: &str) {
        self.set_layer_str(layer);
    }

    fn handle_y(&self) -> f32 {
        self.handle_y()
    }

    fn set_handle_y(&self, y: f32) {
        self.set_handle_y(y);
    }
}

/// 根据 surface 尺寸、模式与 handle 位置计算 input region。
fn input_rect_for(size: ScreenSize, mode: SurfaceMode, handle_y: f32) -> Rect {
    match mode {
        SurfaceMode::Editing => Rect {
            x: 0,
            y: 0,
            width: size.width as i32,
            height: size.height as i32,
        },
        SurfaceMode::Passive => {
            // 热区垂直居中于 handle_y 位置
            let cy = (handle_y * size.height as f32) as i32;
            Rect {
                x: size.width as i32 - HOTZONE_WIDTH,
                y: cy - HOTZONE_HEIGHT / 2,
                width: HOTZONE_WIDTH,
                height: HOTZONE_HEIGHT,
            }
        }
    }
}

/// 设置 input region 的 FFI 封装：
///   region = wl_compositor.create_region(); region.add(x,y,w,h);
///   surface.set_input_region(region); region.destroy();
/// 用 wl_proxy_marshal_array_flags 手工封装（new_id 槽填 0 表示分配新 id）。
fn set_input_region_ffi(
    wl_surface: *mut c_void,
    compositor: *mut c_void,
    rect: Rect,
) -> Result<(), SurfaceError> {
    unsafe {
        let new_id = [ffi::WlArgument { n: 0 }];
        let region = ffi::wl_proxy_marshal_array_flags(
            compositor,
            OP_COMPOSITOR_CREATE_REGION,
            std::ptr::addr_of!(ffi::wl_region_interface).cast(),
            ffi::wl_proxy_get_version(compositor),
            0,
            new_id.as_ptr(),
        );
        if region.is_null() {
            return Err(SurfaceError::NullRegion);
        }

        let rect_args = [
            ffi::WlArgument { i: rect.x },
            ffi::WlArgument { i: rect.y },
            ffi::WlArgument { i: rect.width },
            ffi::WlArgument { i: rect.height },
        ];
        ffi::wl_proxy_marshal_array_flags(
            region,
            OP_REGION_ADD,
            std::ptr::null(),
            0,
            0,
            rect_args.as_ptr(),
        );

        let obj_arg = [ffi::WlArgument { o: region }];
        ffi::wl_proxy_marshal_array_flags(
            wl_surface,
            OP_SURFACE_SET_INPUT_REGION,
            std::ptr::null(),
            0,
            0,
            obj_arg.as_ptr(),
        );

        // wl_surface.set_input_region 在**下一次 commit 才生效**（协议规定）。
        // 没有新渲染时 webview 不会自动 commit，这里必须手动 commit 应用 pending 状态。
        ffi::wl_proxy_marshal_array_flags(
            wl_surface,
            OP_SURFACE_COMMIT,
            std::ptr::null(),
            0,
            0,
            std::ptr::null(),
        );

        ffi::wl_proxy_marshal_array_flags(
            region,
            OP_REGION_DESTROY,
            std::ptr::null(),
            0,
            0,
            std::ptr::null(),
        );
        ffi::wl_proxy_destroy(region);
    }
    Ok(())
}

/// 把排队中的 wayland 请求立即发给 compositor。
fn flush_display(wl_display: *mut c_void) {
    unsafe {
        ffi::wl_display_flush(wl_display);
    }
}

/// 创建一个全屏透明覆盖窗口（gtk-layer-shell overlay，四边锚定充满输出）。
pub fn create_layer_window() -> Result<gtk::Window, SurfaceError> {
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_decorated(false);
    window.set_app_paintable(true);
    window.set_title("inklet");
    window.set_visual(
        gtk::gdk::Screen::default()
            .and_then(|s| s.rgba_visual())
            .as_ref(),
    );

    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_namespace("inklet");
    for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        window.set_anchor(edge, true);
    }
    window.set_keyboard_mode(KeyboardMode::None);

    window.show_all();
    window.realize();
    Ok(window)
}
