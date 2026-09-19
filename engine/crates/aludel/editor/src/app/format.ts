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

//! 格式扩展（x-athanor-format）：受控、有界的格式键集合。
//!
//! 存储：块级段落格式存节点 attrs.extra["x-athanor-format"].paragraph；
//! 字符格式存 span_extra mark attrs.data["x-athanor-format"].character。
//! 两条通道都经 ExtraMap 往返（to_prima 平铺 / from_prima 收集），
//! 未识别键原样保留；清除格式只删受控键。

import type { Attrs, Node as PMNode } from "prosemirror-model";
import type { EditorState, Transaction } from "prosemirror-state";
import type { EditorView } from "prosemirror-view";

export const FORMAT_KEY = "x-athanor-format";

export interface ParagraphFormat {
  align?: "left" | "center" | "right" | "justify";
  indentStartPt?: number;
  indentEndPt?: number;
  firstLinePt?: number;
  lineHeight?: number;
  spaceBeforePt?: number;
  spaceAfterPt?: number;
  keepWithNext?: boolean;
  breakBefore?: boolean;
}

export interface CharacterFormat {
  fontFamily?: string;
  fontSizePt?: number;
  color?: string;
  highlight?: string;
  verticalAlign?: "sub" | "super";
}

// ---------------------------------------------------------------- 读取

type ExtraMap = Record<string, unknown>;

function extraOf(attrs: Attrs | null | undefined): ExtraMap {
  const e = attrs?.extra;
  return e && typeof e === "object" ? (e as ExtraMap) : {};
}

function formatOf(extra: ExtraMap): ExtraMap {
  const f = extra[FORMAT_KEY];
  return f && typeof f === "object" ? (f as ExtraMap) : {};
}

export function paragraphFormat(node: PMNode): ParagraphFormat {
  return { ...(formatOf(extraOf(node.attrs))["paragraph"] as ParagraphFormat | undefined) };
}

// ---------------------------------------------------------------- 写入（块）

/** 对选区内所有块级节点合并段落格式 patch（null/undefined 值删除该键）。 */
export function setParagraphFormat(state: EditorState, dispatch: (tr: Transaction) => void, patch: ParagraphFormat): boolean {
  const { from, to } = state.selection;
  const tr = state.tr;
  let touched = false;
  state.doc.nodesBetween(from, to, (node, pos) => {
    if (!node.isBlock || node.isTextblock === false || !node.attrs || !("extra" in node.attrs)) return true;
    const extra = { ...extraOf(node.attrs) };
    const fmt = { ...formatOf(extra) };
    const para = { ...(fmt["paragraph"] as ParagraphFormat | undefined) };
    for (const [k, v] of Object.entries(patch)) {
      if (v === undefined || v === null) delete (para as Record<string, unknown>)[k];
      else (para as Record<string, unknown>)[k] = v;
    }
    if (Object.keys(para).length === 0) delete fmt["paragraph"];
    else fmt["paragraph"] = para;
    if (Object.keys(fmt).length === 0) delete extra[FORMAT_KEY];
    else extra[FORMAT_KEY] = fmt;
    tr.setNodeAttribute(pos, "extra", Object.keys(extra).length ? extra : null);
    touched = true;
    return true;
  });
  if (touched) dispatch(tr);
  return touched;
}

/** 清除选区块的全部受控段落格式（不动其他 extra 键）。 */
export function clearParagraphFormat(state: EditorState, dispatch: (tr: Transaction) => void): boolean {
  return setParagraphFormat(state, dispatch, {
    align: undefined,
    indentStartPt: undefined,
    indentEndPt: undefined,
    firstLinePt: undefined,
    lineHeight: undefined,
    spaceBeforePt: undefined,
    spaceAfterPt: undefined,
    keepWithNext: undefined,
    breakBefore: undefined,
  });
}

// ---------------------------------------------------------------- 写入（行内）

function spanExtraMark(state: EditorState): { data: ExtraMap } | null {
  const { empty, $from, from, to } = state.selection;
  const mark = state.schema.marks.span_extra;
  if (!mark) return null;
  if (empty) {
    const m = (state.storedMarks ?? $from.marks()).find((x) => x.type === mark);
    return m ? { data: (m.attrs.data as ExtraMap) ?? {} } : null;
  }
  let found: { data: ExtraMap } | null = null;
  state.doc.nodesBetween(from, to, (node) => {
    if (found) return false;
    const m = node.marks.find((x) => x.type === mark);
    if (m) found = { data: (m.attrs.data as ExtraMap) ?? {} };
    return true;
  });
  return found;
}

