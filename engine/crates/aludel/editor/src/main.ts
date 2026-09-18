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

//! 装配层：平台 gateway + 会话控制器 + 命令注册表 + UI 绑定（一次性）。
//! 业务状态在 app/session-store + document-controller；渲染在 ui/panels。

import { TextSelection } from "prosemirror-state";
import { toggleMark, wrapIn, setBlockType } from "prosemirror-commands";
import { wrapInList, liftListItem } from "prosemirror-schema-list";
import { undo, redo } from "prosemirror-history";
import "prosemirror-view/style/prosemirror.css";
import { schema } from "./schema";
import { DocumentController, errText } from "./app/document-controller";
import { CommandRegistry, type CommandContext } from "./app/command-registry";
import { findAll, selectMatch, replaceCurrent, replaceAll } from "./app/find";
import { addTableRow, deleteTableRow, addTableColumn, deleteTableColumn, toggleHeaderRow } from "./app/table-commands";
import {
  setParagraphFormat,
  setCharacterFormat,
  clearParagraphFormat,
  clearCharacterFormat,
  activeCharacterFormat,
  activeParagraphFormat,
} from "./app/format";
import { TauriGateway } from "./platform/tauri";
import { HttpGateway } from "./platform/http";
import { $, renderOutline, renderSidebar, renderStatusBar, setMsg } from "./ui/panels";
import { askText } from "./ui/dialogs";

const tauriMode = "__TAURI_INTERNALS__" in window;
const gateway = tauriMode ? new TauriGateway() : new HttpGateway();

let lastChars = 0;
let zoom = 1;

