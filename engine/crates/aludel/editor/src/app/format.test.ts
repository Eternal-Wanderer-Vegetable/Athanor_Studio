import { describe, expect, it } from "vitest";
import { EditorState, TextSelection } from "prosemirror-state";
import { schema } from "../schema";
import {
  activeCharacterFormat,
  characterCss,
  paragraphCss,
  setCharacterFormat,
} from "./format";

function stateWithText(...parts: { text: string; data?: Record<string, unknown> }[]): EditorState {
  const mark = schema.marks.span_extra;
  const paragraph = schema.nodes.paragraph.create(
    { id: "blk_test" },
    parts.map(({ text, data }) => schema.text(text, data ? [mark.create({ data })] : [])),
  );
  return EditorState.create({ doc: schema.nodes.doc.create(null, [paragraph]) });
}

describe("x-athanor-format", () => {
  it("keeps each segment's unknown extra when formatting a mixed selection", () => {
    const initial = stateWithText(
      { text: "one", data: { vendor: { keep: true }, "x-athanor-format": { character: { color: "#ff0000" } } } },
      { text: "two", data: { vendor: { keep: "second" } } },
    );
    const state = initial.apply(initial.tr.setSelection(TextSelection.create(initial.doc, 1, 7)));
    let next = state;
    setCharacterFormat(state, (tr) => { next = state.apply(tr); }, { fontSizePt: 12 });

    const marks = next.doc.firstChild!.content.content
      .filter((node) => node.isText)
      .map((node) => node.marks.find((m) => m.type === schema.marks.span_extra)?.attrs.data);
    expect(marks[0]).toMatchObject({ vendor: { keep: true }, "x-athanor-format": { character: { color: "#ff0000", fontSizePt: 12 } } });
    expect(marks[1]).toMatchObject({ vendor: { keep: "second" }, "x-athanor-format": { character: { fontSizePt: 12 } } });
  });

  it("writes a stored mark for text typed after a cursor", () => {
    const state = stateWithText({ text: "x" });
    const cursor = state.apply(state.tr.setSelection(TextSelection.create(state.doc, 2)));
    let next = cursor;
    setCharacterFormat(cursor, (tr) => { next = cursor.apply(tr); }, { fontFamily: "Aptos" });
    expect(activeCharacterFormat({ state: next } as never).fontFamily).toBe("Aptos");
    expect(next.storedMarks?.[0].attrs.data).toMatchObject({ "x-athanor-format": { character: { fontFamily: "Aptos" } } });
  });

  it("maps supported paragraph and character values to safe CSS", () => {
    expect(paragraphCss({ align: "center", indentStartPt: 0, keepWithNext: true })).toContain("break-after:avoid");
    expect(characterCss({ fontFamily: "Aptos", fontSizePt: 12, color: "#AABBCC" })).toBe("font-family:Aptos;font-size:12pt;color:#aabbcc");
    expect(characterCss({ fontFamily: "bad; color:red" })).toBe("");
  });
});
