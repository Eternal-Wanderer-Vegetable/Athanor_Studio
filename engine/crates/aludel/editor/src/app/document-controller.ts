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

//! 编辑会话控制器：EditorView 生命周期、dirty 判定、保存编排、
//! 恢复草稿与任务回调。ProseMirror state 是正文唯一权威。

import { EditorState, Transaction } from "prosemirror-state";
import { EditorView } from "prosemirror-view";
import { baseKeymap } from "prosemirror-commands";
import { keymap } from "prosemirror-keymap";
import { liftListItem, sinkListItem, splitListItem } from "prosemirror-schema-list";
import { columnResizing, tableEditing } from "prosemirror-tables";
import { flag } from "../flags";
import { history, undo, redo } from "prosemirror-history";
import { sanitizeHtml, sanitizeNotice, type PasteMode } from "./clipboard";
import { mathNodeViews } from "./math-view";
import { footnoteClick, footnoteDanglingPlugin } from "./footnotes";
import type { Node as PMNode } from "prosemirror-model";
import { schema, setAssetResolver } from "../schema";
import type { DocumentGateway, DocResponse, JobRequest, JobSnapshot, PreviewResult, SaveResult } from "../platform/gateway";
import { SessionStore } from "./session-store";
import { askText } from "../ui/dialogs";
import { DEFAULT_PAGE_THEME, normalizePageTheme, pageThemesEqual, type PageTheme } from "./theme";

export interface ControllerHooks {
  /** 状态栏/标题/按钮刷新。 */
  onStateChange(): void;
  /** 侧栏元数据刷新（历史/标注/损失）。 */
  onDocMeta(d: DocResponse): void;
  /** 消息条。 */
  onMessage(text: string, ok: boolean): void;
  /** 大纲刷新。 */
  onOutline(headings: { level: number; text: string; pos: number }[]): void;
  /** 当前页字数统计等。 */
  onStats(chars: number): void;
}

const RECOVERY_DEBOUNCE_MS = 2_000;
const STATS_DEBOUNCE_MS = 200;
/** 客户端预检的资产字节上限（服务端同值；超限在 stage 前给出可读提示）。 */
const MAX_ASSET_BYTES = 32 * 1024 * 1024;

export class DocumentController {
  view: EditorView | null = null;
  readonly store = new SessionStore();
  private gateway: DocumentGateway;
  private hooks: ControllerHooks;
  private host: HTMLElement;
  /** 保存基线文档（保存/打开确认的快照）。 */
  private baseline: PMNode | null = null;
  private pageTheme: PageTheme = DEFAULT_PAGE_THEME;
  private baselinePageTheme: PageTheme = DEFAULT_PAGE_THEME;
  /** 是否处于 IME 组合输入。 */
  composing = false;
  private recoveryTimer: ReturnType<typeof setTimeout> | null = null;
  private recoveryGeneration = 0;
  private statsTimer: ReturnType<typeof setTimeout> | null = null;
  /** 最近一次 Ctrl/Alt/Shift+V 决定的粘贴模式（ClipboardEvent 无修饰键）。 */
  private pendingPasteMode: PasteMode | null = null;

  constructor(host: HTMLElement, gateway: DocumentGateway, hooks: ControllerHooks) {
    this.host = host;
    this.gateway = gateway;
    this.hooks = hooks;
  }

  get doc(): PMNode | null {
    return this.view?.state.doc ?? null;
  }

  /** 脏标记 = 当前 doc 与保存基线结构不等价（selection/滚动不触发）。 */
  private recomputeDirty(): void {
    if (!this.view || !this.baseline) return;
    this.store.setDirty(!this.view.state.doc.eq(this.baseline) || !pageThemesEqual(this.pageTheme, this.baselinePageTheme));
  }

  get currentPageTheme(): PageTheme {
    return this.pageTheme;
  }

  setPageTheme(theme: PageTheme): void {
    this.pageTheme = normalizePageTheme(theme);
    this.applyPageThemeToWorkspace();
    // 主题与正文共用同一代数：theme-only 修改同样推进代际、
    // 排队在途保存后的补存，并纳入恢复草稿（E5）。
    this.store.markEdited();
    this.recomputeDirty();
    this.scheduleRecovery();
    this.hooks.onStateChange();
  }

