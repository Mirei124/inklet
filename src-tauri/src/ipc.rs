//! IPC 桥接：把 webview 发来的 JSON 命令分发给 commands，
//! 通过 evaluate_javascript 回调 `window.__dc_ipc_reply(id, ok, payload)` 回包。

use crate::commands;
use crate::desktop::surface::DesktopSurface;
use crate::desktop::wayland::WaylandSurface;
use gtk::prelude::*;
use serde_json::{json, Value};
use std::rc::Rc;
use webkit2gtk::{WebView, WebViewExt};

pub struct AppContext {
    pub surface: Rc<WaylandSurface>,
    pub webview: WebView,
}

pub fn handle(ctx: &AppContext, message: &str) {
    let parsed: Value = match serde_json::from_str(message) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("bad ipc message: {e}");
            return;
        }
    };
    let id = parsed.get("id").and_then(Value::as_u64).unwrap_or(0);
    let cmd = parsed.get("cmd").and_then(Value::as_str).unwrap_or("");
    let args = parsed.get("args").cloned().unwrap_or_else(|| json!({}));
    tracing::debug!(cmd, "ipc command");

    let result = dispatch(cmd, &args, ctx.surface.as_ref());

    // 进入编辑模式时确保 webview 拿到 GTK 焦点，键盘输入（文字/快捷键）才生效
    if cmd == "enter_edit_mode" {
        ctx.webview.grab_focus();
    }

    // payload 以 JS 字符串字面量注入（JSON 对象直接裸拼会解析成块语句）：
    //  - 成功：payload_text = 结果的 JSON 文本，前端 JSON.parse(payload) 还原
    //  - 失败：payload_text = 错误消息文本，前端直接作为 Error 消息
    let (ok, payload_text) = match &result {
        Ok(v) => (
            true,
            serde_json::to_string(v).unwrap_or_else(|_| "null".into()),
        ),
        Err(e) => (false, e.to_string()),
    };
    let payload_literal = serde_json::to_string(&payload_text).unwrap_or_else(|_| "\"\"".into());
    let js = format!("window.__dc_ipc_reply({id}, {ok}, {payload_literal});");
    ctx.webview
        .evaluate_javascript(&js, None, None, None::<&gio::Cancellable>, |_| {});
}

fn dispatch(cmd: &str, args: &Value, surface: &WaylandSurface) -> Result<Value, String> {
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
            let storage = crate::storage::Storage::default();
            let settings = json!({ "version": 1, "layer": layer });
            if let Err(e) = storage.save_settings(&settings.to_string()) {
                tracing::warn!("failed to save settings: {e}");
            }
            Ok(json!({ "layer": layer }))
        }
        other => Err(format!("unknown command: {other}")),
    }
}
