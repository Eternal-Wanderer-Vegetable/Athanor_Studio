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

//! 表格命令：prosemirror-tables 官方命令的包装层。
//!
//! 官方命令保证网格结构合法（行/列增删、跨列跨行、合并拆分），
//! 但不认识我们的 attrs 契约（id / column / columns[] / header_row /
//! role）。每条官方命令执行后追加 syncTable 后处理，把这三类元数据
//! 重算回规范形：
//!   1. id：缺/重的 row/cell/col ID 补发（官方 createAndFill 不给 id）；
//!   2. cell.column：按 TableMap 网格重算（跨列取起始列下标）；
//!   3. table.columns：增删列后按索引同步插入/移除 entries
//!      （spec §6.5：删列 MUST 移除对应 columns 项）；
//!   4. header_row：由首行 cell 类型派生（全 header → true）；
//!   5. cell.role：PM 节点类型权威——table_header → "header"，其余清
//!      除（官方 toggleHeader 只换节点类型，不清旧 role 值）。
//!
//! 不规则表（TableMap.problems）只诊断不修复：syncTable 直接跳过，
//! 修复仅经显式 fixTable 命令（可撤销的单事务，spec §6.5）。

import type { Command } from "prosemirror-state";
import { Selection, type EditorState, type Transaction } from "prosemirror-state";
import type { Node as PMNode } from "prosemirror-model";
import {
  addColumnAfter,
  addColumnBefore,
  addRowAfter,
  addRowBefore,
  cellAround,
  CellSelection,
  deleteColumn,
  deleteRow,
  deleteTable,
  fixTables,
  findTable,
  mergeCells,
  selectedRect,
  splitCell,
  TableMap,
} from "prosemirror-tables";

const alphabet = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/** 与 main.ts/document-controller.ts 同款的 Crockford ID（32 字母表，无 I L O U）。 */
function id(prefix: "blk" | "row" | "cel" | "col"): string {
  let time = BigInt(Date.now());
  let value = "";
  for (let i = 0; i < 10; i++) {
    value = alphabet[Number(time & 31n)] + value;
    time >>= 5n;
  }
  const bytes = new Uint8Array(16);
  globalThis.crypto?.getRandomValues(bytes);
  for (const byte of bytes) value += alphabet[byte & 31];
  return `${prefix}_${value}`;
}

interface TableSlot {
  table: PMNode;
  tablePos: number;
  tableStart: number; // = tablePos + 1，TableMap 坐标基准
}

/** 在事务文档中定位选区所在表（位置按 tr.mapping 随命令变换自动跟随）。 */
function tableAt(tr: Transaction): TableSlot | null {
  const found = findTable(tr.selection.$from);
  if (!found || found.node.type.name !== "table") return null;
  return { table: found.node, tablePos: found.pos, tableStart: found.pos + 1 };
}

interface ColSync {
  /** addColumn* 之后：新列索引（在 columns 中补 col_ 项）。 */
  insert?: number;
  /** deleteColumn 之后：被删列区间 [from, to)（在 columns 中移除对应项）。 */
  removeFrom?: number;
  removeTo?: number;
}

/** 结构命令后的元数据重算（对事务内表格就地修补）。 */
function syncTable(tr: Transaction, colSync: ColSync = {}): Transaction {
  const slot = tableAt(tr);
  if (!slot) return tr;
  const map = TableMap.get(slot.table);
  // 不规则表：只诊断，不动结构（spec §6.5 冻结规则）。
  if (map.problems) return tr;

  const seen = new Set<string>();
  let rowOff = 0;
  for (let r = 0; r < slot.table.childCount; r++) {
    const row = slot.table.child(r);
    const rowPos = slot.tableStart + rowOff;
    const wantRowId =
      typeof row.attrs.id === "string" && row.attrs.id && !seen.has(row.attrs.id)
        ? row.attrs.id
        : id("row");
    if (wantRowId !== row.attrs.id) {
      tr.setNodeMarkup(rowPos, undefined, { ...row.attrs, id: wantRowId });
    }
    seen.add(wantRowId);

    let cellOff = 0;
    for (let i = 0; i < row.childCount; i++) {
      const cell = row.child(i);
      const cellPos = rowPos + 1 + cellOff;
      // TableMap 坐标 = cell 相对 tableStart 的偏移
      const gridCol = map.findCell(rowOff + 1 + cellOff).left;
      const isHeader = cell.type.name === "table_header";
      const wantId =
        typeof cell.attrs.id === "string" && cell.attrs.id && !seen.has(cell.attrs.id)
          ? cell.attrs.id
          : id("cel");
      const wantAttrs = { ...cell.attrs, id: wantId, column: gridCol, role: isHeader ? "header" : null };
      if (
        cell.attrs.id !== wantId ||
        cell.attrs.column !== gridCol ||
        (cell.attrs.role ?? null) !== (isHeader ? "header" : null)
      ) {
        tr.setNodeMarkup(cellPos, undefined, wantAttrs);
      }
      seen.add(wantId);
      cellOff += cell.nodeSize;
    }
    rowOff += row.nodeSize;
  }

  // table.columns：按结构命令报告的列索引同步 entries
  let columns = Array.isArray(slot.table.attrs.columns)
    ? (slot.table.attrs.columns as Record<string, unknown>[]).map((c) => ({ ...c }))
    : [];
  if (colSync.removeFrom !== undefined && colSync.removeTo !== undefined) {
    columns.splice(colSync.removeFrom, colSync.removeTo - colSync.removeFrom);
  }
  if (colSync.insert !== undefined) {
    columns.splice(Math.min(colSync.insert, columns.length), 0, { id: id("col") });
  }
  // header_row：首行全 header_cell → true，否则清除（OptBool：null 不落字段）
  const firstRow = slot.table.childCount > 0 ? slot.table.child(0) : null;
  const headerRow =
    !!firstRow &&
    firstRow.childCount > 0 &&
    firstRow.content.content.every((c) => c.type.name === "table_header");
  const wantHeaderRow = headerRow ? true : null;
  if (
    slot.table.attrs.header_row !== wantHeaderRow ||
    JSON.stringify(slot.table.attrs.columns ?? null) !== JSON.stringify(columns.length ? columns : null)
  ) {
    tr.setNodeMarkup(slot.tablePos, undefined, {
      ...slot.table.attrs,
      header_row: wantHeaderRow,
      columns: columns.length ? columns : null,
    });
  }
  return tr;
}

