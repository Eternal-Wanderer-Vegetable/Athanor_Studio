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
import {
  FORMAT_KEY,
  characterCss,
  paragraphCss,
  type CharacterFormat,
  type ParagraphFormat,
} from "./app/format";

// eslint 风格豁免：toDOM/parseDOM 表的值类型由 PM NodeSpec 宽容接收
type AnySpec = unknown;

const spec = schemaSpec as unknown as {
  nodes: Record<string, Record<string, unknown>>;
  marks: Record<string, Record<string, unknown>>;
};

// ---------------------------------------------------------------- 格式扩展

type ExtraBag = Record<string, unknown>;

function extraOf(node: PMNode): ExtraBag {
  const e = node.attrs.extra;
  return e && typeof e === "object" ? (e as ExtraBag) : {};
}

function fmtOf(node: PMNode): ExtraBag {
  const f = extraOf(node)[FORMAT_KEY];
  return f && typeof f === "object" ? (f as ExtraBag) : {};
}

/** 块级节点的段落格式 → CSS 字符串（无效值被忽略，不输出）。 */
function paraStyle(node: PMNode): string {
  return paragraphCss({ ...(fmtOf(node)["paragraph"] as ParagraphFormat | undefined) });
}

function charStyleFromData(data: unknown): string {
  const d = data && typeof data === "object" ? (data as ExtraBag) : {};
  const fmt = d[FORMAT_KEY] && typeof d[FORMAT_KEY] === "object" ? (d[FORMAT_KEY] as ExtraBag) : {};
  return characterCss({ ...(fmt["character"] as CharacterFormat | undefined) });
}

/** 剪贴板解析：DOM 样式 → 受控格式 patch（白名单，其他样式一律不进文档）。 */
function paraFormatFromDom(dom: HTMLElement): ParagraphFormat {
  const out: ParagraphFormat = {};
  const s = dom.style;
  const align = s.textAlign;
  if (align === "left" || align === "center" || align === "right" || align === "justify") {
    out.align = align;
  }
  const ptNum = (v: string): number | undefined => {
    const m = /^([\d.]+)pt$/.exec(v);
    return m ? Number(m[1]) : undefined;
  };
  const pl = ptNum(s.paddingLeft);
  if (pl !== undefined) out.indentStartPt = pl;
  const pr = ptNum(s.paddingRight);
  if (pr !== undefined) out.indentEndPt = pr;
  const ti = ptNum(s.textIndent);
  if (ti !== undefined) out.firstLinePt = ti;
  const lh = Number(s.lineHeight);
  if (Number.isFinite(lh) && lh > 0.5 && lh <= 5) out.lineHeight = lh;
  return out;
}

/** Prima 资产引用 → 可展示 URL；非已知可展示形态的返回 null，
 * 由 toDOM 渲染为占位框（不假装能加载）。
 * `asset://` 经注册的解析器映射到本端 URL（HTTP /api/asset 或
 * Tauri azodoc-asset:// 协议）；未注册解析器时（测试/原型）占位。
 * `data:`/`blob:`/`http(s)` 原样可显示——其中 data: 是 §4.2 的旧形态，
 * 打开可显示、保存时由服务端迁移为 asset://。 */
export type AssetResolver = (ref: string) => string | null;
let assetResolver: AssetResolver | null = null;

/** 注册会话级资产解析器（DocumentController 在挂接 gateway 后调用）。 */
export function setAssetResolver(r: AssetResolver | null): void {
  assetResolver = r;
}

