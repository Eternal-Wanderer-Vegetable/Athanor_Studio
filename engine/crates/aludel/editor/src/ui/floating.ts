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

//! 浮动格式条：非空文本选区出现时浮在选区上方；命令与 ribbon 同投影，
//! 点击不抢占选区（mousedown preventDefault），Escape/选区清空时关闭。

import type { CommandContext, CommandRegistry } from "../app/command-registry";
import type { SelectionContext } from "../app/format";
import { cmdButton, el } from "./dom";
import type { OverlayController } from "./overlay";

export interface FloatingDeps {
  registry: CommandRegistry;
  overlays: OverlayController;
  ctx: () => CommandContext;
  sel: () => SelectionContext | null;
  runCommand: (id: string) => void;
  /** 选区矩形（编辑器坐标 → 页面坐标）；无选区返回 null。 */
  selectionRect: () => { left: number; top: number; width: number } | null;
  /** 浮动条内的 picker 控件工厂（字体/字号/颜色）。 */
  makePicker?: (cmdId: string) => HTMLElement | null;
}

export class FloatingBar {
  private bar: HTMLElement | null = null;

  constructor(private deps: FloatingDeps) {}

  /** 每次选区变化后调用；kind != text 时隐藏。 */
  refresh(): void {
    const sel = this.deps.sel();
    const shouldShow = sel?.kind === "text" && (this.deps.ctx().hasDocument);
    if (!shouldShow) {
      this.hide();
      return;
    }
    const rect = this.deps.selectionRect();
    if (!rect) {
      this.hide();
      return;
    }
    if (!this.bar) {
      this.bar = el("div", "floating-bar");
      this.bar.id = "floating-bar";
      this.bar.setAttribute("role", "toolbar");
      this.bar.setAttribute("aria-label", "格式");
      // 鼠标按下不转移焦点/不清选区（Word 浮动条行为）
      this.bar.addEventListener("mousedown", (e) => e.preventDefault());
      const ctx = this.deps.ctx();
      for (const cmd of this.deps.registry.floating(ctx)) {
        if (cmd.id.startsWith("format.") && this.deps.makePicker &&
            ["format.font", "format.size", "format.color", "format.highlight"].includes(cmd.id)) {
          const picker = this.deps.makePicker(cmd.id);
          if (picker) {
            this.bar.appendChild(picker);
            continue;
          }
        }
        this.bar.appendChild(cmdButton(null, cmd.id, cmd.label, cmd.shortcut));
      }
      document.body.appendChild(this.bar);
    }
    // 定位：选区上方居中；放不下就放到下方
    const bar = this.bar;
    bar.style.left = "0px";
    bar.style.top = "0px";
    const bw = bar.offsetWidth || 280;
    const bh = bar.offsetHeight || 34;
    let left = rect.left + rect.width / 2 - bw / 2;
    left = Math.max(8, Math.min(left, window.innerWidth - bw - 8));
    let top = rect.top - bh - 8;
    if (top < 8) top = rect.top + 28;
    bar.style.left = `${left}px`;
    bar.style.top = `${top}px`;
  }

  hide(): void {
    this.bar?.remove();
    this.bar = null;
  }

  get isVisible(): boolean {
    return this.bar !== null;
  }
}