  private applyPageThemeToWorkspace(): void {
    const page = this.host.closest(".page") as HTMLElement | null;
    if (!page) return;
    const mmToPx = (mm: number) => `${Math.round(mm * 3.78)}px`;
    const { top, right, bottom, left } = this.pageTheme.margins;
    page.style.padding = `${mmToPx(top)} ${mmToPx(right)} ${mmToPx(bottom)} ${mmToPx(left)}`;
    page.style.maxWidth = this.pageTheme.orientation === "landscape" ? "1060px" : "820px";
    page.dataset.pageSize = this.pageTheme.pageSize;
    page.dataset.orientation = this.pageTheme.orientation;
  }

  // ---------------------------------------------------------------- 挂载

  /** 图片文件 → 服务端暂存 → 插入 image 节点（粘贴/拖放/插入命令共用入口）。
   *  正文写 `asset://<as_id>/<filename>`；字节随下次保存落库。
   *  dropPos 给出时插入到指定位置，否则替换当前选区。 */
  async insertImageFile(file: File, alt?: string, dropPos?: number): Promise<void> {
    const v = this.view;
    if (!v) return;
    if (!flag("assetRegistryV2")) {
      // 旗标关闭：图片暂存/插入入口整体停用（旧包仍安全读取既有 asset://）
      this.hooks.onMessage("图片资产功能已关闭。", false);
      return;
    }
    if (!file.type.startsWith("image/")) {
      this.hooks.onMessage("不是图片文件。", false);
      return;
    }
    if (file.size > MAX_ASSET_BYTES) {
      this.hooks.onMessage(`图片过大（上限 ${Math.round(MAX_ASSET_BYTES / 1024 / 1024)} MB）。`, false);
      return;
    }
    try {
      const dataBase64 = await fileToBase64(file);
      if (this.view !== v) return;
      const staged = await this.gateway.stageAsset(this.store.state.sessionId, {
        filename: file.name || "pasted.png",
        mime: file.type,
        dataBase64,
      });
      if (this.view !== v) return;
      const image = schema.nodes.image.create({
        id: makeEditorId("blk"),
        asset: staged.url,
        alt: alt ?? file.name,
      });
      if (dropPos !== undefined) {
        try {
          v.dispatch(v.state.tr.insert(dropPos, image).scrollIntoView());
          return;
        } catch {
          // 目标位置不接受该节点：退回替换当前选区
        }
      }
      v.dispatch(v.state.tr.replaceSelectionWith(image).scrollIntoView());
    } catch (e) {
      this.hooks.onMessage(`图片暂存失败: ${errText(e)}`, false);
    }
  }

