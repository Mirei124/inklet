//! DesktopSurface 抽象。
//!
//! React 只关心“画什么、当前是否 editing”，平台差异
//! （layer-shell / input region / NSWindow / ignoresMouseEvents）
//! 全部收敛在 `DesktopSurface` 实现里。macOS 后续实现时只需
//! 新增一个实现了该 trait 的类型。

use std::fmt;

/// 应用的两个 UI 状态（Hover 是前端视觉子状态，不影响原生输入）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceMode {
    Passive,
    Editing,
}

/// 屏幕坐标矩形（与 webview 视口同一坐标系）。
#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Copy)]
pub struct ScreenSize {
    pub width: u32,
    pub height: u32,
}

/// 桌面覆盖层接口。
pub trait DesktopSurface {
    fn enter_passive(&self) -> Result<(), SurfaceError>;
    fn enter_editing(&self) -> Result<(), SurfaceError>;
    /// macOS 路径当前不通过 dispatch 调用，保留供平台内部/调试使用。
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    fn mode(&self) -> SurfaceMode;
    fn screen_size(&self) -> ScreenSize;
    /// passive 模式下仅接收鼠标的热区（编辑入口）。
    fn handle_rect(&self) -> Rect;
    /// 当前画布所在 layer："overlay"（所有窗口之上）或 "background"（壁纸之上）。
    fn layer(&self) -> &'static str;
    /// 切换画布 layer。
    fn set_layer_str(&self, layer: &str);
    /// 编辑入口的垂直位置（0..1，占屏高比例）。
    fn handle_y(&self) -> f32;
    /// 设置编辑入口垂直位置并更新对应原生 hot zone。
    fn set_handle_y(&self, y: f32);
}

#[derive(Debug)]
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub enum SurfaceError {
    /// 当前不是支持的平台（gdk 拿不到 wl_surface，或窗口未初始化）。
    NotWayland,
    NoGdkWindow,
    NullRegion,
    /// 预留：后续平台（如 macOS 窗口初始化）的错误。
    #[allow(dead_code)]
    Wayland(String),
    /// 预留：GTK 相关错误。
    #[allow(dead_code)]
    Gtk(String),
    /// 平台窗口未就绪（macOS / 通用）。
    #[allow(dead_code)]
    NotAvailable,
    /// macOS Cocoa/AppKit 相关错误。
    #[allow(dead_code)]
    Cocoa(String),
}

impl fmt::Display for SurfaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SurfaceError::NotWayland => write!(f, "not running on Wayland (no wl_surface)"),
            SurfaceError::NoGdkWindow => write!(f, "GTK window has no GDK window (not realized)"),
            SurfaceError::NullRegion => write!(f, "failed to create wl_region"),
            SurfaceError::Wayland(msg) => write!(f, "wayland error: {msg}"),
            SurfaceError::Gtk(msg) => write!(f, "gtk error: {msg}"),
            SurfaceError::NotAvailable => write!(f, "platform window not available"),
            SurfaceError::Cocoa(msg) => write!(f, "cocoa error: {msg}"),
        }
    }
}

impl std::error::Error for SurfaceError {}
