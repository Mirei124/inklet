/**
 * scene.ts
 *
 * 维护 Excalidraw scene（elements + appState），负责：
 *  - 启动时从原生侧加载 scene.json（损坏则回退空 scene）
 *  - 修改后 500ms 防抖自动保存（一条 free draw stroke 只触发少量写入）
 *  - 序列化/反序列化 scene.json（尽量原样保存 Excalidraw 数据）
 */
import { invoke } from "../bridge";
import type { ExcalidrawElement } from "@excalidraw/excalidraw/element/types";
import type { AppState } from "@excalidraw/excalidraw/types";

export type SceneElements = readonly ExcalidrawElement[];
export type SceneAppState = Partial<AppState>;

export interface SceneFile {
  version: 1;
  elements: SceneElements;
  appState: SceneAppState;
}

export const SCENE_FILE_VERSION = 1 as const;

export function emptyScene(): SceneFile {
  return { version: SCENE_FILE_VERSION, elements: [], appState: {} };
}

/** 把 scene 序列化为 scene.json 的内容（Excalidraw 数据尽量原样） */
export function serializeScene(elements: SceneElements, appState: SceneAppState): string {
  return JSON.stringify({ version: SCENE_FILE_VERSION, elements, appState });
}

/**
 * 解析 scene.json 内容。
 * 损坏时打日志并返回空 scene，不抛异常 —— 应用不得因此崩溃。
 */
export function parseScene(json: string): SceneFile {
  try {
    const data: unknown = JSON.parse(json);
    if (data && typeof data === "object") {
      const obj = data as Record<string, unknown>;
      if (Array.isArray(obj.elements)) {
        return {
          version: obj.version === 1 ? 1 : SCENE_FILE_VERSION,
          elements: obj.elements as ExcalidrawElement[],
          appState:
            typeof obj.appState === "object" && obj.appState !== null
              ? (obj.appState as SceneAppState)
              : {},
        };
      }
    }
  } catch (err) {
    console.error("[scene] parse failed, falling back to empty scene", err);
  }
  return emptyScene();
}

/** 从原生侧加载 scene；无文件 / 损坏时返回空 scene */
export async function loadScene(): Promise<SceneFile> {
  const raw = (await invoke("load_scene")) as { json: string | null } | null;
  if (!raw?.json) return emptyScene();
  return parseScene(raw.json);
}

/** 把 scene 持久化到 scene.json */
export async function persistScene(scene: SceneFile): Promise<void> {
  await invoke("save_scene", { json: serializeScene(scene.elements, scene.appState) });
}

export interface DebouncedSaver {
  schedule: (scene: SceneFile) => void;
  /** 立即执行未落盘的保存（返回的 Promise 在保存完成后 resolve） */
  flush: () => Promise<void>;
  cancel: () => void;
}

/**
 * 防抖保存器：场景持续变化时最多每 delayMs 保存一次，
 * 保存期间产生的新修改会排到下一次，确保最终一致。
 */
export function createDebouncedSaver(
  save: (scene: SceneFile) => Promise<void>,
  delayMs = 500,
): DebouncedSaver {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let latest: SceneFile | null = null;
  let inFlight: Promise<void> | null = null;

  function schedule(scene: SceneFile): void {
    latest = scene;
    if (timer !== undefined) clearTimeout(timer);
    timer = setTimeout(() => {
      void run();
    }, delayMs);
  }

  function run(): Promise<void> {
    timer = undefined;
    if (inFlight) return inFlight;
    if (!latest) return Promise.resolve();
    const scene = latest;
    latest = null;
    inFlight = save(scene).finally(() => {
      inFlight = null;
      // 保存期间又有修改 -> 再排一轮，避免丢最后状态
      if (latest) {
        void run();
      }
    });
    return inFlight;
  }

  async function flush(): Promise<void> {
    if (timer !== undefined) {
      clearTimeout(timer);
      timer = undefined;
    }
    // 退出时等待当前保存以及保存期间新增的最后一版内容。
    while (inFlight || latest) {
      await (inFlight ?? run());
    }
  }

  function cancel(): void {
    if (timer !== undefined) clearTimeout(timer);
    timer = undefined;
    latest = null;
  }

  return { schedule, flush, cancel };
}
