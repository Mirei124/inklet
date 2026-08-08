/**
 * Canvas.tsx
 *
 * 透明背景的 Excalidraw 画布。只关心“画什么、当前是否 editing”，
 * 不做任何平台逻辑。编辑工具复用 Excalidraw 自带工具栏，通过配置隐藏
 * 不需要的 UI（不修改其核心逻辑）。
 */
import { Excalidraw } from "@excalidraw/excalidraw";
import type { ExcalidrawProps } from "@excalidraw/excalidraw/types";
import type { SceneFile } from "./state/scene";

/** Excalidraw 的 onChange 签名，App 用它接收 scene 变化 */
export type SceneChange = NonNullable<ExcalidrawProps["onChange"]>;

interface CanvasProps {
  initialScene: SceneFile;
  editing: boolean;
  onChange: SceneChange;
}

const TRANSPARENT = "transparent";

export function Canvas({ initialScene, editing, onChange }: CanvasProps) {
  return (
    <div className="canvas-root">
      <Excalidraw
        initialData={{
          elements: initialScene.elements,
          appState: {
            ...initialScene.appState,
            // collaborators 必须是 Map（Excalidraw 内部会 .forEach）；
            // 从 JSON 还原时是 {}，必须重建为 Map
            collaborators: new Map(),
            viewBackgroundColor: TRANSPARENT,
          },
        }}
        onChange={onChange}
        viewModeEnabled={!editing}
        UIOptions={{
          canvasActions: {
            changeViewBackgroundColor: false,
            clearCanvas: false,
            export: false,
            loadScene: false,
            saveToActiveFile: false,
            toggleTheme: false,
            saveAsImage: false,
          },
          tools: { image: false },
        }}
      />
    </div>
  );
}
