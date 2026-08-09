//! 跨平台 dist 资源服务。
//!
//! 前端构建产物（dist/）在编译时通过 `include_dir!` 嵌入二进制，运行时按路径
//! 提供资源。Linux `dc://` custom protocol 与 macOS `inklet://` wry protocol 共用。

use include_dir::{include_dir, Dir};

/// 编译时嵌入的 dist 目录（release 自包含；debug 也嵌入，Linux debug 走磁盘读取）。
/// Linux debug 下仅 macOS 使用嵌入资源，故此处按平台放行 dead_code。
#[cfg_attr(all(target_os = "linux", debug_assertions), allow(dead_code))]
static DIST: Dir = include_dir!("$CARGO_MANIFEST_DIR/../dist");

/// 从嵌入的 DIST 解析资源并返回 (字节, MIME)。
///
/// `path` 是 URL 的路径部分（如 `index.html`、`assets/x.js`、空串或 `/assets/`）；
/// 目录请求自动映射到 `index.html`。
#[cfg_attr(all(target_os = "linux", debug_assertions), allow(dead_code))]
pub fn resolve_asset(path: &str) -> Option<(&'static [u8], &'static str)> {
    let mut rel = path.trim_start_matches('/').to_string();
    if rel.is_empty() || rel.ends_with('/') {
        rel.push_str("index.html");
    }
    let file = DIST.get_file(&rel)?;
    let mime = mime_for(rel.rsplit('.').next().unwrap_or("html"));
    Some((file.contents(), mime))
}

/// 根据扩展名返回 MIME（跨平台共用）。
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
