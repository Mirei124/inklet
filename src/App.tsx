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
import { invoke } from "./bridge";
import type { AppMode, Rect } from "./bridge";
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
  const [handleRect, setHandleRect] = useState<Rect | null>(null);

  // 保存器只创建一次，保证防抖计时不因重渲染而重置
  const saver = useMemo(() => createDebouncedSaver((scene: SceneFile) => persistScene(scene)), []);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      const [scene, rect] = await Promise.all([
        loadScene(),
        invoke("get_handle_rect") as Promise<Rect>,
      ]);
      if (cancelled) return;
      setInitialScene(scene);
      setHandleRect(rect);
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

  const exitEditMode = useCallback(async () => {
    await saver.flush(); // 退出前先落盘
    await invoke("exit_edit_mode");
    setMode("passive");
  }, [saver]);

  return (
    <div className="app-root">
      {initialScene ? (
        <Canvas
          initialScene={initialScene}
          editing={mode === "editing"}
          onChange={handleSceneChange}
        />
      ) : null}
      {mode === "passive" && handleRect ? (
        <EditHandle rect={handleRect} onEnterEdit={enterEditMode} />
      ) : null}
      {mode === "editing" ? <Toolbar onDone={exitEditMode} /> : null}
    </div>
  );
}
