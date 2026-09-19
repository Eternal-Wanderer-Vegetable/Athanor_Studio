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

//! 功能区：固定页签（开始/插入/视图/文档检查）+ 上下文页签（表格，仅
//! 选区在表格内出现）。组由 registry 投影；容器过窄时低优先级组收进
//! “更多”溢出菜单（命令不消失，仍可达），不产生横向滚动条。
//! 页签选择/折叠状态为 shell 偏好（localStorage），不进文档状态。

import {
  RIBBON_TABS,
  type CommandContext,
  type CommandRegistry,
  type RibbonTab,
} from "../app/command-registry";
import type { SelectionContext } from "../app/format";
import { $, cmdButton, el, loadPref, savePref } from "./dom";
import type { OverlayController } from "./overlay";

export interface RibbonDeps {
  registry: CommandRegistry;
  overlays: OverlayController;
  ctx: () => CommandContext;
  sel: () => SelectionContext | null;
  runCommand: (id: string, from?: HTMLElement) => void;
  /** 刷新后由 shell 调（组内还有 picker 控件需要回显）。 */
  onAfterRefresh?: () => void;
}

const PREF_TAB = "az.ribbon.tab";
const PREF_COLLAPSED = "az.ribbon.collapsed";

export class Ribbon {
  private activeTab: RibbonTab = "home";
  private collapsed = false;
  private builtSignature = "";
  private overflowedGroups: { name: string; commands: string[] }[] = [];
  private overflowBtn: HTMLButtonElement | null = null;
  private resizeObserver: ResizeObserver | null = null;

  constructor(private deps: RibbonDeps) {
    const savedTab = loadPref<string>(PREF_TAB, "home");
    if (RIBBON_TABS.some((t) => t.id === savedTab && !t.contextual)) {
      this.activeTab = savedTab as RibbonTab;
    }
    this.collapsed = loadPref<boolean>(PREF_COLLAPSED, false);
  }

  build(): void {
    this.buildTabs();
    this.refresh(true);
    this.resizeObserver = new ResizeObserver(() => this.applyOverflow());
    this.resizeObserver.observe($("ribbon-panel"));
  }

  // ---------------------------------------------------------------- 页签

  private buildTabs(): void {
    const bar = $("ribbon-tabs");
    bar.replaceChildren();
    for (const t of RIBBON_TABS) {
      const tab = el("button");
      tab.type = "button";
      tab.id = `tab-${t.id}`;
      tab.setAttribute("role", "tab");
      tab.textContent = t.label;
      tab.dataset.tab = t.id;
      if (t.contextual) tab.classList.add("contextual");
      tab.addEventListener("click", () => this.selectTab(t.id));
      tab.addEventListener("keydown", (e) => {
        if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
        e.preventDefault();
        const visible = this.visibleTabs();
        const i = visible.indexOf(this.activeTab);
        const next = visible[(i + (e.key === "ArrowRight" ? 1 : -1) + visible.length) % visible.length];
        this.selectTab(next);
        ($(`tab-${next}`) as HTMLElement | null)?.focus();
      });
      bar.appendChild(tab);
    }
    const collapse = el("button", "collapse-btn");
    collapse.type = "button";
    collapse.id = "ribbon-collapse";
    collapse.dataset.cmd = "view.ribbonCollapse";
    collapse.title = "折叠/展开功能区";
    collapse.textContent = "▴";
    bar.appendChild(collapse);
  }

  private visibleTabs(): RibbonTab[] {
    const sel = this.deps.sel();
    const ctx = this.deps.ctx();
    // 上下文页签按选区；固定页签需至少一个可见命令——没有真实命令的页签
    // 不渲染（能力诚实：不放空壳入口）。
    return RIBBON_TABS
      .filter((t) => (t.contextual ? sel?.inTable : this.deps.registry.byTab(t.id, ctx).length > 0))
      .map((t) => t.id);
  }

