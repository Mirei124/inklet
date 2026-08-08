/**
 * bridge.ts
 *
 * 前后端 IPC 客户端。
 *
 * 原生侧（wry）会注入 `window.ipc.postMessage`；这里把所有命令走
 * id + cmd + args 的 JSON 协议，原生侧处理完成后回调
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

export type Command =
  | "enter_edit_mode"
  | "exit_edit_mode"
  | "load_scene"
  | "save_scene"
  | "screen_size"
  | "get_handle_rect";

interface Pending {
  resolve: (value: unknown) => void;
  reject: (reason: Error) => void;
}

declare global {
  interface Window {
    ipc?: { postMessage: (message: string) => void };
    __dc_ipc_reply?: (id: number, ok: boolean, payload: string) => void;
  }
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
  return typeof window !== "undefined" && typeof window.ipc?.postMessage === "function";
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
    window.ipc!.postMessage(JSON.stringify({ id, cmd, args: args ?? {} }));
  });
}

/* ------------------------------------------------------------------ */
/* 浏览器开发用 mock：把 scene 存在 localStorage，重启页面也能找回     */
/* ------------------------------------------------------------------ */

const MOCK_SCENE_KEY = "desktop-canvas.mock.scene";

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
    default:
      return delay(null);
  }
}
