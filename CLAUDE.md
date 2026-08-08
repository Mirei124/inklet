# Inklet — 项目指南

桌面透明画板。技术栈：React + Excalidraw（前端）、Rust 原生（自建 IPC）。双平台：

- **Linux（wlroots Wayland）**：gtk-layer-shell + WebKitGTK，MVP 验收已在 Wayfire 通过
- **macOS**：tao + wry (WKWebView) + objc2，Canvas + Control 双窗口，验收已通过

详细踩坑记录见 [docs/pitfalls.md](docs/pitfalls.md)；任务追踪用 task-forest（`.agent-workbench/task-forest`）。

## 构建与运行

```bash
# 开发（HMR）：终端1 vite，终端2 原生
pnpm dev
cd src-tauri && cargo run -- --dev

# 生产自包含二进制（前端嵌入）
make build-release            # = pnpm build + cargo build --release
./src-tauri/target/release/inklet

# macOS 分发包（.app bundle，不出现在 Dock）
make app-bundle               # 产物 dist/Inklet.app
```

数据：Linux 在 `~/.local/share/inklet/`；macOS 在 `~/Library/Application Support/inklet/`。
应用日志默认写终端（后台跑时重定向到 `/tmp/dc-app.log` 等）。

## 操作注意事项（反复踩坑，务必先读）

### 1. 绝不用 `pkill -f` / `pgrep -f` 匹配自己命令行里会出现的字符串
`pkill -f "inklet"` 或 `pgrep -f "vite/bin/vite.js"` 会匹配到**执行这条命令的 shell 本身**（shell 命令行里就含该字符串），把自己杀掉，命令退出码 144。这是本项目里反复踩的坑。

正确做法：
- 精确匹配进程名：`pkill -x inklet`
- 按端口找 PID 再 kill：
  ```bash
  VITE_PID=$(ss -ltnp | grep ':1420' | grep -oP 'pid=\K[0-9]+' | head -1)
  kill "$VITE_PID"
  ```

### 2. `pnpm approve-builds` 是交互式的，会卡住
pnpm 11 屏蔽依赖的 postinstall（如 esbuild）。不要跑交互式 `pnpm approve-builds`。
正确做法：改 `pnpm-workspace.yaml` 的 `allowBuilds`，然后 `pnpm rebuild esbuild`。

### 3. 后台进程要用 `nohup ... &`
用 Bash 工具启动长驻进程（vite、应用）时，`nohup cmd > log 2>&1 &` 保证不随工具调用结束被杀。vite 卡死不响应时：按端口（1420）找 PID 强杀后重启。

### 4. 改前端代码后若 webview 没热更新，重启应用
vite HMR 与 webview 的 WebSocket 可能断连。前端改动没生效时：`pkill -x inklet` 后重启原生（它会重新加载 vite 页面）。

### 5. 编辑模式相关
- 编辑模式是整屏输入 + 键盘 Exclusive；passive 是 input region 只留右缘热区。
- input region 改动后必须手动 `wl_surface_commit`（协议规定下次 commit 才生效），否则热区不更新。
- 测试图层切换后 settings.json 可能停在 `background`，画布会跑到窗口下面看起来"丢了"，点 handle 的「置顶」切回。

## 调试

- 前端 `window.onerror` 会通过 `log_debug` 上报到 Rust 日志。
- `grim` 截图 + ImageMagick 统计像素可判断 handle/UI 是否渲染。
- `DC_AUTOEDIT=1` 启动可让应用自动进入编辑模式（配合 URL `?autoedit=1`）。
