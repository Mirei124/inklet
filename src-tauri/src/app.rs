//! 应用装配：平台窗口 + webview + IPC 桥接，然后进入主循环。
//!
//! Linux：gtk-layer-shell + WebKitGTK
//! macOS：tao + wry (WKWebView)

// ── Linux imports ────────────────────────────────────────────
#[cfg(target_os = "linux")]
use crate::desktop::wayland::{self, WaylandSurface};
#[cfg(target_os = "linux")]
use crate::ipc::AppContext;
#[cfg(target_os = "linux")]
use crate::desktop::surface::DesktopSurface;
#[cfg(target_os = "linux")]
use gtk::prelude::*;
#[cfg(target_os = "linux")]
use std::rc::Rc;
#[cfg(all(target_os = "linux", debug_assertions))]
use webkit2gtk::SettingsExt;
#[cfg(target_os = "linux")]
use webkit2gtk::{
    URISchemeRequestExt, URISchemeResponseExt, UserContentManagerExt, WebContextExt, WebViewExt,
};

#[cfg(all(debug_assertions, target_os = "linux"))]
use std::path::PathBuf;

const DEV_SERVER_URL: &str = "http://localhost:1420";
#[cfg(target_os = "linux")]
const APP_SCHEME: &str = "dc";
#[cfg(target_os = "linux")]
const APP_ENTRY: &str = "dc://app/index.html";

// 生产构建（release）：把 dist/ 整个目录嵌入二进制，运行时从内存提供前端资源，
// 使二进制自包含（不需要外部 dist 目录）。
// Linux 用自定义协议；macOS 用 wry 内联 HTML。
use include_dir::{include_dir, Dir};
static DIST: Dir = include_dir!("$CARGO_MANIFEST_DIR/../dist");

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();
    tracing::info!("application start");

    #[cfg(target_os = "linux")]
    {
        run_linux()
    }
    #[cfg(target_os = "macos")]
    {
        run_macos()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Err("unsupported platform".into())
    }
}

// ═══════════════════════════════════════════════════════════════
// Linux (Wayland / GTK)
// ═══════════════════════════════════════════════════════════════

