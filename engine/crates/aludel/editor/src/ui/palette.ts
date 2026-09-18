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

//! 命令面板（Ctrl+K / 顶栏搜索框）：同一 registry 的 palette 投影，
//! 禁用命令显示原因；↑↓/Enter/Escape 键盘闭环。

import type { CommandContext, CommandRegistry } from "../app/command-registry";
import { el } from "./dom";
import type { OverlayController } from "./overlay";

export interface PaletteDeps {
  registry: CommandRegistry;
  overlays: OverlayController;
  ctx: () => CommandContext;
  runCommand: (id: string) => void;
}

export class CommandPalette {
  private input: HTMLInputElement | null = null;
  private list: HTMLElement | null = null;
  private selected = 0;
  private candidates: string[] = [];

  constructor(private deps: PaletteDeps) {}

  get isOpen(): boolean {
    return this.deps.overlays.active?.classList.contains("cmdk-overlay") ?? false;
  }

  open(): void {
    if (this.isOpen) return;
    const wrap = el("div", "cmdk-overlay");
    wrap.id = "cmdk-overlay";
    const box = el("div", "cmdk");
    box.setAttribute("role", "dialog");
    box.setAttribute("aria-label", "命令面板");
    const input = el("input") as HTMLInputElement;
    input.id = "cmdk-search";
    input.type = "text";
    input.placeholder = "输入命令名或关键词…";
    input.setAttribute("role", "combobox");
    input.setAttribute("aria-expanded", "true");
    input.setAttribute("aria-controls", "cmdk-list");
    const list = el("ul");
    list.id = "cmdk-list";
    list.setAttribute("role", "listbox");
    box.append(input, list);
    wrap.appendChild(box);
    this.input = input;
    this.list = list;

    input.addEventListener("input", () => this.render());
    input.addEventListener("keydown", (e) => this.onKeydown(e));
    wrap.addEventListener("pointerdown", (e) => {
      if (e.target === wrap) this.deps.overlays.close();
    });
    this.deps.overlays.show(wrap);
    this.render();
    input.focus();
  }

  private render(): void {
    const q = (this.input?.value ?? "").trim().toLowerCase();
    const ctx = this.deps.ctx();
    const all = this.deps.registry.paletteCandidates(ctx);
    const matched = all.filter((c) => {
      if (!q) return true;
      const hay = `${c.id} ${c.label} ${(c.keywords ?? []).join(" ")}`.toLowerCase();
      return q.split(/\s+/).every((tok) => hay.includes(tok));
    });
    this.candidates = matched.map((c) => c.id);
    this.selected = 0;
    const list = this.list;
    if (!list) return;
    list.replaceChildren();
    if (matched.length === 0) {
      list.appendChild(el("div", "empty", "没有匹配的命令"));
      return;
    }
    matched.slice(0, 50).forEach((cmd, i) => {
      const li = el("li");
      li.setAttribute("role", "option");
      li.dataset.cmd = cmd.id;
      li.setAttribute("aria-selected", i === this.selected ? "true" : "false");
      const enabled = cmd.enabled(ctx);
      if (!enabled) li.classList.add("disabled");
      const label = el("span", undefined, cmd.label);
      li.appendChild(label);
      const hint = enabled ? (cmd.shortcut ?? "") : (cmd.disabledReason?.(ctx) ?? "不可用");
      if (hint) li.appendChild(el("span", "hint", hint));
      li.addEventListener("click", () => this.pick(i));
      li.addEventListener("pointerenter", () => this.select(i));
      list.appendChild(li);
    });
  }

  private select(i: number): void {
    this.selected = i;
    this.list?.querySelectorAll("li").forEach((li, j) => {
      li.setAttribute("aria-selected", j === i ? "true" : "false");
    });
  }

  private pick(i: number): void {
    const id = this.candidates[i];
    if (!id) return;
    const cmd = this.deps.registry.get(id);
    if (!cmd || !cmd.enabled(this.deps.ctx())) return;
    this.deps.overlays.close(false);
    this.deps.runCommand(id);
  }

  private onKeydown(e: KeyboardEvent): void {
    const n = this.candidates.length;
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        if (n) this.select((this.selected + 1) % Math.min(n, 50));
        break;
      case "ArrowUp":
        e.preventDefault();
        if (n) this.select((this.selected - 1 + Math.min(n, 50)) % Math.min(n, 50));
        break;
      case "Enter":
        e.preventDefault();
        this.pick(this.selected);
        break;
      case "Escape":
        // 交给 OverlayController 统一路由
        break;
    }
  }
}
