/**
 * EditHandle.tsx
 *
 * 屏幕右缘的编辑入口，支持：
 *  - 点击竖条：展开 / 收起两个按钮（Edit / 图层切换）
 *  - 拖拽竖条：沿右缘上下移动 handle 位置（y 持久化，Done 按钮跟随）
 *
 * 位置由 `handleY`（0..1，占视口高比例）决定，与原生侧 passive 热区对齐。
 * 拖拽与点击用 pointer 事件区分：移动超过阈值算拖拽，否则算点击。
 */
import { useRef, useState } from "react";
import type { CanvasLayer } from "./bridge";

interface EditHandleProps {
  layer: CanvasLayer;
  handleY: number;
  onEnterEdit: () => void;
  onToggleLayer: () => void;
  /** 拖拽过程中实时更新（本地渲染） */
  onPositionChange: (y: number) => void;
  /** 拖拽结束（持久化 + 同步原生热区） */
  onPositionCommit: (y: number) => void;
}

const DRAG_THRESHOLD_PX = 6;
const MIN_Y = 0.05;
const MAX_Y = 0.95;

interface DragState {
  active: boolean;
  startY: number;
  startHandleY: number;
  moved: boolean;
}

export function EditHandle({
  layer,
  handleY,
  onEnterEdit,
  onToggleLayer,
  onPositionChange,
  onPositionCommit,
}: EditHandleProps) {
  const [expanded, setExpanded] = useState(false);
  const drag = useRef<DragState>({ active: false, startY: 0, startHandleY: 0, moved: false });

  const targetIsButton = (e: React.PointerEvent) =>
    !!(e.target as HTMLElement).closest(".edit-handle-item");

  const handlePointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (targetIsButton(e)) {
      drag.current.active = false;
      return;
    }
    drag.current = { active: true, startY: e.clientY, startHandleY: handleY, moved: false };
    e.currentTarget.setPointerCapture(e.pointerId);
  };

  const handlePointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!drag.current.active) return;
    const dy = e.clientY - drag.current.startY;
    if (Math.abs(dy) > DRAG_THRESHOLD_PX) drag.current.moved = true;
    if (drag.current.moved) {
      onPositionChange(clamp(drag.current.startHandleY + dy / window.innerHeight));
    }
  };

  const handlePointerUp = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!drag.current.active) return;
    const wasMove = drag.current.moved;
    drag.current.active = false;
    if (wasMove) {
      const dy = e.clientY - drag.current.startY;
      onPositionCommit(clamp(drag.current.startHandleY + dy / window.innerHeight));
    } else {
      setExpanded((v) => !v); // 点击：展开 / 收起
    }
  };

  const layerLabel = layer === "overlay" ? "⇵ 置底" : "⇵ 置顶";

  return (
    <div
      className={`edit-handle${expanded ? " is-expanded" : ""}`}
      style={{ top: `${handleY * 100}%` }}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
      onPointerCancel={() => {
        drag.current.active = false;
      }}
    >
      <button
        type="button"
        className="edit-handle-bar"
        aria-label="编辑入口（拖拽移动，点击展开）"
        title="拖拽移动 · 点击展开"
      />
      <button type="button" className="edit-handle-item" onClick={onEnterEdit} title="进入编辑模式">
        <span>✎ Edit</span>
      </button>
      <button
        type="button"
        className="edit-handle-item"
        onClick={onToggleLayer}
        title={layer === "overlay" ? "移到壁纸层（所有窗口之下）" : "移到所有窗口之上"}
      >
        <span>{layerLabel}</span>
      </button>
    </div>
  );
}

function clamp(v: number): number {
  return Math.min(MAX_Y, Math.max(MIN_Y, v));
}
