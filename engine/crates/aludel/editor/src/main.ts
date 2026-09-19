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

//! 装配层：平台 gateway + 会话控制器 + 命令注册表 + shell 组件接线。
//! 菜单/功能区/命令面板/浮动条/状态栏全部投影同一 CommandRegistry；
//! 命令定义按领域拆到 app/commands/*，注册与焦点契约只此一处；
//! 业务状态在 app/session-store + document-controller；渲染在 ui/*。

import { TextSelection } from "prosemirror-state";
import "prosemirror-view/style/prosemirror.css";
import "prosemirror-tables/style/tables.css";
import "./ui/tokens.css";
import "./ui/shell.css";
import { DocumentController, errText } from "./app/document-controller";
import {
  buildCommandContext,
  CommandRegistry,
  MENU_LABELS,
  type Command,
  type CommandContext,
  type FocusPolicy,
} from "./app/command-registry";
import { findAll, selectMatch, replaceCurrent, replaceAll } from "./app/find";
import { PreviewSurface } from "./app/preview";
import {
  setParagraphFormat,
  setCharacterFormat,
  selectionContext,
  type SelectionContext,
} from "./app/format";
import type { RegDef, RegOpts } from "./app/commands/types";
import type { CommandDeps } from "./app/commands/deps";
import { fileCommands } from "./app/commands/file";
import { editCommands } from "./app/commands/edit";
import { formatCommands } from "./app/commands/format";
import { insertCommands } from "./app/commands/insert";
import { layoutViewCommands } from "./app/commands/view";
import { toolsCommands } from "./app/commands/tools";
import { tableCommands } from "./app/commands/table";
import { TauriGateway } from "./platform/tauri";
import { HttpGateway } from "./platform/http";
import { $, setMsg } from "./ui/dom";
import { OverlayController } from "./ui/overlay";
import { AppMenus } from "./ui/menus";
import { Ribbon } from "./ui/ribbon";
import { CommandPalette } from "./ui/palette";
import { FloatingBar } from "./ui/floating";
import { Shell } from "./ui/shell";
import { renderNavPages } from "./ui/nav-panel";
import { renderOutline } from "./ui/outline-panel";
import { renderReviewSections } from "./ui/review-panel";
import { renderInspector } from "./ui/inspector-panel";
import { renderStatusBar } from "./ui/status-bar";
import { flag } from "./flags";

const tauriMode = "__TAURI_INTERNALS__" in window;
const gateway = tauriMode ? new TauriGateway() : new HttpGateway();

let lastChars = 0;
let zoom = 1;
/** 页宽模式前的用户缩放（切回连续模式恢复）。 */
let userZoom = 1;
/** 视图模式：continuous/pageWidth；"page" 由预览浮层承担，不进此变量。 */
let viewMode: "continuous" | "pageWidth" = "continuous";
/** 最近一次成功预览的分页结果（正文一代际变化即失效）。 */
let previewPages: { generation: number; count: number; pageOfBlock: Record<string, number> } | null = null;
/** 预览浮层是否打开（CommandContext.mode = "preview" 的唯一来源）。 */
let previewOpen = false;

const ctl = new DocumentController($("editor"), gateway, {
  onStateChange: refreshUI,
  onDocMeta: (d) => renderReviewSections(d, (rev) => void ctl.checkoutRevision(rev)),
  onMessage: setMsg,
  onOutline: (headings) =>
    renderOutline(headings, (pos) => {
      const view = ctl.view;
      if (!view) return;
      const $pos = view.state.doc.resolve(Math.min(pos + 1, view.state.doc.content.size));
      const sel = TextSelection.near($pos);
      view.dispatch(view.state.tr.setSelection(sel).scrollIntoView());
      view.focus();
    }),
  onStats: (chars) => {
    lastChars = chars;
  },
});

function sel(): SelectionContext | null {
  return ctl.view ? selectionContext(ctl.view) : null;
}

/** chrome 上下文：mode/capability/selection/page 状态的唯一构造点。 */
function ctx(): CommandContext {
  const s = ctl.store.state;
  return buildCommandContext({
    gateway,
    hasDocument: ctl.view !== null,
    dirty: s.dirty,
    busy: s.activeJob !== null || s.saveState === "saving",
    sel: sel(),
    viewMode,
    previewOpen,
    pageCount: previewPageCount(),
  });
}

const registry = new CommandRegistry();
const overlays = new OverlayController();

function runCommand(id: string, from?: HTMLElement): void {
  const cmd = registry.get(id);
  if (!cmd) return;
  void registry.run(id, ctx()).finally(() => {
    applyFocusPolicy(cmd, from);
  });
}

