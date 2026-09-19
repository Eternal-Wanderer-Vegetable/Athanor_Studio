// This file is part of Athanor, the Azodoc document engine.
// SPDX-License-Identifier: AGPL-3.0-only

//! 字符/段落/样式格式命令：开始页签与浮动条共用同一投影；
//! 写入统一走 format.ts 的受控事务（x-athanor-format 键集合）。

import { toggleMark, wrapIn, setBlockType } from "prosemirror-commands";
import { wrapInList, liftListItem } from "prosemirror-schema-list";
import { schema } from "../../schema";
import {
  activeParagraphFormat,
  clearCharacterFormat,
  clearParagraphFormat,
} from "../format";
import type { CommandDeps } from "./deps";
import type { RegDef } from "./types";
import type { CommandSurface } from "../command-registry";

const FLOATING: CommandSurface[] = ["floating"];

export function formatCommands(deps: CommandDeps): RegDef[] {
  const { ctl } = deps;
  const pmRun = (fn: (state: never, dispatch: never, view?: never) => boolean): void => {
    const v = ctl.view;
    if (v) fn(v.state as never, v.dispatch as never, v as never);
  };
  const markActive = (name: string): boolean => {
    const v = ctl.view;
    if (!v) return false;
    const mark = schema.marks[name];
    if (!mark) return false;
    const { from, $from, to, empty } = v.state.selection;
    if (empty) return mark.isInSet(v.state.storedMarks ?? $from.marks()) !== undefined;
    return v.state.doc.rangeHasMark(from, to, mark);
  };
  const paraIs = (key: "align", value: string): boolean => {
    const v = ctl.view;
    if (!v) return false;
    return activeParagraphFormat(v)[key] === value;
  };
  return [
    // ---- picker 型命令：菜单/面板触发打开 ribbon 控件，不执行空 run ----
    { id: "format.font", label: "字体", run: () => deps.openPicker("tb-font"), opts: {
      tab: "home", group: "字体", groupOrder: 20, surfaces: ["ribbon", "palette", ...FLOATING],
      focusPolicy: "keep", keywords: ["font", "ziti"],
    } },
    { id: "format.size", label: "字号", run: () => deps.openPicker("tb-size"), opts: {
      tab: "home", group: "字体", groupOrder: 20, surfaces: ["ribbon", "palette", ...FLOATING],
      focusPolicy: "keep", keywords: ["size", "zihao"],
    } },
    { id: "format.color", label: "文字颜色", run: () => deps.openPicker("tb-color"), opts: {
      tab: "home", group: "字体", groupOrder: 20, surfaces: ["ribbon", "palette", ...FLOATING],
      focusPolicy: "keep", keywords: ["color", "yanse"],
    } },
    { id: "format.highlight", label: "高亮", run: () => deps.openPicker("tb-highlight"), opts: {
      tab: "home", group: "字体", groupOrder: 20, surfaces: ["ribbon", "palette", ...FLOATING],
      focusPolicy: "keep", keywords: ["highlight", "gaoliang"],
    } },
    // ---- 字符格式（mark） ----
    { id: "format.strong", label: "加粗", run: () => pmRun(toggleMark(schema.marks.strong) as never), opts: {
      shortcut: "Ctrl+B", tab: "home", group: "字体", groupOrder: 20, elId: "tb-strong",
      menu: "format", menuOrder: 0, surfaces: ["ribbon", "palette", "menu", ...FLOATING],
      active: () => markActive("strong"), keywords: ["bold", "jiacu"],
    } },
    { id: "format.em", label: "斜体", run: () => pmRun(toggleMark(schema.marks.em) as never), opts: {
      shortcut: "Ctrl+I", tab: "home", group: "字体", groupOrder: 20, elId: "tb-em",
      surfaces: ["ribbon", "palette", "menu", ...FLOATING], menu: "format", menuOrder: 1,
      active: () => markActive("em"), keywords: ["italic", "xieti"],
    } },
    { id: "format.underline", label: "下划线", run: () => pmRun(toggleMark(schema.marks.underline) as never), opts: {
      shortcut: "Ctrl+U", tab: "home", group: "字体", groupOrder: 20, elId: "tb-underline",
      surfaces: ["ribbon", "palette", "menu", ...FLOATING], menu: "format", menuOrder: 2,
      active: () => markActive("underline"), keywords: ["underline", "xiahuaxian"],
    } },
    { id: "format.strike", label: "删除线", run: () => pmRun(toggleMark(schema.marks.strike) as never), opts: {
      tab: "home", group: "字体", groupOrder: 20, elId: "tb-strike",
      surfaces: ["ribbon", "palette", "menu", ...FLOATING], menu: "format", menuOrder: 3,
      active: () => markActive("strike"),
    } },
    { id: "format.code", label: "行内代码", run: () => pmRun(toggleMark(schema.marks.code) as never), opts: {
      tab: "home", group: "字体", groupOrder: 20, elId: "tb-code",
      surfaces: ["ribbon", "palette", "menu", ...FLOATING], menu: "format", menuOrder: 4,
      active: () => markActive("code"), keywords: ["code", "daima"],
    } },
    { id: "format.link", label: "链接", run: () => {
      const v = ctl.view;
      if (!v) return;
      const url = window.prompt("链接 URL：");
      if (url) toggleMark(schema.marks.link, { url })(v.state, v.dispatch);
    }, opts: {
      tab: "insert", group: "链接", groupOrder: 40, elId: "tb-link",
      surfaces: ["ribbon", "palette", "menu", ...FLOATING], menu: "insert", menuOrder: 30,
      focusPolicy: "keep", keywords: ["link", "lianjie"],
    } },
    { id: "format.clear", label: "清除格式", run: () => {
      const v = ctl.view;
      if (!v) return;
      clearParagraphFormat(v.state, v.dispatch);
      clearCharacterFormat(v.state, v.dispatch);
    }, opts: {
      tab: "home", group: "段落", groupOrder: 30, elId: "tb-clear-format",
      menu: "format", menuOrder: 30, keywords: ["clear", "qingchu"],
    } },
    // ---- 段落 ----
    { id: "para.alignLeft", label: "左对齐", run: () => deps.applyPara({ align: "left" }), opts: {
      tab: "home", group: "段落", groupOrder: 30, elId: "tb-align-left",
      menu: "format", menuOrder: 10, active: () => paraIs("align", "left"),
    } },
    { id: "para.alignCenter", label: "居中", run: () => deps.applyPara({ align: "center" }), opts: {
      tab: "home", group: "段落", groupOrder: 30, elId: "tb-align-center",
      menu: "format", menuOrder: 11, active: () => paraIs("align", "center"),
    } },
    { id: "para.alignRight", label: "右对齐", run: () => deps.applyPara({ align: "right" }), opts: {
      tab: "home", group: "段落", groupOrder: 30, elId: "tb-align-right",
      menu: "format", menuOrder: 12, active: () => paraIs("align", "right"),
    } },
    { id: "para.alignJustify", label: "两端对齐", run: () => deps.applyPara({ align: "justify" }), opts: {
      tab: "home", group: "段落", groupOrder: 30, elId: "tb-align-justify",
      menu: "format", menuOrder: 13, active: () => paraIs("align", "justify"),
    } },
    { id: "para.indentMore", label: "增加缩进", run: () => {
      const cur = ctl.view ? activeParagraphFormat(ctl.view).indentStartPt ?? 0 : 0;
      deps.applyPara({ indentStartPt: Math.min(600, cur + 24) });
    }, opts: {
      tab: "home", group: "段落", groupOrder: 30, elId: "tb-indent-more",
      menu: "format", menuOrder: 14,
    } },
    { id: "para.indentLess", label: "减少缩进", run: () => {
      const cur = ctl.view ? activeParagraphFormat(ctl.view).indentStartPt ?? 0 : 0;
      deps.applyPara({ indentStartPt: cur - 24 > 0 ? cur - 24 : undefined });
    }, opts: {
      tab: "home", group: "段落", groupOrder: 30, elId: "tb-indent-less",
      menu: "format", menuOrder: 15,
    } },
    // ---- 样式（schema 支持的块类型；无 schema 支撑的样式不出现） ----
    { id: "block.para", label: "正文", run: () => pmRun(setBlockType(schema.nodes.paragraph) as never), opts: {
      tab: "home", group: "样式", groupOrder: 40, elId: "tb-para",
      menu: "format", menuOrder: 20,
    } },
    { id: "block.h1", label: "标题 1", run: () => pmRun(setBlockType(schema.nodes.heading, { level: 1 }) as never), opts: {
      tab: "home", group: "样式", groupOrder: 40, elId: "tb-h1",
    } },
    { id: "block.h2", label: "标题 2", run: () => pmRun(setBlockType(schema.nodes.heading, { level: 2 }) as never), opts: {
      tab: "home", group: "样式", groupOrder: 40, elId: "tb-h2",
    } },
    { id: "block.h3", label: "标题 3", run: () => pmRun(setBlockType(schema.nodes.heading, { level: 3 }) as never), opts: {
      tab: "home", group: "样式", groupOrder: 40, elId: "tb-h3",
    } },
    { id: "block.quote", label: "引用", run: () => pmRun(wrapIn(schema.nodes.quote) as never), opts: {
      tab: "home", group: "样式", groupOrder: 40, elId: "tb-quote",
    } },
    { id: "block.codeblock", label: "代码块", run: () => pmRun(setBlockType(schema.nodes.code_block, { language: null }) as never), opts: {
      tab: "home", group: "样式", groupOrder: 40, elId: "tb-codeblock",
    } },
    // ---- 列表（并入“段落”组：与 Word 开始页签习惯一致） ----
    { id: "list.bullet", label: "项目符号", run: () => pmRun(wrapInList(schema.nodes.list, { style: "bullet", start: null }) as never), opts: {
      tab: "home", group: "段落", groupOrder: 30, elId: "tb-bullet",
    } },
    { id: "list.ordered", label: "编号", run: () => pmRun(wrapInList(schema.nodes.list, { style: "ordered", start: 1 }) as never), opts: {
      tab: "home", group: "段落", groupOrder: 30, elId: "tb-ordered",
    } },
    { id: "list.outdent", label: "减少列表缩进", run: () => pmRun(liftListItem(schema.nodes.list_item) as never), opts: {
      tab: "home", group: "段落", groupOrder: 30,
    } },
  ];
}
