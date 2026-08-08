//! 前后端命令处理：模式切换 + scene/settings 存取。
//!
//! 前端只发送 `enter_edit_mode` / `exit_edit_mode` 等语义命令，
//! 不感知平台细节。

use crate::storage::Storage;
use serde_json::{json, Value};

/// 加载 scene.json。文件缺失或损坏时回退为空 scene（不崩溃，保留原文件）。
pub fn load_scene() -> Result<Value, String> {
    let storage = Storage::default();
    match storage.load_scene() {
        Ok(Some(json)) => {
            tracing::info!("scene loaded");
            Ok(json!({ "json": json }))
        }
        Ok(None) => Ok(json!({ "json": Value::Null })),
        Err(e) => {
            tracing::warn!("scene load failed, using empty scene: {e}");
            Ok(json!({ "json": Value::Null }))
        }
    }
}

/// 原子保存 scene.json。参数 `json` 是前端序列化好的完整 scene 内容。
pub fn save_scene(args: &Value) -> Result<Value, String> {
    let json = args
        .get("json")
        .and_then(Value::as_str)
        .ok_or_else(|| "save_scene: missing string field `json`".to_string())?;
    let storage = Storage::default();
    storage.save_scene(json).map_err(|e| e.to_string())?;
    tracing::info!("scene saved");
    Ok(Value::Null)
}
