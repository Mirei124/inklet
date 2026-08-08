//! macOS (AppKit / WKWebView)：tao 窗口 + wry + objc2。

use super::{dev_mode, restore_surface_settings, DEV_SERVER_URL};
use crate::desktop::macos::MacSurface;
use crate::ipc;
use serde_json::Value;
use std::cell::RefCell;
use std::rc::Rc;
use tao::event_loop::{ControlFlow, EventLoop};

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
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
            // 解析并分发 IPC 命令（parse_message/dispatch/reply_js 与 Linux 共用）
            if let Some(ref surface) = *sh.borrow() {
                let (id, cmd, args) = ipc::parse_message(&msg);
                if cmd.is_empty() {
                    return;
                }

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

    // 恢复设置（layer/handleY），并同步 Control 窗口语言
    let lang = restore_surface_settings(surface.as_ref()).unwrap_or_else(|| "zh".to_string());
    surface.set_control_lang(&lang);

    // ── 事件循环 ───────────────────────────────────────────────
    let surface_ref = surface.clone();
    let mut dock_hidden = false;
    event_loop.run(move |event, _, control_flow| {
        // 首次进入事件循环时隐藏 Dock：tao 的 run 启动时会激活 NSApp 为
        // Regular（窗口创建前的设置会被覆盖），此时 NSApp 已完全启动，
        // 设置 Accessory 不会再被重置。
        if !dock_hidden {
            dock_hidden = true;
            if let Some(mtm) = objc2_foundation::MainThreadMarker::new() {
                let app = objc2_app_kit::NSApplication::sharedApplication(mtm);
                app.setActivationPolicy(objc2_app_kit::NSApplicationActivationPolicy::Accessory);
            }
        }
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
