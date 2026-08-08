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

/// 编辑热区尺寸（CSS 像素），与前端 EditHandle 对齐。
pub const HOTZONE_WIDTH: i32 = 44;
pub const HOTZONE_HEIGHT: i32 = 44;

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
        // gdk-wayland：从 gdk 窗口/显示拿 wl_surface / wl_compositor
        pub fn gdk_wayland_window_get_wl_surface(window: *mut c_void) -> *mut c_void;
        pub fn gdk_wayland_display_get_wl_compositor(display: *mut c_void) -> *mut c_void;

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

        // 核心协议接口描述符（数据符号）
        pub static wl_region_interface: c_void;
    }
}

// 核心协议 opcode（源自 /usr/share/wayland/wayland.xml）
const OP_COMPOSITOR_CREATE_REGION: u32 = 1;
const OP_REGION_DESTROY: u32 = 0;
const OP_REGION_ADD: u32 = 1;
const OP_SURFACE_SET_INPUT_REGION: u32 = 5;

pub struct WaylandSurface {
    window: gtk::Window,
    wl_surface: *mut c_void,
    compositor: *mut c_void,
    size: ScreenSize,
    mode: RefCell<SurfaceMode>,
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
        if wl_surface.is_null() || compositor.is_null() {
            return Err(SurfaceError::NotWayland);
        }

        let size = output_size(&gdk_window);
        let surface = Self {
            window,
            wl_surface,
            compositor,
            size,
            mode: RefCell::new(SurfaceMode::Passive),
        };
        surface.enter_passive()?;
        Ok(surface)
    }

    fn set_input_region(&self, rect: Rect) -> Result<(), SurfaceError> {
        // 等价于：
        //   region = wl_compositor.create_region()
        //   region.add(x, y, w, h)
        //   surface.set_input_region(region)
        //   region.destroy()
        // 用 wl_proxy_marshal_array_flags 手工封装（new_id 槽填 0 表示分配新 id）。
        unsafe {
            // wl_compositor.create_region -> 返回新 region proxy
            let new_id = [ffi::WlArgument { n: 0 }];
            let region = ffi::wl_proxy_marshal_array_flags(
                self.compositor,
                OP_COMPOSITOR_CREATE_REGION,
                std::ptr::addr_of!(ffi::wl_region_interface).cast(),
                ffi::wl_proxy_get_version(self.compositor),
                0,
                new_id.as_ptr(),
            );
            if region.is_null() {
                return Err(SurfaceError::NullRegion);
            }

            // wl_region.add(x, y, w, h)
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

            // wl_surface.set_input_region(region)
            let obj_arg = [ffi::WlArgument { o: region }];
            ffi::wl_proxy_marshal_array_flags(
                self.wl_surface,
                OP_SURFACE_SET_INPUT_REGION,
                std::ptr::null(),
                0,
                0,
                obj_arg.as_ptr(),
            );

            // wl_region.destroy() + 释放 proxy
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

    fn set_keyboard_mode(&self, mode: KeyboardMode) {
        self.window.set_keyboard_mode(mode);
    }
}

impl DesktopSurface for WaylandSurface {
    fn enter_passive(&self) -> Result<(), SurfaceError> {
        self.set_input_region(self.handle_rect())?;
        self.set_keyboard_mode(KeyboardMode::None);
        *self.mode.borrow_mut() = SurfaceMode::Passive;
        tracing::info!("enter passive (input region = edit handle)");
        Ok(())
    }

    fn enter_editing(&self) -> Result<(), SurfaceError> {
        self.set_input_region(Rect {
            x: 0,
            y: 0,
            width: self.size.width as i32,
            height: self.size.height as i32,
        })?;
        self.set_keyboard_mode(KeyboardMode::Exclusive);
        *self.mode.borrow_mut() = SurfaceMode::Editing;
        tracing::info!("enter editing (input region = fullscreen)");
        Ok(())
    }

    fn mode(&self) -> SurfaceMode {
        *self.mode.borrow()
    }

    fn screen_size(&self) -> ScreenSize {
        self.size
    }

    fn handle_rect(&self) -> Rect {
        Rect {
            x: self.size.width as i32 - HOTZONE_WIDTH,
            y: (self.size.height as i32 - HOTZONE_HEIGHT) / 2,
            width: HOTZONE_WIDTH,
            height: HOTZONE_HEIGHT,
        }
    }
}

/// 创建一个全屏透明覆盖窗口（gtk-layer-shell overlay，四边锚定充满输出）。
pub fn create_layer_window() -> Result<gtk::Window, SurfaceError> {
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_decorated(false);
    window.set_app_paintable(true);
    window.set_title("desktop-canvas");
    window.set_visual(
        gtk::gdk::Screen::default()
            .and_then(|s| s.rgba_visual())
            .as_ref(),
    );

    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_namespace("desktop-canvas");
    for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        window.set_anchor(edge, true);
    }
    window.set_keyboard_mode(KeyboardMode::None);

    window.show_all();
    window.realize();
    Ok(window)
}

fn output_size(gdk_window: &gtk::gdk::Window) -> ScreenSize {
    // layer-shell 四边锚定后 surface 尺寸 = 输出尺寸；用 monitor geometry 更可靠。
    if let Some(monitor) = gdk_window.display().monitor_at_window(gdk_window) {
        let geo = monitor.geometry();
        return ScreenSize {
            width: geo.width() as u32,
            height: geo.height() as u32,
        };
    }
    let (_x, _y, w, h) = gdk_window.geometry();
    ScreenSize {
        width: w as u32,
        height: h as u32,
    }
}
