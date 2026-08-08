# Desktop Canvas 踩坑记录

本文件记录本 MVP 开发过程中踩过的坑与对应的解决方案，按领域分组。
目的是让后续会话/开发者快速了解这些非显而易见的坑，避免重复排查。

---

## 一、Wayland 原生侧

### 1. wry（Tauri 的 WebView 库）在 Linux 只支持 X11
- **现象**：`wry::WebViewBuilder::build()` 在 Linux 走 `new_x11()`，非 X11 直接返回
  `Error::UnsupportedWindowHandle`，无法把 webview 挂进 Wayland layer-shell 窗口。
- **影响**：最初定的 "wry + gtk-layer-shell" 方案不可行。
- **方案**：改用 wry 的底层实现 **`webkit2gtk` 直接嵌入**任何 GTK 容器。
  实现方式几乎相同（`WebView`/`UserContentManager`/`evaluate_javascript`），
  只是 IPC 从 `window.ipc.postMessage` 换成
  `window.webkit.messageHandlers.ipc.postMessage`。

### 2. `wl_compositor_create_region` 等请求函数是 header `static inline`，**不导出**
- **现象**：`nm -D libwayland-client.so.0` 里没有 `wl_compositor_create_region`、
  `wl_region_add`、`wl_surface_set_input_region`、`wl_region_destroy`，
  直接 extern 声明链接报 undefined symbol。
- **原因**：这些核心协议请求函数在 `wayland-client-protocol.h` 里是 `static inline`，
  通过 `wl_proxy_marshal_flags` 内部封装，不导出为库符号。
- **方案**：用真正导出的 `wl_proxy_marshal_array_flags` 手动封装：
  ```rust
  // 等价于：
  //   region = wl_compositor.create_region()          // opcode 1
  //   region.add(x, y, w, h)                          // opcode 1
  //   surface.set_input_region(region)                // opcode 5
  //   region.destroy()                                // opcode 0
  ```
  定义 `#[repr(C)] union WlArgument`（对应 C 的 `union wl_argument`），
  opcode 从 `/usr/share/wayland/wayland.xml` 按声明顺序数（0-based）。
  `new_id` 槽填 `0` 表示由库分配新对象 id。

### 3. `#[link]` 属性导致链接顺序被 `--as-needed` 丢弃
- **现象**：`#[link(name = "wayland-client")]` 生成的 `-lwayland-client` 出现在
  链接行**开头**（rlib 之前），被 `--as-needed` 判定"暂不需要"而丢弃，
  最终 undefined symbol。
- **原因**：rustc 把当前 crate 的 `#[link]` 放在所有依赖 rlib 之前。
- **方案**：用 `build.rs` 把 `-lwayland-client` 放到链接行**末尾**：
  ```rust
  println!("cargo:rustc-link-arg=-Wl,--no-as-needed");
  println!("cargo:rustc-link-arg=-lwayland-client");
  println!("cargo:rustc-link-arg=-Wl,--as-needed");
  ```

### 4. 设置了 layer-shell 请求后必须 `wl_display_flush`
- **现象**：input region 设置了但好像没生效（整屏仍捕获点击）。
- **原因**：marshalled 请求只是写入连接缓冲，GTK 下一轮主循环迭代才 flush；
  若在初始化时设置，可能很久不被发送。
- **方案**：每次 `set_input_region` 后调用导出的 `wl_display_flush(wl_display)`。

### 5. gtk-layer-shell 窗口初始 allocation 是 200×200
- **现象**：`WaylandSurface::new` 时读 `window.allocation()` 得到 200×200，
  compositor 配置后才是真实尺寸（本机 1920×1046）。
- **方案**：不缓存尺寸，改为监听 `connect_size_allocate`，每次 resize 更新
  尺寸并**按当前模式重设 input region**。

### 6. `wl_surface.set_input_region` 在**下一次 commit 才生效**
- **现象**：初始 input region 正常（webview 首帧渲染会自动 commit），
  但拖拽 handle 后热区变了却不生效——点击新位置穿透、旧位置仍可点。
- **原因**：协议规定 input region 请求进入 pending 状态，要等 surface 下一次
  `wl_surface.commit` 才应用。拖拽结束后没有新渲染/commit，新热区一直不生效。
- **方案**：marshalled `set_input_region` 之后**手动 marshal `wl_surface.commit`
  （opcode 6）**强制应用 pending 状态。仅 `wl_display_flush`/`roundtrip` 不够。

### 7. 实际 surface 尺寸（1920×1046）与 monitor 尺寸（1920×1080）不一致
- **现象**：layer-shell 窗口被 compositor 配置成 1046 高（少了 34px，原因未知，
  可能是 Wayfire 保留区域），用 `monitor_at_window().geometry()` 会拿到 1080。
- **方案**：用**窗口 allocation**（即 surface 真实尺寸）作为坐标基准，
  不要用 monitor geometry。

