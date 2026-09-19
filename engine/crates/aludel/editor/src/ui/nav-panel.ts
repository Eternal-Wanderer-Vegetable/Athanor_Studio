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

//! 左导航面板的"页面"区：真实分页映射（PreviewSurface LayoutIndex）
//! 才列页码；没有分页数据时显示可解释的空态，不假造缩略图或页数。
//! 点击页码跳转该页首个块（同一 doc position 通道，不改文档）。

import { $, el } from "./dom";

export interface NavPagesParams {
  /** 页码 → 该页首个块 id（快照有效时非空）。 */
  pageList: { page: number; blockId: string }[];
  /** 快照是否有效（编辑代数一致）；false 时空态说明原因。 */
  valid: boolean;
  /** 跳转到块 id（controller 层的 selection/scroll 通道）。 */
  onGoto: (blockId: string) => void;
}

export function renderNavPages(params: NavPagesParams): void {
  const host = $("nav-pages");
  if (!host) return;
  host.replaceChildren();
  if (!params.valid || params.pageList.length === 0) {
    const empty = el("div", "nav-empty");
    empty.textContent =
      "暂无分页数据——先用“打印预览”生成真实分页（Paged.js）。此处只显示真实结果，不假造页数。";
    host.appendChild(empty);
    return;
  }
  const list = el("ul", "nav-page-list");
  for (const p of params.pageList) {
    const li = el("li") as HTMLLIElement;
    const btn = el("button", "nav-page-btn", `第 ${p.page} 页`) as HTMLButtonElement;
    btn.type = "button";
    btn.title = `跳转到第 ${p.page} 页首个块`;
    btn.addEventListener("click", () => params.onGoto(p.blockId));
    li.appendChild(btn);
    list.appendChild(li);
  }
  host.appendChild(list);
}
