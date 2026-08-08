# Inklet

桌面透明画板：始终显示在桌面上的 Excalidraw 画布，默认鼠标穿透，只保留右缘编辑入口；点击进入编辑模式绘制，完成后恢复穿透，scene 自动持久化到本地 JSON。

目标平台：Linux Wayland（wlroots 系，如 Wayfire / Hyprland / Sway）+ macOS。

## 功能

- 透明全屏覆盖层，默认鼠标穿透（Linux：`wl_surface_set_input_region`；macOS：`ignoresMouseEvents`）
- 右缘编辑入口：点击展开「编辑 / 图层切换」，可拖拽移动位置（y 持久化）
- 图层切换：overlay（所有窗口之上）⇄ background（壁纸之上/所有窗口之下）
- 编辑模式：完整 Excalidraw 工具（笔/矩形/箭头/文字/撤销/重做/删除）
- 500ms 防抖自动保存 scene.json（原子写 tmp→rename），异常退出不损坏
- 多语言：en / zh（settings.json 持久化，默认按系统区域）

## 平台实现

| 平台 | 窗口/WebView | 穿透机制 |
|------|-------------|---------|
| Linux Wayland | gtk-layer-shell + WebKitGTK | wl_surface input region |
| macOS | tao + wry (WKWebView) + objc2 | NSWindow ignoresMouseEvents（Canvas）+ Control 浮窗承载编辑入口 |

## 前置依赖

```bash
# Linux (Arch)
sudo pacman -S gtk-layer-shell  # Wayland layer-shell 覆盖层
# 还需要：gtk3 / webkit2gtk-4.1 / wayland（通常已随桌面环境安装）

# macOS：Xcode Command Line Tools 即可（WebKit/AppKit 系统自带）
```

## 开发模式（HMR）

```bash
pnpm install
pnpm dev     # 并行启动 vite dev server (:1420) + 原生应用（--dev）
# 如需单独启动：pnpm dev:web 只跑前端；cd src-tauri && cargo run -- --dev 只跑原生
```

## 生产模式（自包含二进制）

```bash
make build-release
# 等价于：pnpm build && cd src-tauri && cargo build --release
```

产物：`src-tauri/target/release/inklet`（**前端已嵌入二进制**，单文件可直接运行，无需 dist 目录）。

```bash
# 直接运行自包含二进制
./src-tauri/target/release/inklet

# macOS 分发包（.app bundle，不出现在 Dock，可双击启动）
make app-bundle    # 产物 dist/Inklet.app
```

## 数据文件

- Linux：`~/.local/share/inklet/`（scene.json / settings.json）
- macOS：`~/Library/Application Support/inklet/`

## 项目结构

```
src/            React 前端
  App.tsx        状态机 Passive ⇄ Editing
  Canvas.tsx     Excalidraw 透明画布
  EditHandle.tsx 右缘编辑入口（点击展开/拖拽移动，macOS 用原生 Control 窗口）
  Toolbar.tsx    Done 按钮
  state/scene.ts scene 防抖保存
  bridge.ts      IPC 客户端（webkit / wry 消息通道）
  i18n.ts        多语言
src-tauri/      Rust 原生
  src/app.rs       入口分派 + 共享工具（日志/dev/settings 恢复）
  src/app/assets.rs 跨平台 dist 资源服务（dc:// 与 inklet:// 共用）
  src/app/linux.rs  gtk-layer-shell + WebKitGTK 实现
  src/app/macos.rs  tao + wry (WKWebView) 实现
  src/ipc.rs       命令分发（parse/dispatch/reply 跨平台共用）
  src/commands.rs  scene/settings 命令
  src/desktop/     DesktopSurface 抽象 + wayland.rs / macos.rs 实现
  src/storage/     原子 JSON 存储
```

## 踩坑记录

见 [docs/pitfalls.md](docs/pitfalls.md)：Wayland 侧（wry 仅 X11、input region 需 commit 生效、webkit DPR 等）与 macOS 侧（tao fullscreen 重置 level、物理/逻辑像素混用、Control IPC reply 丢失、activation policy 被覆盖等）。

## 致谢

- [Excalidraw](https://excalidraw.com/)（[MIT](https://github.com/excalidraw/excalidraw/blob/master/LICENSE)）—— 绘图引擎与编辑器 UI，本项目直接嵌入使用
- [Tauri](https://tauri.app/)（MIT/Apache-2.0）—— 工程脚手架基于 `create-tauri-app` 生成（运行时改用 WebKitGTK 直连 layer-shell）
- [Vite](https://vitejs.dev/) / [React](https://react.dev/) / [TypeScript](https://www.typescriptlang.org/) —— 前端构建与框架
- [WebKitGTK](https://webkitgtk.org/)（LGPL）—— Linux 底层 WebView 渲染
- [gtk-layer-shell](https://github.com/wmww/gtk-layer-shell)（MIT）—— Wayland layer-shell 覆盖层
- [tao](https://github.com/tauri-apps/tao) / [wry](https://github.com/tauri-apps/wry) / [objc2](https://github.com/madsmtm/objc2) —— macOS 窗口与 WebView
- 以及所有被依赖的开源项目 🙏

## License

[MIT](LICENSE)
