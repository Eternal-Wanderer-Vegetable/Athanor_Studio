// This file is part of Athanor, the Azodoc document engine.
// Copyright (C) 2026 The Athanor Studio Developers
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published
// by the Free Software Foundation, version 3 of the License only.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Shell 状态：主题、左右面板开合、功能区页签/折叠的偏好持久化
//! （ShellState 只进 localStorage，不进 azodoc/dirty/undo）。
//! 另提供字体/字号/颜色/高亮 picker 控件——ribbon 槽位与浮动条共用，
//! 值仍经 applyChar → PM transaction 写入。

import type { CharacterFormat, ParagraphFormat, SelectionContext } from "../app/format";
import { $, el, loadPref, savePref } from "./dom";

const PREF_THEME = "az.theme";
const PREF_LEFT = "az.panel.left";
const PREF_RIGHT = "az.panel.right";

export type NavSection = "outline" | "pages" | "comments";

const FONT_OPTIONS = ["", "宋体", "黑体", "微软雅黑", "仿宋", "楷体", "SimSun", "Georgia", "Consolas"];
const SIZE_OPTIONS = ["", "9", "10.5", "12", "14", "16", "18", "22", "26", "36"];
const LINE_HEIGHT_OPTIONS = ["", "1.0", "1.15", "1.5", "1.75", "2.0", "2.5", "3.0"];
/** 样式选择器：仅列出 schema 已有命令的块类型（无 schema 支撑的样式不出现）。 */
const STYLE_OPTIONS: { value: string; label: string }[] = [
  { value: "", label: "样式" },
  { value: "paragraph", label: "正文" },
  { value: "heading1", label: "标题 1" },
  { value: "heading2", label: "标题 2" },
  { value: "heading3", label: "标题 3" },
  { value: "quote", label: "引用" },
  { value: "code_block", label: "代码块" },
];

export interface ShellDeps {
  applyChar: (patch: CharacterFormat) => void;
  /** 段落格式事务（行距等 picker 的写入通道）。 */
  applyPara?: (patch: ParagraphFormat) => void;
  /** 块样式切换（样式选择器 → 既有 block.* 命令，不复制实现）。 */
  runCommand?: (id: string) => void;
  /** 状态栏/检查器刷新（偏好变化也要刷新命令 active 态）。 */
  refreshUI: () => void;
  sel: () => SelectionContext | null;
}

export class Shell {
  theme: "light" | "dark";
  private navSection: NavSection = "outline";

  constructor(private deps: ShellDeps) {
    this.theme = loadPref<"light" | "dark">(PREF_THEME, "light");
    document.documentElement.dataset.theme = this.theme;
    $("left-panel").classList.toggle("closed", !loadPref<boolean>(PREF_LEFT, true));
    $("right-panel").classList.toggle("closed", !loadPref<boolean>(PREF_RIGHT, true));
  }

  get leftOpen(): boolean {
    return !$("left-panel").classList.contains("closed");
  }

  get rightOpen(): boolean {
    return !$("right-panel").classList.contains("closed");
  }

  toggleOutline(): void {
    const open = !this.leftOpen;
    $("left-panel").classList.toggle("closed", !open);
    savePref(PREF_LEFT, open);
    this.deps.refreshUI();
  }

  get nav(): NavSection {
    return this.navSection;
  }

  /** 切到指定导航区并确保左面板打开（rail/页签共用入口）。 */
  setNavSection(section: NavSection): void {
    this.navSection = section;
    const panel = $("left-panel");
    if (panel.classList.contains("closed")) {
      panel.classList.remove("closed");
      savePref(PREF_LEFT, true);
    }
    for (const s of ["outline", "pages", "comments"] as const) {
      const active = s === section;
      const tab = document.getElementById(`nav-tab-${s}`);
      tab?.setAttribute("aria-selected", active ? "true" : "false");
      const sec = document.getElementById(`nav-section-${s}`);
      if (sec) sec.hidden = !active;
    }
    this.deps.refreshUI();
  }

  toggleInspector(): void {
    const open = !this.rightOpen;
    $("right-panel").classList.toggle("closed", !open);
    savePref(PREF_RIGHT, open);
    this.deps.refreshUI();
  }

  toggleTheme(): void {
    this.theme = this.theme === "dark" ? "light" : "dark";
    document.documentElement.dataset.theme = this.theme;
    savePref(PREF_THEME, this.theme);
    this.deps.refreshUI();
  }

  // ---------------------------------------------------------------- picker 控件