  private createEditor(docJson: Record<string, unknown>): EditorView {
    const doc = schema.nodeFromJSON(docJson);
    return new EditorView(this.host, {
      state: EditorState.create({
        doc,
        plugins: [
          // 表格编辑栈先于自定义 keymap：tableEditing 自带 CellSelection
          // 与 Tab/Shift-Tab 格间导航；columnResizing 写 cell colwidth
          // （保存时归并进 columns[].width，spec §6.5）。不装 fixTables 自动
          // 修复——不规则表只诊断，修复走显式 table.fix 命令。
          // tableSelectionV2 关闭：退回纯文本式表格编辑（无单元格选区/列宽拖拽）
          ...(flag("tableSelectionV2") ? [columnResizing({ cellMinWidth: 24 }), tableEditing()] : []),
          // 脚注定义被删且仍有引用 → 一次性提示（撤销可恢复；保存侧仍是硬门槛）
          footnoteDanglingPlugin((ids) => {
            this.hooks.onMessage(
              `脚注定义已删除，${ids.length} 处引用失去目标——可撤销恢复，否则保存将被拒绝`,
              false,
            );
          }),
          keymap({
            Enter: splitListItem(schema.nodes.list_item),
            Tab: sinkListItem(schema.nodes.list_item),
            "Shift-Tab": liftListItem(schema.nodes.list_item),
          }),
          keymap({ "Mod-z": undo, "Shift-Mod-z": redo, "Mod-y": redo }),
          keymap(baseKeymap),
          history(),
        ],
      }),
      attributes: { spellcheck: "false" },
      nodeViews: mathNodeViews(),
      handleDoubleClickOn: (v, _pos, node, nodePos) => {
        if (node.type.name === "math_block" || node.type.name === "inline_math") {
          const current = String(node.attrs.latex ?? "");
          void askText("LaTeX 源码", current).then((next) => {
            if (next !== null && this.view === v) v.dispatch(v.state.tr.setNodeAttribute(nodePos, "latex", next));
          });
          return true;
        }
        if (node.type.name === "image" || node.type.name === "figure" || node.type.name === "inline_image") {
          const current = String(node.attrs.alt ?? "");
          void askText("替代文字（无障碍，必填可读描述）", current).then((next) => {
            if (next !== null && this.view === v) v.dispatch(v.state.tr.setNodeAttribute(nodePos, "alt", next));
          });
          return true;
        }
        return false;
      },
      handleDOMEvents: {
        // 粘贴模式 = 最近一次带修饰的 V 组合键（ClipboardEvent 本身无修饰键）：
        // Ctrl+V keep / Ctrl+Alt+V match / Ctrl+Shift+V plain；菜单/右键粘贴按 keep。
        keydown: (_v, event) => {
          const e = event as KeyboardEvent;
          const isV = (e.key ?? "").toLowerCase() === "v";
          if (isV && (e.ctrlKey || e.metaKey)) {
            this.pendingPasteMode = e.shiftKey ? "plain" : e.altKey ? "match" : "keep";
          }
          return false;
        },
        paste: (v, event) => {
          const clipboard = event.clipboardData;
          const file = clipboard?.files?.[0];
          if (file && file.type.startsWith("image/")) {
            event.preventDefault();
            void this.insertImageFile(file);
            return true;
          }
          const html = clipboard?.getData("text/html");
          const text = clipboard?.getData("text/plain");
          const mode = this.pendingPasteMode ?? "keep";
          this.pendingPasteMode = null;
          if (mode === "plain") {
            if (!text) return false;
            event.preventDefault();
            v.pasteText(text);
            return true;
          }
          if (html) {
            event.preventDefault();
            const r = sanitizeHtml(html, mode);
            v.pasteHTML(r.html);
            const note = sanitizeNotice(r);
            if (note) this.hooks.onMessage(note, true);
            return true;
          }
          return false; // 纯文本无 HTML：交给 PM 默认文本粘贴
        },
        click: (v, event) => footnoteClick(v, event),
        drop: (v, event) => {
          const file = event.dataTransfer?.files?.[0];
          if (!file || !file.type.startsWith("image/")) return false;
          event.preventDefault();
          const pos = v.posAtCoords({ left: event.clientX, top: event.clientY })?.pos;
          void this.insertImageFile(file, undefined, pos);
          return true;
        },
        mousedown: (v, event) => {
          const t = event.target as HTMLElement | null;
          if (!t || !t.matches?.("li[data-checked]")) return false;
          const $pos = v.state.doc.resolve(v.posAtDOM(t, 0));
          for (let depth = $pos.depth; depth > 0; depth--) {
            const n = $pos.node(depth);
            if (n.type.name === "list_item") {
              const pos = $pos.before(depth);
              v.dispatch(v.state.tr.setNodeAttribute(pos, "checked", !n.attrs.checked));
              return true;
            }
          }
          return false;
        },
        compositionstart: () => {
          this.composing = true;
          return false;
        },
        compositionend: () => {
          this.composing = false;
          this.refreshDerivedUI();
          return false;
        },
      },
      dispatchTransaction: (tr: Transaction) => {
        const view = this.view;
        if (!view) return;
        view.updateState(view.state.apply(tr));
        if (tr.docChanged) {
          this.store.markEdited();
          this.recomputeDirty();
          this.scheduleRecovery();
        }
        this.refreshDerivedUI();
      },
    });
  }

  /** 挂载一份文档（打开/新建/恢复）。epoch 推进使旧异步响应作废。 */
  mount(result: { sessionId: string; doc: DocResponse }, displayName?: string): void {
    const d = result.doc;
    this.store.open({
      sessionId: result.sessionId,
      path: d.path,
      revision: d.revision,
      fingerprint: d.fingerprint,
      displayName,
    });
    // 资产解析器绑定当前会话（asset:// → gateway 的展示 URL）
    const sessionId = result.sessionId;
    setAssetResolver((ref) => this.gateway.assetUrl(sessionId, ref));
    this.view?.destroy();
    this.view = this.createEditor(d.pm_doc);
    this.baseline = this.view.state.doc;
    this.pageTheme = normalizePageTheme(d.theme);
    this.baselinePageTheme = normalizePageTheme(d.theme);
    this.applyPageThemeToWorkspace();
    this.store.setDirty(false);
    this.hooks.onDocMeta(d);
    this.refreshDerivedUI();
  }

