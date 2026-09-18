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

//! 大纲面板：纯派生渲染（PM headings → DOM），跳转走调用方回调。

import { $ } from "./dom";

export function renderOutline(
  headings: { level: number; text: string; pos: number }[],
  onJump: (pos: number) => void,
): void {
  const ul = $("outline");
  if (headings.length === 0) {
    const li = document.createElement("li");
    li.className = "pane-empty";
    li.textContent = "（无标题）";
    ul.replaceChildren(li);
    return;
  }
  ul.replaceChildren(
    ...headings.map((h) => {
      const li = document.createElement("li");
      li.style.paddingLeft = `${(h.level - 1) * 14}px`;
      const a = document.createElement("a");
      a.href = "#";
      a.textContent = h.text || "（空标题）";
      a.addEventListener("click", (e) => {
        e.preventDefault();
        onJump(h.pos);
      });
      li.append(a);
      return li;
    }),
  );
}
