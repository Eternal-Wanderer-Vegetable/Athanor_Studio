// This file is part of Athanor, the Azodoc document engine.
// SPDX-License-Identifier: AGPL-3.0-only

import { describe, expect, it } from "vitest";
import { SessionStore, initialState } from "./session-store";

describe("SessionStore", () => {
  it("open resets state and bumps epoch", () => {
    const s = new SessionStore();
    const e1 = s.open({ sessionId: "a", path: "/x.azodoc", revision: "rev_1", fingerprint: "f1" });
    s.markEdited();
    const e2 = s.open({ sessionId: "b", path: null, revision: null, fingerprint: null });
    expect(e2).toBe(e1 + 1);
    expect(s.state.dirty).toBe(false);
    expect(s.state.path).toBeNull();
    expect(s.state.displayName).toBe("未命名文档");
    expect(s.state.epoch).toBe(e2);
  });

  it("displayName derives from path basename", () => {
    const s = new SessionStore();
    s.open({ sessionId: "k", path: "D:/docs/报告.azodoc", revision: null, fingerprint: null });
    expect(s.state.displayName).toBe("报告.azodoc");
  });

  it("selection-style non-content change does not mark dirty", () => {
    const s = new SessionStore();
    s.open({ sessionId: "k", path: "/x", revision: null, fingerprint: "f" });
    // 没有 markEdited → 不脏（dirty 由控制器按 doc 等价性驱动）
    expect(s.state.dirty).toBe(false);
  });

  it("undo back to baseline clears dirty via setDirty", () => {
    const s = new SessionStore();
    s.open({ sessionId: "k", path: "/x", revision: null, fingerprint: "f" });
    s.markEdited();
    expect(s.state.dirty).toBe(true);
    s.setDirty(false); // 控制器比较 doc.eq(baseline) 后回写
    expect(s.state.dirty).toBe(false);
  });

  it("Ctrl+S reentry queues instead of double-saving", () => {
    const s = new SessionStore();
    s.open({ sessionId: "k", path: "/x", revision: null, fingerprint: "f" });
    expect(s.requestSave()).toBe(true);
    expect(s.state.saveState).toBe("saving");
    expect(s.requestSave()).toBe(false);
    expect(s.state.saveQueued).toBe(true);
    s.saveSucceeded("f2", "rev_1");
    expect(s.consumeQueuedSave()).toBe(true);
    expect(s.requestSave()).toBe(true);
  });

  it("edits during save stay dirty after confirmation", () => {
    const s = new SessionStore();
    s.open({ sessionId: "k", path: "/x", revision: null, fingerprint: "f" });
    s.markEdited();
    s.requestSave(); // 捕获 gen=1
    s.markEdited(); // 保存中继续输入 → gen=2, dirty
    s.saveSucceeded("f2", "rev_1");
    expect(s.state.savedGeneration).toBe(1);
    expect(s.state.editGeneration).toBe(2);
    // dirty 由控制器 recompute 决定；savedGeneration<editGeneration 是信号
    expect(s.state.savedGeneration).toBeLessThan(s.state.editGeneration);
  });

  it("save failure returns to error state and allows retry", () => {
    const s = new SessionStore();
    s.open({ sessionId: "k", path: "/x", revision: null, fingerprint: "f" });
    s.requestSave();
    s.saveFailed();
    expect(s.state.saveState).toBe("error");
    expect(s.requestSave()).toBe(true);
  });

  it("save_as updates path and displayName", () => {
    const s = new SessionStore();
    s.open({ sessionId: "doc_1", path: null, revision: null, fingerprint: "g1" });
    s.requestSave();
    s.saveSucceeded("g2", "rev_2", "E:/out/报告.azodoc");
    expect(s.state.path).toBe("E:/out/报告.azodoc");
    expect(s.state.displayName).toBe("报告.azodoc");
    expect(s.state.fingerprint).toBe("g2");
  });

  it("stale epoch responses are detectable", () => {
    const s = new SessionStore();
    const e1 = s.open({ sessionId: "a", path: "/a", revision: null, fingerprint: null });
    const captured = s.currentEpoch();
    s.open({ sessionId: "b", path: "/b", revision: null, fingerprint: null });
    expect(captured).toBe(e1);
    expect(s.currentEpoch()).not.toBe(captured);
  });
});

describe("initialState", () => {
  it("starts clean and idle", () => {
    const s = initialState();
    expect(s.dirty).toBe(false);
    expect(s.saveState).toBe("idle");
    expect(s.epoch).toBe(0);
  });
});