  /** 挂载一份尚未保存的 PM 内容（恢复草稿用）。 */
  mountRecovered(pmDoc: Record<string, unknown>, originPath: string | null): void {
    this.store.open({
      sessionId: `recovery-${Date.now()}`,
      path: originPath,
      revision: null,
      fingerprint: null,
    });
    this.view?.destroy();
    this.view = this.createEditor(pmDoc);
    this.baseline = null; // 恢复内容未落盘：恒脏
    this.pageTheme = { ...DEFAULT_PAGE_THEME, margins: { ...DEFAULT_PAGE_THEME.margins } };
    this.baselinePageTheme = { ...this.pageTheme, margins: { ...this.pageTheme.margins } };
    this.store.setDirty(true);
    this.refreshDerivedUI();
  }

  /** 无文档时清空视图。 */
  unmount(): void {
    this.view?.destroy();
    this.view = null;
    this.baseline = null;
    this.store.state = { ...this.store.state, sessionId: "", path: null };
    this.refreshDerivedUI();
  }

  // ---------------------------------------------------------------- 刷新

  /** 派生 UI：大纲/字数/标题/命令态。IME 组合期间只刷状态条。
   *  字数统计为 O(doc) 且对连续输入无时效价值：延迟 200ms 归并，
   *  每次新编辑取消上一笔待算（长文档"取消旧布局任务"的最小实现）。 */
  refreshDerivedUI(): void {
    if (!this.composing && this.view) {
      this.hooks.onOutline(collectHeadings(this.view.state.doc));
      const doc = this.view.state.doc;
      if (this.statsTimer) clearTimeout(this.statsTimer);
      this.statsTimer = setTimeout(() => {
        if (this.view?.state.doc === doc) this.hooks.onStats(countChars(doc));
      }, STATS_DEBOUNCE_MS);
    }
    this.hooks.onStateChange();
  }

  // ---------------------------------------------------------------- 打开/新建/关闭

  /** 未保存改动确认（true=继续放弃/另存，false=取消）。 */
  async confirmDiscardIfDirty(): Promise<boolean> {
    if (!this.store.state.dirty) return true;
    return this.gateway.confirm(
      `「${this.store.state.displayName}」有未保存的更改，继续操作将丢失这些内容。`,
      "未保存的更改",
    );
  }

  async openPath(path: string): Promise<boolean> {
    const epoch = this.store.currentEpoch();
    try {
      const result = await this.gateway.openDocument(path);
      if (epoch !== this.store.currentEpoch() && this.store.state.sessionId !== "") {
        // 已有新会话接管：丢弃过期响应
        return false;
      }
      this.mount(result);
      this.hooks.onMessage("文档已打开。", true);
      return true;
    } catch (e) {
      this.hooks.onMessage(`打开失败: ${errText(e)}`, false);
      return false;
    }
  }

  async pickAndOpen(): Promise<void> {
    if (!(await this.confirmDiscardIfDirty())) return;
    const path = await this.gateway.pickOpen();
    if (!path) return;
    await this.openPath(path);
  }

  async newDocument(): Promise<void> {
    if (!(await this.confirmDiscardIfDirty())) return;
    try {
      const result = await this.gateway.newDocument();
      this.mount(result);
      this.hooks.onMessage("已创建新文档，直接输入即可。", true);
      this.view?.focus();
    } catch (e) {
      this.hooks.onMessage(`新建失败: ${errText(e)}`, false);
    }
  }

  async closeDocument(): Promise<void> {
    if (!(await this.confirmDiscardIfDirty())) return;
    const id = this.store.state.sessionId;
    if (id && this.gateway.desktop) {
      try {
        await this.gateway.closeDocument(id);
        await this.gateway.deleteRecovery(id).catch(() => {});
      } catch {
        /* 关闭失败不阻塞 UI */
      }
    }
    this.unmount();
    this.hooks.onMessage("文档已关闭。", true);
  }

