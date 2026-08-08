/**
 * Toolbar.tsx
 *
 * 编辑模式下的简化工具栏：MVP 阶段复用 Excalidraw 自带工具
 * （pen / rect / arrow / text / eraser / undo / redo / delete），
 * 这里只提供缺失的 “Done” 出口，位置跟随 handleY。
 */
import { t, type Lang } from "./i18n";

interface ToolbarProps {
  lang: Lang;
  onDone: () => void;
  handleY: number;
}

export function Toolbar({ lang, onDone, handleY }: ToolbarProps) {
  return (
    <div className="toolbar" style={{ top: `${handleY * 100}%` }}>
      <button type="button" className="done-button" onClick={onDone}>
        {t(lang, "done")}
      </button>
    </div>
  );
}