  private selectTab(tab: RibbonTab): void {
    if (!this.visibleTabs().includes(tab)) tab = "home";
    this.activeTab = tab;
    if (!RIBBON_TABS.find((t) => t.id === tab)?.contextual) savePref(PREF_TAB, tab);
    if (this.collapsed) {
      // 点页签展开功能区（Word 行为）
      this.collapsed = false;
      savePref(PREF_COLLAPSED, false);
    }
    this.refresh(true);
  }

  /** 上下文页签出现/消失时自动进出 Table（边沿触发：进入表格自动选中，
   *  表格内手动切走后不被刷新拉回；离开表格恢复之前页签）。 */
  private reconcileContextTab(): void {
    const inTable = this.deps.sel()?.inTable ?? false;
    if (inTable && !this.lastInTable && this.activeTab !== "table") {
      this.contextReturnTab = this.activeTab;
      this.activeTab = "table";
    } else if (!inTable && this.lastInTable && this.activeTab === "table") {
      this.activeTab = this.contextReturnTab;
    }
    this.lastInTable = inTable;
  }

  private contextReturnTab: RibbonTab = "home";
  private lastInTable = false;

  /** 程序化选中页签（picker 命令/测试可达；上下文页签按可见性校验）。 */
  activateTab(tab: RibbonTab): void {
    this.selectTab(tab);
  }

  /** 折叠/展开功能区（view.ribbonCollapse 命令与 ▴ 按钮共用）。 */
  toggleCollapse(): void {
    this.collapsed = !this.collapsed;
    savePref(PREF_COLLAPSED, this.collapsed);
    this.refresh(true);
  }

  // ---------------------------------------------------------------- 面板

  /** 签名：tab + 可见命令 id 集 + 折叠态 + 表格上下文；变化才重建 DOM。 */
  private signature(ctx: CommandContext): string {
    const cmds = this.deps.registry.byTab(this.activeTab, ctx).map((c) => c.id).join(",");
    const inTable = this.deps.sel()?.inTable ?? false;
    return `${this.activeTab}|${this.collapsed}|${inTable}|${cmds}`;
  }

  refresh(force = false): void {
    this.reconcileContextTab();
    const ctx = this.deps.ctx();
    const sig = this.signature(ctx);
    if (!force && sig === this.builtSignature) {
      // 即使不重建也复测溢出（CSS 晚到/容器变宽时的自愈路径）
      this.applyOverflow();
      this.deps.onAfterRefresh?.();
      return;
    }
    this.builtSignature = sig;

    // 页签态
    const visible = this.visibleTabs();
    for (const t of RIBBON_TABS) {
      const tab = $(`tab-${t.id}`);
      const isVisible = visible.includes(t.id);
      tab.style.display = isVisible ? "" : "none";
      tab.setAttribute("aria-selected", t.id === this.activeTab ? "true" : "false");
      tab.tabIndex = t.id === this.activeTab ? 0 : -1;
    }

    const panel = $("ribbon-panel");
    panel.classList.toggle("collapsed", this.collapsed);
    ($("ribbon-collapse") as HTMLElement).textContent = this.collapsed ? "▾" : "▴";
    if (this.collapsed) {
      panel.replaceChildren();
      this.deps.onAfterRefresh?.();
      return;
    }
    panel.setAttribute("role", "tabpanel");
    panel.setAttribute("aria-label", RIBBON_TABS.find((t) => t.id === this.activeTab)?.label ?? "");

    panel.replaceChildren();
    this.overflowedGroups = [];
    for (const g of this.deps.registry.groupsFor(this.activeTab, ctx)) {
      const group = el("div", "group");
      group.dataset.group = g.name;
      for (const cmd of g.commands) {
        // picker 型命令（字体/字号/颜色/高亮）由 shell 渲染专属控件
        if (cmd.id === "format.font" || cmd.id === "format.size" ||
            cmd.id === "format.color" || cmd.id === "format.highlight") {
          group.appendChild(this.pickerSlot(cmd.id));
          continue;
        }
        group.appendChild(cmdButton(cmd.elId ?? null, cmd.id, cmd.label, cmd.shortcut));
      }
      panel.appendChild(group);
    }
    // 溢出按钮（占位；applyOverflow 决定是否可见）
    const ob = el("button", "overflow-btn") as HTMLButtonElement;
    ob.type = "button";
    ob.id = "ribbon-overflow";
    ob.textContent = "更多 ▾";
    ob.title = "更多命令";
    ob.style.display = "none";
    ob.addEventListener("click", () => this.openOverflow(ob));
    panel.appendChild(ob);
    this.overflowBtn = ob;

    // picker 控件先落位再测量溢出（否则空槽位测出错误宽度）
    this.deps.onAfterRefresh?.();
    this.applyOverflow();
  }

