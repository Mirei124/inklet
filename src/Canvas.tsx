/**
 * Canvas.tsx
 *
 * 透明背景的 Excalidraw 画布。只关心“画什么、当前是否 editing”，
 * 不做任何平台逻辑。编辑工具复用 Excalidraw 自带工具栏，通过配置隐藏
 * 不需要的 UI（不修改其核心逻辑）。
 */
import { Excalidraw } from "@excalidraw/excalidraw";
import type { ExcalidrawProps } from "@excalidraw/excalidraw/types";
import type { Lang } from "./i18n";
import type { SceneFile } from "./state/scene";

/** Excalidraw 的 onChange 签名，App 用它接收 scene 变化 */
export type SceneChange = NonNullable<ExcalidrawProps["onChange"]>;

interface CanvasProps {
  initialScene: SceneFile;
  editing: boolean;
  visible: boolean;
  onChange: SceneChange;
  /** 界面语言（Excalidraw UI 跟随应用的 en/zh 设置） */
  lang: Lang;
}

const TRANSPARENT = "transparent";

/** 应用 Lang → Excalidraw 语言代码 */
function excalidrawLangCode(lang: Lang): string {
  return lang === "zh" ? "zh-CN" : "en";
}

export function Canvas({ initialScene, editing, visible, onChange, lang }: CanvasProps) {
  return (
    <div
      className={`canvas-root mode-${editing ? "editing" : "passive"}${visible ? "" : " is-hidden"}`}
      inert={!visible}
    >
      <Excalidraw
        langCode={excalidrawLangCode(lang)}
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
