// This file is part of Athanor, the Azodoc document engine.
// Copyright (C) 2026 The Athanor Studio Developers
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published
// by the Free Software Foundation, version 3 of the License only.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Aludel 前端装配：加载 /api/doc → ProseMirror 编辑器；保存走 /api/save
//! （落链 author:human、标注重定位统计）；verify 走 /api/verify。
//! 修订层/语义层/损失报告经右侧栏第一次面向真人。

import { EditorState, Transaction } from "prosemirror-state";
import { EditorView } from "prosemirror-view";
import { baseKeymap, toggleMark, wrapIn, setBlockType } from "prosemirror-commands";
import { keymap } from "prosemirror-keymap";
import { history, undo, redo } from "prosemirror-history";
import { schema } from "./schema";
import "prosemirror-view/style/prosemirror.css";
import { invoke } from "@tauri-apps/api/core";

interface DocResponse {
  path: string;
  revision: string | null;
  pm_doc: Record<string, unknown>;
  annotations: { annotations?: Record<string, unknown>[] };
  history: {
    id: string;
    author_type: string;
    author_id: string | null;
    message: string;
    timestamp: string | null;
    is_current: boolean;
    is_head: boolean;
  }[];
  warnings: string[];
  loss_summary: {
    unknown_blocks: Record<string, unknown>[];
    detached_annotations: number;
  };
}

const $ = (id: string): HTMLElement => document.getElementById(id)!;

let view: EditorView;
let currentDoc: DocResponse | null = null;
let dirty = false;
const tauriMode = "__TAURI_INTERNALS__" in window;

// ---------------------------------------------------------------- 装配

function createEditor(docJson: Record<string, unknown>): EditorView {
  const doc = schema.nodeFromJSON(docJson);
  return new EditorView($("editor"), {
    state: EditorState.create({
      doc,
      plugins: [
        keymap({ "Mod-z": undo, "Shift-Mod-z": redo, "Mod-y": redo }),
        keymap(baseKeymap),
        history(),
      ],
    }),
    attributes: { spellcheck: "false" },
    // 双击数学节点 → LaTeX 源码编辑（M6.3 的数学形态；KaTeX 渲染属 B3）
    handleDoubleClickOn: (v, _pos, node, nodePos) => {
      if (node.type.name !== "math_block" && node.type.name !== "inline_math") return false;
      const current = String(node.attrs.latex ?? "");
      const next = window.prompt("LaTeX 源码：", current);
      if (next === null) return true;
      v.dispatch(v.state.tr.setNodeAttribute(nodePos, "latex", next));
      return true;
    },
    // 任务列表复选框点击切换 checked attr（复选框由 CSS ::before 渲染，点击落在 li 上）
    handleDOMEvents: {
      mousedown: (v, event) => {
        const t = event.target as HTMLElement | null;
        if (!t || !t.matches?.("li[data-checked]")) return false;
        const $pos = v.state.doc.resolve(v.posAtDOM(t, 0));
        for (let depth = $pos.depth; depth > 0; depth--) {
          const n = $pos.node(depth);
          if (n.type.name === "list_item") {
            const pos = $pos.before(depth);
            v.dispatch(v.state.tr.setNodeAttribute(pos, "checked", !n.attrs.checked));
            return true;
          }
        }
        return false;
      },
    },
    dispatchTransaction: (tr: Transaction) => {
      view.updateState(view.state.apply(tr));
      dirty = true;
      document.title = `Athanor Studio — ${currentDoc?.path ?? "未命名"} *`;
    },
  });
}

async function command<T>(name: string, args: Record<string, unknown>): Promise<T> {
  return invoke<T>(name, args);
}

function mountDocument(d: DocResponse): void {
  currentDoc = d;
  view?.destroy();
  view = createEditor(d.pm_doc);
  dirty = false;
  document.title = `Athanor Studio — ${d.path}`;
  renderSidebar(d);
  toolbar();
  $("save").addEventListener("click", () => void save());
  $("verify").addEventListener("click", () => void verify());
}

// ---------------------------------------------------------------- 侧栏

