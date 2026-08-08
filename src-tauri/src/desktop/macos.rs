//! macOS 实现占位（后续阶段）。
//!
//! 目标（spec §6）：Canvas Window（borderless/transparent/fullscreen，
//! passive 时 ignoresMouseEvents=true）+ 独立 Control Window（承载编辑按钮）。
//! 先用 wlroots 跑通，这里只留接口骨架。

use super::surface::{DesktopSurface, Rect, ScreenSize, SurfaceError, SurfaceMode};

pub struct MacSurface;

impl DesktopSurface for MacSurface {
    fn enter_passive(&self) -> Result<(), SurfaceError> {
        Err(SurfaceError::NotWayland)
    }
    fn enter_editing(&self) -> Result<(), SurfaceError> {
        Err(SurfaceError::NotWayland)
    }
    fn mode(&self) -> SurfaceMode {
        SurfaceMode::Passive
    }
    fn screen_size(&self) -> ScreenSize {
        ScreenSize {
            width: 0,
            height: 0,
        }
    }
    fn handle_rect(&self) -> Rect {
        Rect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        }
    }
}
