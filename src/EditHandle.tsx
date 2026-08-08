/**
 * EditHandle.tsx
 *
 * 屏幕右侧中央的编辑入口。
 *  - 默认：一条窄的、半透明的竖条（弱化显示）
 *  - hover：展开为 “✎ Edit” 小药丸
 *  - 点击：进入编辑模式
 *
 * 用 CSS 相对定位（right:0 / top:50%）固定在右缘中央，天然跟随
 * webkit 的 device pixel ratio，与原生侧 passive 模式的 input region
 * 热区（右缘中央）对齐。
 */
interface EditHandleProps {
  onEnterEdit: () => void;
}

export function EditHandle({ onEnterEdit }: EditHandleProps) {
  return (
    <button
      type="button"
      className="edit-handle"
      onClick={onEnterEdit}
      aria-label="进入编辑模式"
      title="点击进入编辑模式"
    >
      <span className="edit-handle-label">✎ Edit</span>
    </button>
  );
}
