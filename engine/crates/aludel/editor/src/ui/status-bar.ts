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

//! 状态栏：字数、保存状态、最近预览页数（过期快照不显示页码）、
//! 编辑模式、语言、缩放。页数只来自最近一次成功的 PreviewOutcome，
//! 正文一代际变化即失效（不把连续布局误报为 Word 分页）。

import { $ } from "./dom";

export interface StatusBarParams {
  chars: number;
  dirty: boolean;
  saveState: string;
  displayName: string;
  path: string | null;
  revision: string | null;
  zoom: number;
  /** 最近预览的有效页数（快照未失效）；null = 无有效分页数据。 */
  pageCount: number | null;
  /** 光标所在块的页码（同一快照内；无分页数据时 null）。 */
  currentPage: number | null;
  /** 视图模式（连续/页宽；页面视图由预览浮层承担）。 */
  viewMode: string;
}

export function renderStatusBar(params: StatusBarParams): void {
  $("sb-chars").textContent = `${params.chars} 字`;
  const saveText =
    params.saveState === "saving"
      ? "保存中…"
      : params.saveState === "error"
        ? "保存失败"
        : params.dirty
          ? "未保存"
          : "已保存";
  const save = $("sb-save");
  save.textContent = saveText;
  save.className = params.dirty ? "dirty" : "";
  const page = $("sb-page");
  if (page) {
    page.textContent =
      params.pageCount !== null && params.currentPage !== null
        ? `第 ${params.currentPage} / ${params.pageCount} 页`
        : params.pageCount !== null
          ? `预览 ${params.pageCount} 页`
          : "";
    page.title = params.pageCount !== null
      ? "最近一次打印预览的分页结果（快照有效；非连续布局的实时页码）"
      : "";
  }
  const mode = $("sb-mode");
  if (mode) {
    mode.textContent = params.viewMode === "pageWidth" ? "页宽视图" : "连续编辑";
  }
  $("sb-zoom").textContent = `${Math.round(params.zoom * 100)}%`;
  document.title = `Athanor Studio — ${params.displayName}${params.dirty ? " *" : ""}`;
}