/** 包装官方命令：先跑原命令，再在当前事务上重算元数据。 */
function wrap(base: Command, colSync?: (state: EditorState, tr: Transaction) => ColSync): Command {
  return (state, dispatch) => {
    if (!dispatch) return base(state, undefined);
    return base(state, (tr) => {
      const sync = colSync ? colSync(state, tr) : {};
      dispatch(syncTable(tr, sync));
    });
  };
}

export const addTableRowBefore: Command = wrap(addRowBefore);
export const addTableRow: Command = wrap(addRowAfter);
export const deleteTableRow: Command = wrap(deleteRow);
export const deleteWholeTable: Command = wrap(deleteTable);

export const addTableColumnBefore: Command = wrap(addColumnBefore, (state) => ({
  insert: selectedRect(state).left,
}));
export const addTableColumn: Command = wrap(addColumnAfter, (state) => ({
  insert: selectedRect(state).right,
}));
export const deleteTableColumn: Command = wrap(deleteColumn, (state) => {
  const rect = selectedRect(state);
  return { removeFrom: rect.left, removeTo: rect.right };
});

/** 合并选区单元格（CellSelection）；普通文本选区时退化为"与右侧合并"。
 *  退化路径：先在内存中把选区换成覆盖当前格+右邻格的 CellSelection
 *  （state.apply，不落历史），再用官方 mergeCells 生成合并步骤追加到
 *  同一事务——单次提交 = 单次撤销步。 */
export const mergeTableCells: Command = (state, dispatch) => {
  if (state.selection instanceof CellSelection) return wrap(mergeCells)(state, dispatch);
  const $from = state.selection.$from;
  const table = findTable($from);
  if (!table) return false;
  const $cell = cellAround($from);
  if (!$cell) return false;
  const map = TableMap.get(table.node);
  if (map.problems) return false;
  const cellPos = $cell.pos; // cell 节点起点（doc 绝对坐标）
  const rect = map.findCell(cellPos - table.start);
  if (rect.right >= map.width) return false;
  if (!dispatch) return true;
  const rightPos = table.start + map.positionAt(rect.top, rect.right, table.node);
  const tr = state.tr.setSelection(CellSelection.create(state.doc, cellPos, rightPos));
  const s2 = state.apply(tr);
  const captured: Transaction[] = [];
  mergeCells(s2, (t) => {
    captured.push(t);
  });
  const mergeTr = captured[0];
  if (!mergeTr) {
    dispatch(tr); // 合并不成立（如被 rowspan 阻塞）也至少落 CellSelection
    return true;
  }
  for (const step of mergeTr.steps) tr.step(step);
  // mergeTr.selection 绑定 mergeTr.doc 实例，不能直接转交——按位置在 tr.doc 重建
  tr.setSelection(Selection.near(tr.doc.resolve(mergeTr.selection.head)));
  dispatch(syncTable(tr));
  return true;
};

export const splitTableCell: Command = wrap(splitCell);

/** 切换首行表头（spec §6.5：header_row 只描述首行）。
 *  节点类型即角色：首行全 header → 还原 table_cell；否则整行升 table_header。 */
export const toggleHeaderRow: Command = (state, dispatch) => {
  const found = findTable(state.selection.$from);
  if (!found || found.node.type.name !== "table" || found.node.childCount === 0) return false;
  if (!dispatch) return true;
  const row0 = found.node.child(0);
  const allHeader = row0.content.content.every((c) => c.type.name === "table_header");
  const toType = allHeader ? state.schema.nodes.table_cell : state.schema.nodes.table_header;
  if (!toType) return false;
  const tr = state.tr;
  let off = 0;
  for (let i = 0; i < row0.childCount; i++) {
    const cell = row0.child(i);
    if (cell.type !== toType) {
      tr.setNodeMarkup(found.start + 1 + off, toType, cell.attrs);
    }
    off += cell.nodeSize;
  }
  dispatch(syncTable(tr));
  return true;
};

/** 显式修复命令：对当前表跑 fixTables（单事务、可撤销），随后重算元数据。
 *  注意：fixTables 扫全文并修所有坏表；调用方仅在选区位于表内时暴露此命令。 */
export const fixTable: Command = (state, dispatch) => {
  if (!findTable(state.selection.$from)) return false;
  const tr = fixTables(state);
  if (!tr) return false;
  if (!dispatch) return true;
  dispatch(syncTable(tr));
  return true;
};

/** 状态栏诊断：当前表是否不规则（TableMap.problems 非空）。 */
export function tableProblems(state: EditorState): readonly unknown[] | null {
  const found = findTable(state.selection.$from);
  if (!found || found.node.type.name !== "table") return null;
  return TableMap.get(found.node).problems;
}
