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

import type { CharacterFormat, SelectionContext } from "../app/format";
import { $, el, loadPref, savePref } from "./dom";

const PREF_THEME = "az.theme";
const PREF_LEFT = "az.panel.left";
const PREF_RIGHT = "az.panel.right";

const FONT_OPTIONS = ["", "宋体", "黑体", "微软雅黑", "仿宋", "楷体", "SimSun", "Georgia", "Consolas"];
const SIZE_OPTIONS = ["", "9", "10.5", "12", "14", "16", "18", "22", "26", "36"];

export interface ShellDeps {
  applyChar: (patch: CharacterFormat) => void;
  /** 状态栏/检查器刷新（偏好变化也要刷新命令 active 态）。 */
  refreshUI: () => void;
  sel: () => SelectionContext | null;
}

export class Shell {
  theme: "light" | "dark";

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
    const set = (id: string, value: string, isMixed: boolean) => {
      const node = document.getElementById(id) as HTMLInputElement | HTMLSelectElement | null;
      if (!node || document.activeElement === node) return;
      node.value = isMixed ? "" : value;
      node.title = isMixed ? "混合" : node.title.split("（混合）")[0];
      node.classList.toggle("mixed-value", isMixed);
    };
    set("tb-font", cf?.values.fontFamily ?? "", mixed.has("fontFamily"));
    set("tb-size", cf?.values.fontSizePt !== undefined ? String(cf!.values.fontSizePt) : "", mixed.has("fontSizePt"));
    set("fb-font", cf?.values.fontFamily ?? "", mixed.has("fontFamily"));
    set("fb-size", cf?.values.fontSizePt !== undefined ? String(cf!.values.fontSizePt) : "", mixed.has("fontSizePt"));
    const colEl = document.getElementById("tb-color") as HTMLInputElement | null;
    if (colEl && document.activeElement !== colEl && cf?.values.color) colEl.value = cf.values.color;
    const hlEl = document.getElementById("tb-highlight") as HTMLInputElement | null;
    if (hlEl && document.activeElement !== hlEl && cf?.values.highlight) hlEl.value = cf.values.highlight;
  }
}
