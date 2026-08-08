/**
 * EditHandle.tsx
 *
 * 屏幕右侧中央的编辑入口，hover 展开两个按钮：
 *  - ✎ Edit：进入编辑模式
 *  - 置底 / 置顶：切换画布 layer（overlay ⇄ background）
 *
 * 用 CSS 相对定位（right:0 / top:50%）固定在右缘中央，天然跟随
 * webkit 的 device pixel ratio，与原生侧 passive 模式的 input region
 * 热区（右缘中央）对齐。
 */
import type { CanvasLayer } from "./bridge";

interface EditHandleProps {
  layer: CanvasLayer;
  onEnterEdit: () => void;
  onToggleLayer: () => void;
}

export function EditHandle({ layer, onEnterEdit, onToggleLayer }: EditHandleProps) {
  const layerLabel = layer === "overlay" ? "⇵ 置底" : "⇵ 置顶";
  return (
    <div className="edit-handle">
      <button
        type="button"
        className="edit-handle-item"
        onClick={onEnterEdit}
        aria-label="进入编辑模式"
        title="点击进入编辑模式"
      >
        <span>✎ Edit</span>
      </button>
      <button
        type="button"
        className="edit-handle-item"
        onClick={onToggleLayer}
        aria-label="切换画布图层"
        title={layer === "overlay" ? "移到壁纸层（所有窗口之下）" : "移到所有窗口之上"}
      >
        <span>{layerLabel}</span>
      </button>
    </div>
  );
}