function renderSidebar(d: DocResponse): void {
  $("doc-path").textContent = d.path;
  $("rev").textContent = d.revision ?? "（未落链）";

  const history = $("history");
  history.replaceChildren(
    ...(d.history ?? [])
      .slice()
      .reverse()
      .map((e) => {
        const li = document.createElement("li");
        const tag = document.createElement("span");
        tag.className = `tag tag--${e.author_type}`;
        tag.textContent = e.author_type;
        li.append(
          tag,
          document.createTextNode(
            ` ${e.id} · ${e.message || "（无说明）"}${e.is_current ? " ⟵当前" : ""}`,
          ),
        );
        return li;
      }),
  );

  const anns = $("annotations");
  anns.replaceChildren(
    ...((d.annotations?.annotations ?? []) as Record<string, unknown>[]).map((a) => {
      const li = document.createElement("li");
      const detached = a["detached"] === true;
      if (detached) li.className = "detached";
      const target = (a["target"] ?? {}) as Record<string, unknown>;
      li.textContent = `${a["id"]} [${a["type"]}] @${target["block"] ?? "?"}${detached ? " · 失配 detached" : ""}`;
      return li;
    }),
  );

  const loss = $("loss");
  const lossItems: HTMLLIElement[] = (d.loss_summary?.unknown_blocks ?? []).map((u) => {
    const li = document.createElement("li");
    li.textContent = `unknown · ${u["summary"] ?? ""}（${u["loss_class"] ?? "?"}）`;
    return li;
  });
  if ((d.loss_summary?.detached_annotations ?? 0) > 0) {
    const li = document.createElement("li");
    li.className = "detached";
    li.textContent = `失配标注 ${d.loss_summary!.detached_annotations} 条`;
    lossItems.push(li);
  }
  if (lossItems.length === 0) {
    const li = document.createElement("li");
    li.textContent = "无";
    lossItems.push(li);
  }
  loss.replaceChildren(...lossItems);

  const warns = $("warnings");
  warns.replaceChildren(
    ...((d.warnings?.length ?? 0) > 0
      ? d.warnings.map((w) => {
          const li = document.createElement("li");
          li.textContent = w;
          return li;
        })
      : [Object.assign(document.createElement("li"), { textContent: "无" })]),
  );
}

// ---------------------------------------------------------------- 保存 / 校验

async function post(path: string, body: unknown): Promise<{ status: number; data: Record<string, unknown> }> {
  const r = await fetch(path, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body ?? {}),
  });
  return { status: r.status, data: (await r.json()) as Record<string, unknown> };
}

function setMsg(text: string, ok: boolean): void {
  const el = $("msg");
  el.textContent = text;
  el.className = ok ? "ok" : "bad";
}

async function save(): Promise<void> {
  const btn = $("save") as HTMLButtonElement;
  btn.disabled = true;
  setMsg("保存中…", true);
  try {
    const message = ($("message") as HTMLInputElement).value.trim();
    const body = {
      pm_doc: view.state.doc.toJSON(),
      message: message || undefined,
      author_id: "local",
    };
    const result = tauriMode
      ? { status: 200, data: await command<Record<string, unknown>>("save_document", { sessionId: currentDoc?.path, body }) }
      : await post("/api/save", body);
    const { status, data } = result;
    if (status === 200) {
      const rel = (data["relocate"] ?? null) as Record<string, number> | null;
      const relText = rel
        ? ` · 标注 未变${rel.unchanged}/重锚${rel.reanchored}/迁移${rel.moved}/失配${rel.detached}`
        : "";
      setMsg(`已落链 ${data["revision"]}（author:human，新 ID ${data["ids_assigned"]}）${relText}`, true);
      // 只刷新侧栏与修订号，不重置编辑器状态（保护光标与未保存输入）
      const d = tauriMode
        ? await command<DocResponse>("open_document", { path: currentDoc?.path })
        : await (await fetch("/api/doc")).json();
      currentDoc = d as DocResponse;
      renderSidebar(currentDoc);
      dirty = false;
      document.title = `Athanor Studio — ${currentDoc.path}`;
    } else {
      setMsg("保存被拒绝: " + (data["error"] ?? status), false);
    }
  } catch (e) {
    setMsg("保存失败: " + (e as Error).message, false);
  } finally {
    btn.disabled = false;
  }
}

