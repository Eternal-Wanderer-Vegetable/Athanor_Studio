// This file is part of Athanor, the Azodoc document engine.
// Copyright (C) 2026 The Athanor Studio Developers
// SPDX-License-Identifier: AGPL-3.0-only

// @vitest-environment jsdom

//! E3 净化测试：恶意 HTML、危险 URL、三种粘贴模式。

import { describe, expect, it } from "vitest";
import { isSafeUrl, sanitizeHtml, sanitizeNotice } from "./clipboard";

describe("isSafeUrl", () => {
  it("http/https/mailto/相对/锚点放行", () => {
    for (const u of ["https://a.b/x", "http://a.b", "mailto:a@b.c", "#frag", "/rel", "./up", "//cdn.x/y"]) {
      expect(isSafeUrl(u, "link")).toBe(true);
    }
  });
  it("javascript:/data:/vbscript: 拒绝", () => {
    for (const u of ["javascript:alert(1)", "JavaScript:alert(1)", "data:text/html,<x>", "vbscript:x", "file:///etc/p"]) {
      expect(isSafeUrl(u, "link")).toBe(false);
    }
  });
  it("img data: 只放位图", () => {
    expect(isSafeUrl("data:image/png;base64,AA==", "image")).toBe(true);
    expect(isSafeUrl("data:image/svg+xml,<svg>", "image")).toBe(false);
    expect(isSafeUrl("asset://as_X/p.png", "image")).toBe(false); // 非白名单 scheme
    expect(isSafeUrl("https://cdn.x/p.png", "image")).toBe(true);
  });
});

describe("sanitizeHtml", () => {
  it("keep 模式：白名单结构+内联保留，script/iframe/on* 剥除", () => {
    const r = sanitizeHtml(
      `<p onclick="x()" style="color:red">hi <strong>bold</strong></p>
       <script>alert(1)</script>
       <iframe src="https://evil"></iframe>
       <a href="https://ok" title="t" onmouseover="y()">link</a>`,
      "keep",
    );
    expect(r.html).not.toContain("script");
    expect(r.html).not.toContain("iframe");
    expect(r.html).not.toContain("onclick");
    expect(r.html).not.toContain("onmouseover");
    expect(r.html).not.toContain("style=");
    expect(r.html).toContain("<strong>bold</strong>");
    expect(r.html).toContain('href="https://ok"');
    expect(r.stats.droppedElements).toBe(2);
    expect(r.lossy).toBe(true);
  });

  it("危险 URL 属性移除但保留元素文本", () => {
    const r = sanitizeHtml(`<a href="javascript:alert(1)">x</a><img src="data:image/svg+xml,<svg>">`, "keep");
    expect(r.html).not.toContain("javascript:");
    expect(r.html).toContain(">x</a>");
    expect(r.html).not.toContain("svg+xml");
    expect(r.stats.unsafeUrls).toBe(2);
  });

  it("match 模式：剥内联保结构", () => {
    const r = sanitizeHtml(`<p>a <strong>b</strong> <em>c</em></p><ul><li>d</li></ul>`, "match");
    expect(r.html).toContain("a b c");
    expect(r.html).not.toContain("<strong>");
    expect(r.html).not.toContain("<em>");
    expect(r.html).toContain("<ul>");
    expect(r.html).toContain("<li>");
  });

  it("plain 模式：所有标签 unwrap", () => {
    const r = sanitizeHtml(`<p>a <strong>b</strong></p><p>c</p>`, "plain");
    expect(r.html).not.toContain("<p>");
    expect(r.html).not.toContain("<strong>");
    expect(r.text).toContain("a b");
    expect(r.text).toContain("c");
  });

  it("未知标签 unwrap 保文字（Word mso 类）", () => {
    const r = sanitizeHtml(`<o:p>word</o:p><w:custom>x</w:custom>`, "keep");
    expect(r.text).toContain("word");
    expect(r.text).toContain("x");
    expect(r.html).not.toContain("o:p");
  });

  it("sanitizeNotice 产生一次性可读提示", () => {
    const r = sanitizeHtml(`<script>x</script><a href="javascript:y">z</a>`, "keep");
    const note = sanitizeNotice(r);
    expect(note).toContain("净化");
    expect(note).toContain("不可信");
  });
});
