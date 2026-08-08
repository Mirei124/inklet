//! 应用装配：平台窗口 + webview + IPC 桥接，然后进入主循环。
//!
//! 模块组织：
//! - `assets`：跨平台 dist 资源服务（Linux `dc://` / macOS `inklet://` 共用）
//! - `linux`：gtk-layer-shell + WebKitGTK 实现
//! - `macos`：tao + wry (WKWebView) 实现
//! - 本文件：入口分派 + 共享工具（日志、dev 判断、settings 恢复）

pub mod assets;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

pub use assets::resolve_asset;

/// vite dev server 地址（`pnpm dev` 启动）。
pub(crate) const DEV_SERVER_URL: &str = "http://localhost:1420";

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();
    tracing::info!("application start");

    #[cfg(target_os = "linux")]
    {
        linux::run()
    }
    #[cfg(target_os = "macos")]
    {
        macos::run()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Err("unsupported platform".into())
    }
}

/// 从 settings.json 恢复 surface 状态（layer / handleY），跨平台共用。
/// 返回 lang（若有），供 macOS 同步 Control 窗口语言。
pub(crate) fn restore_surface_settings(
    surface: &dyn crate::desktop::surface::DesktopSurface,
) -> Option<String> {
    let storage = crate::storage::Storage::default();
    let json = storage.load_settings().ok().flatten()?;
    let settings: serde_json::Value = serde_json::from_str(&json).ok()?;
    if let Some(layer) = settings.get("layer").and_then(serde_json::Value::as_str) {
        surface.set_layer_str(layer);
    }
    if let Some(y) = settings.get("handleY").and_then(serde_json::Value::as_f64) {
        surface.set_handle_y(y as f32);
    }
    settings
        .get("lang")
        .and_then(serde_json::Value::as_str)
        .map(String::from)
}

fn init_tracing() {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};
    // 开发构建默认打印 debug（便于排查）；release 构建只打 warn，
    // 避免常规日志刷屏。仍可用 RUST_LOG 环境变量覆盖。
    #[cfg(debug_assertions)]
    let default_filter = "inklet=debug,warn";
    #[cfg(not(debug_assertions))]
    let default_filter = "inklet=warn";
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));
    let _ = fmt().with_env_filter(filter).finish().try_init();
}

/// `--dev` 参数或 `DC_DEV` 环境变量时加载 vite dev server。
pub(crate) fn dev_mode() -> bool {
    std::env::args().any(|a| a == "--dev") || std::env::var_os("DC_DEV").is_some()
}

/// 构建 vite dev server URL（--dev 模式加载；`DC_AUTOEDIT` 时自动进编辑模式）。
/// Linux 与 macOS 共用。
pub(crate) fn dev_url() -> String {
    if std::env::var_os("DC_AUTOEDIT").is_some() {
        format!("{DEV_SERVER_URL}/?autoedit=1")
    } else {
        DEV_SERVER_URL.to_string()
    }
}
