//! 平台桌面覆盖层。
//!
//! Linux：Wayland (wlroots) 实现。
//! macOS：Canvas Window + Control Window（ignoresMouseEvents）。

pub mod surface;

#[cfg(target_os = "linux")]
pub mod wayland;

#[cfg(target_os = "macos")]
pub mod macos;