  /** 字体/字号/颜色/高亮的专属控件槽位（shell.fillPickers 填充真实控件）。 */
  private pickerSlot(cmdId: string): HTMLElement {
    const slot = el("span", "picker-slot");
    slot.dataset.picker = cmdId;
    return slot;
  }

  /** 容器宽度不足时把低优先级组（groupOrder 大者）标记为溢出。 */
  private applyOverflow(): void {
    const panel = $("ribbon-panel");
    if (this.collapsed) return;
    const groups = [...panel.querySelectorAll<HTMLElement>(".group")];
    const overflow = this.overflowBtn;
    if (!overflow) return;
    // 先全部展开并记录真实宽度（隐藏后 offsetWidth=0，不能边收边量）
    groups.forEach((g) => g.classList.remove("overflowed"));
    overflow.style.display = "none";
    this.overflowedGroups = [];
    const available = panel.clientWidth - 4;
    if (available <= 0) {
      // 布局尚未就绪（CSS 晚注入等）：下一帧重测，不把全组误收
      requestAnimationFrame(() => this.applyOverflow());
      return;
    }
    const widths = groups.map((g) => g.offsetWidth);
    const overflowReserve = 76; // “更多”按钮宽度预算
    let used = widths.reduce((sum, w) => sum + w, 0);
    const reserve = () => (this.overflowedGroups.length ? overflowReserve : 0);
    // 从最后一组（优先级最低）开始收，直到剩余组 + 溢出按钮能放下
    for (let i = groups.length - 1; i >= 0 && used + reserve() > available; i--) {
      const g = groups[i];
      g.classList.add("overflowed");
      used -= widths[i];
      this.overflowedGroups.unshift({
        name: g.dataset.group ?? "",
        commands: [...g.querySelectorAll<HTMLElement>("[data-cmd]")].map((b) => b.dataset.cmd!),
      });
    }
    overflow.style.display = this.overflowedGroups.length ? "" : "none";
  }

  private openOverflow(anchor: HTMLButtonElement): void {
    const menu = el("div", "az-menu ribbon-overflow-menu");
    menu.setAttribute("role", "menu");
    for (const g of this.overflowedGroups) {
      if (g.name) menu.appendChild(el("div", "group-title", g.name));
      for (const id of g.commands) {
        const cmd = this.deps.registry.get(id);
        if (!cmd) continue;
        const it = cmdButton(null, cmd.id, cmd.label, cmd.shortcut);
        it.setAttribute("role", "menuitem");
        it.style.display = "flex";
        it.style.width = "100%";
        it.addEventListener("click", () => {
          this.deps.overlays.close(false);
          this.deps.runCommand(cmd.id, it);
        });
        menu.appendChild(it);
      }
    }
    const r = anchor.getBoundingClientRect();
    menu.style.right = `${Math.max(4, window.innerWidth - r.right)}px`;
    menu.style.top = `${r.bottom + 2}px`;
    menu.style.position = "fixed";
    this.deps.overlays.show(menu, { restoreFocus: anchor, anchors: [anchor] });
  }
}