function assetUrl(asset: unknown): string | null {
  const s = typeof asset === "string" ? asset : "";
  if (s.startsWith("asset://")) {
    return assetResolver ? assetResolver(s) : null;
  }
  return /^(?:https?:\/\/|blob:|data:image\/)/.test(s) ? s : null;
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

/** 官方 setCellAttrs 等价物：colwidth → data-colwidth + style.width。 */
function cellAttrs(node: PMNode): Record<string, unknown> {
  const extra: Record<string, unknown> = {};
  const colwidth = node.attrs.colwidth as number[] | null | undefined;
  const widths = colwidth && colwidth.filter((w) => typeof w === "number");
  if (widths && widths.length > 0) {
    extra["data-colwidth"] = colwidth!.join(",");
    if (colwidth!.length === 1 && widths.length === 1) {
      extra.style = `width: ${widths[0]}px`;
    }
  }
  return {
    colspan: node.attrs.colspan ?? undefined,
    rowspan: node.attrs.rowspan ?? undefined,
    ...extra,
  };
}

const nodeToDOM: Record<string, (node: PMNode) => AnySpec> = {
  section: () => ["div", { class: "az-section" }, 0],
  paragraph: (node) => {
    const style = paraStyle(node);
    return ["p", style ? { style } : {}, 0];
  },
  heading: (node) => {
    const style = paraStyle(node);
    return ["h" + Math.min(6, Math.max(1, Number(node.attrs.level) || 1)), style ? { style } : {}, 0];
  },
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
  // 表格系：对齐 prosemirror-tables 官方 DOM 契约（tbody 包装、
  // data-colwidth 列宽、th/td 角色），TableMap/列宽拖拽依赖此结构。
  table: () => ["table", ["tbody", 0]],
  table_row: () => ["tr", 0],
  table_cell: (node) => ["td", cellAttrs(node), 0],
  table_header: (node) => ["th", cellAttrs(node), 0],
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
  // 手动分页（E4）：编辑器里可见的分页标记；印刷/PDF 由 .page-break 规则分页。
  page_break: () => [
    "div",
    { class: "az-page-break", title: "分页符：此处之后另起一页" },
    ["hr"],
  ],
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
  // 回跳符供 footnoteClick 定位：点击跳回第一个引用点。
  // PM 的 0（内容洞）必须是所在元素的唯一子节点——↩ 与内容 div 作兄弟。
  footnote: () => [
    "aside",
    { class: "az-footnote" },
    ["span", { class: "az-footnote-back", title: "跳回引用" }, "↩"],
    ["div", { class: "az-footnote-body" }, 0],
  ],
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
  // Text.extra 的载体（span_extra）：不再 display:none——携带未知键的正文必须可见；
  // 受控字符格式（x-athanor-format.character）映射为内联样式。
  span_extra: (mark) => {
    const style = charStyleFromData(mark.attrs.data);
    return [
      "span",
      {
        class: "az-extra",
        ...(style ? { style } : {}),
        "data-extra": JSON.stringify(mark.attrs.data),
      },
      0,
    ];
  },
};

// ---------------------------------------------------------------- parseDOM
// 剪贴板粘贴的最小恢复面；无法建模的内容自然退化为纯文本（损失由保存侧报告）。

/** 官方 getCellAttrs 等价物：td/th → colspan/rowspan/colwidth（小写 PM 名）。
 *  显式转 number——PM 不强制 attr 类型，字符串会让 Prima 侧 as_i64 丢失跨度。 */
function cellParseAttrs(dom: string | Node): Record<string, unknown> {
  const el = dom as HTMLElement;
  const int = (name: string): number | undefined => {
    const v = el.getAttribute(name);
    return v && /^\d+$/.test(v) ? Number(v) : undefined;
  };
  const widthAttr = el.getAttribute("data-colwidth");
  const widths =
    widthAttr && /^\d+(,\d+)*$/.test(widthAttr)
      ? widthAttr.split(",").map(Number)
      : [el.offsetWidth];
  return {
    colspan: int("colspan"),
    rowspan: int("rowspan"),
    colwidth: widths,
  };
}

const nodeParseDOM: Record<string, AnySpec[]> = {
  section: [{ tag: "div[data-az-section]" }],
  paragraph: [
    {
      tag: "p",
      getAttrs: (dom: string | Node) => {
        const para = paraFormatFromDom(dom as HTMLElement);
        return Object.keys(para).length
          ? { extra: { [FORMAT_KEY]: { version: 1, paragraph: para } } }
          : {};
      },
    },
  ],
  heading: [1, 2, 3, 4, 5, 6].map((level) => ({
    tag: `h${level}`,
    getAttrs: (dom: string | Node) => {
      const para = paraFormatFromDom(dom as HTMLElement);
      return {
        level,
        ...(Object.keys(para).length
          ? { extra: { [FORMAT_KEY]: { version: 1, paragraph: para } } }
          : {}),
      };
    },
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
  table: [
    {
      tag: "table",
      getAttrs: (dom: string | Node) => {
        // 粘贴的外部 HTML：首行全 th → header_row（本侧 td/th 已能无损
        // 区分，这里只为外部表格补角色元数据）。
        const firstRow = (dom as HTMLElement).querySelector("tr");
        const cells = firstRow ? Array.from(firstRow.children) : [];
        return cells.length > 0 && cells.every((c) => c.tagName === "TH")
          ? { header_row: true }
          : {};
      },
    },
  ],
  table_row: [{ tag: "tr" }],
  // th 走 table_header（下方 parseDOM 注册顺序保证优先级），td 走 table_cell。
  // getCellAttrs：colspan/rowspan 原生属性 + data-colwidth/width 样式读回。
  table_cell: [
    {
      tag: "td",
      getAttrs: cellParseAttrs,
    },
  ],
  table_header: [
    {
      tag: "th",
      getAttrs: cellParseAttrs,
    },
  ],
  horizontal_rule: [{ tag: "hr" }],
  page_break: [{ tag: "div.az-page-break" }, { tag: "div.page-break" }],
  hard_break: [{ tag: "br" }],
  // E3 粘贴恢复面：编辑器自身 toDOM 产物 + 外部网页的最小集。
  image: [
    {
      tag: "img[src]",
      getAttrs: (dom: string | Node) => {
        const el = dom as HTMLImageElement;
        return { asset: el.getAttribute("src"), alt: el.getAttribute("alt") ?? "" };
      },
    },
  ],
  footnote_ref: [
    {
      tag: "sup.az-footnote-ref",
      getAttrs: (dom: string | Node) => ({ id: (dom as HTMLElement).getAttribute("title") }),
    },
  ],
  math_block: [
    {
      tag: "div.az-math[data-latex]",
      getAttrs: (dom: string | Node) => ({ latex: (dom as HTMLElement).dataset.latex }),
    },
  ],
  inline_math: [
    {
      tag: "code.az-inline-math[data-latex]",
      getAttrs: (dom: string | Node) => ({ latex: (dom as HTMLElement).dataset.latex }),
    },
  ],
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
  span_extra: [{
    tag: "span.az-extra[data-extra]",
    getAttrs: (dom: string | Node) => {
      try {
        const raw = (dom as HTMLElement).getAttribute("data-extra");
        if (!raw) return false;
        const parsed = JSON.parse(raw);
        return parsed && typeof parsed === "object" ? { data: parsed } : false;
      } catch {
        return false;
      }
    },
  }],
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
