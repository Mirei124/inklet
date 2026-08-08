//! 应用装配：创建 gtk-layer-shell 覆盖窗口 + webkit webview + IPC 桥接，
//! 然后进入 GTK 主循环。

use crate::desktop::surface::DesktopSurface;
use crate::desktop::wayland::{self, WaylandSurface};
use crate::ipc::{self, AppContext};
use gtk::prelude::*;
#[cfg(debug_assertions)]
use std::path::PathBuf;
use std::rc::Rc;
#[cfg(debug_assertions)]
use webkit2gtk::SettingsExt;
use webkit2gtk::{
    URISchemeRequestExt, URISchemeResponseExt, UserContentManagerExt, WebContextExt, WebViewExt,
};

const DEV_SERVER_URL: &str = "http://localhost:1420";
const APP_SCHEME: &str = "dc";
const APP_ENTRY: &str = "dc://app/index.html";

// 生产构建（release）：把 dist/ 整个目录嵌入二进制，运行时从内存提供前端资源，
// 使二进制自包含（不需要外部 dist 目录）。
#[cfg(not(debug_assertions))]
use include_dir::{include_dir, Dir};
#[cfg(not(debug_assertions))]
static DIST: Dir = include_dir!("$CARGO_MANIFEST_DIR/../dist");

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();
    tracing::info!("application start");

    gtk::init()?;

    // 1. 全屏透明覆盖窗口（gtk-layer-shell overlay）
    let window = wayland::create_layer_window()?;

    // 2. Wayland surface：input region + 键盘模式，初始为 Passive
    let surface = Rc::new(WaylandSurface::new(window.clone())?);
    tracing::info!("wayland surface initialized, mode = {:?}", surface.mode());

    // 恢复上次保存的 canvas layer 与 handle 位置
    let storage = crate::storage::Storage::default();
    if let Ok(Some(json)) = storage.load_settings() {
        if let Ok(settings) = serde_json::from_str::<serde_json::Value>(&json) {
            if let Some(layer) = settings.get("layer").and_then(serde_json::Value::as_str) {
                surface.set_layer_str(layer);
            }
            if let Some(y) = settings.get("handleY").and_then(serde_json::Value::as_f64) {
                surface.set_handle_y(y as f32);
            }
        }
    }

    // 3. webview（透明背景）
    let web_context =
        webkit2gtk::WebContext::default().ok_or("failed to get default webkit WebContext")?;
    if !dev_mode() {
        register_custom_protocol(&web_context)?;
    }

    let user_content = webkit2gtk::UserContentManager::new();
    // 前端 bridge 通过 window.webkit.messageHandlers.ipc.postMessage 调起
    user_content.register_script_message_handler("ipc");

    let webview = webkit2gtk::WebView::builder()
        .web_context(&web_context)
        .user_content_manager(&user_content)
        .build();
    webview.set_background_color(&gtk::gdk::RGBA::new(0.0, 0.0, 0.0, 0.0));
    // 调试：允许 webkit inspector（WEBKIT_INSPECTOR_SERVER 可远程调试）
    #[cfg(debug_assertions)]
    if let Some(settings) = WebViewExt::settings(&webview) {
        settings.set_enable_developer_extras(true);
    }

    // 4. IPC 桥接（webkit 的 script message 回调在 GTK 主线程执行，无需 Send）
    let state = Rc::new(AppContext {
        surface: surface.clone(),
        webview: webview.clone(),
    });
    user_content.connect_script_message_received(None, move |_m, msg| {
        if let Some(js) = msg.js_value() {
            ipc::handle(&state, &js.to_string());
        }
    });

    // 5. 挂进覆盖窗口并加载前端
    window.add(&webview);
    window.show_all();
    webview.grab_focus();

    // 观察 layer-shell 表面尺寸变化序列
    window.connect_size_allocate(|_win, alloc| {
        tracing::debug!(?alloc, "window size-allocate");
    });
    webview.connect_size_allocate(|_wv, alloc| {
        tracing::debug!(?alloc, "webview size-allocate");
    });

    if dev_mode() {
        let url = if std::env::var_os("DC_AUTOEDIT").is_some() {
            format!("{DEV_SERVER_URL}/?autoedit=1")
        } else {
            DEV_SERVER_URL.to_string()
        };
        tracing::info!(url = %url, "loading dev server");
        webview.load_uri(&url);
    } else {
        tracing::info!(url = APP_ENTRY, "loading packaged assets");
        webview.load_uri(APP_ENTRY);
    }

    gtk::main();
    Ok(())
}

