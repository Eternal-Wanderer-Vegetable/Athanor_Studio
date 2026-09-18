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

//! 脚注导航（E3）：footnote_ref ↔ footnote 定义互跳、删除后悬空提示。
//!
//! 语义（mapping.rs）：footnote_ref.id 是对 footnote 块 id 的**引用**
//! （id_kind: None，不参与补发）。互跳按文档全序建索引；
//! 删除定义后引用悬空由保存侧拒绝——这里给出**编辑时**的一次性
//! 提示（撤销可恢复，不阻止操作）。

import type { Command, EditorState } from "prosemirror-state";
import { Plugin, PluginKey, TextSelection } from "prosemirror-state";
import type { Node as PMNode } from "prosemirror-model";
import type { EditorView } from "prosemirror-view";

interface FootnoteIndex {
  /** 引用 id → 第一个 footnote_ref 节点位置。 */
  firstRef: Map<string, number>;
  /** footnote 块 id → 节点位置。 */
  defs: Map<string, number>;
  /** 引用存在但定义缺失的 id 数。 */
  dangling: number;
}

/** 全文一遍建索引（位置为 doc 绝对坐标）。 */
export function indexFootnotes(doc: PMNode): FootnoteIndex {
  const firstRef = new Map<string, number>();
  const defs = new Map<string, number>();
  let dangling = 0;
  doc.descendants((node, pos) => {
    if (node.type.name === "footnote_ref") {
      const id = String(node.attrs.id ?? "");
      if (id && !firstRef.has(id)) firstRef.set(id, pos);
      return false;
    }
    if (node.type.name === "footnote") {
      const id = String(node.attrs.id ?? "");
      if (id && !defs.has(id)) defs.set(id, pos);
    }
    return true;
  });
  for (const id of firstRef.keys()) if (!defs.has(id)) dangling++;
  return { firstRef, defs, dangling };
}

/** 选区是否贴着一个 footnote_ref（NodeSelection 或紧邻 text 边界）。 */
function refAtSelection(state: EditorState): { id: string; pos: number } | null {
  const { $from, empty, node } = state.selection as { $from: { pos: number; nodeBefore: PMNode | null; nodeAfter: PMNode | null }; empty: boolean; node?: PMNode | null };
  if (node && node.type.name === "footnote_ref") {
    return { id: String(node.attrs.id ?? ""), pos: state.selection.from };
  }
  const near = [$from.nodeBefore, $from.nodeAfter].find((n) => n?.type.name === "footnote_ref");
  if (!empty && !near) return null;
  if (near) {
    const pos = near === $from.nodeBefore ? $from.pos - near.nodeSize : $from.pos;
    return { id: String(near.attrs.id ?? ""), pos };
  }
  return null;
}

/** 选区是否在 footnote 定义块内。 */
function defDepth(state: EditorState): number {
  const { $from } = state.selection;
  for (let d = $from.depth; d > 0; d--) {
    if ($from.node(d).type.name === "footnote") return d;
  }
  return -1;
}

/** 跳转到 doc 位置附近（Ref→Def 跳块起点，Def→Ref 跳引用起点）。 */
function jumpTo(view: EditorView, pos: number): void {
  const $pos = view.state.doc.resolve(Math.max(0, Math.min(pos, view.state.doc.content.size)));
  const sel = TextSelection.near($pos, 1);
  view.dispatch(view.state.tr.setSelection(sel).scrollIntoView());
}

/** 双向跳转：ref 旁 → 定义块；定义块内 → 第一个引用点。 */
export const gotoFootnote: Command = (state, dispatch) => {
  const idx = indexFootnotes(state.doc);
  const ref = refAtSelection(state);
  if (ref?.id) {
    const defPos = idx.defs.get(ref.id);
    if (defPos === undefined) return false;
    if (!dispatch) return true;
    dispatch(
      state.tr.setSelection(TextSelection.near(state.doc.resolve(defPos + 1), 1)).scrollIntoView(),
    );
    return true;
  }
  const d = defDepth(state);
  if (d >= 0) {
    const id = String(state.selection.$from.node(d).attrs.id ?? "");
    const refPos = idx.firstRef.get(id);
    if (refPos === undefined) return false;
    if (!dispatch) return true;
    dispatch(
      state.tr.setSelection(TextSelection.near(state.doc.resolve(refPos + 1), 1)).scrollIntoView(),
    );
    return true;
  }
  return false;
};

/** 点击处理（handleDOMEvents.click）：ref → 定义；footnote 回跳符 → 引用。 */
export function footnoteClick(view: EditorView, event: Event): boolean {
  const t = event.target as HTMLElement | null;
  if (!t) return false;
  // 定义块里的回跳符号 → 第一个引用
  const back = t.closest(".az-footnote-back");
  if (back) {
    const aside = back.closest("aside.az-footnote");
    if (!aside) return false;
    // posAtDOM 返回节点前位置；nodeAt 拿到的就是 aside 对应的 footnote
    const node = view.state.doc.nodeAt(view.posAtDOM(aside, 0));
    if (node?.type.name === "footnote") {
      const refPos = indexFootnotes(view.state.doc).firstRef.get(String(node.attrs.id ?? ""));
      if (refPos !== undefined) {
        jumpTo(view, refPos + 1);
        return true;
      }
    }
    return false;
  }
  // 引用上 → 定义
  const refEl = t.closest(".az-footnote-ref");
  if (refEl) {
    const refPos = view.posAtDOM(refEl, 0);
    const node = view.state.doc.nodeAt(refPos);
    if (node?.type.name === "footnote_ref") {
      const idx = indexFootnotes(view.state.doc);
      const defPos = idx.defs.get(String(node.attrs.id ?? ""));
      if (defPos !== undefined) {
        jumpTo(view, defPos + 1);
        return true;
      }
    }
  }
  return false;
}

const danglingKey = new PluginKey<number>("footnoteDangling");

/** 悬空引用监视：定义被删且仍有引用 → 一次性提示（计数变化才提醒）。
 *  不阻止、不修复——撤销可恢复；保存侧的 dangling check 仍是硬门槛。 */
export function footnoteDanglingPlugin(onWarn: (danglingIds: string[]) => void): Plugin {
  return new Plugin({
    key: danglingKey,
    state: {
      init: (_config, state) => indexFootnotes(state.doc).dangling,
      apply: (_tr, prev, _old, state) => indexFootnotes(state.doc).dangling,
    },
    view: () => ({
      update(view, prevState) {
        const prev = danglingKey.getState(prevState) ?? 0;
        const cur = danglingKey.getState(view.state) ?? 0;
        if (cur > prev) {
          const idx = indexFootnotes(view.state.doc);
          const missing = [...idx.firstRef.keys()].filter((id) => !idx.defs.has(id));
          onWarn(missing);
        }
      },
    }),
  });
}
