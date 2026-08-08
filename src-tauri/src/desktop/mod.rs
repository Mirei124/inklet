//! 平台桌面覆盖层。
//!
//! 当前阶段只实现 Wayland (wlroots)。macOS 在后续阶段按 spec §6
//! 实现 Canvas Window + Control Window。

pub mod surface;
pub mod wayland;

#[cfg(target_os = "macos")]
pub mod macos;
