//! IPC 桥接：把 webview 发来的 JSON 命令分发给 commands，
//! 通过 evaluate_javascript 回调 `window.__dc_ipc_reply(id, ok, payload)` 回包。

use crate::commands;
use crate::desktop::surface::DesktopSurface;
use crate::desktop::wayland::WaylandSurface;
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

    let (ok, payload) = match &result {
        Ok(v) => (
            true,
            serde_json::to_string(v).unwrap_or_else(|_| "null".into()),
        ),
        Err(e) => (
            false,
            serde_json::to_string(&e.to_string()).unwrap_or_else(|_| "\"error\"".into()),
        ),
    };
    let js = format!("window.__dc_ipc_reply({id}, {ok}, {payload});");
    ctx.webview
        .evaluate_javascript(&js, None, None, None::<&gio::Cancellable>, |_| {});
}

fn dispatch(cmd: &str, args: &Value, surface: &dyn DesktopSurface) -> Result<Value, String> {
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
        other => Err(format!("unknown command: {other}")),
    }
}