const ctl = new DocumentController($("editor"), gateway, {
  onStateChange: refreshUI,
  onDocMeta: renderSidebar,
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

function ctx(): CommandContext {
  const s = ctl.store.state;
  return {
    hasDocument: ctl.view !== null,
    desktop: gateway.desktop,
    dirty: s.dirty,
    busy: s.activeJob !== null || s.saveState === "saving",
  };
}

const registry = new CommandRegistry();

function reg(id: string, label: string, run: () => void | Promise<void>, opts?: {
  shortcut?: string;
  desktopOnly?: boolean;
  needsDoc?: boolean;
  active?: () => boolean;
}) {
  registry.register({
    id,
    label,
    shortcut: opts?.shortcut,
    active: opts?.active,
    enabled: (c) =>
      (opts?.needsDoc === false || c.hasDocument) && (!opts?.desktopOnly || c.desktop),
    run,
  });
}

// ---------------------------------------------------------------- 命令

function pmRun(fn: (state: never, dispatch: never) => boolean): void {
  const v = ctl.view;
  if (v) fn(v.state as never, v.dispatch as never);
}

reg("file.new", "新建", () => ctl.newDocument(), { shortcut: "Ctrl+N", desktopOnly: true, needsDoc: false });
reg("file.open", "打开…", () => ctl.pickAndOpen(), { shortcut: "Ctrl+O", desktopOnly: true, needsDoc: false });
reg("file.save", "保存", () => ctl.requestSave(), { shortcut: "Ctrl+S" });
reg("file.saveAs", "另存为…", () => ctl.saveAs(), { shortcut: "Ctrl+Shift+S", desktopOnly: true });
reg("file.close", "关闭", () => ctl.closeDocument(), { desktopOnly: true });
reg("file.verify", "检查文档", () => ctl.verify(), {});
reg("edit.undo", "撤销", () => pmRun(undo as never), { shortcut: "Ctrl+Z" });
reg("edit.redo", "重做", () => pmRun(redo as never), { shortcut: "Ctrl+Y" });
reg("edit.find", "查找", () => toggleFindBar(true), { shortcut: "Ctrl+F" });
reg("edit.replace", "替换", () => toggleFindBar(true, true), { shortcut: "Ctrl+H" });
reg("format.strong", "加粗", () => pmRun(toggleMark(schema.marks.strong) as never), {
  shortcut: "Ctrl+B",
  active: () => markActive("strong"),
});
reg("format.em", "斜体", () => pmRun(toggleMark(schema.marks.em) as never), {
  shortcut: "Ctrl+I",
  active: () => markActive("em"),
});
reg("format.underline", "下划线", () => pmRun(toggleMark(schema.marks.underline) as never), {
  shortcut: "Ctrl+U",
  active: () => markActive("underline"),
});
reg("format.strike", "删除线", () => pmRun(toggleMark(schema.marks.strike) as never), {
  active: () => markActive("strike"),
});
reg("format.code", "行内代码", () => pmRun(toggleMark(schema.marks.code) as never), {
  active: () => markActive("code"),
});
reg("format.link", "链接", () => {
  const v = ctl.view;
  if (!v) return;
  const url = window.prompt("链接 URL：");
  if (url) toggleMark(schema.marks.link, { url })(v.state, v.dispatch);
});
reg("format.font", "字体", () => {}, {});
reg("format.size", "字号", () => {}, {});
reg("format.color", "文字颜色", () => {}, {});
reg("format.highlight", "高亮", () => {}, {});
reg("para.alignLeft", "左对齐", () => applyPara({ align: "left" }), { active: () => paraIs("align", "left") });
reg("para.alignCenter", "居中", () => applyPara({ align: "center" }), { active: () => paraIs("align", "center") });
reg("para.alignRight", "右对齐", () => applyPara({ align: "right" }), { active: () => paraIs("align", "right") });
reg("para.alignJustify", "两端对齐", () => applyPara({ align: "justify" }), { active: () => paraIs("align", "justify") });
reg("para.indentMore", "增加缩进", () => {
  const cur = ctl.view ? activeParagraphFormat(ctl.view).indentStartPt ?? 0 : 0;
  applyPara({ indentStartPt: Math.min(600, cur + 24) });
});
reg("para.indentLess", "减少缩进", () => {
  const cur = ctl.view ? activeParagraphFormat(ctl.view).indentStartPt ?? 0 : 0;
  applyPara({ indentStartPt: cur - 24 > 0 ? cur - 24 : undefined });
});
reg("format.clear", "清除格式", () => {
  const v = ctl.view;
  if (!v) return;
  clearParagraphFormat(v.state, v.dispatch);
  clearCharacterFormat(v.state, v.dispatch);
});

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
reg("block.para", "正文", () => pmRun(setBlockType(schema.nodes.paragraph) as never));
reg("block.h1", "标题 1", () => pmRun(setBlockType(schema.nodes.heading, { level: 1 }) as never));
reg("block.h2", "标题 2", () => pmRun(setBlockType(schema.nodes.heading, { level: 2 }) as never));
reg("block.h3", "标题 3", () => pmRun(setBlockType(schema.nodes.heading, { level: 3 }) as never));
reg("block.quote", "引用", () => pmRun(wrapIn(schema.nodes.quote) as never));
reg("block.codeblock", "代码块", () => pmRun(setBlockType(schema.nodes.code_block, { language: null }) as never));
reg("list.bullet", "项目符号", () => pmRun(wrapInList(schema.nodes.list, { style: "bullet", start: null }) as never));
reg("list.ordered", "编号", () => pmRun(wrapInList(schema.nodes.list, { style: "ordered", start: 1 }) as never));
reg("list.outdent", "减少列表缩进", () => pmRun(liftListItem(schema.nodes.list_item) as never));
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
});
reg("insert.image", "图片", () => {
  const v = ctl.view;
  if (!v) return;
  const input = document.createElement("input");
  input.type = "file";
  input.accept = "image/*";
  input.onchange = () => {
    const file = input.files?.[0];
    if (!file || file.size > 10 * 1024 * 1024) {
      if (file) setMsg("图片过大（上限 10 MB）。", false);
      return;
    }
    const reader = new FileReader();
    reader.onload = () => {
      if (typeof reader.result !== "string" || ctl.view !== v) return;
      const alt = window.prompt("替代文字（可选）：", file.name) ?? file.name;
      v.dispatch(v.state.tr.replaceSelectionWith(schema.nodes.image.create({ id: newId("blk"), asset: reader.result, alt })));
      v.focus();
    };
    reader.readAsDataURL(file);
  };
  input.click();
});
reg("insert.rule", "分隔线", () => {
  const v = ctl.view;
  if (v) v.dispatch(v.state.tr.replaceSelectionWith(schema.nodes.horizontal_rule.create()));
});
reg("table.rowAdd", "增加表格行", () => pmRun(addTableRow as never));
reg("table.rowDelete", "删除表格行", () => pmRun(deleteTableRow as never));
reg("table.colAdd", "增加表格列", () => pmRun(addTableColumn as never));
reg("table.colDelete", "删除表格列", () => pmRun(deleteTableColumn as never));
reg("table.header", "切换表头", () => pmRun(toggleHeaderRow as never));
reg("insert.math", "数学块", () => {
  const v = ctl.view;
  if (!v) return;
  void askText("LaTeX 源码", "E = mc^2").then((latex) => {
    if (latex === null || ctl.view !== v) return;
    v.dispatch(v.state.tr.replaceSelectionWith(schema.nodes.math_block.create({ latex })));
    v.focus();
  });
});
reg("job.import", "导入…", () => ctl.runJob("import"), { desktopOnly: true, needsDoc: false });
reg("job.export", "导出 Markdown…", () => ctl.runJob("export"), { desktopOnly: true });
reg("job.exportHtml", "导出 HTML…", () => ctl.runJob("export", "html"), { desktopOnly: true });
reg("job.exportText", "导出纯文本…", () => ctl.runJob("export", "text"), { desktopOnly: true });
reg("job.exportDocx", "导出 Word 文档…", () => ctl.runJob("export", "docx"), { desktopOnly: true });
reg("job.publish", "导出 PDF…", () => ctl.runJob("publish"), { desktopOnly: true });
reg("job.cancel", "取消任务", () => ctl.cancelJob(), { desktopOnly: true });
reg("view.zoomIn", "放大", () => setZoom(zoom + 0.1));
reg("view.zoomOut", "缩小", () => setZoom(zoom - 0.1));
reg("view.zoomReset", "100%", () => setZoom(1));

