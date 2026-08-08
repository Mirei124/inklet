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
    fn mode(&self) -> SurfaceMode;
    fn screen_size(&self) -> ScreenSize;
    /// passive 模式下仅接收鼠标的热区（编辑入口）。
    fn handle_rect(&self) -> Rect;
}

#[derive(Debug)]
pub enum SurfaceError {
    /// 当前不是 Wayland（gdk 拿不到 wl_surface），或窗口未 realize。
    NotWayland,
    NoGdkWindow,
    NullRegion,
    /// 预留：后续平台（如 macOS 窗口初始化）的错误。
    #[allow(dead_code)]
    Wayland(String),
    /// 预留：GTK 相关错误。
    #[allow(dead_code)]
    Gtk(String),
}

impl fmt::Display for SurfaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SurfaceError::NotWayland => write!(f, "not running on Wayland (no wl_surface)"),
            SurfaceError::NoGdkWindow => write!(f, "GTK window has no GDK window (not realized)"),
            SurfaceError::NullRegion => write!(f, "failed to create wl_region"),
            SurfaceError::Wayland(msg) => write!(f, "wayland error: {msg}"),
            SurfaceError::Gtk(msg) => write!(f, "gtk error: {msg}"),
        }
    }
}

impl std::error::Error for SurfaceError {}
