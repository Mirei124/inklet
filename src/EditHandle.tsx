/**
 * EditHandle.tsx
 *
 * 屏幕右侧中央的编辑入口。
 *  - 默认：一条窄的、半透明的竖条（弱化显示）
 *  - hover：展开为 “✎ Edit” 小药丸
 *  - 点击：进入编辑模式
 *
 * 整块热区由原生侧 input region 决定（passive 模式下只有这块区域接收鼠标），
 * 这里用 rect 把按钮精确对齐到热区。
 */
import type { Rect } from "./bridge";

interface EditHandleProps {
  rect: Rect;
  onEnterEdit: () => void;
}

export function EditHandle({ rect, onEnterEdit }: EditHandleProps) {
  return (
    <button
      type="button"
      className="edit-handle"
      style={{ left: rect.x, top: rect.y, width: rect.width, height: rect.height }}
      onClick={onEnterEdit}
      aria-label="进入编辑模式"
      title="点击进入编辑模式"
    >
      <span className="edit-handle-label">✎ Edit</span>
    </button>
  );
}
