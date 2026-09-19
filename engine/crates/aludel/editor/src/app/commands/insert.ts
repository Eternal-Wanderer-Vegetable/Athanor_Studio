// This file is part of Athanor, the Azodoc document engine.
// SPDX-License-Identifier: AGPL-3.0-only

//! 插入域命令：表格/图片/数学/脚注/分隔线/分页符 + 跳转脚注。
//! 引用页签承载脚注相关命令（Word 习惯）；模型未就绪的引用入口不放假按钮。

import { TextSelection } from "prosemirror-state";
import { gotoFootnote } from "../footnotes";
import { askText } from "../../ui/dialogs";
import { schema } from "../../schema";
import type { CommandDeps } from "./deps";
import type { RegDef } from "./types";

export function insertCommands(deps: CommandDeps): RegDef[] {
  const { ctl, newId } = deps;
  return [
    { id: "insert.table", label: "表格", run: () => {
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
    }, opts: {
      tab: "insert", group: "表格", groupOrder: 10, elId: "tb-table",
      menu: "insert", menuOrder: 0, keywords: ["table", "biaoge"],
    } },
    { id: "insert.image", label: "图片", run: () => {
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
    }, opts: {
      caps: ["assetRegistry"],
      tab: "insert", group: "插图", groupOrder: 20, elId: "tb-image",
      menu: "insert", menuOrder: 10, focusPolicy: "keep", keywords: ["image", "tupian"],
    } },
    { id: "insert.math", label: "数学块", run: () => {
      const v = ctl.view;
      if (!v) return;
      void askText("LaTeX 源码", "E = mc^2").then((latex) => {
        if (latex === null || ctl.view !== v) return;
        v.dispatch(v.state.tr.replaceSelectionWith(schema.nodes.math_block.create({ latex })));
        v.focus();
      });
    }, opts: {
      tab: "insert", group: "符号", groupOrder: 30, elId: "tb-math",
      menu: "insert", menuOrder: 20, focusPolicy: "keep", keywords: ["math", "latex", "shuxue"],
    } },
    { id: "insert.footnote", label: "脚注", run: () => {
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
    }, opts: {
      // 脚注命令同时投影到 插入 与 引用 页签（Word 习惯：引用页签承载脚注）
      tab: "references", group: "脚注", groupOrder: 10, elId: "tb-footnote",
      menu: "insert", menuOrder: 21, focusPolicy: "keep", keywords: ["footnote", "jiaozhu"],
    } },
    { id: "footnote.goto", label: "跳转脚注", run: () => {
      const v = ctl.view;
      if (v) gotoFootnote(v.state as never, v.dispatch as never);
    }, opts: {
      tab: "references", group: "脚注", groupOrder: 10,
    } },
    { id: "insert.rule", label: "分隔线", run: () => {
      const v = ctl.view;
      if (v) v.dispatch(v.state.tr.replaceSelectionWith(schema.nodes.horizontal_rule.create()));
    }, opts: {
      tab: "insert", group: "符号", groupOrder: 50, elId: "tb-rule",
      menu: "insert", menuOrder: 40,
    } },
    // 手动分页（E4）：插入 page_break 节点并补一段供光标落点（若文档末尾无后续块）。
    { id: "insert.pageBreak", label: "分页符", run: () => {
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
    }, opts: {
      tab: "insert", group: "页面", groupOrder: 60, elId: "tb-page-break",
      menu: "insert", menuOrder: 41, keywords: ["page break", "fenye"],
    } },
  ];
}