#[cfg(target_os = "linux")]
fn run_linux() -> Result<(), Box<dyn std::error::Error>> {
    use crate::desktop::surface::DesktopSurface;
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

// ═══════════════════════════════════════════════════════════════
// macOS (AppKit / WKWebView)
// ═══════════════════════════════════════════════════════════════

#[cfg(target_os = "macos")]
fn run_macos() -> Result<(), Box<dyn std::error::Error>> {
    use crate::desktop::macos::MacSurface;
    use crate::desktop::surface::DesktopSurface;
    use crate::ipc;
    use crate::storage::Storage;
    use serde_json::{json, Value};
    use std::cell::RefCell;
    use std::rc::Rc;
    use tao::event_loop::{ControlFlow, EventLoop};

    let event_loop = EventLoop::new();

    // ── 创建 MacSurface ────────────────────────────────────────

    // IPC handler：Canvas 和 Control 的 webview 共用
    // 注意：必须在 surface 创建后才能接收 IPC 消息。这里用 Option 延迟绑定。
    let surface_holder: Rc<RefCell<Option<Rc<MacSurface>>>> = Rc::new(RefCell::new(None));

    let sh = surface_holder.clone();
    let surface = MacSurface::new(
        &event_loop,
        |wv| {
            if dev_mode() {
                let url = if std::env::var_os("DC_AUTOEDIT").is_some() {
                    format!("{DEV_SERVER_URL}/?autoedit=1")
                } else {
                    DEV_SERVER_URL.to_string()
                };
                tracing::info!(url = %url, "loading dev server");
                wv.load_url(&url).ok();
            } else {
                // 生产模式加载内嵌 dist
                tracing::info!("loading packaged assets from inline HTML");
                load_embedded_app(wv);
            }
        },
        move |msg| {
            // 解析并分发 IPC 命令
            if let Some(ref surface) = *sh.borrow() {
                let parsed: Value = match serde_json::from_str(&msg) {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!("bad ipc message: {e}");
                        return;
                    }
                };
                let id = parsed.get("id").and_then(Value::as_u64).unwrap_or(0);
                let cmd = parsed
                    .get("cmd")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let args = parsed.get("args").cloned().unwrap_or_else(|| json!({}));

                let result = ipc::dispatch(&cmd, &args, surface.as_ref());

                if cmd == "enter_edit_mode" {
                    // 确保 Canvas 获得键盘焦点
                    surface.eval("window.focus();");
                }
                if cmd == "set_lang" {
                    // 同步 Control 窗口按钮语言
                    if let Some(l) = args.get("lang").and_then(Value::as_str) {
                        surface.set_control_lang(l);
                    }
                }

                let js = ipc::reply_js(id, &result);
                surface.eval(&js);
            }
        },
    )?;

    *surface_holder.borrow_mut() = Some(surface.clone());

    // 恢复设置
    let storage = Storage::default();
    if let Ok(Some(settings_json)) = storage.load_settings() {
        if let Ok(settings) = serde_json::from_str::<Value>(&settings_json) {
            if let Some(layer) = settings.get("layer").and_then(Value::as_str) {
                surface.set_layer_str(layer);
            }
            if let Some(y) = settings.get("handleY").and_then(Value::as_f64) {
                surface.set_handle_y(y as f32);
            }
            // 恢复语言并同步 Control 按钮
            let lang = settings
                .get("lang")
                .and_then(Value::as_str)
                .unwrap_or("zh");
            surface.set_control_lang(lang);
        }
    }

    // ── 事件循环 ───────────────────────────────────────────────
    let surface_ref = surface.clone();
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            tao::event::Event::WindowEvent {
                event: tao::event::WindowEvent::Resized(size),
                window_id,
                ..
            } => {
                // Canvas 窗口 resize → 更新尺寸 + 重定位 Control
                if window_id == surface_ref.canvas_window.id() {
                    surface_ref.update_size(size.width, size.height);
                }
            }
            tao::event::Event::WindowEvent {
                event: tao::event::WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    });

    // event_loop.run() 返回 never，永不返回
    #[allow(unreachable_code)]
    Ok(())
}

/// 生产模式：从嵌入 dist 加载前端（wry custom protocol `inklet://`）。
#[cfg(target_os = "macos")]
fn load_embedded_app(wv: &wry::WebView) {
    // serve_dist 处理 inklet://index.html 及其相对资源 ./assets/xxx.js
    // （vite 生产构建 base 是 "./"，资源路径解析到 inklet://assets/...）
    let url = if std::env::var_os("DC_AUTOEDIT").is_some() {
        "inklet://index.html?autoedit=1"
    } else {
        "inklet://index.html"
    };
    let _ = wv.load_url(url);
}

fn init_tracing() {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("inklet=debug,warn"));
    let _ = fmt().with_env_filter(filter).finish().try_init();
}

/// `--dev` 参数或 `DC_DEV` 环境变量时加载 vite dev server。
fn dev_mode() -> bool {
    std::env::args().any(|a| a == "--dev") || std::env::var_os("DC_DEV").is_some()
}

/// 生产模式：注册 `dc` scheme（Linux/WebKitGTK）。
#[cfg(target_os = "linux")]
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

/// 解析前端资源路径，返回 (字节, MIME)。（Linux WebKitGTK custom protocol）
#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
fn normalize_rel(path: Option<&str>) -> String {
    path.unwrap_or("/index.html")
        .trim_start_matches('/')
        .to_string()
}

/// 根据扩展名返回 MIME（Linux custom protocol 与 macOS wry protocol 共用）。
pub fn mime_for(ext: &str) -> &'static str {
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

/// 从嵌入的 DIST 读取资源文件，返回文件字节（macOS wry protocol 用）。
#[cfg(target_os = "macos")]
pub fn dist_file(rel: &str) -> Option<&'static [u8]> {
    DIST.get_file(rel).map(|f| f.contents())
}