/** 对选区施加字符格式 patch（合并进现有 span_extra.data，保留其他键）。 */
export function setCharacterFormat(state: EditorState, dispatch: (tr: Transaction) => void, patch: CharacterFormat): boolean {
  const mark = state.schema.marks.span_extra;
  if (!mark) return false;
  const { empty, from, to } = state.selection;
  const tr = state.tr;
  if (empty) {
    // 无选区：写入 storedMarks，作用于后续输入
    const existing = spanExtraMark(state)?.data ?? {};
    const data = patchSpanExtra(existing, patch);
    const stripped = tr.storedMarks?.filter((m) => m.type !== mark) ?? [];
    tr.setStoredMarks(Object.keys(data).length ? [...stripped, mark.create({ data })] : stripped);
  } else {
    // 混合格式选区必须逐文本节点合并，不能把第一个 span 的未知扩展复制到整段。
    tr.removeMark(from, to, mark);
    state.doc.nodesBetween(from, to, (node, pos) => {
      if (!node.isText) return true;
      const start = Math.max(from, pos);
      const end = Math.min(to, pos + node.nodeSize);
      if (start >= end) return true;
      const existing = node.marks.find((m) => m.type === mark)?.attrs.data;
      const data = patchSpanExtra(existing && typeof existing === "object" ? (existing as ExtraMap) : {}, patch);
      if (Object.keys(data).length) tr.addMark(start, end, mark.create({ data }));
      return true;
    });
  }
  dispatch(tr);
  return true;
}

function patchSpanExtra(existing: ExtraMap, patch: CharacterFormat): ExtraMap {
  const fmt = { ...((existing[FORMAT_KEY] as ExtraMap) ?? {}) };
  const ch = { ...((fmt["character"] as CharacterFormat) ?? {}) };
  for (const [k, v] of Object.entries(patch)) {
    if (v === undefined || v === null) delete (ch as Record<string, unknown>)[k];
    else (ch as Record<string, unknown>)[k] = v;
  }
  if (Object.keys(ch).length === 0) delete fmt["character"];
  else fmt["character"] = ch;
  const data = { ...existing };
  if (Object.keys(fmt).length === 0) delete data[FORMAT_KEY];
  else data[FORMAT_KEY] = fmt;
  return data;
}

export function clearCharacterFormat(state: EditorState, dispatch: (tr: Transaction) => void): boolean {
  return setCharacterFormat(state, dispatch, {
    fontFamily: undefined,
    fontSizePt: undefined,
    color: undefined,
    highlight: undefined,
    verticalAlign: undefined,
  });
}

// ---------------------------------------------------------------- CSS 映射

function pt(v: unknown): string | null {
  const n = Number(v);
  return Number.isFinite(n) && n >= 0 && n <= 999 ? `${n}pt` : null;
}

function color(v: unknown): string | null {
  if (typeof v !== "string") return null;
  const m = /^#([0-9a-fA-F]{6})$/.exec(v);
  return m ? v.toLowerCase() : null;
}