function markActive(name: string): boolean {
  const v = ctl.view;
  if (!v) return false;
  const mark = schema.marks[name];
  if (!mark) return false;
  const { from, $from, to, empty } = v.state.selection;
  if (empty) return mark.isInSet(v.state.storedMarks ?? $from.marks()) !== undefined;
  return v.state.doc.rangeHasMark(from, to, mark);
}

// ---------------------------------------------------------------- UI 绑定（一次性）

function bindButton(id: string, commandId: string): void {
  const el = $(id) as HTMLButtonElement;
  const cmd = registry.get(commandId);
  if (cmd?.shortcut) el.title = `${cmd.label} (${cmd.shortcut})`;
  el.addEventListener("click", () => {
    void registry.run(commandId, ctx()).finally(() => {
      // Word 风格工具栏：命令执行后把输入焦点还给正文，保留原选区继续输入。
      if (ctl.view && !["file.new", "file.open", "file.close"].includes(commandId)) ctl.view.focus();
    });
  });
}

function refreshUI(): void {
  const c = ctx();
  for (const cmd of registry.all()) {
    const el = document.querySelector<HTMLElement>(`[data-cmd="${cmd.id}"]`);
    if (el instanceof HTMLButtonElement) {
      el.disabled = !cmd.enabled(c);
      if (cmd.active) el.classList.toggle("active", cmd.active());
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
  });
  // 格式控件回显（选区所在位置的字符格式）
  if (ctl.view && !ctl.composing) {
    const cf = activeCharacterFormat(ctl.view);
    const fontSel = $("tb-font") as HTMLSelectElement;
    const sizeSel = $("tb-size") as HTMLSelectElement;
    if (document.activeElement !== fontSel) fontSel.value = cf.fontFamily ?? "";
    if (document.activeElement !== sizeSel) sizeSel.value = cf.fontSizePt ? String(cf.fontSizePt) : "";
    const colEl = $("tb-color") as HTMLInputElement;
    if (document.activeElement !== colEl && cf.color) colEl.value = cf.color;
    const hlEl = $("tb-highlight") as HTMLInputElement;
    if (document.activeElement !== hlEl && cf.highlight) hlEl.value = cf.highlight;
  }
}

