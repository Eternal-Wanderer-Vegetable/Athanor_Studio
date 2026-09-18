// This file is part of Athanor, the Azodoc document engine.
// Copyright (C) 2026 The Athanor Studio Developers
// SPDX-License-Identifier: AGPL-3.0-only

// @vitest-environment jsdom

import { describe, expect, it } from "vitest";
import { buildLayoutIndex } from "./preview";

/** 造一个 Paged.js 完成态的 DOM：若干 .pagedjs_page，页内含 data-block-id 克隆体。 */
function pagedDom(pages: string[]): Document {
  const html = `<html><body>${pages
    .map((p) => `<div class="pagedjs_page"><div class="pagedjs_area">${p}</div></div>`)
    .join("")}</body></html>`;
  return new DOMParser().parseFromString(html, "text/html");
}

describe("LayoutIndex (E4)", () => {
  it("maps block ids to their first page", () => {
    const doc = pagedDom([
      `<p data-block-id="blk_1">a</p><p data-block-id="blk_2">b</p>`,
      `<p data-block-id="blk_3">c</p>`,
    ]);
    const idx = buildLayoutIndex(doc);
    expect(idx.page_count).toBe(2);
    expect(idx.page_of_block["blk_1"]).toBe(1);
    expect(idx.page_of_block["blk_2"]).toBe(1);
    expect(idx.page_of_block["blk_3"]).toBe(2);
  });

  it("records manual page_break blocks with their page", () => {
    const doc = pagedDom([
      `<p data-block-id="blk_1">a</p><div class="page-break" data-block-id="blk_9"></div>`,
      `<p data-block-id="blk_2">b</p>`,
    ]);
    const idx = buildLayoutIndex(doc);
    expect(idx.page_breaks).toEqual([{ id: "blk_9", page: 1 }]);
    expect(idx.page_of_block["blk_9"]).toBe(1);
  });

  it("a block cloned across two pages reports its first page", () => {
    const doc = pagedDom([
      `<p data-block-id="blk_1">第一页部分</p>`,
      `<p data-block-id="blk_1">续页部分</p>`,
    ]);
    const idx = buildLayoutIndex(doc);
    expect(idx.page_of_block["blk_1"]).toBe(1);
  });

  it("empty pagination yields zero pages, no crash", () => {
    const doc = pagedDom([]);
    const idx = buildLayoutIndex(doc);
    expect(idx.page_count).toBe(0);
    expect(idx.page_of_block).toEqual({});
  });
});
