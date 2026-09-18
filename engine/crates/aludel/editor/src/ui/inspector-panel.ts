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

//! 检查面板“属性”区：SelectionContext 的只读投影——段落/字符格式、
//! 图片 alt、表格上下文；混合选区键显示“混合”而不是最后一 run 的值。
//! 不写回文档（派生视图原则）。

import type { SelectionContext } from "../app/format";
import { $, el } from "./dom";

function kv(label: string, value: string, mixed = false): HTMLElement {
  const row = el("div", "kv");
  row.appendChild(el("b", undefined, label));
  const v = el("span", mixed ? "mixed" : undefined, mixed ? "混合" : value);
  row.appendChild(v);
  return row;
}

const ALIGN_LABEL: Record<string, string> = {
  left: "左对齐",
  center: "居中",
  right: "右对齐",
  justify: "两端对齐",
};

export function renderInspector(sel: SelectionContext | null): void {
  const body = $("inspector-body");
  if (!body) return;
  body.replaceChildren();
  if (!sel) {
    body.appendChild(el("div", "kv", "无文档"));
    return;
  }
  const p = sel.paragraph;
  const c = sel.character;
  body.appendChild(
    kv("段落", p.mixed.includes("align") ? "" : (ALIGN_LABEL[p.values.align ?? "left"] ?? "左对齐"),
      p.mixed.includes("align")),
  );
  if (p.values.indentStartPt !== undefined || p.mixed.includes("indentStartPt")) {
    body.appendChild(
      kv("缩进", p.mixed.includes("indentStartPt") ? "" : `${p.values.indentStartPt}pt`,
        p.mixed.includes("indentStartPt")),
    );
  }
  body.appendChild(
    kv("字体", c.mixed.includes("fontFamily") ? "" : (c.values.fontFamily ?? "默认"),
      c.mixed.includes("fontFamily")),
  );
  body.appendChild(
    kv("字号", c.mixed.includes("fontSizePt") ? "" : (c.values.fontSizePt !== undefined ? `${c.values.fontSizePt}pt` : "默认"),
      c.mixed.includes("fontSizePt")),
  );
  body.appendChild(
    kv("颜色", c.mixed.includes("color") ? "" : (c.values.color ?? "默认"),
      c.mixed.includes("color")),
  );
  if (c.values.highlight !== undefined || c.mixed.includes("highlight")) {
    body.appendChild(
      kv("高亮", c.mixed.includes("highlight") ? "" : (c.values.highlight ?? ""),
        c.mixed.includes("highlight")),
    );
  }
  if (sel.kind === "image") {
    body.appendChild(kv("图片", sel.imageAlt ? `alt: ${sel.imageAlt}` : "alt: （未设置，双击图片编辑）"));
  }
  if (sel.inTable) {
    body.appendChild(kv("表格", "光标在表格内 — 表格页签已可用"));
  }
}
