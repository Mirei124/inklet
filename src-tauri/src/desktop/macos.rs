//! macOS 实现：Canvas Window（borderless/transparent/fullscreen WKWebView）
//! + Control Overlay（浮动编辑入口）。
//!
//! 核心机制（对应 spec §6）：
//!   - Passive：Canvas ignoresMouseEvents=true，Control Overlay 始终可交互。
//!   - Editing：Canvas ignoresMouseEvents=false，Control Overlay 隐藏。
//!
//! 技术栈：tao（窗口）+ wry（WKWebView）+ objc2（NSWindow 属性控制）。

use crate::desktop::surface::{DesktopSurface, Rect, ScreenSize, SurfaceError, SurfaceMode};
use objc2_app_kit::{NSWindow, NSWindowCollectionBehavior, NSWindowLevel};
use std::cell::RefCell;
use std::rc::Rc;
use tao::window::Window;

/// 编辑热区尺寸（CSS 像素）。
const HOTZONE_WIDTH: i32 = 200;
const HOTZONE_HEIGHT: i32 = 150;

/// NSWindow level 常量（macOS CGWindowLevel 对应值）。
/// overlay：kCGStatusWindowLevel = 21（在普通/浮动窗口之上，用于覆盖层）。
/// background：低于普通窗口（0），但高于桌面图标，用于"壁纸之上、所有窗口之下"。
const OVERLAY_LEVEL: NSWindowLevel = 21;
const BACKGROUND_LEVEL: NSWindowLevel = -1;

/// 控制面板尺寸。
const CONTROL_WIDTH: f64 = 130.0;
const CONTROL_HEIGHT: f64 = 150.0;

