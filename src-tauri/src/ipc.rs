//! IPC 桥接：把 webview 发来的 JSON 命令分发给 commands，
//! 通过 evaluate_javascript 回调 `window.__dc_ipc_reply(id, ok, payload)` 回包。
//!
//! `dispatch()` 是平台无关的命令分发，Linux 和 macOS 共用。
//! `handle()` 是 Linux/WebKitGTK 的入口。

use crate::commands;
use crate::desktop::surface::DesktopSurface;
#[cfg(target_os = "linux")]
use gtk::prelude::*;
use serde_json::{json, Value};
#[cfg(target_os = "linux")]
use std::rc::Rc;
#[cfg(target_os = "linux")]
use webkit2gtk::{WebView, WebViewExt};

/// Linux 侧的 IPC 上下文（macOS 直接持有 MacSurface + 共享 dispatch）。
#[cfg(target_os = "linux")]
pub struct AppContext {
    pub surface: Rc<dyn DesktopSurface>,
    pub webview: WebView,
}

/// 构建 IPC 回包的 JS 代码（供平台调用方 eval）。
pub fn reply_js(id: u64, result: &Result<Value, String>) -> String {
    let (ok, payload_text) = match result {
        Ok(v) => (
            true,
            serde_json::to_string(v).unwrap_or_else(|_| "null".into()),
        ),
        Err(e) => (false, e.to_string()),
    };
    let payload_literal = serde_json::to_string(&payload_text).unwrap_or_else(|_| "\"\"".into());
    format!("window.__dc_ipc_reply({id}, {ok}, {payload_literal});")
}

#[cfg(target_os = "linux")]
pub fn handle(ctx: &AppContext, message: &str) {
    let (id, cmd, args) = parse_message(message);
    if cmd == "quit_app" {
        tracing::info!("quitting from floating toolbar");
        gtk::main_quit();
        return;
    }
    let result = dispatch(&cmd, &args, ctx.surface.as_ref());

    if cmd == "enter_edit_mode" {
        ctx.webview.grab_focus();
    }

    let js = reply_js(id, &result);
    ctx.webview
        .evaluate_javascript(&js, None, None, None::<&gio::Cancellable>, |_| {});
}

/// 平台无关的消息解析：从 `{id, cmd, args}` JSON 提取字段。
/// Linux WebKitGTK 与 macOS wry 共用（ipc_handler 都拿到 body 字符串）。
pub fn parse_message(message: &str) -> (u64, String, Value) {
    let parsed: Value = match serde_json::from_str(message) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("bad ipc message: {e}");
            return (0, String::new(), json!({}));
        }
    };
    let id = parsed.get("id").and_then(Value::as_u64).unwrap_or(0);
    let cmd = parsed
        .get("cmd")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let args = parsed.get("args").cloned().unwrap_or_else(|| json!({}));
    tracing::debug!(cmd = %cmd, "ipc command");
    (id, cmd, args)
}

/// 平台无关的命令分发。Linux 与 macOS 共用。
pub fn dispatch(cmd: &str, args: &Value, surface: &dyn DesktopSurface) -> Result<Value, String> {
    match cmd {
        "enter_edit_mode" => {
            surface.enter_editing().map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "exit_edit_mode" => {
            surface.enter_passive().map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "load_scene" => commands::load_scene(),
        "save_scene" => commands::save_scene(args),
        "log_debug" => commands::log_debug(args),
        "screen_size" => {
            let s = surface.screen_size();
            Ok(json!({ "width": s.width, "height": s.height }))
        }
        "get_handle_rect" => {
            let r = surface.handle_rect();
            Ok(json!({
                "x": r.x,
                "y": r.y,
                "width": r.width,
                "height": r.height
            }))
        }
        "get_canvas_layer" => Ok(json!({ "layer": surface.layer() })),
        "set_canvas_layer" => {
            let layer = args
                .get("layer")
                .and_then(Value::as_str)
                .unwrap_or("overlay");
            surface.set_layer_str(layer);
            let mut settings = read_settings();
            settings["layer"] = json!(layer);
            write_settings(&settings);
            Ok(json!({ "layer": layer }))
        }
        "get_handle_position" => Ok(json!({ "y": surface.handle_y() })),
        "set_handle_position" => {
            let y = args.get("y").and_then(Value::as_f64).unwrap_or(0.5) as f32;
            surface.set_handle_y(y);
            let mut settings = read_settings();
            settings["handleY"] = json!(y);
            write_settings(&settings);
            Ok(json!({ "y": surface.handle_y() }))
        }
        "get_settings" => {
            let s = read_settings();
            Ok(json!({
                "layer": surface.layer(),
                "handleY": surface.handle_y(),
                "lang": s.get("lang").and_then(Value::as_str).unwrap_or("zh"),
            }))
        }
        "set_lang" => {
            let lang = args.get("lang").and_then(Value::as_str).unwrap_or("zh");
            let lang = if lang == "en" { "en" } else { "zh" };
            let mut settings = read_settings();
            settings["lang"] = json!(lang);
            write_settings(&settings);
            Ok(json!({ "lang": lang }))
        }
        other => Err(format!("unknown command: {other}")),
    }
}

/// 读取 settings.json，缺失/损坏时返回默认值。
fn read_settings() -> Value {
    crate::storage::Storage::default()
        .load_settings()
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(
            || json!({ "version": 1, "layer": "overlay", "handleY": 0.5, "lang": "zh" }),
        )
}

/// 写入 settings.json。
fn write_settings(settings: &Value) {
    let storage = crate::storage::Storage::default();
    if let Err(e) = storage.save_settings(&settings.to_string()) {
        tracing::warn!("failed to save settings: {e}");
    }
}