async function verify(): Promise<void> {
  const btn = $("verify") as HTMLButtonElement;
  btn.disabled = true;
  setMsg("athanor verify 运行中…", true);
  try {
    const data = tauriMode
      ? await command<Record<string, unknown>>("verify_document", { sessionId: currentDoc?.path })
      : (await post("/api/verify", {})).data;
    setMsg(data["ok"] === true ? "athanor verify 通过 ✓" : "athanor verify 未通过（详见服务端输出）", data["ok"] === true);
  } finally {
    btn.disabled = false;
  }
}

// ---------------------------------------------------------------- 工具栏

function toolbar(): void {
  const cmd = (fn: () => void): (() => void) => fn;
  const bind = (id: string, fn: () => void, title: string): void => {
    const b = $(id) as HTMLButtonElement;
    b.title = title;
    b.addEventListener("click", () => cmd(fn)());
  };
  bind("tb-strong", () => toggleMark(schema.marks.strong)(view.state, view.dispatch), "加粗 (Ctrl+B)");
  bind("tb-em", () => toggleMark(schema.marks.em)(view.state, view.dispatch), "斜体 (Ctrl+I)");
  bind("tb-underline", () => toggleMark(schema.marks.underline)(view.state, view.dispatch), "下划线");
  bind("tb-strike", () => toggleMark(schema.marks.strike)(view.state, view.dispatch), "删除线");
  bind("tb-code", () => toggleMark(schema.marks.code)(view.state, view.dispatch), "行内代码");
  bind(
    "tb-link",
    () => {
      const url = window.prompt("链接 URL：");
      if (url) toggleMark(schema.marks.link, { url })(view.state, view.dispatch);
    },
    "链接",
  );
  bind("tb-para", () => setBlockType(schema.nodes.paragraph)(view.state, view.dispatch), "正文");
  bind("tb-h1", () => setBlockType(schema.nodes.heading, { level: 1 })(view.state, view.dispatch), "标题 1");
  bind("tb-h2", () => setBlockType(schema.nodes.heading, { level: 2 })(view.state, view.dispatch), "标题 2");
  bind("tb-quote", () => wrapIn(schema.nodes.quote)(view.state, view.dispatch), "引用");
  bind("tb-codeblock", () => setBlockType(schema.nodes.code_block, { language: null })(view.state, view.dispatch), "代码块");
  bind(
    "tb-math",
    () => {
      const latex = window.prompt("LaTeX 源码：", "E = mc^2");
      if (latex === null) return;
      const node = schema.nodes.math_block.create({ latex });
      view.dispatch(view.state.tr.replaceSelectionWith(node));
    },
    "数学块",
  );
  bind("tb-undo", () => undo(view.state, view.dispatch), "撤销");
  bind("tb-redo", () => redo(view.state, view.dispatch), "重做");
}

// ---------------------------------------------------------------- 入口

async function boot(): Promise<void> {
  try {
    if (tauriMode) {
      setMsg("Athanor Studio 已启动，请点击“打开文档”。", true);
      $("open").addEventListener("click", () => void openDocument());
      return;
    }
    const resp = await fetch("/api/doc");
    if (!resp.ok) {
      const err = (await resp.json()) as { error?: string };
      setMsg("文档打开失败: " + (err.error ?? resp.status), false);
      return;
    }
    mountDocument((await resp.json()) as DocResponse);
  } catch (e) {
    setMsg("启动失败: " + (e as Error).stack, false);
  }
}

async function openDocument(): Promise<void> {
  const path = window.prompt("Azodoc 文件路径：", currentDoc?.path ?? "");
  if (!path) return;
  try {
    const d = await command<DocResponse>("open_document", { path });
    mountDocument(d);
    setMsg("文档已打开。", true);
  } catch (e) {
    setMsg(`打开失败: ${e instanceof Error ? e.message : String(e)}`, false);
  }
}

window.addEventListener("beforeunload", (event) => {
  if (!dirty) return;
  event.preventDefault();
  event.returnValue = "";
});

void boot();
