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

//! Editor schema：结构（节点/标记/attrs）来自 `azodoc-pm` 的生成物
//! `gen/aludel-schema.json`——**不要在这里手写结构**；本文件只补 DOM 呈现层
//! （toDOM/parseDOM，编辑器渲染与剪贴板解析 concern），以及少量只读交互。

import { Schema, type Node as PMNode } from "prosemirror-model";
import schemaSpec from "../../../azodoc-pm/gen/aludel-schema.json";

// eslint 风格豁免：toDOM/parseDOM 表的值类型由 PM NodeSpec 宽容接收
type AnySpec = unknown;

const spec = schemaSpec as unknown as {
  nodes: Record<string, Record<string, unknown>>;
  marks: Record<string, Record<string, unknown>>;
};

/** Prima 资产引用（asset://<id>/<path>）→ 可展示 URL；非 http(s) 的返回 null，
 * 由 toDOM 渲染为占位框（真实资源解析属表现层/导出，编辑器内不假装能加载）。 */
function assetUrl(asset: unknown): string | null {
  const s = typeof asset === "string" ? asset : "";
  return /^https?:\/\//.test(s) ? s : null;
}

/** 把 caption attr（Prima span JSON 数组）压平为纯文本（只读展示用）。 */
function captionText(caption: unknown): string {
  const parts: string[] = [];
  const walk = (v: unknown): void => {
    if (Array.isArray(v)) return void v.forEach(walk);
    if (v && typeof v === "object") {
      const o = v as Record<string, unknown>;
      if (typeof o.text === "string") parts.push(o.text);
      for (const k of ["content", "caption"]) if (o[k] !== undefined) walk(o[k]);
    }
  };
  walk(caption);
  return parts.join("");
}

// ---------------------------------------------------------------- toDOM

const nodeToDOM: Record<string, (node: PMNode) => AnySpec> = {
  section: () => ["div", { class: "az-section" }, 0],
  paragraph: () => ["p", 0],
  heading: (node) => ["h" + Math.min(6, Math.max(1, Number(node.attrs.level) || 1)), 0],
  quote: () => ["blockquote", 0],
  list: (node) =>
    node.attrs.style === "ordered"
      ? ["ol", { start: node.attrs.start ?? undefined }, 0]
      : ["ul", 0],
  list_item: (node) => [
    "li",
    {
      "data-checked":
        node.attrs.checked === true ? "true" : node.attrs.checked === false ? "false" : undefined,
      title: node.attrs.checked === true || node.attrs.checked === false ? "点击切换完成状态" : undefined,
    },
    0,
  ],
  code_block: (node) => [
    "pre",
    node.attrs.language ? { "data-language": node.attrs.language } : {},
    ["code", 0],
  ],
  table: () => ["table", 0],
  table_row: () => ["tr", 0],
  table_cell: (node) => [
    "td",
    {
      colspan: node.attrs.colSpan ?? undefined,
      rowspan: node.attrs.rowSpan ?? undefined,
    },
    0,
  ],
  figure: (node) => {
    const src = assetUrl(node.attrs.asset);
    const img = src
      ? ["img", { src, alt: node.attrs.alt }]
      : ["div", { class: "az-asset-placeholder" }, `asset: ${node.attrs.asset}（${node.attrs.alt}）`];
    const children: unknown[] = [img];
    if (node.attrs.caption) children.push(["figcaption", captionText(node.attrs.caption)]);
    return ["figure", ...children];
  },
  image: (node) => {
    const src = assetUrl(node.attrs.asset);
    return src
      ? ["img", { src, alt: node.attrs.alt }]
      : ["span", { class: "az-asset-placeholder" }, `asset: ${node.attrs.asset}`];
  },
  horizontal_rule: () => ["hr"],
  // 数学在编辑器内以 LaTeX 源码呈现（KaTeX 渲染属 Future Work B3）；双击可编辑
  math_block: (node) => [
    "div",
    { class: "az-math", "data-latex": node.attrs.latex, title: "双击编辑 LaTeX" },
    "$$" + node.attrs.latex + "$$",
  ],
  callout: (node) => [
    "div",
    { class: `az-callout az-callout--${node.attrs.variant}`, "data-variant": node.attrs.variant },
    0,
  ],
  embed: (node) => ["div", { class: "az-embed" }, `embed: ${node.attrs.asset}`],
  footnote: () => ["aside", { class: "az-footnote" }, 0],
  // unknown = R2/R3 通道：只读卡片，payload 不进 DOM，只显示损失元数据
  unknown_block: (node) => [
    "div",
    {
      class: "az-unknown",
      title: `${node.attrs.origin} · ${node.attrs.loss_class} · ${node.attrs.summary}`,
      contenteditable: "false",
    },
    `〔不可编辑：${node.attrs.summary}〕`,
  ],
  hard_break: () => ["br"],
  footnote_ref: (node) => ["sup", { class: "az-footnote-ref", title: node.attrs.id }, "[^]"],
  inline_math: (node) => [
    "code",
    { class: "az-inline-math", title: "双击编辑 LaTeX" },
    "$" + node.attrs.latex + "$",
  ],
  inline_image: (node) => {
    const src = assetUrl(node.attrs.asset);
    return src
      ? ["img", { src, alt: node.attrs.alt }]
      : ["span", { class: "az-asset-placeholder" }, `asset: ${node.attrs.asset}`];
  },
  mention: (node) => ["span", { class: "az-mention" }, "@" + node.attrs.target],
  cite: (node) => ["span", { class: "az-cite" }, "[@" + node.attrs.key + "]"],
  unknown_span: (node) => [
    "span",
    {
      class: "az-unknown az-unknown--inline",
      title: `${node.attrs.origin} · ${node.attrs.summary}`,
      contenteditable: "false",
    },
    `〔${node.attrs.summary}〕`,
  ],
};

