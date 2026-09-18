// This file is part of Athanor, the Azodoc document engine.
// Copyright (C) 2026 The Athanor Studio Developers
// SPDX-License-Identifier: AGPL-3.0-only

import { afterEach, describe, expect, it, vi } from "vitest";
import { HttpGateway } from "./http";

describe("HttpGateway assets", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("assetUrl maps asset:// to /api/asset/<id>", () => {
    const g = new HttpGateway();
    expect(g.assetUrl("s", "asset://as_ABCDEFGHJKMNPQRSTVWXYZ01234/pic.png")).toBe(
      "/api/asset/as_ABCDEFGHJKMNPQRSTVWXYZ01234",
    );
    // 非 asset:// 引用原样返回
    expect(g.assetUrl("s", "https://x.test/p.png")).toBe("https://x.test/p.png");
    expect(g.assetUrl("s", "data:image/png;base64,AA==")).toBe("data:image/png;base64,AA==");
  });

  it("stageAsset posts base64 payload and returns asset:// ref", async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      status: 200,
      json: async () => ({ id: "as_0123456789ABCDEFGHJKMNPQRS", url: "asset://as_0123456789ABCDEFGHJKMNPQRS/pic.png" }),
    });
    vi.stubGlobal("fetch", fetchMock);
    const g = new HttpGateway();
    const ref = await g.stageAsset("s", {
      filename: "pic.png",
      mime: "image/png",
      dataBase64: "AAE=",
    });
    expect(ref.url.startsWith("asset://as_")).toBe(true);
    const [path, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(path).toBe("/api/asset/stage");
    expect(JSON.parse(String(init.body))).toEqual({
      filename: "pic.png",
      mime: "image/png",
      data: "AAE=",
    });
  });

  it("stageAsset surfaces server errors", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        status: 400,
        json: async () => ({ error: "不支持的资产类型" }),
      }),
    );
    const g = new HttpGateway();
    await expect(
      g.stageAsset("s", { filename: "x.svg", mime: "image/svg+xml", dataBase64: "AA==" }),
    ).rejects.toThrow("不支持的资产类型");
  });
});