function family(v: unknown): string | null {
  if (typeof v !== "string" || v.length > 200) return null;
  // 拒绝引号/分号等 CSS 注入字符
  return v.trim() && !/[\u0000-\u001f\u007f\\'";{}<>]/.test(v) ? v : null;
}

export function paragraphCss(f: ParagraphFormat): string {
  const parts: string[] = [];
  if (f.align && ["left", "center", "right", "justify"].includes(f.align)) {
    parts.push(`text-align:${f.align}`);
  }
  const start = pt(f.indentStartPt);
  if (start) parts.push(`padding-left:${start}`);
  const end = pt(f.indentEndPt);
  if (end) parts.push(`padding-right:${end}`);
  const first = pt(f.firstLinePt);
  if (first) parts.push(`text-indent:${first}`);
  if (typeof f.lineHeight === "number" && f.lineHeight > 0.5 && f.lineHeight <= 5) {
    parts.push(`line-height:${f.lineHeight}`);
  }
  const before = pt(f.spaceBeforePt);
  if (before) parts.push(`margin-top:${before}`);
  const after = pt(f.spaceAfterPt);
  if (after) parts.push(`margin-bottom:${after}`);
  if (f.keepWithNext) parts.push("break-after:avoid");
  if (f.breakBefore) parts.push("break-before:page");
  return parts.join(";");
}

export function characterCss(f: CharacterFormat): string {
  const parts: string[] = [];
  const fam = family(f.fontFamily);
  if (fam) parts.push(`font-family:${fam}`);
  const size = pt(f.fontSizePt);
  if (size) parts.push(`font-size:${size}`);
  const col = color(f.color);
  if (col) parts.push(`color:${col}`);
  const hl = color(f.highlight);
  if (hl) parts.push(`background-color:${hl}`);
  if (f.verticalAlign === "sub" || f.verticalAlign === "super") {
    parts.push(`vertical-align:${f.verticalAlign}`);
  }
  return parts.join(";");
}

/** 当前选区首个 span_extra 字符格式（工具栏回显用）。 */
export function activeCharacterFormat(view: EditorView): CharacterFormat {
  const { $from } = view.state.selection;
  const mark = view.state.schema.marks.span_extra;
  if (!mark) return {};
  const m = (view.state.storedMarks ?? $from.marks()).find((x) => x.type === mark);
  const data = (m?.attrs.data as ExtraMap) ?? {};
  const fmt = (data[FORMAT_KEY] as ExtraMap) ?? {};
  return { ...((fmt["character"] as CharacterFormat) ?? {}) };
}

/** 选区所在块的段落格式（工具栏回显）。 */
export function activeParagraphFormat(view: EditorView): ParagraphFormat {
  const { $from } = view.state.selection;
  for (let d = $from.depth; d >= 0; d--) {
    const n = $from.node(d);
    if (n.isBlock) return paragraphFormat(n);
  }
  return {};
}

// ---------------------------------------------------------------- 选区归并反射
//
// 浮动工具栏/检查器需要整段选区的“归并视图”：逐文本节点收集值，
// 任何键出现两种不同值记为 mixed（显示 Mixed/空值），不读最后一个 run。
// 反射是纯读操作：不写入正文，不产生事务。

export const CHARACTER_FORMAT_KEYS = [
  "fontFamily",
  "fontSizePt",
  "color",
  "highlight",
  "verticalAlign",
] as const;

export type CharacterFormatKey = (typeof CHARACTER_FORMAT_KEYS)[number];

export interface SelectionCharacterFormat {
  /** 每个键的归并值；不一致时省略该键并在 mixed 标记。 */
  values: CharacterFormat;
  /** 选区内取值不一致的键。 */
  mixed: CharacterFormatKey[];
}

/** 收集单个节点的 span_extra 字符格式。 */
function charFormatOfNode(node: PMNode): CharacterFormat {
  const m = node.marks.find((x) => x.type.name === "span_extra");
  const data = (m?.attrs.data as ExtraMap) ?? {};
  const fmt = (data[FORMAT_KEY] as ExtraMap) ?? {};
  return { ...((fmt["character"] as CharacterFormat) ?? {}) };
}

/** 归并选区字符格式（空选区退化为光标处 storedMarks/activeCharacterFormat）。 */
export function selectionCharacterFormat(view: EditorView): SelectionCharacterFormat {
  const { from, to, empty } = view.state.selection;
  if (empty) {
    return { values: activeCharacterFormat(view), mixed: [] };
  }
  const seen = new Map<CharacterFormatKey, { value: unknown; mixed: boolean }>();
  const consider = (fmt: CharacterFormat) => {
    for (const key of CHARACTER_FORMAT_KEYS) {
      const v = fmt[key];
      const rec = seen.get(key);
      if (!rec) {
        seen.set(key, { value: v, mixed: false });
      } else if (!rec.mixed && rec.value !== v) {
        rec.mixed = true;
      }
    }
  };
  view.state.doc.nodesBetween(from, to, (node) => {
    if (node.isText) consider(charFormatOfNode(node));
    return true;
  });
  const values: CharacterFormat = {};
  const mixed: CharacterFormatKey[] = [];
  for (const key of CHARACTER_FORMAT_KEYS) {
    const rec = seen.get(key);
    if (!rec || rec.value === undefined) continue;
    if (rec.mixed) mixed.push(key);
    else (values as Record<string, unknown>)[key] = rec.value;
  }
  return { values, mixed };
}

export interface SelectionParagraphFormat {
  values: ParagraphFormat;
  mixed: (keyof ParagraphFormat)[];
}

/** 归并选区覆盖的块级段落格式。 */
export function selectionParagraphFormat(view: EditorView): SelectionParagraphFormat {
  const { from, to } = view.state.selection;
  const seen = new Map<keyof ParagraphFormat, { value: unknown; mixed: boolean }>();
  view.state.doc.nodesBetween(from, to, (node) => {
    if (!node.isBlock || node.isTextblock === false) return true;
    const fmt = paragraphFormat(node);
    for (const [k, v] of Object.entries(fmt) as [keyof ParagraphFormat, unknown][]) {
      const rec = seen.get(k);
      if (!rec) seen.set(k, { value: v, mixed: false });
      else if (!rec.mixed && rec.value !== v) rec.mixed = true;
    }
    return true;
  });
  const values: ParagraphFormat = {};
  const mixed: (keyof ParagraphFormat)[] = [];
  for (const [k, rec] of seen) {
    if (rec.value === undefined) continue;
    if (rec.mixed) mixed.push(k);
    else (values as Record<string, unknown>)[k] = rec.value;
  }
  return { values, mixed };
}

export type SelectionKind = "empty" | "text" | "table" | "image" | "node";

export interface SelectionContext {
  kind: SelectionKind;
  /** 选区是否位于表格内（上下文 Table tab 与表格命令的启用依据）。 */
  inTable: boolean;
  character: SelectionCharacterFormat;
  paragraph: SelectionParagraphFormat;
  /** 当前块类型键（样式选择器回显：paragraph/heading1-3/quote/code_block/其他）。 */
  blockType: string;
  /** image 节点的 alt（图片上下文用；无则 undefined）。 */
  imageAlt?: string;
}

/** 判断选区/光标是否处于表格结构内（prosemirror-tables 的 table/table_row/table_cell）。 */
function selectionInTable(view: EditorView): boolean {
  const { $from } = view.state.selection;
  for (let d = $from.depth; d >= 0; d--) {
    const name = $from.node(d).type.name;
    if (name === "table" || name === "table_row" || name === "table_cell" || name === "table_header") {
      return true;
    }
  }
  // CellSelection（跨单元格选区）的 $from 已在 table_cell 内；NodeSelection
  // 选中整表时 $from 在表外一层，检查选中节点本身。
  const sel = view.state.selection as { node?: PMNode };
  if (sel.node && (sel.node.type.name === "table" || sel.node.type.name.startsWith("table_"))) {
    return true;
  }
  return false;
}

/** 当前块类型键（样式选择器回显）。 */
function blockTypeOf(view: EditorView): string {
  const { $from } = view.state.selection;
  for (let d = $from.depth; d >= 0; d--) {
    const n = $from.node(d);
    if (!n.isBlock) continue;
    const name = n.type.name;
    if (name === "heading") return `heading${Number(n.attrs.level) || 1}`;
    if (name === "list_item") continue; // 列表项往上找父块
    return name;
  }
  return "paragraph";
}

/** 派生只读选区上下文（ribbon/inspector/floating toolbar 的输入）。 */
export function selectionContext(view: EditorView): SelectionContext {
  const sel = view.state.selection;
  const inTable = selectionInTable(view);
  const node = (sel as { node?: PMNode }).node;
  let kind: SelectionKind = "empty";
  if (node) {
    kind = node.type.name === "table" ? "table" : node.type.name === "image" || node.type.name === "figure" || node.type.name === "inline_image" ? "image" : "node";
  } else if (!sel.empty) {
    kind = inTable ? "table" : "text";
  }
  return {
    kind,
    inTable,
    character: selectionCharacterFormat(view),
    paragraph: selectionParagraphFormat(view),
    blockType: blockTypeOf(view),
    imageAlt: kind === "image" ? (node?.attrs.alt as string | undefined) : undefined,
  };
}