  // ---------------------------------------------------------------- 保存

  /** 捕获当前快照并保存；在途时排队。 */
  async requestSave(): Promise<void> {
    if (!this.view) return;
    if (!this.store.requestSave()) return; // 已在途：saveQueued 已置位
    await this.performSave();
  }

  private async performSave(): Promise<void> {
    const view = this.view;
    if (!view) return;
    const s = this.store.state;
    const snapshot = view.state.doc;
    const epoch = s.epoch;
    const sessionId = s.sessionId;
    const capturedGeneration = s.editGeneration;
    const body = {
      pm_doc: snapshot.toJSON(),
      theme: this.pageTheme,
      message: (document.getElementById("message") as HTMLInputElement | null)?.value.trim() || undefined,
      author_id: "local",
      expected_fingerprint: s.fingerprint ?? undefined,
    };
    this.hooks.onMessage("保存中…", true);
    this.hooks.onStateChange();
    try {
      let result;
      if (s.path === null && this.gateway.desktop) {
        // 未命名文档：先选路径
        const target = await this.gateway.pickSaveAs("未命名文档.azodoc");
        if (!target) {
          this.store.saveFailed();
          this.store.state.saveState = "idle";
          this.hooks.onMessage("已取消保存（未选择路径）。", true);
          this.hooks.onStateChange();
          return;
        }
        const r = await this.gateway.saveDocumentAs(sessionId, target, false, body);
        if (r.session_id) this.store.state.sessionId = r.session_id;
        result = r.doc ?? r;
      } else {
        result = await this.gateway.saveDocument(sessionId, body);
      }
      if (epoch !== this.store.currentEpoch()) return; // 过期响应丢弃
      if (result.ok === false) throw new Error(result.error ?? "保存被拒绝");
      // Only the captured snapshot is acknowledged. Input entered while the
      // dialog/request was in flight remains dirty and will be queued below.
      if (this.view?.state.doc.eq(snapshot) && this.store.state.editGeneration === capturedGeneration) {
        this.baseline = snapshot;
        this.baselinePageTheme = this.pageTheme;
      }
      this.store.saveSucceeded(
        result.fingerprint ?? null,
        result.revision ?? null,
        result.path !== undefined ? result.path : undefined,
      );
      this.recomputeDirty();
      if (this.store.state.editGeneration !== capturedGeneration) this.store.state.saveQueued = true;
      // 落链元数据刷新（不重建 view，保护光标）
      const rel = result.relocate;
      const relText = rel
        ? ` · 标注 未变${rel.unchanged}/重锚${rel.reanchored}/迁移${rel.moved}/失配${rel.detached}`
        : "";
      this.hooks.onMessage(
        `已保存 ${result.revision ?? ""}${relText}`,
        true,
      );
      await this.gateway.deleteRecovery(sessionId).catch(() => {});
      if (this.store.consumeQueuedSave()) void this.requestSave();
    } catch (e) {
      if (epoch !== this.store.currentEpoch()) return;
      this.store.saveFailed();
      const msg = errText(e);
      if (msg.includes("target_exists") || msg.includes("已存在")) {
        this.hooks.onMessage(`另存失败：目标已存在。`, false);
      } else if (msg.includes("外部")) {
        this.hooks.onMessage(`${msg}`, false);
      } else {
        this.hooks.onMessage(`保存失败: ${msg}`, false);
      }
      if (this.store.consumeQueuedSave()) void this.requestSave();
    } finally {
      this.hooks.onStateChange();
    }
  }

