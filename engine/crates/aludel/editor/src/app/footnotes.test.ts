// This file is part of Athanor, the Azodoc document engine.
// Copyright (C) 2026 The Athanor Studio Developers
// SPDX-License-Identifier: AGPL-3.0-only

//! E3 脚注导航测试：indexFootnotes 索引、gotoFootnote 双向跳转、悬空计数。

import { describe, expect, it } from "vitest";
import { EditorState, NodeSelection, TextSelection, type Command, type Transaction } from "prosemirror-state";
import type { Node as PMNode } from "prosemirror-model";
import { schema } from "../schema";
import { gotoFootnote, indexFootnotes } from "./footnotes";

let n = 0;
const uid = (p: string) => `${p}_${String(++n).padStart(26, "0")}`;

function para(text: string): PMNode {
  return schema.nodes.paragraph.create({ id: uid("blk") }, text ? schema.text(text) : undefined);
}

function paraWithRef(text: string, refId: string): PMNode {
  return schema.nodes.paragraph.create({ id: uid("blk") }, [
    schema.text(text),
    schema.nodes.footnote_ref.create({ id: refId }),
  ]);
}

function footnote(id: string, text: string): PMNode {
  return schema.nodes.footnote.create({ id }, para(text));
}

function docWith(...blocks: PMNode[]): PMNode {
  return schema.nodes.doc.create({ schema_version: "1.0" }, blocks);
}

function run(state: EditorState, cmd: Command): EditorState {
  const trs: Transaction[] = [];
  const ok = cmd(state, (tr) => trs.push(tr));
  expect(ok).toBe(true);
  let s = state;
  for (const tr of trs) s = s.apply(tr);
  return s;
}

describe("indexFootnotes", () => {
  it("引用/定义索引与悬空计数", () => {
    const doc = docWith(
      paraWithRef("a", "blk_A"),
      paraWithRef("b", "blk_B"),
      footnote("blk_A", "注一"),
    );
    const idx = indexFootnotes(doc);
    expect(idx.defs.has("blk_A")).toBe(true);
    expect(idx.firstRef.has("blk_A")).toBe(true);
    expect(idx.firstRef.has("blk_B")).toBe(true);
    expect(idx.dangling).toBe(1); // blk_B 引用无定义
  });
});

describe("gotoFootnote", () => {
  it("ref NodeSelection → 跳到定义块内", () => {
    const doc = docWith(paraWithRef("a", "blk_A"), footnote("blk_A", "注一"));
    const paraNode = doc.child(0);
    const refPos = paraNode.nodeSize - 2; // ref 在段尾（para start+1 起算）
    const s0 = EditorState.create({
      doc,
      selection: NodeSelection.create(doc, 1 + paraNode.content.size - 1),
    });
    const s1 = run(s0, gotoFootnote);
    // 跳进定义块：$from 在 footnote 内
    const $from = s1.selection.$from;
    let inFootnote = false;
    for (let d = $from.depth; d > 0; d--) {
      if ($from.node(d).type.name === "footnote") inFootnote = true;
    }
    expect(inFootnote).toBe(true);
    void refPos;
  });

  it("定义块内 → 跳回第一个引用", () => {
    const doc = docWith(paraWithRef("a", "blk_A"), footnote("blk_A", "注一"));
    const note = doc.child(1);
    // 定义块内段落文本处建 TextSelection：
    // doc(1) + para(0)(nodeSize) + note_start(1) + para_start(1) + 1 = 文本中
    const inside = 1 + doc.child(0).nodeSize + 1 + 1 + 1;
    const s0 = EditorState.create({ doc, selection: TextSelection.create(doc, inside) });
    void note;
    const s1 = run(s0, gotoFootnote);
    // 应跳到第 0 段内 ref 之后附近（深度 1，仍在顶层段落）
    expect(s1.selection.$from.depth).toBe(1);
    expect(s1.selection.$from.node(1)).toBe(doc.child(0));
  });
});
