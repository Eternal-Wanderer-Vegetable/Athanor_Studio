// This file is part of Athanor, the Azodoc document engine.
// Copyright (C) 2026 The Athanor Studio Developers
// SPDX-License-Identifier: AGPL-3.0-only

//! E2 表格命令契约测试（Node 环境直接驱动 EditorState + Command）。
//! 锁定：官方命令 + syncTable 后处理产出的元数据规范形——
//! 新鲜 ID、TableMap 重算的 column、columns[] 同步、header_row/role 派生。

import { describe, expect, it } from "vitest";
import { EditorState, TextSelection, type Command, type Transaction } from "prosemirror-state";
import type { Node as PMNode } from "prosemirror-model";
import { schema } from "../schema";
import {
  addTableColumn,
  addTableRow,
  deleteTableColumn,
  fixTable,
  mergeTableCells,
  splitTableCell,
  tableProblems,
  toggleHeaderRow,
} from "./table-commands";

let n = 0;
const uid = (p: string) => `${p}_${String(++n).padStart(26, "0")}`;

function cell(children: PMNode[] | null = null, attrs: Record<string, unknown> = {}): PMNode {
  return schema.nodes.table_cell.create(
    { id: uid("cel"), ...attrs },
    children ?? [schema.nodes.paragraph.create({ id: uid("blk") })],
  );
}

function table(columns: Record<string, unknown>[], rows: PMNode[], extra: Record<string, unknown> = {}): PMNode {
  return schema.nodes.table.create({ id: uid("blk"), columns, ...extra }, rows);
}

function row(cells: PMNode[]): PMNode {
  return schema.nodes.table_row.create({ id: uid("row") }, cells);
}

function docWith(node: PMNode): PMNode {
  return schema.nodes.doc.create({ schema_version: "1.0" }, [node]);
}

/** (row, cell) → 该格内第一个文本位置（doc>table>row>cell>para>text）。 */
function cellTextPos(doc: PMNode, rowIdx: number, cellIdx: number): number {
  const table = doc.child(0);
  let pos = 1; // table start
  for (let r = 0; r < rowIdx; r++) pos += table.child(r).nodeSize;
  const row = table.child(rowIdx);
  pos += 1; // row start
  for (let c = 0; c < cellIdx; c++) pos += row.child(c).nodeSize;
  return pos + 2; // cell start + para start
}

function stateAt(doc: PMNode, pos: number): EditorState {
  return EditorState.create({ doc, selection: TextSelection.create(doc, pos) });
}

/** 跑命令并把捕获的事务逐个应用到 state（模拟 v.dispatch）。 */
function run(state: EditorState, cmd: Command): EditorState {
  const trs: Transaction[] = [];
  const ok = cmd(state, (tr) => {
    trs.push(tr);
  });
  expect(ok).toBe(true);
  let s = state;
  for (const tr of trs) s = s.apply(tr);
  return s;
}

const ids = (row: PMNode) => row.content.content.map((c) => c.attrs.id as string);

