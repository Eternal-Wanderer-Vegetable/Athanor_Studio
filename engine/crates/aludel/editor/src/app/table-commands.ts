import type { Command } from "prosemirror-state";
import { Fragment, type Node as PMNode } from "prosemirror-model";

const alphabet = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

function id(prefix: "blk" | "row" | "cel"): string {
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

function tableContext(state: Parameters<Command>[0]): { table: PMNode; tableDepth: number; tablePos: number; rowIndex: number; cellIndex: number } | null {
  const { $from } = state.selection;
  for (let depth = $from.depth; depth > 0; depth--) {
    if ($from.node(depth).type.name !== "table") continue;
    const rowDepth = depth + 1;
    return {
      table: $from.node(depth),
      tableDepth: depth,
      tablePos: $from.before(depth),
      rowIndex: $from.index(depth),
      cellIndex: $from.index(rowDepth),
    };
  }
  return null;
}

function replaceTable(state: Parameters<Command>[0], dispatch: Parameters<Command>[1], ctx: ReturnType<typeof tableContext>, rows: PMNode[]): boolean {
  if (!ctx || !dispatch) return false;
  dispatch(state.tr.replaceWith(ctx.tablePos, ctx.tablePos + ctx.table.nodeSize, ctx.table.copy(Fragment.from(rows))));
  return true;
}

export const addTableRow: Command = (state, dispatch) => {
  const ctx = tableContext(state);
  if (!ctx) return false;
  const template = ctx.table.child(ctx.rowIndex);
  const row = template.type.create(
    { ...template.attrs, id: id("row") },
    Array.from({ length: template.childCount }, (_, index) => {
      const cell = template.child(index);
      return cell.type.create({ ...cell.attrs, id: id("cel"), column: index }, cell.content);
    }),
  );
  const rows = ctx.table.content.content.slice();
  rows.splice(ctx.rowIndex + 1, 0, row);
  return replaceTable(state, dispatch, ctx, rows);
};

export const deleteTableRow: Command = (state, dispatch) => {
  const ctx = tableContext(state);
  if (!ctx || ctx.table.childCount <= 1) return false;
  const rows = ctx.table.content.content.slice();
  rows.splice(ctx.rowIndex, 1);
  return replaceTable(state, dispatch, ctx, rows);
};

export const addTableColumn: Command = (state, dispatch) => {
  const ctx = tableContext(state);
  if (!ctx) return false;
  const rows = ctx.table.content.content.map((row) => {
    const cells = row.content.content.slice();
    const cellType = row.type.schema.nodes.table_cell;
    cells.splice(ctx.cellIndex + 1, 0, cellType.create({ id: id("cel"), column: ctx.cellIndex + 1 }, cellType.schema.nodes.paragraph.create({ id: id("blk") })));
    return row.copy(Fragment.from(cells));
  });
  return replaceTable(state, dispatch, ctx, rows);
};

export const deleteTableColumn: Command = (state, dispatch) => {
  const ctx = tableContext(state);
  if (!ctx || ctx.table.child(0).childCount <= 1) return false;
  const rows = ctx.table.content.content.map((row) => {
    const cells = row.content.content.slice();
    cells.splice(ctx.cellIndex, 1);
    return row.copy(Fragment.from(cells.map((cell, index) => cell.type.create({ ...cell.attrs, column: index }, cell.content))));
  });
  return replaceTable(state, dispatch, ctx, rows);
};

export const toggleHeaderRow: Command = (state, dispatch) => {
  const ctx = tableContext(state);
  if (!ctx || !dispatch) return false;
  dispatch(state.tr.setNodeMarkup(ctx.tablePos, ctx.table.type, { ...ctx.table.attrs, header_row: ctx.table.attrs.header_row === true ? null : true }));
  return true;
};
