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

//! 查找/替换：字面搜索（不跨原子/unknown 节点），上一个/下一个/计数/
//! 替换/全部替换。替换走单个 transaction，全部替换也是单个 transaction
//! （一次撤销可还原）。

import { EditorState, TextSelection } from "prosemirror-state";
import type { EditorView } from "prosemirror-view";

export interface FindMatch {
  from: number;
  to: number;
}

/** 收集所有文本命中（对 doc.textBetween 做按块级拼接的字面搜索）。
 *  位置基于 PM 文档坐标；跳过 isAtom 叶节点的内部文本。 */
export function findAll(state: EditorState, query: string, caseSensitive: boolean): FindMatch[] {
  if (!query) return [];
  const out: FindMatch[] = [];
  const needle = caseSensitive ? query : query.toLowerCase();
  state.doc.descendants((node, pos) => {
    if (node.isText && node.text) {
      const hay = caseSensitive ? node.text : node.text.toLowerCase();
      let idx = 0;
      while (true) {
        const hit = hay.indexOf(needle, idx);
        if (hit < 0) break;
        out.push({ from: pos + hit, to: pos + hit + query.length });
        idx = hit + Math.max(1, query.length);
      }
      return false;
    }
    return !node.isAtom;
  });
  return out.sort((a, b) => a.from - b.from);
}

/** 从当前选区出发找下一个/上一个匹配并选中。 */
export function selectMatch(view: EditorView, matches: FindMatch[], backwards: boolean): boolean {
  if (matches.length === 0) return false;
  const { from } = view.state.selection;
  let target: FindMatch | undefined;
  if (backwards) {
    for (let i = matches.length - 1; i >= 0; i--) {
      if (matches[i].to < from || matches[i].to === from && matches[i].from < from) {
        target = matches[i];
        break;
      }
    }
    target ??= matches[matches.length - 1];
  } else {
    target = matches.find((m) => m.from > from) ?? matches[0];
  }
  if (!target) return false;
  const tr = view.state.tr.setSelection(TextSelection.create(view.state.doc, target.from, target.to));
  view.dispatch(tr.scrollIntoView());
  view.focus();
  return true;
}

/** 替换当前选区（若与 matches 中某项一致），返回是否替换成功。 */
export function replaceCurrent(view: EditorView, matches: FindMatch[], replacement: string): boolean {
  const { from, to, empty } = view.state.selection;
  if (empty) return false;
  if (!matches.some((m) => m.from === from && m.to === to)) return false;
  view.dispatch(view.state.tr.insertText(replacement, from, to));
  return true;
}

/** 全部替换：一个 transaction（一次撤销恢复）。 */
export function replaceAll(view: EditorView, matches: FindMatch[], replacement: string): number {
  if (matches.length === 0) return 0;
  const tr = view.state.tr;
  for (const m of [...matches].reverse()) {
    tr.insertText(replacement, m.from, m.to);
  }
  view.dispatch(tr);
  return matches.length;
}