function setZoom(z: number): void {
  zoom = Math.min(2.5, Math.max(0.5, z));
  ($("editor").closest(".page") as HTMLElement | null)?.style.setProperty("zoom", String(zoom));
  refreshUI();
}

// ---------------------------------------------------------------- 查找栏

function toggleFindBar(show: boolean, withReplace = false): void {
  const bar = $("findbar");
  bar.style.display = show ? "flex" : "none";
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

// ---------------------------------------------------------------- 快捷键

function onKeydown(e: KeyboardEvent): void {
  const target = e.target as HTMLElement | null;
  if (target?.matches("input, textarea, select, [contenteditable=false]")) return;
  const mod = e.ctrlKey || e.metaKey;
  if (!mod) return;
  const k = e.key.toLowerCase();
  const map: Record<string, string> = {
    s: e.shiftKey ? "file.saveAs" : "file.save",
    n: "file.new",
    o: "file.open",
    f: "edit.find",
    h: "edit.replace",
    b: "format.strong",
    i: "format.em",
    u: "format.underline",
  };
  const id = map[k];
  if (!id) return;
  const cmd = registry.get(id);
  if (!cmd || !cmd.enabled(ctx())) return;
  e.preventDefault();
  void registry.run(id, ctx());
}

// ---------------------------------------------------------------- 入口

async function boot(): Promise<void> {
  // 一次性绑定全部按钮/菜单（mountDocument 不再触碰 DOM 监听器）
  const bindings: [string, string][] = [
    ["m-new", "file.new"],
    ["m-open", "file.open"],
    ["m-save", "file.save"],
    ["m-saveas", "file.saveAs"],
    ["m-close", "file.close"],
    ["tb-para", "block.para"],
    ["tb-h1", "block.h1"],
    ["tb-h2", "block.h2"],
    ["tb-h3", "block.h3"],
    ["tb-quote", "block.quote"],
    ["tb-codeblock", "block.codeblock"],
    ["tb-table", "insert.table"],
    ["tb-image", "insert.image"],
    ["tb-row-add", "table.rowAdd"],
    ["tb-col-add", "table.colAdd"],
    ["tb-row-del", "table.rowDelete"],
    ["tb-col-del", "table.colDelete"],
    ["tb-math", "insert.math"],
    ["tb-rule", "insert.rule"],
    ["tb-strong", "format.strong"],
    ["tb-em", "format.em"],
    ["tb-underline", "format.underline"],
    ["tb-strike", "format.strike"],
    ["tb-code", "format.code"],
    ["tb-link", "format.link"],
    ["tb-undo", "edit.undo"],
    ["tb-redo", "edit.redo"],
    ["m-verify", "file.verify"],
    ["m-import", "job.import"],
    ["m-export", "job.export"],
    ["m-publish", "job.publish"],
    ["cancel-job", "job.cancel"],
    ["sb-zoom-in", "view.zoomIn"],
    ["sb-zoom-out", "view.zoomOut"],
    ["sb-zoom-reset", "view.zoomReset"],
    ["tb-align-left", "para.alignLeft"],
    ["tb-align-center", "para.alignCenter"],
    ["tb-align-right", "para.alignRight"],
    ["tb-align-justify", "para.alignJustify"],
    ["tb-indent-more", "para.indentMore"],
    ["tb-indent-less", "para.indentLess"],
    ["tb-clear-format", "format.clear"],
  ];
  for (const [el, cmd] of bindings) bindButton(el, cmd);

  // 下拉/颜色控件（非按钮）
  $("tb-font").addEventListener("change", (e) => {
    applyChar({ fontFamily: (e.target as HTMLSelectElement).value || undefined });
  });
  $("tb-size").addEventListener("change", (e) => {
    const v = Number((e.target as HTMLSelectElement).value);
    applyChar({ fontSizePt: Number.isFinite(v) && v > 0 ? v : undefined });
  });
  $("tb-color").addEventListener("input", (e) => {
    applyChar({ color: (e.target as HTMLInputElement).value });
  });
  $("tb-highlight").addEventListener("input", (e) => {
    applyChar({ highlight: (e.target as HTMLInputElement).value });
  });

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
