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

//! 公式渲染：KaTeX NodeView（E3 spike 验收：离线打包 ~4MB unpacked、
//! MIT 许可、同步渲染单条 < 毫秒级；不达标则退回纯源码态）。
//!
//! LaTeX 是唯一真源（node.attrs.latex），渲染失败回退可读源码态
//! （不静默丢公式，不阻止编辑）。双击编辑由 controller 的
//! handleDoubleClickOn 负责（NodeView 放行冒泡事件）。

import katex from "katex";
import "katex/dist/katex.min.css";
import type { Node as PMNode } from "prosemirror-model";
import type { NodeView } from "prosemirror-view";

export type MathVariant = "block" | "inline";

/** 渲染 LaTeX → HTML；失败返回 null（调用方回退源码态）。纯函数便于测试。 */
export function renderMathHtml(latex: string, displayMode: boolean): string | null {
  if (!latex.trim()) return null;
  try {
    return katex.renderToString(latex, {
      displayMode,
      throwOnError: true,
      strict: "warn",
      output: "html",
    });
  } catch {
    return null;
  }
}

/** KaTeX 渲染包装元素；renderError 时显示源码 + data-error 标记。 */
function buildDom(node: PMNode, variant: MathVariant): HTMLElement {
  const isBlock = variant === "block";
  const dom = document.createElement(isBlock ? "div" : "span");
  dom.className = isBlock ? "az-math" : "az-inline-math";
  dom.dataset.latex = node.attrs.latex;
  dom.title = "双击编辑 LaTeX";
  const html = renderMathHtml(String(node.attrs.latex ?? ""), isBlock);
  if (html) {
    dom.innerHTML = html;
  } else {
    dom.dataset.error = "1";
    dom.textContent = (isBlock ? "$$" : "$") + String(node.attrs.latex ?? "") + (isBlock ? "$$" : "$");
  }
  return dom;
}

class MathView implements NodeView {
  dom: HTMLElement;
  constructor(
    node: PMNode,
    private variant: MathVariant,
  ) {
    this.dom = buildDom(node, variant);
  }
  update(node: PMNode): boolean {
    const type = node.type.name === "math_block" ? "block" : "inline";
    if (type !== this.variant) return false;
    const next = buildDom(node, this.variant);
    this.dom.replaceWith(next);
    this.dom = next;
    return true;
  }
  // stopEvent/selectNode 默认即可：不拦截冒泡（双击编辑靠 controller）
}

/** createEditor 装配用：math_block / inline_math 的 NodeView 工厂。 */
export function mathNodeViews(): Record<string, (node: PMNode) => NodeView> {
  return {
    math_block: (node) => new MathView(node, "block"),
    inline_math: (node) => new MathView(node, "inline"),
  };
}