/** 统一焦点契约：即时命令焦点还正文；keep 由命令自身管理；none 不动。 */
function applyFocusPolicy(cmd: { id: string; focusPolicy?: FocusPolicy }, _from?: HTMLElement): void {
  const policy = cmd.focusPolicy ?? "editor";
  if (policy === "editor" && ctl.view && !ctl.composing) ctl.view.focus();
}

// ---------------------------------------------------------------- shell 组件
// （先建组件再注册命令：命令定义只拿 deps，run() 在装配完成后才可达）

const shell = new Shell({
  applyChar,
  applyPara: (p) => applyPara(p),
  runCommand,
  refreshUI,
  sel,
});

const ribbon = new Ribbon({
  registry,
  overlays,
  ctx,
  sel,
  runCommand,
  // 溢出菜单里的 picker 与 ribbon 槽位共用控件工厂（id 前缀 ov-*，
  // 回显仍走 SelectionContext —— reflectPickers 按 picker 前缀名单回写）。
  pickerFor: (cmdId) => shell.createPicker(cmdId, "ov"),
  onAfterRefresh: () => {
    shell.fillPickerSlots();
    shell.reflectPickers();
  },
});

const menus = new AppMenus({ registry, overlays, ctx, runCommand });
const palette = new CommandPalette({ registry, overlays, ctx, runCommand });
const floating = new FloatingBar({
  registry,
  overlays,
  ctx,
  sel,
  runCommand,
  selectionRect: () => {
    const v = ctl.view;
    if (!v || v.state.selection.empty) return null;
    const { from } = v.state.selection;
    const c = v.coordsAtPos(from);
    return { left: c.left, top: c.top, width: 0 };
  },
  makePicker: (cmdId) => shell.createPicker(cmdId, "fb"),
});

// ---------------------------------------------------------------- 命令注册
// 领域模块只返回定义；enabled/visible/reason 的绑定只在 reg() 这一处。

function applyPara(patch: Parameters<typeof setParagraphFormat>[2]): void {
  const v = ctl.view;
  if (v) setParagraphFormat(v.state, v.dispatch, patch);
}

function applyChar(patch: Parameters<typeof setCharacterFormat>[2]): void {
  const v = ctl.view;
  if (v) setCharacterFormat(v.state, v.dispatch, patch);
}

function setZoom(z: number): void {
  zoom = Math.min(2.5, Math.max(0.5, z));
  ($("editor").closest(".page") as HTMLElement | null)?.style.setProperty("zoom", String(zoom));
  refreshUI();
}

/** 页宽视图：算出恰好铺满工作区宽度的缩放（CSS zoom，与手动缩放同一通道）。 */
function setViewMode(mode: "continuous" | "pageWidth" | "page"): void {
  if (mode === "page") { void openPreview(); return; }
  if (mode === viewMode) return;
  const workspace = document.querySelector<HTMLElement>(".workspace");
  const page = document.querySelector<HTMLElement>(".page");
  if (mode === "pageWidth") {
    userZoom = zoom;
    if (workspace && page) {
      // zoom 作用于 .page 的视觉宽度；rect 已是缩放后的值，除回原宽度
      const rawPageWidth = page.getBoundingClientRect().width / zoom;
      if (rawPageWidth > 0) setZoom(Math.floor(workspace.clientWidth / rawPageWidth * 100) / 100);
    }
  } else {
    setZoom(userZoom);
  }
  viewMode = mode;
  refreshUI();
}

/** picker 型命令（字体/字号/颜色/高亮）：菜单/面板触发打开 ribbon 控件，
 *  不执行空 run（命令矩阵 §5）。 */
function openPicker(pickerId: string): void {
  ribbon.activateTab("home");
  setTimeout(() => {
    const node = document.getElementById(pickerId) as HTMLElement | null;
    node?.focus();
    if (node instanceof HTMLSelectElement) node.click();
  }, 0);
}

function newId(prefix: "blk" | "row" | "cel" | "col"): string {
  // Azodoc IDs use Crockford ULID, so newly inserted nodes are valid before
  // the first save (the Rust converter can still repair imported IDs).
  const alphabet = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";
  let time = BigInt(Date.now());
  let suffix = "";
  for (let i = 0; i < 10; i++) {
    suffix = alphabet[Number(time & 31n)] + suffix;
    time >>= 5n;
  }
  const bytes = new Uint8Array(16);
  globalThis.crypto?.getRandomValues(bytes);
  let random = "";
  for (const byte of bytes) random += alphabet[byte & 31];
  return `${prefix}_${suffix}${random}`;
}

