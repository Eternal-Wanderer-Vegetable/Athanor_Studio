// This file is part of Athanor, the Azodoc document engine.
// SPDX-License-Identifier: AGPL-3.0-only

//! 表格域命令（上下文页签）：行列增删、单元格合并/拆分、表头、删除、修复。
//! 全部 needsTable gated——离开表格后入口不启用。

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
} from "../table-commands";
import type { CommandDeps } from "./deps";
import type { RegDef } from "./types";

export function tableCommands(deps: CommandDeps): RegDef[] {
  const { ctl } = deps;
  const pmRun = (fn: (state: never, dispatch: never, view?: never) => boolean): void => {
    const v = ctl.view;
    if (v) fn(v.state as never, v.dispatch as never, v as never);
  };
  return [
    { id: "table.rowAddBefore", label: "在上方插入行", run: () => pmRun(addTableRowBefore as never), opts: {
      tab: "table", group: "行", groupOrder: 10, needsTable: true,
    } },
    { id: "table.rowAdd", label: "在下方插入行", run: () => pmRun(addTableRow as never), opts: {
      tab: "table", group: "行", groupOrder: 10, needsTable: true, elId: "tb-row-add",
    } },
    { id: "table.rowDelete", label: "删除行", run: () => pmRun(deleteTableRow as never), opts: {
      tab: "table", group: "行", groupOrder: 10, needsTable: true, elId: "tb-row-del",
    } },
    { id: "table.colAddBefore", label: "在左侧插入列", run: () => pmRun(addTableColumnBefore as never), opts: {
      tab: "table", group: "列", groupOrder: 20, needsTable: true,
    } },
    { id: "table.colAdd", label: "在右侧插入列", run: () => pmRun(addTableColumn as never), opts: {
      tab: "table", group: "列", groupOrder: 20, needsTable: true, elId: "tb-col-add",
    } },
    { id: "table.colDelete", label: "删除列", run: () => pmRun(deleteTableColumn as never), opts: {
      tab: "table", group: "列", groupOrder: 20, needsTable: true, elId: "tb-col-del",
    } },
    { id: "table.merge", label: "合并单元格", run: () => pmRun(mergeTableCells as never), opts: {
      tab: "table", group: "单元格", groupOrder: 30, needsTable: true, elId: "tb-cell-merge",
    } },
    { id: "table.split", label: "拆分单元格", run: () => pmRun(splitTableCell as never), opts: {
      tab: "table", group: "单元格", groupOrder: 30, needsTable: true, elId: "tb-cell-split",
    } },
    { id: "table.header", label: "切换表头行", run: () => pmRun(toggleHeaderRow as never), opts: {
      tab: "table", group: "属性", groupOrder: 40, needsTable: true,
    } },
    { id: "table.delete", label: "删除表格", run: () => pmRun(deleteWholeTable as never), opts: {
      tab: "table", group: "属性", groupOrder: 40, needsTable: true,
    } },
    { id: "table.fix", label: "修复表格结构", run: () => pmRun(fixTable as never), opts: {
      tab: "table", group: "属性", groupOrder: 40, needsTable: true,
    } },
  ];
}
