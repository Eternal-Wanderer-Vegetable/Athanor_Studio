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
//! 业务状态在 app/session-store + document-controller；渲染在 ui/*。

import { TextSelection } from "prosemirror-state";
import { toggleMark, wrapIn, setBlockType } from "prosemirror-commands";
import { wrapInList, liftListItem } from "prosemirror-schema-list";
import { undo, redo } from "prosemirror-history";
import "prosemirror-view/style/prosemirror.css";
import "prosemirror-tables/style/tables.css";
import "./ui/tokens.css";
import "./ui/shell.css";
import { schema } from "./schema";
import { DocumentController, errText } from "./app/document-controller";
import {
  CommandRegistry,
  type CommandContext,
  type FocusPolicy,
  type MenuGroup,
  type RibbonTab,
} from "./app/command-registry";
import { findAll, selectMatch, replaceCurrent, replaceAll } from "./app/find";
import { gotoFootnote } from "./app/footnotes";
import { PreviewSurface } from "./app/preview";
import {
  addTableRow,
  addTableRowBefore,
  deleteTableRow,
  addTableColumn,
  addTableColumnBefore,
  deleteTableColumn,
  deleteWholeTable,
  fixTable,
  toggleHeaderRow,
  mergeTableCells,
  splitTableCell,
} from "./app/table-commands";
import {
  setParagraphFormat,
  setCharacterFormat,
  clearParagraphFormat,
  clearCharacterFormat,
  activeParagraphFormat,
  selectionContext,
  type SelectionContext,
} from "./app/format";
import { TauriGateway } from "./platform/tauri";
import { HttpGateway } from "./platform/http";
import { $, setMsg } from "./ui/dom";
import { OverlayController } from "./ui/overlay";
import { AppMenus } from "./ui/menus";
import { Ribbon } from "./ui/ribbon";
import { CommandPalette } from "./ui/palette";
import { FloatingBar } from "./ui/floating";
import { Shell } from "./ui/shell";
import { renderOutline } from "./ui/outline-panel";
import { renderReviewSections } from "./ui/review-panel";
import { renderInspector } from "./ui/inspector-panel";
import { renderStatusBar } from "./ui/status-bar";
import { askText } from "./ui/dialogs";
import { askPageSettings } from "./ui/page-settings";
import { flag } from "./flags";

const tauriMode = "__TAURI_INTERNALS__" in window;
const gateway = tauriMode ? new TauriGateway() : new HttpGateway();

let lastChars = 0;
let zoom = 1;
/** 最近一次成功预览的分页结果（正文一代际变化即失效）。 */
let previewPages: { generation: number; count: number } | null = null;

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