  async saveAs(): Promise<void> {
    const view = this.view;
    if (!view || !this.gateway.desktop) return;
    const target = await this.gateway.pickSaveAs(this.store.state.displayName);
    if (!target) return;
    const s = this.store.state;
    const snapshot = view.state.doc;
    const capturedGeneration = s.editGeneration;
    const body = {
      pm_doc: snapshot.toJSON(),
      theme: this.pageTheme,
      message: undefined,
      author_id: "local",
    };
    const apply = (result: SaveResult) => {
      if (result.session_id) this.store.state.sessionId = result.session_id;
      const doc = result.doc ?? result;
      if (this.view?.state.doc.eq(snapshot) && this.store.state.editGeneration === capturedGeneration) {
        this.baseline = snapshot;
        this.baselinePageTheme = this.pageTheme;
      }
      this.store.saveSucceeded(doc.fingerprint ?? null, doc.revision ?? null, target);
      this.recomputeDirty();
      if (this.store.state.editGeneration !== capturedGeneration) {
        this.store.state.savedGeneration = capturedGeneration;
        this.store.state.saveState = "idle";
        this.store.state.saveQueued = false;
        this.store.state.dirty = true;
      }
      this.hooks.onMessage(`已另存为 ${target}`, true);
      this.hooks.onStateChange();
    };
    try {
      apply(await this.gateway.saveDocumentAs(s.sessionId, target, false, body));
    } catch (e) {
      const msg = errText(e);
      if (msg.includes("target_exists") || msg.includes("已存在")) {
        const yes = await this.gateway.confirm(`「${target}」已存在，要覆盖吗？`, "另存为");
        if (!yes) return;
        try {
          apply(await this.gateway.saveDocumentAs(s.sessionId, target, true, body));
        } catch (e2) {
          this.hooks.onMessage(`另存失败: ${errText(e2)}`, false);
        }
      } else {
        this.hooks.onMessage(`另存失败: ${msg}`, false);
      }
    }
    this.hooks.onStateChange();
  }

  async verify(): Promise<void> {
    const s = this.store.state;
    this.hooks.onMessage("文档检查中…", true);
    try {
      const data = await this.gateway.verifyDocument(s.sessionId);
      this.hooks.onMessage(
        data["ok"] === true ? "文档检查通过 ✓" : "文档检查未通过（详见服务端输出）",
        data["ok"] === true,
      );
    } catch (e) {
      this.hooks.onMessage(`检查失败: ${errText(e)}`, false);
    }
  }

  // ---------------------------------------------------------------- 预览

  /** 印刷预览（E4）：当前 PM 快照 + theme 发服务端渲染，
   *  返回 Paged.js 增强 HTML 与快照指纹（不出版、不提交）。 */
  async requestPreview(): Promise<PreviewResult | null> {
    const view = this.view;
    if (!view) return null;
    const s = this.store.state;
    const epoch = s.epoch;
    const body = {
      pm_doc: view.state.doc.toJSON(),
      theme: this.pageTheme,
    };
    this.hooks.onMessage("预览生成中…", true);
    try {
      const result = await this.gateway.renderPreview(s.sessionId, body);
      if (epoch !== this.store.currentEpoch()) return null;
      return result;
    } catch (e) {
      this.hooks.onMessage(`预览失败: ${errText(e)}`, false);
      return null;
    }
  }

  // ---------------------------------------------------------------- 修订

  /** 还原到某修订快照（E5）。未保存改动先确认丢弃；
   *  修订未带主题快照时如实提示"仅正文历史"（主题继承最近状态）。 */
  async checkoutRevision(revId: string): Promise<void> {
    if (!this.view) return;
    const s = this.store.state;
    if (!(await this.confirmDiscardIfDirty())) return;
    const epoch = s.epoch;
    const sessionId = s.sessionId;
    this.hooks.onMessage(`还原修订 ${revId}…`, true);
    try {
      const doc = await this.gateway.checkoutRevision(sessionId, {
        revision: revId,
        expected_fingerprint: s.fingerprint ?? undefined,
      });
      if (epoch !== this.store.currentEpoch()) return;
      this.mount({ sessionId, doc });
      this.hooks.onMessage(
        doc.theme_restored === false
          ? `已还原修订 ${revId}（该修订无主题快照——仅正文历史，主题继承最近状态）`
          : `已还原修订 ${revId}`,
        true,
      );
    } catch (e) {
      if (epoch !== this.store.currentEpoch()) return;
      this.hooks.onMessage(`还原失败: ${errText(e)}`, false);
    }
    this.hooks.onStateChange();
  }

  // ---------------------------------------------------------------- 任务

