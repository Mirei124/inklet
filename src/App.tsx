/**
 * App.tsx
 *
 * 顶层状态机：Passive ↔ Editing。
 *  - Passive：画布只读、鼠标穿透（原生侧 input region 只保留右缘热区），
 *    仅 EditHandle 可交互。
 *  - Editing：整画布接收输入，显示工具栏 + Done。
 *
 * 模式切换以 backend 成功为准：先 await invoke，再更新 UI 状态。
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke, logDebug } from "./bridge";
import type { AppMode, CanvasLayer } from "./bridge";
import { Canvas } from "./Canvas";
import type { SceneChange } from "./Canvas";
import { EditHandle } from "./EditHandle";
import { Toolbar } from "./Toolbar";
import { createDebouncedSaver, loadScene, persistScene } from "./state/scene";
import type { SceneFile } from "./state/scene";

const TRANSPARENT = "transparent";

export default function App() {
  const [mode, setMode] = useState<AppMode>("passive");
  const [initialScene, setInitialScene] = useState<SceneFile | null>(null);
  const [layer, setLayer] = useState<CanvasLayer>("overlay");

  // 前端 JS 错误上报到 Rust 日志（调试用）
  useEffect(() => {
    const onError = (event: ErrorEvent) => {
      logDebug(`JS error: ${event.message} @ ${event.filename}:${event.lineno}`);
    };
    const onRejection = (event: PromiseRejectionEvent) => {
      logDebug(`unhandled rejection: ${String(event.reason)}`);
    };
    window.addEventListener("error", onError);
    window.addEventListener("unhandledrejection", onRejection);
    logDebug(`app mounted, size=${window.innerWidth}x${window.innerHeight}`);
    return () => {
      window.removeEventListener("error", onError);
      window.removeEventListener("unhandledrejection", onRejection);
    };
  }, []);

  // 保存器只创建一次，保证防抖计时不因重渲染而重置
  const saver = useMemo(() => createDebouncedSaver((scene: SceneFile) => persistScene(scene)), []);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      const [scene, layerResult] = await Promise.all([
        loadScene(),
        invoke("get_canvas_layer") as Promise<{ layer: CanvasLayer }>,
      ]);
      if (cancelled) return;
      logDebug(
        `loaded scene: ${scene.elements.length} elements; layer=${layerResult.layer}; ` +
          `viewport=${window.innerWidth}x${window.innerHeight}`,
      );
      setInitialScene(scene);
      setLayer(layerResult.layer);
    })();
    return () => {
      cancelled = true;
      saver.cancel();
    };
  }, [saver]);

  const handleSceneChange: SceneChange = useCallback(
    (elements, appState) => {
      saver.schedule({
        version: 1,
        elements,
        appState: { ...appState, viewBackgroundColor: TRANSPARENT },
      });
    },
    [saver],
  );

  const enterEditMode = useCallback(async () => {
    await invoke("enter_edit_mode");
    setMode("editing");
  }, []);

  // 调试：URL 带 ?autoedit=1 时启动后自动进入编辑模式（配合 DC_AUTOEDIT）
  useEffect(() => {
    if (new URLSearchParams(window.location.search).has("autoedit")) {
      logDebug("autoedit: entering editing mode");
      void enterEditMode();
    }
  }, [enterEditMode]);

  // 调试：列出 passive 模式下仍可见的 Excalidraw UI 元素
  useEffect(() => {
    const t = setTimeout(() => {
      const topMenu = document.querySelector(".canvas-root.mode-passive .App-menu_top");
      const topMenuAny = document.querySelector(".App-menu_top");
      const rule = Array.from(document.styleSheets)
        .flatMap((s) => {
          try {
            return Array.from(s.cssRules);
          } catch {
            return [];
          }
        })
        .filter((r) => r.cssText.includes("mode-passive") && r.cssText.includes("display: none"))
        .map((r) => r.cssText.slice(0, 80))
        .join(" || ");
      logDebug(
        `DIAG-UI: modePassiveTop=${topMenu ? getComputedStyle(topMenu).display : "absent"} ` +
          `anyTop=${topMenuAny ? getComputedStyle(topMenuAny).display : "absent"} ` +
          `matchedRules=${rule || "(none)"}`,
      );
    }, 3000);
    return () => clearTimeout(t);
  }, []);

  const exitEditMode = useCallback(async () => {
    await saver.flush(); // 退出前先落盘
    await invoke("exit_edit_mode");
    setMode("passive");
  }, [saver]);

  const toggleLayer = useCallback(async () => {
    const next: CanvasLayer = layer === "overlay" ? "background" : "overlay";
    const result = (await invoke("set_canvas_layer", { layer: next })) as {
      layer: CanvasLayer;
    };
    setLayer(result.layer);
    logDebug(`canvas layer -> ${result.layer}`);
  }, [layer]);

  return (
    <div className="app-root">
      {initialScene ? (
        <Canvas
          initialScene={initialScene}
          editing={mode === "editing"}
          onChange={handleSceneChange}
        />
      ) : null}
      {mode === "passive" ? (
        <EditHandle layer={layer} onEnterEdit={enterEditMode} onToggleLayer={toggleLayer} />
      ) : null}
      {mode === "editing" ? <Toolbar onDone={exitEditMode} /> : null}
    </div>
  );
}
