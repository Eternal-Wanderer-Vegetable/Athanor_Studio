// This file is part of Athanor, the Azodoc document engine.
// Copyright (C) 2026 The Athanor Studio Developers
// SPDX-License-Identifier: AGPL-3.0-only

// @vitest-environment jsdom

import { describe, expect, it } from "vitest";
import { EditorState, TextSelection } from "prosemirror-state";
import { EditorView } from "prosemirror-view";
import { schema } from "../schema";
import { selectionCharacterFormat, selectionContext } from "./format";

function viewWith(...parts: { text: string; data?: Record<string, unknown> }[]): EditorView {
  const mark = schema.marks.span_extra;
  const paragraph = schema.nodes.paragraph.create(
    { id: "blk_test" },
    parts.map(({ text, data }) => schema.text(text, data ? [mark.create({ data })] : [])),
  );
  const doc = schema.nodes.doc.create(null, [paragraph]);
  const host = document.createElement("div");
  document.body.appendChild(host);
  return new EditorView(host, { state: EditorState.create({ doc }) });
}

const FMT = "x-athanor-format";

describe("selectionCharacterFormat (mixed merge)", () => {
  it("uniform selection reports value, no mixed", () => {
    const view = viewWith(
      { text: "ab", data: { [FMT]: { character: { color: "#ff0000" } } } },
      { text: "cd", data: { [FMT]: { character: { color: "#ff0000" } } } },
    );
    view.dispatch(view.state.tr.setSelection(TextSelection.create(view.state.doc, 1, 5)));
    const r = selectionCharacterFormat(view);
    expect(r.values.color).toBe("#ff0000");
    expect(r.mixed).toEqual([]);
    view.destroy();
  });

  it("mixed selection marks differing keys, keeps uniform keys", () => {
    const view = viewWith(
      { text: "ab", data: { [FMT]: { character: { color: "#ff0000", fontFamily: "宋体" } } } },
      { text: "cd", data: { [FMT]: { character: { color: "#00ff00", fontFamily: "宋体" } } } },
    );
    view.dispatch(view.state.tr.setSelection(TextSelection.create(view.state.doc, 1, 5)));
    const r = selectionCharacterFormat(view);
    expect(r.mixed).toContain("color");
    expect(r.values.color).toBeUndefined(); // 混合键不读最后一个 run
    expect(r.values.fontFamily).toBe("宋体");
    view.destroy();
  });

  it("mark-less segment counts as undefined value (mixed vs formatted)", () => {
    const view = viewWith(
      { text: "ab", data: { [FMT]: { character: { color: "#ff0000" } } } },
      { text: "cd" },
    );
    view.dispatch(view.state.tr.setSelection(TextSelection.create(view.state.doc, 1, 5)));
    const r = selectionCharacterFormat(view);
    expect(r.mixed).toContain("color");
    view.destroy();
  });
});

describe("selectionContext", () => {
  it("text selection kind + not in table", () => {
    const view = viewWith({ text: "hello" });
    view.dispatch(view.state.tr.setSelection(TextSelection.create(view.state.doc, 1, 4)));
    const sc = selectionContext(view);
    expect(sc.kind).toBe("text");
    expect(sc.inTable).toBe(false);
    expect(sc.blockType).toBe("paragraph");
    view.destroy();
  });

  it("blockType reflects enclosing heading level", () => {
    const heading = schema.nodes.heading.create({ id: "blk_h", level: 2 }, schema.text("标题"));
    const doc = schema.nodes.doc.create(null, [heading]);
    const host = document.createElement("div");
    document.body.appendChild(host);
    const view = new EditorView(host, { state: EditorState.create({ doc }) });
    view.dispatch(view.state.tr.setSelection(TextSelection.create(view.state.doc, 2)));
    expect(selectionContext(view).blockType).toBe("heading2");
    view.destroy();
  });
});
