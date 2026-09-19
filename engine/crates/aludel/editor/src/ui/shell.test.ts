// This file is part of Athanor, the Azodoc document engine.
// SPDX-License-Identifier: AGPL-3.0-only

// @vitest-environment jsdom

import { beforeEach, describe, expect, it } from "vitest";
import type { SelectionContext } from "../app/format";
import { Shell } from "./shell";

function hostDom(): void {
  document.body.innerHTML = `
    <div id="left-panel"></div>
    <div id="right-panel"></div>`;
}

function selCtx(values: Record<string, unknown>, mixed: string[] = []): SelectionContext {
  return {
    kind: "text",
    inTable: false,
    character: { values: values as SelectionContext["character"]["values"], mixed: mixed as never },
    paragraph: { values: {}, mixed: [] },
  };
}

function makeShell(current: SelectionContext | null): { shell: Shell; applied: Record<string, unknown>[] } {
  const applied: Record<string, unknown>[] = [];
  const shell = new Shell({
    applyChar: (p) => applied.push(p),
    refreshUI: () => {},
    sel: () => current,
  });
  return { shell, applied };
}

describe("Shell picker controls", () => {
  beforeEach(() => {
    hostDom();
    window.localStorage.clear();
  });

  it("font select change applies one character-format transaction", () => {
    const { shell, applied } = makeShell(null);
    const sel = shell.createPicker("format.font", "tb") as HTMLSelectElement;
    document.body.appendChild(sel);
    sel.value = "宋体";
    sel.dispatchEvent(new Event("change"));
    expect(applied).toEqual([{ fontFamily: "宋体" }]);
  });

  it("size select with empty option clears the key", () => {
    const { shell, applied } = makeShell(null);
    const sel = shell.createPicker("format.size", "tb") as HTMLSelectElement;
    document.body.appendChild(sel);
    sel.value = "";
    sel.dispatchEvent(new Event("change"));
    expect(applied).toEqual([{ fontSizePt: undefined }]);
  });

  it("mixed color is marked, never written as the previous value", () => {
    const { shell } = makeShell(selCtx({}, ["color"]));
    const input = shell.createPicker("format.color", "tb") as HTMLInputElement;
    document.body.appendChild(input);
    input.value = "#123456"; // 上一次选区留下的颜色
    shell.reflectPickers();
    expect(input.value).toBe("#123456"); // color input 不能写空串：保留控件值但标混合
    expect(input.classList.contains("mixed-value")).toBe(true);
    expect(input.dataset.mixed).toBe("true");
    expect(input.title).toContain("混合");
  });

  it("uniform color reflects value and clears the mixed marker", () => {
    const { shell } = makeShell(selCtx({ color: "#ff0000" }));
    const input = shell.createPicker("format.color", "tb") as HTMLInputElement;
    document.body.appendChild(input);
    input.classList.add("mixed-value");
    input.dataset.mixed = "true";
    shell.reflectPickers();
    expect(input.value).toBe("#ff0000");
    expect(input.classList.contains("mixed-value")).toBe(false);
    expect(input.dataset.mixed).toBe("");
    expect(input.title).not.toContain("混合");
  });

  it("mixed font/size reflect as empty option with mixed marker", () => {
    const { shell } = makeShell(selCtx({}, ["fontFamily", "fontSizePt"]));
    const font = shell.createPicker("format.font", "tb") as HTMLSelectElement;
    const size = shell.createPicker("format.size", "fb") as HTMLSelectElement;
    document.body.append(font, size);
    shell.reflectPickers();
    expect(font.value).toBe("");
    expect(font.classList.contains("mixed-value")).toBe(true);
    expect(size.value).toBe("");
    expect(size.classList.contains("mixed-value")).toBe(true);
  });

  it("focused picker is not overwritten by refresh", () => {
    const { shell } = makeShell(selCtx({ fontFamily: "宋体" }));
    const font = shell.createPicker("format.font", "tb") as HTMLSelectElement;
    document.body.appendChild(font);
    font.value = "黑体";
    font.focus();
    shell.reflectPickers();
    expect(font.value).toBe("黑体");
  });

  it("float-bar pickers reflect the same selection as ribbon pickers", () => {
    const { shell } = makeShell(selCtx({ color: "#00ff00", highlight: "#ffff00" }));
    const tbColor = shell.createPicker("format.color", "tb") as HTMLInputElement;
    const fbColor = shell.createPicker("format.color", "fb") as HTMLInputElement;
    const fbHl = shell.createPicker("format.highlight", "fb") as HTMLInputElement;
    document.body.append(tbColor, fbColor, fbHl);
    shell.reflectPickers();
    expect(tbColor.value).toBe("#00ff00");
    expect(fbColor.value).toBe("#00ff00");
    expect(fbHl.value).toBe("#ffff00");
  });
});