function ctx(): CommandContext {
  const s = ctl.store.state;
  const sc = sel();
  return {
    hasDocument: ctl.view !== null,
    desktop: gateway.desktop,
    dirty: s.dirty,
    busy: s.activeJob !== null || s.saveState === "saving",
    hasSelection: sc !== null && sc.kind !== "empty",
    inTable: sc?.inTable ?? false,
  };
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

// ---------------------------------------------------------------- 命令注册

interface RegOpts {
  shortcut?: string;
  desktopOnly?: boolean;
  needsDoc?: boolean;
  needsSelection?: boolean;
  needsTable?: boolean;
  needsJob?: boolean;
  active?: () => boolean;
  disabledReason?: string | ((c: CommandContext) => string);
  tab?: RibbonTab;
  group?: string;
  order?: number;
  groupOrder?: number;
  menu?: MenuGroup;
  menuOrder?: number;
  menuId?: string;
  elId?: string;
  surfaces?: import("./app/command-registry").CommandSurface[];
  keywords?: string[];
  focusPolicy?: FocusPolicy;
}

function reg(id: string, label: string, run: () => void | Promise<void>, opts: RegOpts = {}) {
  const reason = (c: CommandContext): string | null => {
    if (opts.needsDoc !== false && !c.hasDocument) return "无已打开文档";
    if (opts.desktopOnly && !c.desktop) return "需要桌面版本（文件对话框/任务能力）";
    if (opts.needsSelection && !c.hasSelection) return "需要先选择内容";
    if (opts.needsTable && !c.inTable) return "光标需在表格内";
    if (opts.needsJob && !c.busy) return "没有在途任务";
    return typeof opts.disabledReason === "function" ? opts.disabledReason(c) : (opts.disabledReason ?? null);
  };
  registry.register({
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
    // 能力矩阵（§6.6）：桌面专属命令在 HTTP 模式不渲染入口，不显示死按钮
    visible: opts.desktopOnly ? (c) => c.desktop : undefined,
    run,
    tab: opts.tab,
    group: opts.group,
    order: opts.order,
    groupOrder: opts.groupOrder,
    menu: opts.menu,
    menuOrder: opts.menuOrder,
    menuId: opts.menuId,
    elId: opts.elId,
    surfaces: opts.surfaces,
    keywords: opts.keywords,
    focusPolicy: opts.focusPolicy,
  });
}

function pmRun(fn: (state: never, dispatch: never) => boolean): void {
  const v = ctl.view;
  if (v) fn(v.state as never, v.dispatch as never);
}

// ---- 文件 ----
reg("file.new", "新建", () => ctl.newDocument(), {
  shortcut: "Ctrl+N", desktopOnly: true, needsDoc: false,
  menu: "file", menuOrder: 0, menuId: "m-new", focusPolicy: "none",
  keywords: ["new", "xinjian"],
});
reg("file.open", "打开…", () => ctl.pickAndOpen(), {
  shortcut: "Ctrl+O", desktopOnly: true, needsDoc: false,
  menu: "file", menuOrder: 1, menuId: "m-open", focusPolicy: "none",
  keywords: ["open", "dakai"],
});
reg("file.save", "保存", () => ctl.requestSave(), {
  shortcut: "Ctrl+S", menu: "file", menuOrder: 2, menuId: "m-save",
  keywords: ["save", "baocun"],
});
reg("file.saveAs", "另存为…", () => ctl.saveAs(), {
  shortcut: "Ctrl+Shift+S", desktopOnly: true,
  menu: "file", menuOrder: 3, menuId: "m-saveas", focusPolicy: "none",
  keywords: ["save as", "saveas", "lingcunwei"],
});
reg("file.close", "关闭", () => ctl.closeDocument(), {
  desktopOnly: true, menu: "file", menuOrder: 4, menuId: "m-close", focusPolicy: "none",
  keywords: ["close", "guanbi"],
});
reg("job.import", "导入…", () => ctl.runJob("import"), {
  desktopOnly: true, needsDoc: false, menu: "file", menuOrder: 10, menuId: "m-import",
  focusPolicy: "keep", keywords: ["import", "daoru"],
});
reg("job.export", "导出 Markdown…", () => ctl.runJob("export"), {
  desktopOnly: true, menu: "file", menuOrder: 11, menuId: "m-export", focusPolicy: "keep",
  keywords: ["export", "markdown", "daochu"],
});
reg("job.exportHtml", "导出 HTML…", () => ctl.runJob("export", "html"), {
  desktopOnly: true, menu: "file", menuOrder: 12, menuId: "m-export-html", focusPolicy: "keep",
});
reg("job.exportText", "导出纯文本…", () => ctl.runJob("export", "text"), {
  desktopOnly: true, menu: "file", menuOrder: 13, menuId: "m-export-text", focusPolicy: "keep",
});
reg("job.exportDocx", "导出 Word 文档…", () => ctl.runJob("export", "docx"), {
  desktopOnly: true, menu: "file", menuOrder: 14, menuId: "m-export-docx", focusPolicy: "keep",
  keywords: ["docx", "word"],
});
reg("job.publish", "导出 PDF…", () => ctl.runJob("publish"), {
  desktopOnly: true, menu: "file", menuOrder: 15, menuId: "m-publish", focusPolicy: "keep",
  keywords: ["pdf", "publish"],
});
reg("job.cancel", "取消任务", () => ctl.cancelJob(), {
  desktopOnly: true, needsJob: true, needsDoc: false,
  menu: "file", menuOrder: 16, menuId: "cancel-job", focusPolicy: "keep",
});

// ---- 编辑 ----
reg("edit.undo", "撤销", () => pmRun(undo as never), {
  shortcut: "Ctrl+Z", tab: "home", group: "历史", groupOrder: 10, elId: "tb-undo",
  menu: "edit", menuOrder: 0, keywords: ["undo", "chexiao"],
});
reg("edit.redo", "重做", () => pmRun(redo as never), {
  shortcut: "Ctrl+Y", tab: "home", group: "历史", groupOrder: 10, elId: "tb-redo",
  menu: "edit", menuOrder: 1, keywords: ["redo", "chongzuo"],
});
reg("edit.find", "查找", () => toggleFindBar(true), {
  shortcut: "Ctrl+F", tab: "home", group: "编辑", groupOrder: 60,
  menu: "edit", menuOrder: 10, focusPolicy: "keep", keywords: ["find", "chazhao"],
});
reg("edit.replace", "替换", () => toggleFindBar(true, true), {
  shortcut: "Ctrl+H", tab: "home", group: "编辑", groupOrder: 60,
  menu: "edit", menuOrder: 11, focusPolicy: "keep", keywords: ["replace", "tihuan"],
});

// ---- 字符格式（开始/浮动条共用投影） ----
const FLOATING: import("./app/command-registry").CommandSurface[] = ["floating"];
reg("format.font", "字体", () => {}, {
  tab: "home", group: "字体", groupOrder: 20, surfaces: ["ribbon", "palette", ...FLOATING],
  focusPolicy: "keep", keywords: ["font", "ziti"],
});
reg("format.size", "字号", () => {}, {
  tab: "home", group: "字体", groupOrder: 20, surfaces: ["ribbon", "palette", ...FLOATING],
  focusPolicy: "keep", keywords: ["size", "zihao"],
});
reg("format.color", "文字颜色", () => {}, {
  tab: "home", group: "字体", groupOrder: 20, surfaces: ["ribbon", "palette", ...FLOATING],
  focusPolicy: "keep", keywords: ["color", "yanse"],
});
reg("format.highlight", "高亮", () => {}, {
  tab: "home", group: "字体", groupOrder: 20, surfaces: ["ribbon", "palette", ...FLOATING],
  focusPolicy: "keep", keywords: ["highlight", "gaoliang"],
});
reg("format.strong", "加粗", () => pmRun(toggleMark(schema.marks.strong) as never), {
  shortcut: "Ctrl+B", tab: "home", group: "字体", groupOrder: 20, elId: "tb-strong",
  menu: "edit", menuOrder: 20, surfaces: ["ribbon", "palette", "menu", ...FLOATING],
  active: () => markActive("strong"), keywords: ["bold", "jiacu"],
});
reg("format.em", "斜体", () => pmRun(toggleMark(schema.marks.em) as never), {
  shortcut: "Ctrl+I", tab: "home", group: "字体", groupOrder: 20, elId: "tb-em",
  surfaces: ["ribbon", "palette", "menu", ...FLOATING], menu: "edit", menuOrder: 21,
  active: () => markActive("em"), keywords: ["italic", "xieti"],
});
reg("format.underline", "下划线", () => pmRun(toggleMark(schema.marks.underline) as never), {
  shortcut: "Ctrl+U", tab: "home", group: "字体", groupOrder: 20, elId: "tb-underline",
  surfaces: ["ribbon", "palette", "menu", ...FLOATING], menu: "edit", menuOrder: 22,
  active: () => markActive("underline"), keywords: ["underline", "xiahuaxian"],
});
reg("format.strike", "删除线", () => pmRun(toggleMark(schema.marks.strike) as never), {
  tab: "home", group: "字体", groupOrder: 20, elId: "tb-strike",
  surfaces: ["ribbon", "palette", ...FLOATING],
  active: () => markActive("strike"),
});
reg("format.code", "行内代码", () => pmRun(toggleMark(schema.marks.code) as never), {
  tab: "home", group: "字体", groupOrder: 20, elId: "tb-code",
  surfaces: ["ribbon", "palette", ...FLOATING],
  active: () => markActive("code"), keywords: ["code", "daima"],
});
reg("format.link", "链接", () => {
  const v = ctl.view;
  if (!v) return;
  const url = window.prompt("链接 URL：");
  if (url) toggleMark(schema.marks.link, { url })(v.state, v.dispatch);
}, {
  tab: "insert", group: "链接", groupOrder: 40, elId: "tb-link",
  surfaces: ["ribbon", "palette", "menu", ...FLOATING], menu: "insert", menuOrder: 30,
  focusPolicy: "keep", keywords: ["link", "lianjie"],
});

// ---- 段落 ----
reg("para.alignLeft", "左对齐", () => applyPara({ align: "left" }), {
  tab: "home", group: "段落", groupOrder: 30, elId: "tb-align-left",
  active: () => paraIs("align", "left"),
});
reg("para.alignCenter", "居中", () => applyPara({ align: "center" }), {
  tab: "home", group: "段落", groupOrder: 30, elId: "tb-align-center",
  active: () => paraIs("align", "center"),
});
reg("para.alignRight", "右对齐", () => applyPara({ align: "right" }), {
  tab: "home", group: "段落", groupOrder: 30, elId: "tb-align-right",
  active: () => paraIs("align", "right"),
});
reg("para.alignJustify", "两端对齐", () => applyPara({ align: "justify" }), {
  tab: "home", group: "段落", groupOrder: 30, elId: "tb-align-justify",
  active: () => paraIs("align", "justify"),
});
reg("para.indentMore", "增加缩进", () => {
  const cur = ctl.view ? activeParagraphFormat(ctl.view).indentStartPt ?? 0 : 0;
  applyPara({ indentStartPt: Math.min(600, cur + 24) });
}, { tab: "home", group: "段落", groupOrder: 30, elId: "tb-indent-more" });
reg("para.indentLess", "减少缩进", () => {
  const cur = ctl.view ? activeParagraphFormat(ctl.view).indentStartPt ?? 0 : 0;
  applyPara({ indentStartPt: cur - 24 > 0 ? cur - 24 : undefined });
}, { tab: "home", group: "段落", groupOrder: 30, elId: "tb-indent-less" });
reg("format.clear", "清除格式", () => {
  const v = ctl.view;
  if (!v) return;
  clearParagraphFormat(v.state, v.dispatch);
  clearCharacterFormat(v.state, v.dispatch);
}, { tab: "home", group: "段落", groupOrder: 30, elId: "tb-clear-format", menu: "edit", menuOrder: 30 });

// ---- 样式/列表 ----
reg("block.para", "正文", () => pmRun(setBlockType(schema.nodes.paragraph) as never), {
  tab: "home", group: "样式", groupOrder: 40, elId: "tb-para",
});
reg("block.h1", "标题 1", () => pmRun(setBlockType(schema.nodes.heading, { level: 1 }) as never), {
  tab: "home", group: "样式", groupOrder: 40, elId: "tb-h1",
});
reg("block.h2", "标题 2", () => pmRun(setBlockType(schema.nodes.heading, { level: 2 }) as never), {
  tab: "home", group: "样式", groupOrder: 40, elId: "tb-h2",
});
reg("block.h3", "标题 3", () => pmRun(setBlockType(schema.nodes.heading, { level: 3 }) as never), {
  tab: "home", group: "样式", groupOrder: 40, elId: "tb-h3",
});
reg("block.quote", "引用", () => pmRun(wrapIn(schema.nodes.quote) as never), {
  tab: "home", group: "样式", groupOrder: 40, elId: "tb-quote",
});
reg("block.codeblock", "代码块", () => pmRun(setBlockType(schema.nodes.code_block, { language: null }) as never), {
  tab: "home", group: "样式", groupOrder: 40, elId: "tb-codeblock",
});
reg("list.bullet", "项目符号", () => pmRun(wrapInList(schema.nodes.list, { style: "bullet", start: null }) as never), {
  tab: "home", group: "列表", groupOrder: 50, elId: "tb-bullet",
});
reg("list.ordered", "编号", () => pmRun(wrapInList(schema.nodes.list, { style: "ordered", start: 1 }) as never), {
  tab: "home", group: "列表", groupOrder: 50, elId: "tb-ordered",
});
reg("list.outdent", "减少列表缩进", () => pmRun(liftListItem(schema.nodes.list_item) as never), {
  tab: "home", group: "列表", groupOrder: 50,
});

// ---- 页面设置（视图） ----
reg("layout.pageSettings", "页面设置…", () => {
  void askPageSettings(ctl.currentPageTheme).then((theme) => {
    if (theme) ctl.setPageTheme(theme);
  });
}, {
  tab: "view", group: "页面", groupOrder: 20, elId: "m-page-settings",
  menu: "view", menuOrder: 20, focusPolicy: "keep", keywords: ["page", "yemian"],
});

// ---- 插入 ----
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
reg("insert.table", "表格", () => {
  const v = ctl.view;
  if (!v) return;
  const rows = Array.from({ length: 2 }, () => schema.nodes.table_row.create(
      { id: newId("row") },
    Array.from({ length: 2 }, (_, column) => schema.nodes.table_cell.create(
      { id: newId("cel"), column },
      schema.nodes.paragraph.create({ id: newId("blk") }),
    )),
  ));
  const columns = [0, 1].map((index) => ({ id: newId("col"), name: `列 ${index + 1}` }));
  const table = schema.nodes.table.create({ id: newId("blk"), columns }, rows);
  v.dispatch(v.state.tr.replaceSelectionWith(table));
}, {
  tab: "insert", group: "表格", groupOrder: 10, elId: "tb-table",
  menu: "insert", menuOrder: 0, keywords: ["table", "biaoge"],
});
reg("insert.image", "图片", () => {
  const v = ctl.view;
  if (!v) return;
  const input = document.createElement("input");
  input.type = "file";
  input.accept = "image/*";
  input.onchange = () => {
    const file = input.files?.[0];
    if (!file) return;
    const alt = window.prompt("替代文字（可选）：", file.name) ?? file.name;
    // 与粘贴/拖入共用服务端暂存入口（写 asset:// 引用）
    void ctl.insertImageFile(file, alt).then(() => v.focus());
  };
  input.click();
}, {
  tab: "insert", group: "插图", groupOrder: 20, elId: "tb-image",
  menu: "insert", menuOrder: 10, focusPolicy: "keep", keywords: ["image", "tupian"],
});
reg("insert.math", "数学块", () => {
  const v = ctl.view;
  if (!v) return;
  void askText("LaTeX 源码", "E = mc^2").then((latex) => {
    if (latex === null || ctl.view !== v) return;
    v.dispatch(v.state.tr.replaceSelectionWith(schema.nodes.math_block.create({ latex })));
    v.focus();
  });
}, {
  tab: "insert", group: "符号", groupOrder: 30, elId: "tb-math",
  menu: "insert", menuOrder: 20, focusPolicy: "keep", keywords: ["math", "latex", "shuxue"],
});
reg("insert.footnote", "脚注", () => {
  const v = ctl.view;
  if (!v) return;
  void askText("脚注内容", "").then((text) => {
    if (text === null || ctl.view !== v) return;
    const noteId = newId("blk");
    const paragraph = schema.nodes.paragraph.create(
      { id: newId("blk") },
      text ? schema.text(text) : undefined,
    );
    const ref = schema.nodes.footnote_ref.create({ id: noteId });
    const note = schema.nodes.footnote.create({ id: noteId }, paragraph);
    const tr = v.state.tr.replaceSelectionWith(ref);
    tr.insert(tr.doc.content.size, note);
    v.dispatch(tr.scrollIntoView());
    v.focus();
  });
}, {
  tab: "insert", group: "符号", groupOrder: 30, elId: "tb-footnote",
  menu: "insert", menuOrder: 21, focusPolicy: "keep", keywords: ["footnote", "jiaozhu"],
});
reg("footnote.goto", "跳转脚注", () => pmRun(gotoFootnote as never), {
  tab: "insert", group: "符号", groupOrder: 30,
});
reg("insert.rule", "分隔线", () => {
  const v = ctl.view;
  if (v) v.dispatch(v.state.tr.replaceSelectionWith(schema.nodes.horizontal_rule.create()));
}, {
  tab: "insert", group: "符号", groupOrder: 50, elId: "tb-rule",
  menu: "insert", menuOrder: 40,
});
// 手动分页（E4）：插入 page_break 节点并补一段供光标落点（若文档末尾无后续块）。
reg("insert.pageBreak", "分页符", () => {
  const v = ctl.view;
  if (!v) return;
  const pb = schema.nodes.page_break.create({ id: newId("blk") });
  const tr = v.state.tr.replaceSelectionWith(pb);
  const after = tr.mapping.map(tr.selection.from);
  const $after = tr.doc.resolve(after);
  if ($after.nodeAfter === null || $after.nodeAfter.type.name === "page_break") {
    tr.insert(after, schema.nodes.paragraph.create({ id: newId("blk") }));
    tr.setSelection(TextSelection.create(tr.doc, after + 1));
  }
  v.dispatch(tr.scrollIntoView());
  v.focus();
}, {
  tab: "insert", group: "页面", groupOrder: 60, elId: "tb-page-break",
  menu: "insert", menuOrder: 41, keywords: ["page break", "fenye"],
});

// ---- 视图 ----
reg("view.preview", "打印预览", () => void openPreview(), {
  shortcut: "Ctrl+P", tab: "view", group: "预览", groupOrder: 10, elId: "m-preview",
  menu: "view", menuOrder: 10, menuId: "m-preview-menu", focusPolicy: "keep",
  keywords: ["preview", "print", "yulan", "dayin"],
});
reg("view.toggleOutline", "大纲面板", () => shell.toggleOutline(), {
  tab: "view", group: "面板", groupOrder: 30, needsDoc: false,
  menu: "view", menuOrder: 30, active: () => shell.leftOpen,
});
reg("view.toggleInspector", "检查面板", () => shell.toggleInspector(), {
  tab: "view", group: "面板", groupOrder: 30, needsDoc: false,
  menu: "view", menuOrder: 31, active: () => shell.rightOpen,
});
reg("view.ribbonCollapse", "折叠功能区", () => {
  ribbonToggleCollapse();
}, {
  tab: "view", group: "面板", groupOrder: 30, needsDoc: false,
  menu: "view", menuOrder: 32,
});
reg("view.theme", "深色主题", () => shell.toggleTheme(), {
  tab: "view", group: "外观", groupOrder: 40, needsDoc: false,
  menu: "view", menuOrder: 40, active: () => shell.theme === "dark",
  keywords: ["dark", "theme", "shense"],
});
reg("view.palette", "命令面板", () => palette.open(), {
  shortcut: "Ctrl+K", tab: "view", group: "工具", groupOrder: 50, needsDoc: false,
  menu: "view", menuOrder: 50, focusPolicy: "keep", keywords: ["command", "palette", "mingling"],
});
reg("view.zoomIn", "放大", () => setZoom(zoom + 0.1), {
  tab: "view", group: "缩放", groupOrder: 60, needsDoc: false,
  menu: "view", menuOrder: 60, elId: "view-zoom-in",
});
reg("view.zoomOut", "缩小", () => setZoom(zoom - 0.1), {
  tab: "view", group: "缩放", groupOrder: 60, needsDoc: false,
  menu: "view", menuOrder: 61, elId: "view-zoom-out",
});
reg("view.zoomReset", "100%", () => setZoom(1), {
  tab: "view", group: "缩放", groupOrder: 60, needsDoc: false,
  menu: "view", menuOrder: 62, elId: "view-zoom-reset",
});

// ---- 文档检查 ----
reg("file.verify", "检查文档", () => ctl.verify(), {
  tab: "review", group: "校验", groupOrder: 10,
  menu: "review", menuOrder: 0, menuId: "m-verify", focusPolicy: "keep",
  keywords: ["verify", "jiancha"],
});
// （修订历史/标注/损耗在检查面板中派生渲染；还原入口在历史列表项上）

// ---- 表格（上下文页签） ----
reg("table.rowAddBefore", "在上方插入行", () => pmRun(addTableRowBefore as never), {
  tab: "table", group: "行", groupOrder: 10, needsTable: true,
});
reg("table.rowAdd", "在下方插入行", () => pmRun(addTableRow as never), {
  tab: "table", group: "行", groupOrder: 10, needsTable: true, elId: "tb-row-add",
});
reg("table.rowDelete", "删除行", () => pmRun(deleteTableRow as never), {
  tab: "table", group: "行", groupOrder: 10, needsTable: true, elId: "tb-row-del",
});
reg("table.colAddBefore", "在左侧插入列", () => pmRun(addTableColumnBefore as never), {
  tab: "table", group: "列", groupOrder: 20, needsTable: true,
});
reg("table.colAdd", "在右侧插入列", () => pmRun(addTableColumn as never), {
  tab: "table", group: "列", groupOrder: 20, needsTable: true, elId: "tb-col-add",
});
reg("table.colDelete", "删除列", () => pmRun(deleteTableColumn as never), {
  tab: "table", group: "列", groupOrder: 20, needsTable: true, elId: "tb-col-del",
});
reg("table.merge", "合并单元格", () => pmRun(mergeTableCells as never), {
  tab: "table", group: "单元格", groupOrder: 30, needsTable: true, elId: "tb-cell-merge",
});
reg("table.split", "拆分单元格", () => pmRun(splitTableCell as never), {
  tab: "table", group: "单元格", groupOrder: 30, needsTable: true, elId: "tb-cell-split",
});
reg("table.header", "切换表头行", () => pmRun(toggleHeaderRow as never), {
  tab: "table", group: "属性", groupOrder: 40, needsTable: true,
});
reg("table.delete", "删除表格", () => pmRun(deleteWholeTable as never), {
  tab: "table", group: "属性", groupOrder: 40, needsTable: true,
});
reg("table.fix", "修复表格结构", () => pmRun(fixTable as never), {
  tab: "table", group: "属性", groupOrder: 40, needsTable: true,
});

// ---------------------------------------------------------------- shell 组件

const shell = new Shell({
  applyChar,
  refreshUI,
  sel,
});

const ribbon = new Ribbon({
  registry,
  overlays,
  ctx,
  sel,
  runCommand,
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

function ribbonToggleCollapse(): void {
  ribbon.toggleCollapse();
}

function applyPara(patch: Parameters<typeof setParagraphFormat>[2]): void {
  const v = ctl.view;
  if (v) setParagraphFormat(v.state, v.dispatch, patch);
}

function applyChar(patch: Parameters<typeof setCharacterFormat>[2]): void {
  const v = ctl.view;
  if (v) setCharacterFormat(v.state, v.dispatch, patch);
}

function paraIs(key: "align", value: string): boolean {
  const v = ctl.view;
  if (!v) return false;
  return activeParagraphFormat(v)[key] === value;
}

function setZoom(z: number): void {
  zoom = Math.min(2.5, Math.max(0.5, z));
  ($("editor").closest(".page") as HTMLElement | null)?.style.setProperty("zoom", String(zoom));
  refreshUI();
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
  if (typeof override === "number") {
    const surface = new PreviewSurface(override);
    const outcome = await surface.open(result);
    previewPages = outcome.fallback || outcome.page_count === null
      ? null
      : { generation, count: outcome.page_count };
    return;
  }
  const outcome = await previewSurface.open(result);
  const breaks = outcome.layout?.page_breaks.length ?? 0;
  previewPages = outcome.fallback || outcome.page_count === null
    ? null
    : { generation, count: outcome.page_count };
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
  refreshUI();
}

/** 最近预览页数（快照代数与当前一致才有效；过期不显示）。 */
function previewPageCount(): number | null {
  if (!previewPages) return null;
  return previewPages.generation === ctl.store.state.editGeneration ? previewPages.count : null;
}

function markActive(name: string): boolean {
  const v = ctl.view;
  if (!v) return false;
  const mark = schema.marks[name];
  if (!mark) return false;
  const { from, $from, to, empty } = v.state.selection;
  if (empty) return mark.isInSet(v.state.storedMarks ?? $from.marks()) !== undefined;
  return v.state.doc.rangeHasMark(from, to, mark);
}

// ---------------------------------------------------------------- 统一刷新

function refreshUI(): void {
  const c = ctx();
  ribbon.refresh();
  menus.refresh();
  // 所有 surface 的同 id 控件（ribbon/menu/浮动条/状态栏/顶栏）一起更新
  for (const cmd of registry.all()) {
    for (const el of document.querySelectorAll<HTMLElement>(`[data-cmd="${cmd.id}"]`)) {
      const visible = cmd.visible?.(c) ?? true;
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
  });
  renderInspector(sel());
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
    if (!cmd.enabled(ctx())) return;
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