  /** 导出/出版固定为“先保存当前快照，再对该版本跑任务”。 */
  async runJob(
    kind: "import" | "export" | "publish",
    format: "markdown" | "html" | "text" | "docx" = "markdown",
  ): Promise<void> {
    if (!this.gateway.desktop) {
      this.hooks.onMessage("浏览器模式不支持后台任务。", false);
      return;
    }
    const s = this.store.state;
    if (s.activeJob !== null) {
      this.hooks.onMessage("已有任务在运行。", false);
      return;
    }
    if (kind === "import") {
      if (!(await this.confirmDiscardIfDirty())) return;
      const input = await this.gateway.pickImportSource();
      if (!input) return;
      const output = await this.gateway.pickSaveAs("导入结果.azodoc");
      if (!output) return;
      const epoch = this.store.currentEpoch();
      try {
        const jobId = await this.gateway.runJob(
          { kind, input, output, reader: "auto" },
          s.sessionId || null,
          (snap) => this.onJobUpdate(snap, epoch),
        );
        this.store.setActiveJob(jobId);
        this.hooks.onMessage(`任务 #${jobId} 已排队`, true);
      } catch (e) {
        this.hooks.onMessage(`任务无法启动: ${errText(e)}`, false);
      }
      this.hooks.onStateChange();
      return;
    }
    // export/publish：先保存
    if (s.dirty || s.path === null) {
      await this.requestSave();
      // 保存为异步编排；若仍在 saving 则提示重试
      if (this.store.state.saveState === "saving" || this.store.state.dirty) {
        this.hooks.onMessage("请先完成保存再导出。", false);
        return;
      }
    }
    const input = s.path;
    if (!input) {
      this.hooks.onMessage("请先保存文档再导出。", false);
      return;
    }
    const extension =
      kind === "publish"
        ? "pdf"
        : ({ markdown: "md", html: "html", text: "txt", docx: "docx" } as const)[format];
    const output = await this.gateway.pickSaveAs(`${this.store.state.displayName}.${extension}`);
    if (!output) return;
    const epoch = this.store.currentEpoch();
    const request: JobRequest =
      kind === "export"
        ? { kind, input, output, format }
        : { kind, input, output, noPaged: false };
    try {
      const jobId = await this.gateway.runJob(request, s.sessionId || null, (snap) =>
        this.onJobUpdate(snap, epoch),
      );
      this.store.setActiveJob(jobId);
      this.hooks.onMessage(`任务 #${jobId} 已排队（文档版本已固定）`, true);
    } catch (e) {
      this.hooks.onMessage(`任务无法启动: ${errText(e)}`, false);
    }
    this.hooks.onStateChange();
  }

  private onJobUpdate(snapshot: JobSnapshot, epoch: number): void {
    if (epoch !== this.store.currentEpoch()) return;
    const terminal = ["succeeded", "failed", "cancelled"].includes(snapshot.phase);
    // 终态不能被晚到的 jobId 重置为运行中；只认属于当前会话的任务
    if (snapshot.session_id && this.store.state.sessionId && snapshot.session_id !== this.store.state.sessionId) {
      return;
    }
    this.store.setActiveJob(terminal ? null : snapshot.id);
    if (snapshot.error) {
      this.hooks.onMessage(`任务失败: ${snapshot.error.message}`, false);
      this.hooks.onStateChange();
      return;
    }
    const loss = snapshot.result?.report?.summary?.loss;
    const lossText = loss ? ` · 损失 ${Object.values(loss).reduce((a, b) => a + b, 0)} 项` : "";
    // E5：输出修订 + DOCX 能力矩阵摘要（支持/降级/保留计数）。
    const outRev = snapshot.result?.report?.target?.revision;
    const revText = outRev ? ` · 输出修订 ${outRev}` : "";
    const cap = snapshot.result?.report?.capabilities?.counts;
    const capText = cap
      ? ` · 能力矩阵 支持${cap["supported"] ?? 0}/降级${cap["degraded"] ?? 0}/保留${cap["preserved"] ?? 0}`
      : "";
    const labels: Record<string, string> = {
      queued: "排队中",
      running: "运行中",
      cancelling: "取消中",
      committing: "提交结果中",
      succeeded: "已完成",
      cancelled: "已取消",
      failed: "失败",
    };
    this.hooks.onMessage(
      `任务 #${snapshot.id} ${labels[snapshot.phase] ?? snapshot.phase} ${snapshot.progress}%${lossText}${revText}${capText}`,
      snapshot.phase !== "failed",
    );
    // 导入完成后重新打开产物（epoch 守卫由 openPath 自己再做一次）
    if (snapshot.phase === "succeeded" && snapshot.result?.document) {
      const docPath = snapshot.result.document;
      if (docPath !== this.store.state.path) void this.openPath(docPath);
    }
    this.hooks.onStateChange();
  }