const deps: CommandDeps = {
  ctl,
  shell,
  ribbon,
  menus,
  palette,
  overlays,
  registry,
  applyPara,
  applyChar,
  sel,
  getZoom: () => zoom,
  setZoom,
  getViewMode: () => viewMode,
  setViewMode,
  openPreview,
  toggleFindBar,
  openPicker,
  newId,
};

function reg(def: RegDef): void {
  const { id, label, run } = def;
  const opts: RegOpts = def.opts ?? {};
  const caps = opts.caps ?? (opts.desktopOnly ? ["desktopFileDialogs" as const] : undefined);
  const reason = (c: CommandContext): string | null => {
    if (opts.needsDoc !== false && !c.hasDocument) return "无已打开文档";
    if (opts.desktopOnly && !c.desktop) return "需要桌面版本（文件对话框/任务能力）";
    if (caps?.length && !caps.every((k) => c.capabilities[k])) return "当前平台不支持该能力";
    if (opts.needsSelection && !c.hasSelection) return "需要先选择内容";
    if (opts.needsTable && !c.inTable) return "光标需在表格内";
    if (opts.needsJob && !c.busy) return "没有在途任务";
    return typeof opts.disabledReason === "function" ? opts.disabledReason(c) : (opts.disabledReason ?? null);
  };
  const cmd: Command = {
    id,
    label,
    shortcut: opts.shortcut,
    active: opts.active,
    enabled: (c) =>
      (opts.needsDoc === false || c.hasDocument) &&
      (!opts.desktopOnly || c.desktop) &&
      (!opts.needsSelection || c.hasSelection) &&
      (!opts.needsTable || c.inTable) &&
      (!opts.needsJob || c.busy),
    disabledReason: (c) => (registry.get(id)!.enabled(c) ? null : reason(c)),
    // 能力矩阵（§6.6）：能力不满足的命令不渲染入口，不显示死按钮
    visible: opts.desktopOnly ? (c) => c.desktop : undefined,
    requiredCapabilities: caps ? [...caps] : undefined,
    modes: opts.modes,
    run,
    tab: opts.tab,
    group: opts.group,
    order: opts.order,
    groupOrder: opts.groupOrder,
    menu: opts.menu,
    menuOrder: opts.menuOrder,
    menuPath: opts.menu ? `${MENU_LABELS[opts.menu === "review" ? "tools" : opts.menu]} > ${label}` : undefined,
    menuId: opts.menuId,
    elId: opts.elId,
    surfaces: opts.surfaces,
    keywords: opts.keywords,
    focusPolicy: opts.focusPolicy,
  };
  registry.register(cmd);
}

// 领域命令定义：模块只返回定义，绑定全部经 deps 注入。
for (const def of [
  ...fileCommands(deps),
  ...editCommands(deps),
  ...formatCommands(deps),
  ...insertCommands(deps),
  ...layoutViewCommands(deps),
  ...toolsCommands(deps),
  ...tableCommands(deps),
]) {
  reg(def);
}

// ---------------------------------------------------------------- 预览

const previewSurface = new PreviewSurface();

/** 打印预览：当前快照渲染 → 浮层分页 → 页数/快照指纹（不出版）。 */
async function openPreview(): Promise<void> {
  if (!flag("printPreviewV1")) {
    setMsg("打印预览功能已关闭。", true);
    return;
  }
  const result = await ctl.requestPreview();
  if (!result) return;
  const generation = ctl.store.state.editGeneration;
  // 测试可注入短超时（分页回退路径）
  const override = (window as unknown as { __previewTimeoutMs?: number }).__previewTimeoutMs;
  previewOpen = true;
  refreshUI();
  let outcome;
  if (typeof override === "number") {
    const surface = new PreviewSurface(override);
    outcome = await surface.open(result);
  } else {
    outcome = await previewSurface.open(result);
  }
  previewOpen = false;
  previewPages = outcome.fallback || outcome.page_count === null || !outcome.layout
    ? null
    : { generation, count: outcome.page_count, pageOfBlock: outcome.layout.page_of_block };
  if (typeof override !== "number") {
    const breaks = outcome.layout?.page_breaks.length ?? 0;
    if (outcome.fallback) {
      setMsg("预览已打开（未分页回退）——点“打印…”走系统打印。", true);
    } else {
      setMsg(
        `预览已打开：${outcome.page_count ?? "?"} 页` +
          (breaks > 0 ? `（含 ${breaks} 处手动分页）` : "") +
          " ——点“打印…”走系统打印。",
        true,
      );
    }
  }
  refreshUI();
}

