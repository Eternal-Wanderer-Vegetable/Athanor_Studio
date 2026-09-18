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

//! 粘贴净化：外部 HTML → 白名单子集，三种粘贴模式。
//!
//! 模式（spec E3）：
//!   keep   — 保留受支持格式（默认 Ctrl+V）：白名单结构 + 内联样式；
//!   match  — 匹配目标格式（Ctrl+Alt+V）：保结构，剥内联格式；
//!   plain  — 纯文本（Ctrl+Shift+V）：所有标签 unwrap，只剩文字。
//!
//! 净化规则：
//!   - script/style/iframe/object/embed/form/input/button/select/textarea
//!     整块丢弃（不保留文本——可能含脚本/表单载荷）；
//!   - 其余未知标签 unwrap（保文字，贴进不了模型的自然退化为文本）；
//!   - on* 事件处理属性全部移除；style/class/title 属性移除；
//!   - URL 走白名单 scheme：a.href 仅 http/https/mailto/相对锚点；
//!     img.src 仅 http/https/data:image/相对路径；其余丢弃该属性。
//!   - data-extra 必须是合法 JSON（span_extra 的解析入口防注入）。

/** 允许的 URL scheme（a/img 各有独立白名单）。 */
export type PasteMode = "keep" | "match" | "plain";

const LINK_SCHEMES = new Set(["http:", "https:", "mailto:", "tel:"]);
const IMAGE_SCHEMES = new Set(["http:", "https:", "data:"]);