  async cancelJob(): Promise<void> {
    const id = this.store.state.activeJob;
    if (id === null) return;
    try {
      await this.gateway.cancelJob(id);
    } catch (e) {
      this.hooks.onMessage(`取消失败: ${errText(e)}`, false);
    }
  }

  // ---------------------------------------------------------------- 恢复草稿

  private scheduleRecovery(): void {
    if (!this.gateway.desktop) return;
    this.recoveryGeneration = this.store.state.editGeneration;
    if (this.recoveryTimer) clearTimeout(this.recoveryTimer);
    this.recoveryTimer = setTimeout(() => void this.writeRecovery(), RECOVERY_DEBOUNCE_MS);
  }

  private async writeRecovery(): Promise<void> {
    const view = this.view;
    const s = this.store.state;
    if (!view || !s.sessionId) return;
    try {
    await this.gateway.writeRecovery(
      s.sessionId,
      this.recoveryGeneration,
      view.state.doc.toJSON(),
      s.path,
      this.pageTheme,
    );
    } catch {
      /* 草稿失败静默，不影响正文 */
    }
  }

  /** 启动时检查恢复草稿；返回是否接管了内容。 */
  async offerRecoveries(): Promise<void> {
    if (!this.gateway.desktop) return;
    let list;
    try {
      list = await this.gateway.listRecoveries();
    } catch {
      return;
    }
    for (const r of list) {
      const label = r.session_path ?? "未命名文档";
      const restore = await this.gateway.confirm(
        `发现未保存的草稿「${label}」（${r.saved_at ?? "时间未知"}）。恢复它吗？选择“否”将丢弃草稿。`,
        "恢复草稿",
      );
      if (restore) {
        try {
          const draft = await this.gateway.readRecovery(r.file);
          const pmDoc = draft["pm_doc"] as Record<string, unknown> | undefined;
          if (pmDoc) {
            this.mountRecovered(pmDoc, r.session_path);
            if (draft["theme"]) this.setPageTheme(normalizePageTheme(draft["theme"]));
            this.hooks.onMessage(`已恢复草稿「${label}」，请另存为或保存。`, true);
            return;
          }
        } catch (e) {
          this.hooks.onMessage(`草稿读取失败: ${errText(e)}`, false);
        }
      }
      await this.gateway.deleteRecovery(r.file).catch(() => {});
    }
  }
}

// ---------------------------------------------------------------- 纯函数

function collectHeadings(doc: PMNode): { level: number; text: string; pos: number }[] {
  const out: { level: number; text: string; pos: number }[] = [];
  doc.descendants((node, pos) => {
    if (node.type.name === "heading") {
      out.push({ level: Number(node.attrs.level) || 1, text: node.textContent, pos });
      return false;
    }
    return true;
  });
  return out;
}

function countChars(doc: PMNode): number {
  let n = 0;
  doc.descendants((node) => {
    if (node.isText) n += node.text?.length ?? 0;
    return true;
  });
  return n;
}

/** File → base64（不含 data: 前缀）；stageAsset 入参。 */
function fileToBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => {
      const res = r.result;
      if (typeof res !== "string") return reject(new Error("读取失败"));
      const comma = res.indexOf(",");
      resolve(comma >= 0 ? res.slice(comma + 1) : res);
    };
    r.onerror = () => reject(new Error("读取文件失败"));
    r.readAsDataURL(file);
  });
}

function makeEditorId(prefix: "blk"): string {
  const alphabet = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";
  let time = BigInt(Date.now());
  let suffix = "";
  for (let i = 0; i < 10; i++) {
    suffix = alphabet[Number(time & 31n)] + suffix;
    time >>= 5n;
  }
  const bytes = new Uint8Array(16);
  globalThis.crypto?.getRandomValues(bytes);
  for (const byte of bytes) suffix += alphabet[byte & 31];
  return `${prefix}_${suffix}`;
}

export function errText(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "object" && e !== null && "message" in e) return String((e as { message: unknown }).message);
  return String(e);
}