  /** picker 控件：prefix "tb" = ribbon 稳定 id（回归选择器依赖），"fb" = 浮动条。 */
  createPicker(cmdId: string, prefix: string): HTMLElement | null {
    switch (cmdId) {
      case "format.font": {
        const sel = el("select") as HTMLSelectElement;
        sel.id = `${prefix}-font`;
        sel.title = "字体";
        sel.setAttribute("aria-label", "字体");
        for (const f of FONT_OPTIONS) {
          sel.appendChild(new Option(f || (prefix === "fb" ? "字体" : "字体"), f));
        }
        sel.addEventListener("change", () => {
          this.deps.applyChar({ fontFamily: sel.value || undefined });
        });
        return sel;
      }
      case "format.size": {
        const sel = el("select") as HTMLSelectElement;
        sel.id = `${prefix}-size`;
        sel.title = "字号 (pt)";
        sel.setAttribute("aria-label", "字号");
        for (const s of SIZE_OPTIONS) {
          sel.appendChild(new Option(s || "字号", s));
        }
        sel.addEventListener("change", () => {
          const v = Number(sel.value);
          this.deps.applyChar({ fontSizePt: Number.isFinite(v) && v > 0 ? v : undefined });
        });
        return sel;
      }
      case "format.color": {
        const input = el("input") as HTMLInputElement;
        input.id = `${prefix}-color`;
        input.type = "color";
        input.title = "文字颜色";
        input.setAttribute("aria-label", "文字颜色");
        input.value = "#000000";
        input.addEventListener("input", () => this.deps.applyChar({ color: input.value }));
        return input;
      }
      case "format.highlight": {
        const input = el("input") as HTMLInputElement;
        input.id = `${prefix}-highlight`;
        input.type = "color";
        input.title = "高亮";
        input.setAttribute("aria-label", "高亮");
        input.value = "#ffff00";
        input.addEventListener("input", () => this.deps.applyChar({ highlight: input.value }));
        return input;
      }
      case "para.lineHeight": {
        const sel = el("select") as HTMLSelectElement;
        sel.id = `${prefix}-lineheight`;
        sel.title = "行距";
        sel.setAttribute("aria-label", "行距");
        for (const s of LINE_HEIGHT_OPTIONS) {
          sel.appendChild(new Option(s || "行距", s));
        }
        sel.addEventListener("change", () => {
          const v = Number(sel.value);
          this.deps.applyPara?.({ lineHeight: Number.isFinite(v) && v > 0 ? v : undefined });
        });
        return sel;
      }
      case "block.style": {
        const sel = el("select") as HTMLSelectElement;
        sel.id = `${prefix}-style`;
        sel.title = "样式";
        sel.setAttribute("aria-label", "样式");
        for (const o of STYLE_OPTIONS) {
          sel.appendChild(new Option(o.label, o.value));
        }
        sel.addEventListener("change", () => {
          // 样式选择器把选项路由到既有 block.* 命令——不复制实现。
          const map: Record<string, string> = {
            paragraph: "block.para", heading1: "block.h1", heading2: "block.h2",
            heading3: "block.h3", quote: "block.quote", code_block: "block.codeblock",
          };
          const cmdId = map[sel.value];
          if (cmdId) this.deps.runCommand?.(cmdId);
        });
        return sel;
      }
      default:
        return null;
    }
  }

  /** 把 picker 控件填入 ribbon 的槽位（refresh 后调用，幂等）。 */
  fillPickerSlots(): void {
    document.querySelectorAll<HTMLElement>(".picker-slot").forEach((slot) => {
      if (slot.childElementCount > 0) return;
      const ctl = this.createPicker(slot.dataset.picker ?? "", "tb");
      if (ctl) slot.appendChild(ctl);
    });
  }

  /** 选区格式反射到所有 picker（混合键回退为空值；控件聚焦时不回写）。 */
  reflectPickers(): void {
    const sel = this.deps.sel();
    const cf = sel?.character;
    const mixed = new Set(cf?.mixed ?? []);
    const baseTitle = (node: HTMLElement): string =>
      (node.dataset.baseTitle ??= node.title || node.getAttribute("aria-label") || "");
    // select：混合态回退为空选项，附混合标记
    const setSelect = (id: string, value: string, isMixed: boolean) => {
      const node = document.getElementById(id) as HTMLSelectElement | null;
      if (!node || document.activeElement === node) return;
      node.value = isMixed ? "" : value;
      node.title = isMixed ? `${baseTitle(node)}（混合）` : baseTitle(node);
      node.classList.toggle("mixed-value", isMixed);
    };
    // input[type=color]：不能写空串——混合态只打标记，不把旧选区颜色
    // 当当前值显示；统一值/无值时才回写或清除标记。
    const setColor = (id: string, value: string | undefined, isMixed: boolean) => {
      const node = document.getElementById(id) as HTMLInputElement | null;
      if (!node || document.activeElement === node) return;
      node.title = isMixed ? `${baseTitle(node)}（混合）` : baseTitle(node);
      node.classList.toggle("mixed-value", isMixed);
      node.dataset.mixed = isMixed ? "true" : "";
      if (!isMixed && value) node.value = value;
    };
    setSelect("tb-font", cf?.values.fontFamily ?? "", mixed.has("fontFamily"));
    setSelect("tb-size", cf?.values.fontSizePt !== undefined ? String(cf!.values.fontSizePt) : "", mixed.has("fontSizePt"));
    setSelect("fb-font", cf?.values.fontFamily ?? "", mixed.has("fontFamily"));
    setSelect("fb-size", cf?.values.fontSizePt !== undefined ? String(cf!.values.fontSizePt) : "", mixed.has("fontSizePt"));
    setColor("tb-color", cf?.values.color, mixed.has("color"));
    setColor("fb-color", cf?.values.color, mixed.has("color"));
    setColor("tb-highlight", cf?.values.highlight, mixed.has("highlight"));
    setColor("fb-highlight", cf?.values.highlight, mixed.has("highlight"));
    // 段落/样式 picker：同一 SelectionContext 的归并视图
    const pf = sel?.paragraph;
    const pMixed = new Set(pf?.mixed ?? []);
    setSelect("tb-lineheight",
      pf?.values.lineHeight !== undefined ? String(pf!.values.lineHeight) : "",
      pMixed.has("lineHeight"));
    setSelect("tb-style", sel?.blockType ?? "", false);
  }
}