describe("table-commands（官方命令 + syncTable）", () => {
  it("addTableRow 追加行：新鲜 ID + column 按网格重算", () => {
    const t = table(
      [{ id: uid("col") }, { id: uid("col") }],
      [row([cell(null, { column: 0 }), cell(null, { column: 1 })])],
    );
    const s0 = stateAt(docWith(t), cellTextPos(docWith(t), 0, 0));
    const s1 = run(s0, addTableRow);
    const tab = s1.doc.child(0);
    expect(tab.childCount).toBe(2);
    const added = tab.child(1);
    expect(added.attrs.id).toMatch(/^row_/);
    const cols = added.content.content.map((c) => c.attrs.column);
    expect(cols).toEqual([0, 1]);
    const cellIds = ids(added);
    expect(new Set(cellIds).size).toBe(cellIds.length);
    expect(cellIds[0]).not.toBe(tab.child(0).child(0).attrs.id);
  });

  it("addTableColumn 在 columns 中补 col_ 项", () => {
    const cols = [{ id: uid("col") }, { id: uid("col") }];
    const t = table(cols, [
      row([cell(null, { column: 0 }), cell(null, { column: 1 })]),
    ]);
    const doc = docWith(t);
    const s1 = run(stateAt(doc, cellTextPos(doc, 0, 0)), addTableColumn);
    const tab = s1.doc.child(0);
    const newCols = tab.attrs.columns as { id: string }[];
    expect(newCols).toHaveLength(3);
    expect(newCols[1].id).toMatch(/^col_/);
    // 网格 3 列、cell.column 重算
    const r0 = tab.child(0);
    expect(r0.childCount).toBe(3);
    expect(r0.content.content.map((c) => c.attrs.column)).toEqual([0, 1, 2]);
  });

  it("deleteTableColumn 同步移除 columns 项（spec §6.5）", () => {
    const keep = { id: uid("col") };
    const t = table([{ id: uid("col") }, keep], [
      row([cell(null, { column: 0 }), cell(null, { column: 1 })]),
    ]);
    const doc = docWith(t);
    const s1 = run(stateAt(doc, cellTextPos(doc, 0, 0)), deleteTableColumn);
    const tab = s1.doc.child(0);
    const newCols = tab.attrs.columns as { id: string }[];
    expect(newCols).toHaveLength(1);
    expect(newCols[0].id).toBe(keep.id);
    const r0 = tab.child(0);
    expect(r0.childCount).toBe(1);
    expect(r0.child(0).attrs.column).toBe(0);
  });

  it("toggleHeaderRow 首行转 table_header + header_row/role 派生", () => {
    const t = table(
      [{ id: uid("col") }],
      [
        row([cell(null, { column: 0 })]),
        row([cell(null, { column: 0 })]),
      ],
    );
    const doc = docWith(t);
    const s1 = run(stateAt(doc, cellTextPos(doc, 1, 0)), toggleHeaderRow);
    const tab = s1.doc.child(0);
    expect(tab.attrs.header_row).toBe(true);
    const head = tab.child(0).child(0);
    expect(head.type.name).toBe("table_header");
    expect(head.attrs.role).toBe("header");
    expect(tab.child(1).child(0).type.name).toBe("table_cell");
    // 再切一次还原
    const s2 = run(s1, toggleHeaderRow);
    const tab2 = s2.doc.child(0);
    expect(tab2.attrs.header_row).toBeNull();
    expect(tab2.child(0).child(0).type.name).toBe("table_cell");
    expect(tab2.child(0).child(0).attrs.role ?? null).toBeNull();
  });

  it("mergeTableCells（文本选区→向右合并）与 splitTableCell 往返", () => {
    const t = table(
      [{ id: uid("col") }, { id: uid("col") }],
      [row([cell(null, { column: 0 }), cell(null, { column: 1 })])],
    );
    const doc = docWith(t);
    const s1 = run(stateAt(doc, cellTextPos(doc, 0, 0)), mergeTableCells);
    const tab = s1.doc.child(0);
    const r0 = tab.child(0);
    expect(r0.childCount).toBe(1);
    expect(r0.child(0).attrs.colspan).toBe(2);
    expect(r0.child(0).attrs.column).toBe(0);
    // 拆分还原（选区在合并格内）
    const s2 = run(s1, splitTableCell);
    const r0b = s2.doc.child(0).child(0);
    expect(r0b.childCount).toBe(2);
    expect(r0b.content.content.map((c) => c.attrs.column)).toEqual([0, 1]);
    expect(ids(r0b)[1]).toMatch(/^cel_/);
  });

  it("tableProblems 对健康表返回 null，fixTable 对健康表不产出", () => {
    const t = table([{ id: uid("col") }], [row([cell(null, { column: 0 })])]);
    const doc = docWith(t);
    const s = stateAt(doc, cellTextPos(doc, 0, 0));
    expect(tableProblems(s)).toBeNull();
    expect(fixTable(s, () => {})).toBe(false);
  });
});