/// 控制窗口内联 HTML：右缘竖条（悬停展开操作按钮、拖拽定位），
/// 走 wry IPC (`window.ipc.postMessage`)。
const CONTROL_HTML: &str = r#"<!DOCTYPE html><html><head><meta charset="utf-8"><style>
*{margin:0;padding:0;box-sizing:border-box}
html,body{background:transparent;height:100%;overflow:hidden;user-select:none}
body{font-family:-apple-system,system-ui,sans-serif;display:flex;
align-items:center;justify-content:flex-end;height:100vh}
.wrap{display:flex;align-items:center;gap:8px;padding-right:6px}
/* 竖条：折叠态唯一可见的交互元素 */
.bar{width:8px;height:34px;border:1px solid rgba(255,255,255,0.35);
background:rgba(0,0,0,0.45);border-radius:17px;cursor:grab;flex-shrink:0}
.bar:active{cursor:grabbing}
/* 展开的按钮面板 */
.panel{display:none;flex-direction:column;gap:6px;align-items:center;min-width:92px}
.panel.show{display:flex}
.btn{border:none;border-radius:14px;color:#fff;font-size:13px;font-weight:500;
cursor:pointer;padding:7px 12px;width:100%;background:rgba(37,99,235,0.92);
box-shadow:0 1px 6px rgba(0,0,0,0.35);white-space:nowrap}
.btn:hover{background:rgba(29,78,216,0.95)}
.bar:focus-visible,.btn:focus-visible{outline:2px solid #fff;outline-offset:2px}
</style></head><body>
<div class="wrap" id="wrap">
  <div class="panel" id="panel">
    <button class="btn" id="editBtn" onclick="enterEdit()">&#9998; Edit</button>
    <button class="btn" id="layerBtn" onclick="toggleLayer()">&#8693; Top</button>
    <button class="btn" id="visibilityBtn" onclick="toggleVisibility()">&#9673; Hide</button>
  </div>
  <button class="bar" id="bar" aria-label="Hover to expand · drag to move" aria-expanded="false" aria-controls="panel"></button>
</div>
<script>
var currentY = 0.5;          // 当前 handleY（0..1），native 启动时注入
var layer = 'overlay';
var canvasVisible = true;
var LANG = 'en';             // native 启动时注入（settings.json 的 lang）
// 与前端 src/i18n.ts 文案保持一致
var T = {
  en: { edit: '&#9998; Edit', top: '&#8693; Top', back: '&#8693; Back', hide: '&#9673; Hide', show: '&#9673; Show', bar: 'Hover to expand · drag to move' },
  zh: { edit: '&#9998; &#32534;&#36753;', top: '&#8693; &#32622;&#39030;', back: '&#8693; &#32622;&#24213;', hide: '&#9673; &#38544;&#34255;', show: '&#9673; &#26174;&#31034;', bar: '悬停展开 · 拖拽移动' }
};
function send(cmd, args) {
  window.ipc.postMessage(JSON.stringify({id: Math.floor(Math.random()*1e9), cmd: cmd, args: args||{}}));
}
function applyLang() {
  var t = T[LANG] || T.en;
  document.getElementById('editBtn').innerHTML = t.edit;
  document.getElementById('bar').setAttribute('aria-label', t.bar);
  toggleLabel();
  visibilityLabel();
}
function setLang(l) { LANG = (l === 'zh') ? 'zh' : 'en'; applyLang(); }
function toggleLayer() {
  layer = (layer === 'overlay') ? 'background' : 'overlay';
  send('set_canvas_layer', {layer: layer});
  toggleLabel();
  closePanel();
}
function enterEdit() {
  canvasVisible = true;
  visibilityLabel();
  closePanel();
  send('enter_edit_mode', {});
}
function toggleVisibility() {
  canvasVisible = !canvasVisible;
  send('set_canvas_visibility', {visible: canvasVisible});
  visibilityLabel();
  closePanel();
}
function visibilityLabel() {
  var t = T[LANG] || T.en;
  document.getElementById('visibilityBtn').innerHTML = canvasVisible ? t.hide : t.show;
}
function setLayerFromNative(l) { layer = l; }
function toggleLabel() {
  var t = T[LANG] || T.en;
  document.getElementById('layerBtn').innerHTML =
    (layer === 'overlay') ? t.back : t.top;
}
applyLang(); // 首帧按默认语言渲染
// 拖拽竖条：沿屏幕右缘移动，按 handleY 比例换算
var dragging = false, moved = false, startScreenY = 0, startHandleY = 0.5;
var bar = document.getElementById('bar');
var panel = document.getElementById('panel');
var wrap = document.getElementById('wrap');
var suppressHover = false;
function openPanel() { if (!suppressHover) { panel.classList.add('show'); bar.setAttribute('aria-expanded', 'true'); } }
function closePanel() { panel.classList.remove('show'); bar.setAttribute('aria-expanded', 'false'); suppressHover = true; bar.focus(); }
wrap.addEventListener('pointerenter', openPanel);
wrap.addEventListener('pointerleave', function() { suppressHover = false; panel.classList.remove('show'); bar.setAttribute('aria-expanded', 'false'); });
bar.addEventListener('pointerdown', function(e) {
  dragging = true;
  moved = false;
  startScreenY = e.screenY;
  startHandleY = currentY;
  bar.setPointerCapture(e.pointerId);
});
bar.addEventListener('pointermove', function(e) {
  if (!dragging) return;
  if (Math.abs(e.screenY - startScreenY) > 6) moved = true;
  if (!moved) return;
  var h = window.screen.height || 1000;
  var dy = (e.screenY - startScreenY) / h;
  var ny = Math.min(0.98, Math.max(0.02, startHandleY + dy));
  currentY = ny;
  send('set_handle_position', {y: ny});
});
function endDrag() { dragging = false; }
bar.addEventListener('pointerup', endDrag);
bar.addEventListener('pointercancel', endDrag);
// 键盘也可展开/收起
bar.addEventListener('click', function() {
  if (moved) { moved = false; return; }
  if (panel.classList.contains('show')) { closePanel(); }
  else { suppressHover = false; openPanel(); }
});
</script>
</body></html>"#;

/// macOS 画布覆盖层。
///
/// 两个窗口：
/// - Canvas：tao 全屏透明窗口 + wry WKWebView（渲染 React app）
/// - Control：tao 小型浮动窗口 + 内联 HTML（编辑入口按钮）
pub struct MacSurface {
    pub canvas_window: Window,
    canvas_wv: RefCell<wry::WebView>,
    control_window: RefCell<Option<Window>>,
    control_wv: RefCell<Option<wry::WebView>>,
    ns_window: *mut NSWindow,
    size: RefCell<ScreenSize>,
    mode: RefCell<SurfaceMode>,
    handle_y: RefCell<f32>,
    layer: RefCell<String>,
}

impl MacSurface {
    /// 创建 macOS 覆盖层。
    ///
    /// - `event_loop`: 已创建的 tao EventLoop
    /// - `load_canvas`: 给 Canvas webview 加载 URL 或 HTML
    /// - `on_ipc`: 共享 IPC 回调（Canvas 和 Control 共用）
    pub fn new(
        event_loop: &tao::event_loop::EventLoop<()>,
        load_canvas: impl FnOnce(&wry::WebView),
        on_ipc: impl Fn(String) + 'static,
    ) -> Result<Rc<Self>, SurfaceError> {
        // 所有 AppKit 调用必须在主线程
        let mtm = objc2_foundation::MainThreadMarker::new().ok_or(SurfaceError::NotAvailable)?;
        // on_ipc 需要被 Canvas 和 Control 两个 webview 共享，包进 Rc
        let on_ipc = Rc::new(on_ipc);
        // ── Canvas Window ──────────────────────────────────────
        // 注意：不用 tao 的 with_fullscreen（native fullscreen 会重置 NSWindow level）。
        // 改为 borderless 窗口 + 手动 setFrame 铺满屏幕 + 高 level。
        let canvas_window = tao::window::WindowBuilder::new()
            .with_decorations(false)
            .with_transparent(true)
            .build(event_loop)
            .map_err(|e| SurfaceError::Cocoa(format!("canvas window: {e}")))?;

        let ns_window = get_ns_window_ptr(&canvas_window)?;
        unsafe {
            (*ns_window).setOpaque(false);
            (*ns_window).setHasShadow(false);
            // 关键：传 nil (None) 不会清空背景，必须用 clearColor 才能透明
            let clear = objc2_app_kit::NSColor::clearColor();
            (*ns_window).setBackgroundColor(Some(&clear));
            (*ns_window).setIgnoresMouseEvents(true);
            (*ns_window).setLevel(OVERLAY_LEVEL);
            (*ns_window).setCollectionBehavior(
                // MoveToActiveSpace：窗口跟随当前活跃桌面（不切到其他桌面）
                // FullScreenNone：明确不进全屏 Space，避免被自动移走
                NSWindowCollectionBehavior::MoveToActiveSpace
                    | NSWindowCollectionBehavior::Stationary
                    | NSWindowCollectionBehavior::IgnoresCycle
                    | NSWindowCollectionBehavior::FullScreenNone,
            );
            // 铺满主屏幕，但避开菜单栏/Dock（visibleFrame），
            // 否则 Excalidraw 顶部工具栏会被 macOS 菜单栏遮挡。
            // 菜单栏 layer 高于画布，无法靠 level 盖住，必须让窗口从菜单栏下方开始。
            if let Some(screen) = objc2_app_kit::NSScreen::mainScreen(mtm) {
                let frame = screen.visibleFrame();
                (*ns_window).setFrame_display(frame, false);
            }
        }

        let canvas_wv = RefCell::new(
            wry::WebViewBuilder::new_with_web_context(&mut wry::WebContext::new(Some(
                std::path::PathBuf::from("inklet-canvas"),
            )))
            .with_transparent(true)
            .with_initialization_script("window.__dc_platform = 'macos';")
            .with_custom_protocol("inklet".into(), serve_dist)
            .with_ipc_handler({
                let on_ipc = Rc::clone(&on_ipc);
                move |req| on_ipc(req.body().to_string())
            })
            .build(&canvas_window)
            .map_err(|e| SurfaceError::Cocoa(format!("canvas webview: {e}")))?,
        );

        load_canvas(&canvas_wv.borrow());

        // 统一用逻辑尺寸（CSS 像素/points，与前端 viewport 一致）。
        // tao inner_size() 返回物理像素，需除以 scale_factor。
        let scale = canvas_window.scale_factor();
        let alloc = canvas_window.inner_size();
        let size = RefCell::new(ScreenSize {
            width: (alloc.width as f64 / scale) as u32,
            height: (alloc.height as f64 / scale) as u32,
        });

        // ── Control Window ─────────────────────────────────────
        let (control_window, control_wv) = create_control_panel(event_loop, on_ipc)?;
        // 注入初始 handleY 到 Control 内联页（拖拽起点）
        let _ = control_wv.evaluate_script("window.currentY = 0.5;");
        let control_window = RefCell::new(Some(control_window));

        // 注意：隐藏 Dock（NSApplicationActivationPolicy::Accessory）不在这里设置。
        // tao 在窗口创建 / 事件循环 run 时会把 NSApp 激活策略重置为 Regular，
        // 提前设置会被覆盖。真正的设置放在 app.rs 事件循环首次回调里
        // （此时 NSApp 已完全启动，不会再被重置）。
        let this = Rc::new(Self {
            canvas_window,
            canvas_wv,
            control_window,
            control_wv: RefCell::new(Some(control_wv)),
            ns_window,
            size,
            mode: RefCell::new(SurfaceMode::Passive),
            handle_y: RefCell::new(0.5),
            layer: RefCell::new("overlay".into()),
        });

        Ok(this)
    }

    /// 在 Canvas webview 中执行 JavaScript。
    pub fn eval(&self, js: &str) {
        let _ = self.canvas_wv.borrow().evaluate_script(js);
    }

    /// Canvas 窗口尺寸变化时调用（tao Resized 事件给物理像素，这里转逻辑）。
    pub fn update_size(&self, width: u32, height: u32) {
        let scale = self.canvas_window.scale_factor();
        *self.size.borrow_mut() = ScreenSize {
            width: (width as f64 / scale) as u32,
            height: (height as f64 / scale) as u32,
        };
        if *self.mode.borrow() == SurfaceMode::Passive {
            self.reposition_control();
        }
    }

    fn reposition_control(&self) {
        let hy = *self.handle_y.borrow();
        let sz = *self.size.borrow();
        let x = (sz.width as f64 - CONTROL_WIDTH).max(0.0);
        let cy = (hy * sz.height as f32) as f64;
        let y = (cy - CONTROL_HEIGHT / 2.0)
            .max(0.0)
            .min((sz.height as f64 - CONTROL_HEIGHT).max(0.0));

        if let Some(ref win) = *self.control_window.borrow() {
            win.set_outer_position(tao::dpi::LogicalPosition::new(x, y));
        }
        // 同步 handleY 到 Control 内联页（拖拽起点基准）
        if let Some(ref wv) = *self.control_wv.borrow() {
            let _ = wv.evaluate_script(&format!("window.currentY = {hy};"));
        }
    }

    /// 同步 Control 窗口语言（设置/切换 lang 时调用）。
    pub fn set_control_lang(&self, lang: &str) {
        if let Some(ref wv) = *self.control_wv.borrow() {
            let _ = wv.evaluate_script(&format!("setLang('{lang}');"));
        }
        tracing::info!(lang, "control lang synced");
    }

    fn show_control(&self, visible: bool) {
        if let Some(ref win) = *self.control_window.borrow() {
            win.set_visible(visible);
        }
    }

    /// 通知 Canvas 前端模式已切换（window.__dc_sync_mode）。
    /// Control 窗口通过裸 postMessage 触发切换时，前端 invoke 没有 pending，
    /// reply 会丢失；这里直接推送给前端，保证 React mode 状态正确。
    fn sync_frontend_mode(&self, mode: &str) {
        self.eval(&format!(
            "window.__dc_sync_mode && window.__dc_sync_mode('{mode}');"
        ));
    }

    /// # Safety
    /// 必须在主线程调用。
    unsafe fn ns(&self) -> &NSWindow {
        unsafe { &*self.ns_window }
    }
}

impl DesktopSurface for MacSurface {
    fn enter_passive(&self) -> Result<(), SurfaceError> {
        *self.mode.borrow_mut() = SurfaceMode::Passive;
        unsafe {
            self.ns().setIgnoresMouseEvents(true);
        }
        self.show_control(true);
        self.reposition_control();
        // 主动同步前端 mode（Control 窗口触发的切换不走前端 invoke，reply 无人接收）
        self.sync_frontend_mode("passive");
        tracing::info!("macos enter passive");
        Ok(())
    }

    fn enter_editing(&self) -> Result<(), SurfaceError> {
        *self.mode.borrow_mut() = SurfaceMode::Editing;
        unsafe {
            self.ns().setIgnoresMouseEvents(false);
        }
        self.show_control(false);
        // 主动同步前端 mode
        self.sync_frontend_mode("editing");
        tracing::info!("macos enter editing");
        Ok(())
    }

    fn mode(&self) -> SurfaceMode {
        *self.mode.borrow()
    }

    fn screen_size(&self) -> ScreenSize {
        *self.size.borrow()
    }

    fn handle_rect(&self) -> Rect {
        let sz = *self.size.borrow();
        let hy = *self.handle_y.borrow();
        let cy = (hy * sz.height as f32) as i32;
        Rect {
            x: sz.width as i32 - HOTZONE_WIDTH,
            y: cy - HOTZONE_HEIGHT / 2,
            width: HOTZONE_WIDTH,
            height: HOTZONE_HEIGHT,
        }
    }

    fn layer(&self) -> &'static str {
        if *self.layer.borrow() == "background" {
            "background"
        } else {
            "overlay"
        }
    }

    fn set_layer_str(&self, layer: &str) {
        let l = match layer {
            "background" => "background",
            _ => "overlay",
        };
        *self.layer.borrow_mut() = l.to_string();
        let level = if l == "background" {
            BACKGROUND_LEVEL
        } else {
            OVERLAY_LEVEL
        };
        unsafe {
            self.ns().setLevel(level);
            // Control 窗口跟随 Canvas 层级（置底时 Control 也应沉到壁纸层）
            if let Some(ref ctrl) = *self.control_window.borrow() {
                if let Ok(ctrl_ns) = get_ns_window_ptr(ctrl) {
                    (*ctrl_ns).setLevel(level + 1);
                }
            }
        }
        // 同步 layer 到 Control 按钮标签
        if let Some(ref wv) = *self.control_wv.borrow() {
            let _ = wv.evaluate_script(&format!("setLayerFromNative('{l}'); toggleLabel();"));
        }
        tracing::info!(layer = l, "macos canvas layer switched");
    }

    fn handle_y(&self) -> f32 {
        *self.handle_y.borrow()
    }

    fn set_handle_y(&self, y: f32) {
        let y = y.clamp(0.02, 0.98);
        *self.handle_y.borrow_mut() = y;
        if *self.mode.borrow() == SurfaceMode::Passive {
            self.reposition_control();
        }
        tracing::info!(y, "macos handle position set");
    }
}