fn init_tracing() {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("desktop_canvas=debug,warn"));
    let _ = fmt().with_env_filter(filter).finish().try_init();
}

/// `--dev` 参数或 `DC_DEV` 环境变量时加载 vite dev server。
fn dev_mode() -> bool {
    std::env::args().any(|a| a == "--dev") || std::env::var_os("DC_DEV").is_some()
}

/// 生产模式：注册 `dc` scheme。release 构建从嵌入的 dist 提供资源（自包含二进制），
/// debug 构建从 `../dist` 磁盘目录读取（便于开发）。
fn register_custom_protocol(
    web_context: &webkit2gtk::WebContext,
) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(debug_assertions)]
    {
        let dist = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../dist");
        tracing::info!(dist = %dist.display(), "serving assets from dist (debug)");
    }
    #[cfg(not(debug_assertions))]
    tracing::info!("serving embedded assets from dist (release)");

    web_context.register_uri_scheme(APP_SCHEME, move |request| {
        // path() 返回 URI 的路径部分：dc://app/index.html -> /index.html
        if let Some((bytes, mime)) = serve_file(request.path().as_deref()) {
            let stream = gtk::gio::MemoryInputStream::from_bytes(&glib::Bytes::from(&bytes));
            let response = webkit2gtk::URISchemeResponse::new(&stream, bytes.len() as i64);
            response.set_content_type(mime);
            request.finish_with_response(&response);
        } else {
            let mut err = glib::Error::new(gtk::gio::IOErrorEnum::NotFound, "not found");
            request.finish_error(&mut err);
        }
    });
    Ok(())
}

/// 解析前端资源路径，返回 (字节, MIME)。
fn serve_file(path: Option<&str>) -> Option<(Vec<u8>, &'static str)> {
    let rel = normalize_rel(path);
    // 目录请求（如 "/" 或 "/assets/"）-> 尝试 index.html
    let rel = if rel.is_empty() {
        "index.html".to_string()
    } else if rel.ends_with('/') {
        format!("{rel}index.html")
    } else {
        rel
    };
    let rel = rel.trim_start_matches('/');

    #[cfg(not(debug_assertions))]
    {
        let file = DIST.get_file(rel)?;
        let mime = mime_for(rel.rsplit('.').next().unwrap_or("html"));
        Some((file.contents().to_vec(), mime))
    }
    #[cfg(debug_assertions)]
    {
        let dist = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../dist");
        let full = dist.join(rel);
        let canonical = full.canonicalize().ok()?;
        if !canonical.starts_with(&dist) {
            return None; // 防路径穿越
        }
        let bytes = std::fs::read(&canonical).ok()?;
        let mime = mime_for(
            canonical
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("html"),
        );
        Some((bytes, mime))
    }
}

fn normalize_rel(path: Option<&str>) -> String {
    path.unwrap_or("/index.html")
        .trim_start_matches('/')
        .to_string()
}

fn mime_for(ext: &str) -> &'static str {
    match ext {
        "html" | "htm" => "text/html",
        "js" | "mjs" => "application/javascript",
        "css" => "text/css",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "wasm" => "application/wasm",
        "map" => "application/json",
        _ => "application/octet-stream",
    }
}
