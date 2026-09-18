// This file is part of Athanor, the Azodoc document engine.
// Copyright (C) 2026 The Athanor Studio Developers
// SPDX-License-Identifier: AGPL-3.0-only

import { describe, expect, it } from "vitest";
import { schema, setAssetResolver } from "./schema";

function imageDom(asset: string): unknown {
  const node = schema.nodes.image.create({ asset, alt: "a" });
  return (schema.nodes.image.spec.toDOM as (n: typeof node) => unknown)(node);
}

describe("schema asset resolution", () => {
  it("asset:// uses the registered resolver", () => {
    setAssetResolver((ref) => `/api/asset/${ref.slice("asset://".length).split("/")[0]}`);
    const dom = imageDom("asset://as_X/p.png") as [string, Record<string, unknown>];
    expect(dom[0]).toBe("img");
    expect(dom[1]["src"]).toBe("/api/asset/as_X");
    setAssetResolver(null);
  });

  it("asset:// without resolver renders placeholder", () => {
    setAssetResolver(null);
    const dom = imageDom("asset://as_X/p.png") as [string, Record<string, unknown>];
    expect(dom[0]).toBe("span");
    expect((dom[1]["class"] as string)).toContain("az-asset-placeholder");
  });

  it("data:image stays directly displayable (legacy readable form)", () => {
    setAssetResolver(null);
    const dom = imageDom("data:image/png;base64,AA==") as [string, Record<string, unknown>];
    expect(dom[0]).toBe("img");
    expect(dom[1]["src"]).toBe("data:image/png;base64,AA==");
  });
});

describe("page_break node (E4)", () => {
  it("toDOM renders a visible divider marker", () => {
    const node = schema.nodes.page_break.create({ id: "blk_X" });
    const dom = (schema.nodes.page_break.spec.toDOM as (n: typeof node) => readonly unknown[])(node);
    expect(dom[0]).toBe("div");
    expect((dom[1] as Record<string, unknown>)["class"]).toBe("az-page-break");
  });

  it("parseDOM accepts the editor marker and print marker", () => {
    const specs = schema.nodes.page_break.spec.parseDOM as { tag: string }[];
    const tags = specs.map((s) => s.tag);
    expect(tags).toContain("div.az-page-break");
    expect(tags).toContain("div.page-break");
  });
});