/** 最近预览页数（快照代数与当前一致才有效；过期不显示）。 */
function previewPageCount(): number | null {
  if (!previewPages) return null;
  return previewPages.generation === ctl.store.state.editGeneration ? previewPages.count : null;
}

/** 光标所在块的预览页码（同一快照内才有效；无数据返回 null）。 */
function currentPageNumber(): number | null {
  const v = ctl.view;
  if (!v || !previewPages || previewPages.generation !== ctl.store.state.editGeneration) return null;
  const { $from } = v.state.selection;
  for (let d = $from.depth; d >= 0; d--) {
    const id = $from.node(d).attrs?.id as string | undefined;
    if (id && previewPages.pageOfBlock[id] !== undefined) return previewPages.pageOfBlock[id];
  }
  return null;
}

/** 页码 → 该页首个块 id（page_of_block 按文档序注入，首遇即首页）。 */
function navPageList(): { page: number; blockId: string }[] {
  if (!previewPages || previewPages.generation !== ctl.store.state.editGeneration) return [];
  const first = new Map<number, string>();
  for (const [blockId, page] of Object.entries(previewPages.pageOfBlock)) {
    if (!first.has(page)) first.set(page, blockId);
  }
  return [...first.entries()].sort((a, b) => a[0] - b[0]).map(([page, blockId]) => ({ page, blockId }));
}

/** 页面导航：按块 id 跳转到文档位置（同一 selection/scroll 通道）。 */
function gotoBlockId(blockId: string): void {
  const v = ctl.view;
  if (!v) return;
  let pos: number | null = null;
  v.state.doc.descendants((node, p) => {
    if (pos !== null) return false;
    if (node.attrs?.id === blockId) { pos = p; return false; }
    return true;
  });
  if (pos === null) return;
  const $pos = v.state.doc.resolve(Math.min(pos + 1, v.state.doc.content.size));
  v.dispatch(v.state.tr.setSelection(TextSelection.near($pos)).scrollIntoView());
  v.focus();
}

// ---------------------------------------------------------------- 统一刷新

function refreshUI(): void {
  const c = ctx();
  ribbon.refresh();
  menus.refresh();
  // 所有 surface 的同 id 控件（ribbon/menu/浮动条/状态栏/顶栏）一起更新
  for (const cmd of registry.all()) {
    for (const el of document.querySelectorAll<HTMLElement>(`[data-cmd="${cmd.id}"]`)) {
      const visible = registry.isVisible(cmd, c);
      el.style.display = visible ? "" : "none";
      if (el instanceof HTMLButtonElement || el instanceof HTMLSelectElement || el instanceof HTMLInputElement) {
        const enabled = visible && cmd.enabled(c);
        el.disabled = !enabled;
        if (!enabled) {
          const reason = cmd.disabledReason?.(c);
          const base = cmd.shortcut ? `${cmd.label} (${cmd.shortcut})` : cmd.label;
          el.title = reason ? `${base} — ${reason}` : base;
        } else {
          el.title = cmd.shortcut ? `${cmd.label} (${cmd.shortcut})` : cmd.label;
        }
      }
      if (cmd.active) {
        const on = cmd.active();
        el.classList.toggle("active", on);
        if (el instanceof HTMLButtonElement) {
          // 切换态按钮的 a11y 语义（E6）：aria-pressed 与视觉态同步
          el.setAttribute("aria-pressed", on ? "true" : "false");
        }
      } else if (el instanceof HTMLButtonElement && el.getAttribute("aria-pressed")) {
        el.removeAttribute("aria-pressed");
      }
    }
  }
  const s = ctl.store.state;
  renderStatusBar({
    chars: lastChars,
    dirty: s.dirty,
    saveState: s.saveState,
    displayName: s.displayName,
    path: s.path,
    revision: s.revision,
    zoom,
    pageCount: previewPageCount(),
    currentPage: currentPageNumber(),
    viewMode,
  });
  renderInspector(sel());
  renderNavPages({
    pageList: navPageList(),
    valid: previewPageCount() !== null,
    onGoto: gotoBlockId,
  });
  // 格式控件回显（含混合选区归并；IME 组合中不回写）
  if (ctl.view && !ctl.composing) {
    shell.reflectPickers();
  }
  floating.refresh();
}

// ---------------------------------------------------------------- 查找栏

function toggleFindBar(show: boolean, withReplace = false): void {
  const bar = $("findbar");
  bar.classList.toggle("open", show);
  $("find-replace-row").style.display = withReplace ? "flex" : "none";
  if (show) ($("find-input") as HTMLInputElement).focus();
}

function currentMatches() {
  const v = ctl.view;
  if (!v) return [];
  const q = ($("find-input") as HTMLInputElement).value;
  const cs = ($("find-case") as HTMLInputElement).checked;
  return findAll(v.state, q, cs);
}

