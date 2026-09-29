/**
 * i18n.ts
 *
 * 极简多语言支持：当前支持 en / zh，语言统一从 settings.json 读取（默认按系统区域检测）。
 * 覆盖 UI 文案：编辑入口、图层切换、Done 等按钮，保证同一界面语言一致。
 */

export type Lang = "en" | "zh";

const messages = {
  en: {
    edit: "✎ Edit",
    layerToBack: "⇵ Back",
    layerToFront: "⇵ Top",
    hideCanvas: "◉ Hide",
    showCanvas: "◉ Show",
    done: "✓ Done",
    editBarTitle: "Hover to expand · drag to move",
    enterEditTitle: "Enter edit mode",
    layerToBackTitle: "Move to wallpaper layer (below all windows)",
    layerToFrontTitle: "Move above all windows",
  },
  zh: {
    edit: "✎ 编辑",
    layerToBack: "⇵ 置底",
    layerToFront: "⇵ 置顶",
    hideCanvas: "◉ 隐藏",
    showCanvas: "◉ 显示",
    done: "✓ 完成",
    editBarTitle: "悬停展开 · 拖拽移动",
    enterEditTitle: "进入编辑模式",
    layerToBackTitle: "移到壁纸层（所有窗口之下）",
    layerToFrontTitle: "移到所有窗口之上",
  },
} as const;

export type MessageKey = keyof (typeof messages)["en"];

export function t(lang: Lang, key: MessageKey): string {
  return messages[lang][key];
}

/** 从系统区域检测语言（zh 前缀 -> 中文，否则英文）。 */
export function detectLang(): Lang {
  const l = (typeof navigator !== "undefined" && navigator.language) || "";
  return l.toLowerCase().startsWith("zh") ? "zh" : "en";
}