// ── helpers ────────────────────────────────────────────────────

/// wry custom protocol handler：从嵌入的 DIST 提供 `inklet://` 资源。
///
/// `inklet://index.html` → dist/index.html；`inklet://assets/xxx.js` → dist/assets/xxx.js。
/// 请求路径直接映射到 dist 目录内的相对文件。
fn serve_dist(
    _webview_id: wry::WebViewId,
    request: wry::http::Request<Vec<u8>>,
) -> wry::http::Response<std::borrow::Cow<'static, [u8]>> {
    use wry::http::header::{HeaderValue, CONTENT_TYPE};
    use wry::http::{Response, StatusCode};

    // URI 形如 inklet://index.html 或 inklet://assets/xxx.js
    let uri_path = request.uri().path().to_string();

    let not_found = Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(std::borrow::Cow::Borrowed(&b"not found"[..]))
        .unwrap();

    let Some((bytes, mime)) = crate::app::resolve_asset(&uri_path) else {
        tracing::warn!(path = %uri_path, "inklet:// resource not found");
        return not_found;
    };

    let mut builder = Response::builder().status(StatusCode::OK);
    if let Ok(hv) = HeaderValue::from_str(mime) {
        builder = builder.header(CONTENT_TYPE, hv);
    }
    builder.body(std::borrow::Cow::Borrowed(bytes)).unwrap()
}

