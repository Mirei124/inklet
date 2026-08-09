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
import { invoke, logDebug, isNativeMacOS } from "./bridge";
import type { AppMode, CanvasLayer } from "./bridge";
import { Canvas } from "./Canvas";
import type { SceneChange } from "./Canvas";
import { EditHandle } from "./EditHandle";
import { Toolbar } from "./Toolbar";
import { detectLang } from "./i18n";
import type { Lang } from "./i18n";
import { createDebouncedSaver, loadScene, persistScene } from "./state/scene";
import type { SceneFile } from "./state/scene";

const TRANSPARENT = "transparent";

export default function App() {
  const [mode, setMode] = useState<AppMode>("passive");

  // 原生主动推送模式切换（macOS Control 窗口触发，不走 invoke/pending）。
  // 必须尽早注册，确保 Control 按钮点击时能更新 React state。
  useEffect(() => {
    window.__dc_sync_mode = (m) => setMode(m);
    return () => {
      delete window.__dc_sync_mode;
    };
  }, []);
  const [initialScene, setInitialScene] = useState<SceneFile | null>(null);
  const [layer, setLayer] = useState<CanvasLayer>("overlay");
  const [handleY, setHandleY] = useState(0.5);
  const [lang, setLang] = useState<Lang>("zh");

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
      const [scene, settings] = await Promise.all([
        loadScene(),
        invoke("get_settings") as Promise<{
          layer: CanvasLayer;
          handleY: number;
          lang?: string;
        }>,
      ]);
      if (cancelled) return;
      // 语言：settings 里有则用，没有则按系统区域检测并持久化
      const resolvedLang: Lang =
        settings.lang === "en" ? "en" : settings.lang === "zh" ? "zh" : detectLang();
      if (!settings.lang) {
        void invoke("set_lang", { lang: resolvedLang });
      }
      logDebug(
        `loaded scene: ${scene.elements.length} elements; layer=${settings.layer}; ` +
          `handleY=${settings.handleY.toFixed(2)}; lang=${resolvedLang}; ` +
          `viewport=${window.innerWidth}x${window.innerHeight}`,
      );
      setInitialScene(scene);
      setLayer(settings.layer);
      setHandleY(settings.handleY);
      setLang(resolvedLang);
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

  // 调试：记录 webview 收到的指针事件（定位输入区域问题）。
  // 仅开发模式（vite dev）启用，生产构建（dist）关闭，避免 release 日志噪音。
  useEffect(() => {
    if (!import.meta.env.DEV) return;
    const onDown = (e: PointerEvent) =>
      logDebug(
        `PTR down x=${e.clientX.toFixed(0)} y=${e.clientY.toFixed(0)} ` +
          `target=${(e.target as HTMLElement).className || "?"}`,
      );
    const onUp = (e: PointerEvent) =>
      logDebug(`PTR up x=${e.clientX.toFixed(0)} y=${e.clientY.toFixed(0)}`);
    document.addEventListener("pointerdown", onDown);
    document.addEventListener("pointerup", onUp);
    return () => {
      document.removeEventListener("pointerdown", onDown);
      document.removeEventListener("pointerup", onUp);
    };
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

  // handle 拖拽：实时更新渲染位置（本地状态）
  const handlePositionChange = useCallback((y: number) => {
    setHandleY(y);
  }, []);

  // handle 拖拽结束：同步原生热区 + 持久化
  const handlePositionCommit = useCallback((y: number) => {
    setHandleY(y);
    void invoke("set_handle_position", { y });
    logDebug(`handle position committed: ${y.toFixed(2)}`);
  }, []);

  return (
    <div className="app-root">
      {initialScene ? (
        <Canvas
          initialScene={initialScene}
          editing={mode === "editing"}
          onChange={handleSceneChange}
          lang={lang}
        />
      ) : null}
      {mode === "passive" && !isNativeMacOS() ? (
        <EditHandle
          lang={lang}
          layer={layer}
          handleY={handleY}
          onEnterEdit={enterEditMode}
          onToggleLayer={toggleLayer}
          onPositionChange={handlePositionChange}
          onPositionCommit={handlePositionCommit}
        />
      ) : null}
      {mode === "editing" ? <Toolbar lang={lang} onDone={exitEditMode} handleY={handleY} /> : null}
    </div>
  );
}