function updateFindCount(): void {
  const n = currentMatches().length;
  $("find-count").textContent = `${n} 项`;
}

// ---------------------------------------------------------------- 快捷键（registry 投影）

/** "Ctrl+Shift+S" → { mod, shift, key }；未声明快捷键的命令无绑定。 */
function parseShortcut(shortcut: string): { key: string; shift: boolean } | null {
  const parts = shortcut.split("+").map((p) => p.trim());
  const key = parts.pop()?.toLowerCase();
  if (!key) return null;
  return { key, shift: parts.some((p) => p.toLowerCase() === "shift") };
}

function onKeydown(e: KeyboardEvent): void {
  if (overlays.handleKeydown(e)) return;
  if (e.key === "Escape") {
    if ($("findbar").classList.contains("open")) {
      toggleFindBar(false);
      ctl.view?.focus();
      e.preventDefault();
    }
    return;
  }
  const target = e.target as HTMLElement | null;
  if (target?.matches("input, textarea, select, [contenteditable=false]")) return;
  const mod = e.ctrlKey || e.metaKey;
  if (!mod) return;
  const k = e.key.toLowerCase();
  for (const cmd of registry.all()) {
    if (!cmd.shortcut) continue;
    const parsed = parseShortcut(cmd.shortcut);
    if (!parsed || parsed.key !== k || parsed.shift !== e.shiftKey) continue;
    if (!registry.isVisible(cmd, ctx()) || !cmd.enabled(ctx())) return;
    e.preventDefault();
    void registry.run(cmd.id, ctx()).finally(() => applyFocusPolicy(cmd));
    return;
  }
}

// ---------------------------------------------------------------- 入口

async function boot(): Promise<void> {
  menus.build();
  ribbon.build();

  // 统一命令派发：surface 只放 data-cmd，执行/焦点契约只此一处
  document.addEventListener("click", (e) => {
    const btn = (e.target as HTMLElement | null)?.closest<HTMLElement>("[data-cmd]");
    if (!btn || btn.closest(".az-menu")) return; // 菜单项走 menus 自身的执行路径
    if (btn instanceof HTMLButtonElement && btn.disabled) return;
    runCommand(btn.dataset.cmd!, btn);
  });
  // 覆盖层外点关闭（捕获阶段，先于此处 click 派发）
  document.addEventListener("pointerdown", (e) => {
    overlays.handlePointerDown(e.target as HTMLElement | null);
  });
  // 顶栏搜索框只读，点击即开命令面板（Tab 聚焦不自动开——保持键盘导航顺序）
  $("cmdk-input").addEventListener("click", () => palette.open());

  // 查找栏
  $("find-close").addEventListener("click", () => toggleFindBar(false));
  $("find-input").addEventListener("input", updateFindCount);
  $("find-case").addEventListener("change", updateFindCount);
  $("find-next").addEventListener("click", () => {
    const v = ctl.view;
    if (v) selectMatch(v, currentMatches(), false);
  });
  $("find-prev").addEventListener("click", () => {
    const v = ctl.view;
    if (v) selectMatch(v, currentMatches(), true);
  });
  $("replace-one").addEventListener("click", () => {
    const v = ctl.view;
    if (v) {
      replaceCurrent(v, currentMatches(), ($("replace-input") as HTMLInputElement).value);
      updateFindCount();
    }
  });
  $("replace-all").addEventListener("click", () => {
    const v = ctl.view;
    if (v) {
      const n = replaceAll(v, currentMatches(), ($("replace-input") as HTMLInputElement).value);
      setMsg(`已替换 ${n} 处`, true);
      updateFindCount();
    }
  });

  document.addEventListener("keydown", onKeydown);
  registry.onChange(refreshUI);

  // 浮动条重定位：工作区滚动/窗口缩放时跟随选区（选区不变只移动，不重建）
  document.querySelector(".workspace")?.addEventListener("scroll", () => floating.refresh());
  window.addEventListener("resize", () => floating.refresh());

  window.addEventListener("beforeunload", (event) => {
    if (!ctl.store.state.dirty) return;
    event.preventDefault();
    event.returnValue = "";
  });

  try {
    if (tauriMode) {
      await ctl.offerRecoveries();
      if (!ctl.view) await ctl.newDocument();
      setMsg("就绪。", true);
    } else {
      const result = await gateway.openInitial();
      ctl.mount(result);
      setMsg("就绪。", true);
    }
  } catch (e) {
    setMsg(`启动失败: ${errText(e)}`, false);
  }
  refreshUI();
}

void boot();