### 7. gdk 0.18 没有 `Window::monitor()`
- **现象**：`gdk_window.monitor()` 编译不过。
- **方案**：用 `gdk_window.display().monitor_at_window(gdk_window)`。
  另外 `gdk_window.geometry()` 返回 `(i32,i32,i32,i32)` 元组，不是 Rectangle。

### 8. `pkill -f` / `pgrep -f` 会杀掉执行命令的 shell 自己
- **现象**：`pkill -f "target/debug/desktop-canvas"` 或
  `pgrep -f "vite/bin/vite.js"` 把当前 shell 也杀了（bash 的命令行里就含
  这个字符串，`-f` 全命令行匹配），命令退出码 144（被信号杀死）。
  这条**反复出现**，务必记住。
- **方案**：
  - 精确匹配进程名：`pkill -x desktop-canvas`（`-x` 只匹配进程名）。
  - 按端口找 PID 再 kill：`ss -ltnp | grep :1420` → 取 `pid=` 字段 → `kill <pid>`。
  - 绝不把"要匹配的字符串"写进自己命令行的 `-f` 模式里。

---

## 二、WebKitGTK / webview

### 9. webkit2gtk 2.0.2 的类型按 API 版本特性门控
- **现象**：默认无特性时 `UserContentManager`、`WebContext`、`URISchemeResponse`
  等类型找不到。
- **方案**：启用 `features = ["v2_40"]`（对应 WebKitGTK 4.1）。

### 10. webkit2gtk 没有 `prelude`，trait 要按名导入
- **现象**：`WebView::load_uri`、`UserContentManager::register_script_message_handler`
  等方法找不到——它们都在 `*Ext` trait 里。
- **方案**：没有 `webkit2gtk::prelude`，逐个导入：
  `WebViewExt`、`WebContextExt`、`UserContentManagerExt`、`SettingsExt`、
  `URISchemeRequestExt`、`URISchemeResponseExt`。

### 11. `run_javascript` 已弃用（Since 2.40），返回 `()`
- **现象**：`run_javascript` 触发 deprecation 警告，且返回 `()` 不能 `if let Err`。
- **方案**：改用 `evaluate_javascript(&script, None, None, None, |_| {})`
  （也是返回 `()`，不需要处理 Result）。

### 12. `connect_script_message_received` 带 `detail: Option<&str>` 参数
- **现象**：只传回调会报 E0061 / E0282。
- **方案**：`user_content.connect_script_message_received(None, move |_m, msg| {...})`，
  回调参数 `msg: &JavascriptResult`，用 `msg.js_value()` 取 JS 值。

### 13. `URISchemeRequest::finish` 是 3 参数版本
- **现象**：`request.finish(&response)` 报 "takes 3 arguments but 1 supplied"。
- **方案**：用 `request.finish_with_response(&response)`。

### 14. `settings()` 方法有歧义
- **现象**：`webview.settings()` 编译报 E0034（`WebViewExt::settings` 与
  `gtk::prelude::WidgetExt::settings` 冲突）。
- **方案**：用全限定 `WebViewExt::settings(&webview)`。

### 15. webkit 应用 1.25 的 device pixel ratio
- **现象**：物理窗口 1920×1046，但 `window.innerWidth/innerHeight` 是 1536×836
  （正好 ×0.8）。Wayfire 输出 scale 是 1，但 webkit 自行推算出 1.25 DPR
  （可能从物理 DPI 推断）。
- **影响**：用物理像素算 handle 位置会落在 CSS 视口外，完全不可见。
- **方案**：前端 handle 用 **CSS 相对定位**（`right:0; top:50%; translateY(-50%)`），
  天然跟随 DPR；Rust 的 input region 用一个**足够大的物理热区**覆盖 handle。

---

## 三、Excalidraw / 前端

### 16. vite 预打包剥离 Excalidraw 内部 SCSS
- **现象**：Excalidraw 挂载了但工具栏/欢迎页渲染成裸样式（巨大黑色锁图标、
  图标散落）。诊断：`document.styleSheets.length` 只有 2（正常应有 1000+ 条规则）。
- **原因**：vite `optimizeDeps` 预打包 `@excalidraw/excalidraw` 时，
  其内部 `import "./css/app.scss"` 没被注入。
- **方案**：显式 `import "@excalidraw/excalidraw/index.css"`（包 exports 里有）。

### 17. Excalidraw 的 `appState.collaborators` 必须是 `Map`
- **现象**：`TypeError: props.appState.collaborators.forEach is not a function`，
  整个页面被 React 卸载（handle 也消失）。
- **原因**：Excalidraw 内部 `.forEach((collaborator, socketId))` 期望 Map；
  JSON 还原的 appState 里它是 `{}` 对象。
- **方案**：`initialData.appState` 里显式 `collaborators: new Map()`。

### 18. Excalidraw 深层类型导入路径
- `@excalidraw/excalidraw/types` → `dist/types/excalidraw/types.d.ts`
  （`AppState`、`ExcalidrawProps`）