/** URL 是否安全：白名单 scheme 或相对/锚点引用。 */
export function isSafeUrl(raw: string, kind: "link" | "image"): boolean {
  const v = raw.trim();
  if (!v) return false;
  // 锚点 / 相对路径 / 协议相对 //host/path 一律放行（不含 scheme）
  if (/^(#|\/|\.\/|\.\.\/)/.test(v)) return true;
  if (/^\/\//.test(v)) return true; // protocol-relative → http(s)
  const m = /^([a-zA-Z][a-zA-Z0-9+.-]*):/.exec(v);
  if (!m) return true; // 无 scheme 的裸路径
  const schemes = kind === "image" ? IMAGE_SCHEMES : LINK_SCHEMES;
  if (!schemes.has(m[1].toLowerCase() + ":")) return false;
  if (kind === "image" && m[1].toLowerCase() === "data") {
    // data: 仅允许位图/内联图像（与 stage 白名单一致：SVG 拒绝）
    return /^data:image\/(png|jpeg|jpg|gif|webp|bmp|avif);/i.test(v);
  }
  return true;
}

/** 整块丢弃的标签（内容与标签一起删）。 */
const DROP_TAGS = new Set([
  "SCRIPT", "STYLE", "IFRAME", "OBJECT", "EMBED", "APPLET",
  "FORM", "INPUT", "BUTTON", "SELECT", "TEXTAREA", "OPTION",
  "NOSCRIPT", "TEMPLATE", "SLOT", "CANVAS", "SVG", "MATH", // MathML 暂不建模
]);

/** 结构标签（match 模式保留）。 */
const STRUCT_TAGS = new Set([
  "DIV", "P", "BLOCKQUOTE", "PRE", "HR", "BR",
  "UL", "OL", "LI", "DL", "DT", "DD",
  "TABLE", "THEAD", "TBODY", "TFOOT", "TR", "TD", "TH", "CAPTION", "COLGROUP", "COL",
  "H1", "H2", "H3", "H4", "H5", "H6",
  "FIGURE", "FIGCAPTION", "IMG", "ASIDE", "SECTION", "ARTICLE",
]);

/** 内联格式标签（keep 保留、match 剥离成文字）。 */
const INLINE_TAGS = new Set([
  "A", "STRONG", "B", "EM", "I", "U", "S", "DEL", "STRIKE",
  "CODE", "SPAN", "SUP", "SUB", "MARK", "SMALL", "ABBR",
]);

interface SanitizeStats {
  droppedElements: number;
  strippedAttributes: number;
  unsafeUrls: number;
}

export interface SanitizeResult {
  /** 净化后的 HTML 片段（可直接交给 view.pasteHTML）。 */
  html: string;
  /** 是否产生过净化损失（用于一次性提示）。 */
  lossy: boolean;
  /** 文本模式可直接取到的纯文本。 */
  text: string;
  stats: SanitizeStats;
}

const ATTR_WHITELIST: Record<string, string[]> = {
  A: ["href", "title"],
  IMG: ["src", "alt", "width", "height"],
  OL: ["start", "type"],
  LI: ["data-checked"],
  TD: ["colspan", "rowspan", "data-colwidth"],
  TH: ["colspan", "rowspan", "data-colwidth"],
  PRE: ["data-language"],
  CODE: ["class", "data-latex"], // language-xxx / inline_math 的 LaTeX 载体
  SPAN: ["data-extra", "data-latex"],
  SUP: ["class", "title"], // footnote_ref 的 id 存在 title 里
  ASIDE: ["class"],
  DIV: ["data-az-section", "data-latex", "data-extra"],
};

/** class 白名单：只有编辑器自身产生的 az-* 类允许保留。 */
const ALLOWED_CLASS = /^az-(extra|footnote|footnote-ref|inline-math|math|asset-placeholder|cite|mention|unknown|callout)$/;

function cleanAttrs(el: Element, stats: SanitizeStats): void {
  const keep = new Set(ATTR_WHITELIST[el.tagName] ?? []);
  for (const attr of Array.from(el.attributes)) {
    const name = attr.name.toLowerCase();
    if (name.startsWith("on")) {
      el.removeAttribute(attr.name);
      stats.strippedAttributes++;
      continue;
    }
    if (name === "style" || name === "class") {
      if (name === "class" && el.tagName === "CODE" && /^language-\S+$/.test(attr.value)) continue;
      if (name === "class" && ALLOWED_CLASS.test(attr.value.trim())) continue;
      el.removeAttribute(attr.name);
      stats.strippedAttributes++;
      continue;
    }
    if (!keep.has(name)) {
      el.removeAttribute(attr.name);
      stats.strippedAttributes++;
      continue;
    }
    if ((el.tagName === "A" && name === "href" && !isSafeUrl(attr.value, "link")) ||
        (el.tagName === "IMG" && name === "src" && !isSafeUrl(attr.value, "image"))) {
      el.removeAttribute(attr.name);
      stats.unsafeUrls++;
      continue;
    }
    if (name === "data-extra") {
      try {
        JSON.parse(attr.value);
      } catch {
        el.removeAttribute(attr.name);
        stats.strippedAttributes++;
      }
    }
  }
}

function unwrap(el: Element): void {
  const parent = el.parentNode;
  if (!parent) return;
  while (el.firstChild) parent.insertBefore(el.firstChild, el);
  parent.removeChild(el);
}

function walk(el: Element, mode: PasteMode, stats: SanitizeStats): void {
  // 先深度处理子节点（快照列表，unwrap 会动 DOM）
  for (const child of Array.from(el.childNodes)) {
    if (child.nodeType === Node.ELEMENT_NODE) {
      walk(child as Element, mode, stats);
    }
  }
  const tag = el.tagName;
  if (DROP_TAGS.has(tag)) {
    stats.droppedElements++;
    el.remove();
    return;
  }
  if (mode === "plain") {
    if (tag === "BR" || tag === "HR") return; // 保留换行结构
    unwrap(el);
    return;
  }
  const structural = STRUCT_TAGS.has(tag);
  const inline = INLINE_TAGS.has(tag);
  if (!structural && !inline) {
    // 未知标签：unwrap 保文字（Word 的 mso 标签、自定义元素等）
    unwrap(el);
    return;
  }
  if (mode === "match" && inline && tag !== "BR") {
    // 匹配目标格式：剥内联，结构留
    unwrap(el);
    return;
  }
  cleanAttrs(el, stats);
}

/** 净化一段外部 HTML；plain 模式同时给出换行化文本。 */
export function sanitizeHtml(html: string, mode: PasteMode, doc: Document = document): SanitizeResult {
  const stats: SanitizeStats = { droppedElements: 0, strippedAttributes: 0, unsafeUrls: 0 };
  const host = doc.createElement("div");
  host.innerHTML = html;
  for (const child of Array.from(host.childNodes)) {
    if (child.nodeType === Node.ELEMENT_NODE) walk(child as Element, mode, stats);
  }
  const out = host.innerHTML;
  const text = host.textContent ?? "";
  return {
    html: out,
    text,
    lossy: stats.droppedElements + stats.strippedAttributes + stats.unsafeUrls > 0,
    stats,
  };
}

/** 净化提示（一次性，截断避免刷屏）。 */
export function sanitizeNotice(r: SanitizeResult): string | null {
  if (!r.lossy) return null;
  const parts: string[] = [];
  if (r.stats.droppedElements) parts.push(`丢弃 ${r.stats.droppedElements} 个不可信元素`);
  if (r.stats.unsafeUrls) parts.push(`移除 ${r.stats.unsafeUrls} 个危险链接`);
  if (r.stats.strippedAttributes) parts.push(`剥离 ${r.stats.strippedAttributes} 个属性`);
  return `已净化粘贴内容：${parts.join("，")}`;
}
