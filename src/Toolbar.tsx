/**
 * Toolbar.tsx
 *
 * 编辑模式下的简化工具栏：MVP 阶段复用 Excalidraw 自带工具
 * （pen / rect / arrow / text / eraser / undo / redo / delete），
 * 这里只提供缺失的 “Done” 出口。
 */
interface ToolbarProps {
  onDone: () => void;
}

export function Toolbar({ onDone }: ToolbarProps) {
  return (
    <div className="toolbar">
      <button type="button" className="done-button" onClick={onDone}>
        ✓ Done
      </button>
    </div>
  );
}
