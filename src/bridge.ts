/**
 * bridge.ts
 *
 * 前后端 IPC 客户端。
 *
 * 原生侧（WebKitGTK）注册了名为 `ipc` 的 script message handler，这里把所有
 * 命令走 id + cmd + args 的 JSON 协议，原生侧处理完成后回调
 * `window.__dc_ipc_reply(id, ok, payload)`。
 *
 * 在普通浏览器里（无原生注入）自动回退到内存 mock，方便单独开发 UI。
 *
 * React 层不应感知 wl_surface / wl_region / layer-shell 等平台细节，
 * 只通过这里暴露的语义命令与 native backend 交互。
 */

export type AppMode = "passive" | "editing";

/** 屏幕上的矩形（单位：CSS 像素），坐标系与 webview 视口一致 */
export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface ScreenSize {
  width: number;
  height: number;
}

/** 序列化后的 scene 文件内容（scene.json 原样保存） */
export interface SceneFile {
  version: 1;
  elements: unknown[];
  appState: Record<string, unknown>;
}

/** 画布所在 layer：overlay=所有窗口之上，background=壁纸之上/所有窗口之下 */
export type CanvasLayer = "overlay" | "background";

export type Command =
  | "enter_edit_mode"
  | "exit_edit_mode"
  | "load_scene"
  | "save_scene"
  | "screen_size"
  | "get_handle_rect"
  | "get_canvas_layer"
  | "set_canvas_layer"
  | "get_handle_position"
  | "set_handle_position"
  | "get_settings"
  | "set_lang"
  | "log_debug";

/** 把前端状态/错误上报到 Rust 日志（调试用，浏览器 mock 下忽略）。 */
export function logDebug(msg: string): void {
  void invoke("log_debug", { msg });
}

interface Pending {
  resolve: (value: unknown) => void;
  reject: (reason: Error) => void;
}

declare global {
  interface Window {
    webkit?: {
      messageHandlers?: {
        ipc?: { postMessage: (message: string) => void };
      };
    };
    __dc_ipc_reply?: (id: number, ok: boolean, payload: string) => void;
    /** 原生侧注入的平台标记（macOS = "macos"；Linux 不注入） */
    __dc_platform?: string;
    /**
     * 原生主动推送模式切换（macOS Control 窗口触发的切换不走 invoke/pending，
     * reply 无人接收，需要原生直接推送给前端同步 React state）。
     */
    __dc_sync_mode?: (mode: "passive" | "editing") => void;
  }
}

/**
 * 是否为 macOS 原生环境。
 *
 * macOS 上编辑入口（竖条）由独立 Control 窗口承载，Canvas 里的
 * EditHandle 需要隐藏（因为 Canvas 全屏穿透不可交互）。Linux 保持原样。
 */
export function isNativeMacOS(): boolean {
  return typeof window !== "undefined" && window.__dc_platform === "macos";
}

let nextId = 0;
const pending = new Map<number, Pending>();

/** 原生侧回调入口（wry evaluate_script 调用） */
if (typeof window !== "undefined") {
  window.__dc_ipc_reply = (id, ok, payload) => {
    const entry = pending.get(id);
    if (!entry) return;
    pending.delete(id);
    if (ok) {
      entry.resolve(payload ? JSON.parse(payload) : null);
    } else {
      entry.reject(new Error(payload || `command failed (id=${id})`));
    }
  };
}

function nativeAvailable(): boolean {
  return (
    typeof window !== "undefined" &&
    (typeof window.webkit?.messageHandlers?.ipc?.postMessage === "function" ||
     typeof (window as unknown as Record<string, unknown>).ipc === "object")
  );
}

/** 发送消息到原生侧，自动选择可用的传输层。 */
function postNativeMessage(msg: string): void {
  if (typeof window.webkit?.messageHandlers?.ipc?.postMessage === "function") {
    window.webkit!.messageHandlers!.ipc!.postMessage(msg);
  } else {
    ((window as unknown as Record<string, { postMessage: (m: string) => void }>).ipc).postMessage(msg);
  }
}

/**
 * 调用原生命令。浏览器开发模式下走 mock，保证 UI 可独立开发。
 */
export function invoke(cmd: Command, args?: Record<string, unknown>): Promise<unknown> {
  if (!nativeAvailable()) {
    return mockInvoke(cmd, args);
  }
  return new Promise((resolve, reject) => {
    const id = nextId++;
    pending.set(id, { resolve, reject });
    postNativeMessage(JSON.stringify({ id, cmd, args: args ?? {} }));
  });
}

/* ------------------------------------------------------------------ */
/* 浏览器开发用 mock：把 scene 存在 localStorage，重启页面也能找回     */
/* ------------------------------------------------------------------ */

const MOCK_SCENE_KEY = "inklet.mock.scene";

function mockScene(): SceneFile {
  try {
    const raw = localStorage.getItem(MOCK_SCENE_KEY);
    if (raw) return JSON.parse(raw) as SceneFile;
  } catch {
    /* ignore corrupt mock */
  }
  return { version: 1, elements: [], appState: {} };
}

function mockInvoke(cmd: Command, args?: Record<string, unknown>): Promise<unknown> {
  const scene = mockScene();
  const delay = (value: unknown) => Promise.resolve(value);
  switch (cmd) {
    case "load_scene":
      return delay(scene);
    case "save_scene": {
      const json = typeof args?.json === "string" ? (args.json as string) : null;
      if (json) {
        try {
          localStorage.setItem(MOCK_SCENE_KEY, json);
        } catch {
          /* storage full — ignore in mock */
        }
      }
      return delay(null);
    }
    case "enter_edit_mode":
    case "exit_edit_mode":
      return delay(null);
    case "screen_size":
      return delay({ width: window.innerWidth, height: window.innerHeight });
    case "get_handle_rect": {
      const w = window.innerWidth;
      const h = window.innerHeight;
      return delay({ x: w - 44, y: Math.round((h - 44) / 2), width: 44, height: 44 });
    }
    case "get_canvas_layer":
      return delay({ layer: localStorage.getItem(MOCK_LAYER_KEY) || "overlay" });
    case "set_canvas_layer": {
      const layer = args?.layer === "background" ? "background" : "overlay";
      localStorage.setItem(MOCK_LAYER_KEY, layer);
      return delay({ layer });
    }
    case "get_handle_position":
      return delay({ y: Number(localStorage.getItem(MOCK_HANDLE_Y_KEY) || "0.5") });
    case "set_handle_position": {
      const y = typeof args?.y === "number" ? args.y : 0.5;
      localStorage.setItem(MOCK_HANDLE_Y_KEY, String(y));
      return delay({ y });
    }
    case "get_settings":
      return delay({
        layer: localStorage.getItem(MOCK_LAYER_KEY) || "overlay",
        handleY: Number(localStorage.getItem(MOCK_HANDLE_Y_KEY) || "0.5"),
        lang: localStorage.getItem(MOCK_LANG_KEY) || undefined,
      });
    case "set_lang": {
      const lang = args?.lang === "en" ? "en" : "zh";
      localStorage.setItem(MOCK_LANG_KEY, lang);
      return delay({ lang });
    }
    default:
      return delay(null);
  }
}

const MOCK_LAYER_KEY = "inklet.mock.layer";
const MOCK_HANDLE_Y_KEY = "inklet.mock.handleY";
const MOCK_LANG_KEY = "inklet.mock.lang";