/// 从 tao Window 获取 NSWindow 裸指针。
fn get_ns_window_ptr(window: &Window) -> Result<*mut NSWindow, SurfaceError> {
    use tao::platform::macos::WindowExtMacOS;
    let ptr = window.ns_window();
    if ptr.is_null() {
        Err(SurfaceError::Cocoa("ns_window is null".into()))
    } else {
        Ok(ptr as *mut NSWindow)
    }
}

/// 创建浮动控制窗口（小 tao 窗口 + 内联 HTML wry webview）。
fn create_control_panel(
    event_loop: &tao::event_loop::EventLoop<()>,
    on_ipc: Rc<impl Fn(String) + 'static>,
) -> Result<(Window, wry::WebView), SurfaceError> {
    use tao::dpi::{LogicalPosition, LogicalSize};
    use tao::window::WindowBuilder;

    let window = WindowBuilder::new()
        .with_decorations(false)
        .with_transparent(true)
        // always_on_top set via NSWindow level below
        .with_resizable(false)
        .with_focused(false)
        .with_inner_size(LogicalSize::new(CONTROL_WIDTH, CONTROL_HEIGHT))
        .with_position(LogicalPosition::new(100.0, 400.0))
        .build(event_loop)
        .map_err(|e| SurfaceError::Cocoa(format!("control window: {e}")))?;

    let ns_ctrl = get_ns_window_ptr(&window)?;
    unsafe {
        (*ns_ctrl).setHasShadow(false);
        let clear = objc2_app_kit::NSColor::clearColor();
        (*ns_ctrl).setBackgroundColor(Some(&clear));
        (*ns_ctrl).setLevel(OVERLAY_LEVEL + 1);
        (*ns_ctrl).setCollectionBehavior(
            NSWindowCollectionBehavior::MoveToActiveSpace
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::IgnoresCycle
                | NSWindowCollectionBehavior::FullScreenNone,
        );
    }

    let wv = wry::WebViewBuilder::new_with_web_context(&mut wry::WebContext::new(Some(
        std::path::PathBuf::from("inklet-control"),
    )))
    .with_transparent(true)
    .with_html(CONTROL_HTML)
    .with_ipc_handler(move |req| on_ipc(req.body().to_string()))
    .build(&window)
    .map_err(|e| SurfaceError::Cocoa(format!("control webview: {e}")))?;

    Ok((window, wv))
}
