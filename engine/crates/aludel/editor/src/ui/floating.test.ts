// This file is part of Athanor, the Azodoc document engine.
// SPDX-License-Identifier: AGPL-3.0-only

// @vitest-environment jsdom

import { describe, expect, it } from "vitest";
import { CommandRegistry, type CommandContext } from "../app/command-registry";
import type { SelectionContext } from "../app/format";
import { OverlayController } from "./overlay";
import { FloatingBar } from "./floating";

const CTX: CommandContext = {
  hasDocument: true,
  desktop: true,
  dirty: false,
  busy: false,
  hasSelection: true,
  inTable: false,
};

function selCtx(kind: SelectionContext["kind"] = "text"): SelectionContext {
  return {
    kind,
    inTable: false,
    character: { values: {}, mixed: [] },
    paragraph: { values: {}, mixed: [] },
  };
}

function makeBar(selKind: SelectionContext["kind"] = "text") {
  const registry = new CommandRegistry();
  registry.register({
    id: "format.font", label: "字体",
    enabled: () => true, run: () => {}, surfaces: ["floating"],
  });
  registry.register({
    id: "format.strong", label: "加粗",
    enabled: () => true, run: () => {}, surfaces: ["floating"],
  });
  const bar = new FloatingBar({
    registry,
    overlays: new OverlayController(),
    ctx: () => CTX,
    sel: () => (selKind === "empty" ? { ...selCtx("empty") } : selCtx(selKind)),
    runCommand: () => {},
    selectionRect: () => ({ left: 100, top: 100, width: 0 }),
    makePicker: (cmdId) => {
      if (cmdId !== "format.font") return null;
      const s = document.createElement("select");
      s.id = "fb-font";
      return s;
    },
  });
  return { bar, registry };
}

describe("FloatingBar selection preservation", () => {
  it("shows for text selection and hides for empty/none", () => {
    const { bar } = makeBar("text");
    bar.refresh();
    expect(bar.isVisible).toBe(true);
    bar.hide();
    const empty = makeBar("empty");
    empty.bar.refresh();
    expect(empty.bar.isVisible).toBe(false);
  });

  it("mousedown on command button is prevented (keeps editor selection)", () => {
    const { bar } = makeBar();
    bar.refresh();
    const btn = document.querySelector<HTMLElement>('#floating-bar [data-cmd="format.strong"]')!;
    const ev = new MouseEvent("mousedown", { bubbles: true, cancelable: true });
    btn.dispatchEvent(ev);
    expect(ev.defaultPrevented).toBe(true);
    bar.hide();
  });

  it("mousedown on native select/input is not prevented (picker can focus)", () => {
    const { bar } = makeBar();
    bar.refresh();
    const sel = document.getElementById("fb-font")!;
    const ev = new MouseEvent("mousedown", { bubbles: true, cancelable: true });
    sel.dispatchEvent(ev);
    expect(ev.defaultPrevented).toBe(false);
    bar.hide();
  });

  it("Escape inside a picker blurs the control without clearing the bar", () => {
    const { bar } = makeBar();
    bar.refresh();
    const sel = document.getElementById("fb-font")!;
    sel.focus();
    expect(document.activeElement).toBe(sel);
    const ev = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true });
    sel.dispatchEvent(ev);
    expect(document.activeElement).not.toBe(sel);
    expect(bar.isVisible).toBe(true);
    bar.hide();
  });
});