- `@excalidraw/excalidraw/element/types` → `dist/types/excalidraw/element/types.d.ts`
  （`ExcalidrawElement`）
- 注意 exports map `"./*"` 映射到 `dist/types/excalidraw/*.d.ts`，
  别写成 `types/types`（会解析到不存在的文件）。

### 19. Excalidraw 空场景欢迎页（WelcomeScreen）
- 桌面覆盖层不需要 WelcomeScreen，用 CSS 隐藏：
  `.excalidraw .welcome-screen-center, .welcome-screen-decor { display: none; }`

### 20. IPC 回包不能裸拼 JSON 对象
- **现象**：`window.__dc_ipc_reply(1, true, {"x":1,...})` 是 JS 语法错误
  （对象字面量在表达式开头被当块语句）。
- **方案**：把 payload 作为 **JS 字符串字面量**注入：
  ```rust
  let payload_literal = serde_json::to_string(&payload_text)?;
  format!("window.__dc_ipc_reply({id}, {ok}, {payload_literal});")
  ```
  前端 `JSON.parse(payload)` 还原（成功时），失败时 payload 就是错误消息。

---

## 四、构建 / 工具链

### 21. pnpm 11 屏蔽依赖的 postinstall 脚本
- **现象**：`esbuild` 的 postinstall 被忽略（`ERR_PNPM_IGNORED_BUILDS`），
  vite 无法工作。
- **方案**：pnpm 11 把配置移到 `pnpm-workspace.yaml`（不是 package.json 的
  `pnpm` 字段）：
  ```yaml
  allowBuilds:
    esbuild: true
  ```

### 22. task-forest skill 的 `update-node` 有个 bug
- **现象**：`update-node` 报 `'Namespace' object has no attribute 'desired_outcome'`，
  无法更新任何节点。
- **原因**：`cmd_update_node` 访问了 parser 没定义的 `--desired-outcome` 等参数。
- **方案**：修脚本，把 `getattr(args, arg_name)` 改为 `getattr(args, arg_name, None)`。

### 23. 本机没有 GTK3 版 gtk-layer-shell
- 系统只有 `gtk4-layer-shell`，wry/webkit 用的是 GTK3。
- **方案**：Arch 安装 `sudo pacman -S gtk-layer-shell`。

---

## 五、打包 / 发布

### 24. 自定义协议要用 `request.path()` 而非 `request.uri()`
- **现象**：加载 `dc://app/index.html` 时按 `uri` 解析成 `app/index.html`，
  映射到 `dist/app/index.html`（不存在），页面 404。
- **方案**：用 webkit2gtk 的 `URISchemeRequestExt::path()`（返回路径部分
  `/index.html`），干净且正确。

### 25. vite 生产构建必须 `base: './'`
- **现象**：默认 `base: '/'` 使内置 HTML 引用 `/assets/xxx.js` 绝对路径，
  在自定义协议 `dc://app/index.html` 下解析错乱。
- **方案**：`vite.config.ts` 设 `base: "./"`，HTML 用相对路径 `./assets/...`。

### 26. 自包含二进制的资源嵌入：`include_dir`
- 需求：仿 Tauri 把前端嵌进二进制，单文件可运行（无需外部 dist）。
- **方案**：`include_dir!("$CARGO_MANIFEST_DIR/../dist")` 静态嵌入目录，
  `DIST.get_file(rel)` 运行时取字节。注意 dist 必须在**编译时**存在
  （先 `pnpm build`）。
- **区分 debug/release**：`#[cfg(not(debug_assertions))]` 嵌入；
  debug 读磁盘（`../dist`），避免开发时也强制先 build 前端。

### 27. debug-only 的导入要 `#[cfg(debug_assertions)]` 门控
- **现象**：只在 `#[cfg(debug_assertions)]` 代码块里用的导入（如 `SettingsExt`
  用于 inspector、`PathBuf` 用于 debug 磁盘读取），release 编译报 unused import。
- **方案**：给这些 `use` 也加 `#[cfg(debug_assertions)]`。

---

## 调试技巧

- **看 webview 里的状态**：前端通过 `invoke("log_debug", {msg})` 把 DOM 诊断
  上报到 Rust 日志（`tracing`），比连 webkit inspector 省事很多。
- **看真实渲染**：`grim`（Wayland 截图）+ ImageMagick 统计像素
  （`mean`/`standard_deviation`）判断 handle/UI 是否渲染。
- **进入编辑模式调试**：`DC_AUTOEDIT=1` 启动 + URL `?autoedit=1` 自动进入编辑。
- **webkit inspector**：`WEBKIT_INSPECTOR_SERVER=127.0.0.1:9222` + 代码里
  `settings.set_enable_developer_extras(true)`（需要 `SettingsExt` trait）。
  注意它是 WebSocket 协议，`curl /json` 拿不到内容。
