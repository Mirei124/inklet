//! Linux (Wayland / GTK)：gtk-layer-shell 覆盖窗口 + WebKitGTK。

use super::assets::mime_for;
use super::{dev_mode, dev_url, restore_surface_settings, resolve_asset};
use crate::desktop::wayland::{self, WaylandSurface};
use crate::ipc;
use crate::ipc::AppContext;
use gtk::prelude::*;
use std::rc::Rc;
#[cfg(all(debug_assertions, target_os = "linux"))]
use std::path::PathBuf;
#[cfg(debug_assertions)]
use webkit2gtk::SettingsExt;
use webkit2gtk::{
    URISchemeRequestExt, URISchemeResponseExt, UserContentManagerExt, WebContextExt, WebViewExt,
};

const APP_SCHEME: &str = "dc";
const APP_ENTRY: &str = "dc://app/index.html";

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    gtk::init()?;

    // 1. 全屏透明覆盖窗口（gtk-layer-shell overlay）
    let window = wayland::create_layer_window()?;

    // 2. Wayland surface：input region + 键盘模式，初始为 Passive
    let surface = Rc::new(WaylandSurface::new(window.clone())?);
    tracing::info!("wayland surface initialized, mode = {:?}", surface.mode());

    // 恢复上次保存的 canvas layer 与 handle 位置
    restore_surface_settings(surface.as_ref());

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
        let url = dev_url();
        tracing::info!(url = %url, "loading dev server");
        webview.load_uri(&url);
    } else {
        tracing::info!(url = APP_ENTRY, "loading packaged assets");
        webview.load_uri(APP_ENTRY);
    }

    gtk::main();
    Ok(())
}

/// 生产模式：注册 `dc` scheme（Linux/WebKitGTK）。
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

/// 解析前端资源路径，返回 (字节, MIME)。Linux WebKitGTK custom protocol。
fn serve_file(path: Option<&str>) -> Option<(Vec<u8>, &'static str)> {
    let raw = path.unwrap_or("/index.html");
    #[cfg(debug_assertions)]
    {
        // 开发时从磁盘读（无需先 build dist），并防路径穿越
        let mut rel = raw.trim_start_matches('/').to_string();
        if rel.is_empty() || rel.ends_with('/') {
            rel.push_str("index.html");
        }
        let dist = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../dist");
        let canonical = dist.join(rel).canonicalize().ok()?;
        if !canonical.starts_with(&dist) {
            return None;
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
    #[cfg(not(debug_assertions))]
    {
        let (bytes, mime) = resolve_asset(raw)?;
        Some((bytes.to_vec(), mime))
    }
}
