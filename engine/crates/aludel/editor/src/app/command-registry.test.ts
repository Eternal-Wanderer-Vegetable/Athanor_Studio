// This file is part of Athanor, the Azodoc document engine.
// Copyright (C) 2026 The Athanor Studio Developers
// SPDX-License-Identifier: AGPL-3.0-only

import { describe, expect, it } from "vitest";
import {
  buildCommandContext,
  CommandRegistry,
  MENU_GROUPS,
  RIBBON_TABS,
  type Command,
  type CommandContext,
} from "./command-registry";
import type { GatewayCapabilities } from "../platform/gateway";

const CAPS: GatewayCapabilities = {
  desktopFileDialogs: true,
  backgroundJobs: true,
  recoveryDrafts: true,
  previewPaged: true,
  assetRegistry: true,
  comments: false,
  trackedChanges: false,
  fields: false,
  toc: false,
  referenceCitations: false,
};

const CTX: CommandContext = {
  hasDocument: true,
  desktop: true,
  capabilities: CAPS,
  mode: "editing",
  viewMode: "continuous",
  dirty: false,
  busy: false,
  hasSelection: false,
  inTable: false,
  pageCount: null,
};

function cmd(partial: Partial<Command> & { id: string }): Command {
  return { label: partial.id, enabled: () => true, run: () => {}, ...partial };
}

describe("CommandRegistry projections", () => {
  it("byTab groups commands by declared group order", () => {
    const r = new CommandRegistry();
    r.register(cmd({ id: "a", tab: "home", group: "字体", groupOrder: 20, order: 1 }));
    r.register(cmd({ id: "b", tab: "home", group: "历史", groupOrder: 10 }));
    r.register(cmd({ id: "c", tab: "home", group: "字体", groupOrder: 20, order: 0 }));
    r.register(cmd({ id: "d", tab: "insert", group: "表格" }));
    const groups = r.groupsFor("home", CTX);
    expect(groups.map((g) => g.name)).toEqual(["历史", "字体"]);
    expect(groups[1].commands.map((c) => c.id)).toEqual(["c", "a"]);
  });

  it("invisible commands are excluded from every surface projection", () => {
    const r = new CommandRegistry();
    r.register(cmd({ id: "desk.only", tab: "home", group: "g", visible: (c) => c.desktop }));
    r.register(cmd({ id: "any", tab: "home", group: "g", menu: "view" }));
    const http = { ...CTX, desktop: false };
    expect(r.byTab("home", http).map((c) => c.id)).toEqual(["any"]);
    expect(r.byTab("home", CTX).map((c) => c.id)).toEqual(["desk.only", "any"]);
    expect(r.byMenu("view", http).map((c) => c.id)).toEqual(["any"]);
  });

  it("paletteCandidates includes visible commands regardless of tab/menu", () => {
    const r = new CommandRegistry();
    r.register(cmd({ id: "only.palette", surfaces: ["palette"] }));
    r.register(cmd({ id: "ribbon.only", tab: "home", group: "g", surfaces: ["ribbon"] }));
    const ids = r.paletteCandidates(CTX).map((c) => c.id);
    expect(ids).toContain("only.palette");
    expect(ids).not.toContain("ribbon.only");
  });

  it("floating surface is opt-in only", () => {
    const r = new CommandRegistry();
    r.register(cmd({ id: "plain", tab: "home", group: "g" }));
    r.register(cmd({ id: "flt", surfaces: ["floating"] }));
    expect(r.floating(CTX).map((c) => c.id)).toEqual(["flt"]);
  });

  it("disabledReason explains instead of silently greying", () => {
    const r = new CommandRegistry();
    r.register(
      cmd({
        id: "x",
        enabled: (c) => c.hasDocument,
        disabledReason: (c) => (c.hasDocument ? null : "无已打开文档"),
      }),
    );
    const cmdNoDoc = r.get("x")!;
    expect(cmdNoDoc.enabled({ ...CTX, hasDocument: false })).toBe(false);
    expect(cmdNoDoc.disabledReason?.({ ...CTX, hasDocument: false })).toBe("无已打开文档");
    expect(cmdNoDoc.disabledReason?.(CTX)).toBeNull();
  });

  it("run refuses disabled commands and emits change", async () => {
    const r = new CommandRegistry();
    let ran = 0;
    let emitted = 0;
    r.register(cmd({ id: "y", enabled: (c) => c.hasSelection, run: () => { ran += 1; } }));
    r.onChange(() => { emitted += 1; });
    await r.run("y", CTX);
    expect(ran).toBe(0);
    await r.run("y", { ...CTX, hasSelection: true });
    expect(ran).toBe(1);
    expect(emitted).toBe(1);
  });
});

