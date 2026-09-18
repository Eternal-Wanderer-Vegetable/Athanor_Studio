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

//! 应用菜单（文件/编辑/插入/视图/文档检查）：由 CommandRegistry 投影，
//! 菜单项 role=menuitem（与顶栏 button 区分）；箭头键导航、Esc/外点关闭、
//! 焦点回触发按钮。

import {
  MENU_GROUPS,
  type Command,
  type CommandContext,
  type CommandRegistry,
  type MenuGroup,
} from "../app/command-registry";
import { $, el } from "./dom";
import type { OverlayController } from "./overlay";

export interface MenusDeps {
  registry: CommandRegistry;
  overlays: OverlayController;
  ctx: () => CommandContext;
  runCommand: (id: string, from?: HTMLElement) => void;
}

export class AppMenus {
  private triggers = new Map<MenuGroup, HTMLButtonElement>();

  constructor(private deps: MenusDeps) {}

  /** 构建 menubar 触发按钮（一次性；菜单内容每次打开时按当前 ctx 投影）。 */
  build(): void {
    const bar = $("app-menus");
    bar.replaceChildren();
    this.triggers.clear();
    for (const g of MENU_GROUPS) {
      const btn = el("button", "menu-group");
      btn.type = "button";
      btn.id = `menu-${g.id}`;
      btn.textContent = g.label;
      btn.setAttribute("aria-haspopup", "menu");
      btn.setAttribute("aria-expanded", "false");
      btn.dataset.menuGroup = g.id;
      btn.addEventListener("click", () => this.toggle(g.id, btn));
      bar.appendChild(btn);
      this.triggers.set(g.id, btn);
    }
  }

  /** 能力不可用的菜单组整体隐藏（如 HTTP 模式没有文件对话框能力时文件菜单收窄）。 */
  refresh(): void {
    const ctx = this.deps.ctx();
    for (const [group, btn] of this.triggers) {
      const visible = this.deps.registry.byMenu(group, ctx).length > 0;
      btn.style.display = visible ? "" : "none";
    }
  }

  private toggle(group: MenuGroup, trigger: HTMLButtonElement): void {
    const open = this.deps.overlays.active;
    if (open?.dataset.menuGroup === group) {
      this.deps.overlays.close();
      return;
    }
    this.openMenu(group, trigger);
  }

  private openMenu(group: MenuGroup, trigger: HTMLButtonElement): void {
    const menu = el("div", "az-menu");
    menu.setAttribute("role", "menu");
    menu.dataset.menuGroup = group;
    const cmds = this.deps.registry.byMenu(group, this.deps.ctx());
    let lastSep = -1;
    cmds.forEach((cmd, i) => {
      // 同 menuOrder 分组前缀变化时插分隔线（file/job 等命名空间边界）
      const ns = cmd.id.split(".")[0];
      const prevNs = i > 0 ? cmds[i - 1].id.split(".")[0] : ns;
      if (i > 0 && ns !== prevNs && lastSep !== i - 1) {
        menu.appendChild(el("div", "menu-sep"));
      }
      lastSep = i;
      menu.appendChild(this.item(cmd));
    });
    if (cmds.length === 0) {
      const empty = el("div", "menu-sep");
      menu.appendChild(empty);
    }

    // 定位：触发按钮下方左对齐
    const r = trigger.getBoundingClientRect();
    menu.style.left = `${Math.max(4, r.left)}px`;
    menu.style.top = `${r.bottom + 2}px`;
    trigger.setAttribute("aria-expanded", "true");

    menu.addEventListener("keydown", (e) => this.onMenuKeydown(e, menu));
    this.deps.overlays.show(menu, {
      restoreFocus: trigger,
      anchors: [trigger],
      onClose: () => trigger.setAttribute("aria-expanded", "false"),
    });
    (menu.querySelector<HTMLElement>("[role=menuitem]:not(:disabled)") ?? menu).focus?.();
    const first = menu.querySelector<HTMLElement>("[role=menuitem]");
    first?.classList.add("focus");
  }

  private item(cmd: Command): HTMLButtonElement {
    const it = el("button") as HTMLButtonElement;
    it.type = "button";
    if (cmd.menuId) it.id = cmd.menuId;
    it.setAttribute("role", "menuitem");
    it.dataset.cmd = cmd.id;
    const ctx = this.deps.ctx();
    const enabled = cmd.enabled(ctx);
    if (!enabled) {
      it.disabled = true;
      const reason = cmd.disabledReason?.(ctx);
      if (reason) it.title = reason;
    }
    const label = el("span", undefined, cmd.label);
    it.appendChild(label);
    if (cmd.shortcut) it.appendChild(el("span", "menu-shortcut", cmd.shortcut));
    it.addEventListener("click", () => {
      this.deps.overlays.close(false);
      this.deps.runCommand(cmd.id);
    });
    return it;
  }

  private onMenuKeydown(e: KeyboardEvent, menu: HTMLElement): void {
    const items = [...menu.querySelectorAll<HTMLElement>("[role=menuitem]:not(:disabled)")];
    const idx = items.findIndex((it) => it.classList.contains("focus"));
    const move = (next: number) => {
      if (items.length === 0) return;
      const target = items[((next % items.length) + items.length) % items.length];
      items.forEach((it) => it.classList.remove("focus"));
      target.classList.add("focus");
      target.focus();
    };
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        move(idx < 0 ? 0 : idx + 1);
        break;
      case "ArrowUp":
        e.preventDefault();
        move(idx < 0 ? items.length - 1 : idx - 1);
        break;
      case "Home":
        e.preventDefault();
        move(0);
        break;
      case "End":
        e.preventDefault();
        move(items.length - 1);
        break;
      case "ArrowLeft":
      case "ArrowRight": {
        e.preventDefault();
        const order = MENU_GROUPS.map((g) => g.id);
        const cur = order.indexOf(menu.dataset.menuGroup as MenuGroup);
        const dir = e.key === "ArrowRight" ? 1 : -1;
        const next = order[(cur + dir + order.length) % order.length];
        const trigger = this.triggers.get(next);
        if (trigger && trigger.style.display !== "none") this.openMenu(next, trigger);
        break;
      }
      case "Enter":
      case " ": {
        e.preventDefault();
        const focused = items[idx];
        focused?.click();
        break;
      }
      case "Tab":
        this.deps.overlays.close();
        break;
      case "Escape":
        // Escape 由 document 级 OverlayController 路由统一处理
        break;
    }
  }

  /** 菜单是否打开（main.ts 的 Esc/外点路由用）。 */
  get isOpen(): boolean {
    return this.deps.overlays.active?.classList.contains("az-menu") ?? false;
  }
}
