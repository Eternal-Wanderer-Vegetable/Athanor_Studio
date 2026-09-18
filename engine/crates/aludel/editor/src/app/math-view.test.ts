// This file is part of Athanor, the Azodoc document engine.
// Copyright (C) 2026 The Athanor Studio Developers
// SPDX-License-Identifier: AGPL-3.0-only

//! E3 KaTeX 渲染测试（Node 环境可跑 renderToString；失败回退源码态）。

import { describe, expect, it } from "vitest";
import { renderMathHtml } from "./math-view";

describe("renderMathHtml", () => {
  it("合法 LaTeX → KaTeX HTML", () => {
    const html = renderMathHtml("E=mc^2", false);
    expect(html).not.toBeNull();
    expect(html).toContain("katex");
  });
  it("displayMode 块级渲染", () => {
    const html = renderMathHtml("\\int_0^1 x^2 dx", true);
    expect(html).toContain("katex-display");
  });
  it("非法 LaTeX → null（调用方回退源码）", () => {
    expect(renderMathHtml("\\undefined{oops", true)).toBeNull();
  });
  it("空串 → null", () => {
    expect(renderMathHtml("   ", false)).toBeNull();
  });
});