const markToDOM: Record<string, (mark: PMNode) => AnySpec> = {
  link: (mark) => ["a", { href: mark.attrs.url, title: mark.attrs.title ?? undefined }, 0],
  strong: () => ["strong", 0],
  em: () => ["em", 0],
  underline: () => ["u", 0],
  strike: () => ["s", 0],
  code: () => ["code", 0],
  // 内部隐形标记：Text.extra 的载体（span_extra），DOM 中不可见
  span_extra: (mark) => [
    "span",
    { class: "az-extra", style: "display:none", "data-extra": JSON.stringify(mark.attrs.data) },
    0,
  ],
};

// ---------------------------------------------------------------- parseDOM
// 剪贴板粘贴的最小恢复面；无法建模的内容自然退化为纯文本（损失由保存侧报告）。

const nodeParseDOM: Record<string, AnySpec[]> = {
  section: [{ tag: "div[data-az-section]" }],
  paragraph: [{ tag: "p" }],
  heading: [1, 2, 3, 4, 5, 6].map((level) => ({
    tag: `h${level}`,
    attrs: { level },
  })),
  quote: [{ tag: "blockquote" }],
  list: [
    { tag: "ul", attrs: { style: "bullet" } },
    {
      tag: "ol",
      getAttrs: (dom: string | Node) => ({
        style: "ordered",
        start: (dom as HTMLOListElement).start ?? null,
      }),
    },
  ],
  list_item: [{ tag: "li" }],
  code_block: [
    {
      tag: "pre",
      preserveWhitespace: "full" as const,
      getAttrs: (dom: string | Node) => {
        const code = (dom as HTMLElement).querySelector("code");
        const cls = code?.className ?? "";
        const m = /language-(\S+)/.exec(cls);
        return { language: m ? m[1] : null };
      },
    },
  ],
  table: [{ tag: "table" }],
  table_row: [{ tag: "tr" }],
  table_cell: [{ tag: "td" }, { tag: "th" }],
  horizontal_rule: [{ tag: "hr" }],
  hard_break: [{ tag: "br" }],
};

const markParseDOM: Record<string, AnySpec[]> = {
  link: [
    {
      tag: "a[href]",
      getAttrs: (dom: string | Node) => {
        const el = dom as HTMLAnchorElement;
        return { url: el.getAttribute("href"), title: el.getAttribute("title") };
      },
    },
  ],
  strong: [{ tag: "strong" }, { tag: "b" }],
  em: [{ tag: "em" }, { tag: "i" }],
  underline: [{ tag: "u" }],
  strike: [{ tag: "s" }, { tag: "del" }],
  code: [{ tag: "code" }],
};

// ---------------------------------------------------------------- 组装

function buildSchema(): Schema {
  const nodes: Record<string, Record<string, unknown>> = {};
  for (const [name, entry] of Object.entries(spec.nodes)) {
    nodes[name] = {
      ...entry,
      toDOM: nodeToDOM[name],
      parseDOM: nodeParseDOM[name],
    };
  }
  const marks: Record<string, Record<string, unknown>> = {};
  for (const [name, entry] of Object.entries(spec.marks)) {
    marks[name] = {
      ...entry,
      toDOM: markToDOM[name],
      parseDOM: markParseDOM[name],
    };
  }
  return new Schema({ nodes: nodes as never, marks: marks as never });
}

export const schema: Schema = buildSchema();