describe("capability/mode projections", () => {
  const HTTP_CAPS: GatewayCapabilities = { ...CAPS, desktopFileDialogs: false, backgroundJobs: false };
  const http = { ...CTX, desktop: false, capabilities: HTTP_CAPS };

  it("requiredCapabilities hides commands the platform cannot run", () => {
    const r = new CommandRegistry();
    r.register(cmd({ id: "file.open", tab: "home", group: "g", menu: "file", requiredCapabilities: ["desktopFileDialogs"] }));
    r.register(cmd({ id: "edit.undo", tab: "home", group: "g", menu: "edit" }));
    expect(r.byTab("home", http).map((c) => c.id)).toEqual(["edit.undo"]);
    expect(r.byMenu("file", http)).toEqual([]);
    expect(r.paletteCandidates(http).map((c) => c.id)).toEqual(["edit.undo"]);
    expect(r.byMenu("file", CTX).map((c) => c.id)).toEqual(["file.open"]);
  });

  it("run refuses hidden commands (shortcut cannot bypass capability)", async () => {
    const r = new CommandRegistry();
    let ran = 0;
    r.register(cmd({ id: "file.open", requiredCapabilities: ["desktopFileDialogs"], run: () => { ran += 1; } }));
    await r.run("file.open", http);
    expect(ran).toBe(0);
    await r.run("file.open", CTX);
    expect(ran).toBe(1);
  });

  it("modes restrict a command to the declared editor mode", () => {
    const r = new CommandRegistry();
    r.register(cmd({ id: "preview.only", modes: ["preview"], menu: "view" }));
    expect(r.byMenu("view", { ...CTX, mode: "editing" })).toEqual([]);
    expect(r.byMenu("view", { ...CTX, mode: "preview" }).map((c) => c.id)).toEqual(["preview.only"]);
  });

  it("review menu alias folds into tools", () => {
    const r = new CommandRegistry();
    r.register(cmd({ id: "file.verify", menu: "review" }));
    expect(r.byMenu("tools", CTX).map((c) => c.id)).toEqual(["file.verify"]);
    expect(r.byMenu("review", CTX).map((c) => c.id)).toEqual(["file.verify"]);
  });

  it("seven Word-style menu groups and six tabs plus contextual table", () => {
    expect(MENU_GROUPS.map((g) => g.id)).toEqual(["file", "edit", "view", "insert", "format", "tools", "help"]);
    expect(RIBBON_TABS.filter((t) => !t.contextual).map((t) => t.id)).toEqual(
      ["home", "insert", "layout", "references", "view", "review"],
    );
    expect(RIBBON_TABS.find((t) => t.id === "table")?.contextual).toBe(true);
  });

  it("buildCommandContext centralizes mode/capability/selection/page state", () => {
    const gw = { desktop: true, capabilities: CAPS };
    const c = buildCommandContext({
      gateway: gw, hasDocument: true, dirty: true, busy: false,
      sel: { kind: "text", inTable: false, character: { values: {}, mixed: [] }, paragraph: { values: {}, mixed: [] } },
      viewMode: "page", previewOpen: false, pageCount: 3,
    });
    expect(c).toMatchObject({
      mode: "editing", viewMode: "page", hasSelection: true,
      inTable: false, pageCount: 3, dirty: true, desktop: true,
    });
    expect(buildCommandContext({
      gateway: gw, hasDocument: true, dirty: false, busy: false,
      sel: null, viewMode: "continuous", previewOpen: true, pageCount: 1,
    }).mode).toBe("preview");
    expect(buildCommandContext({
      gateway: gw, hasDocument: false, dirty: false, busy: false,
      sel: null, viewMode: "continuous", previewOpen: false, pageCount: null,
    }).mode).toBe("noDocument");
  });
});
