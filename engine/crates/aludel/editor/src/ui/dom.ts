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

//! DOM 小工具与本地偏好存储（shell/ribbon/menus 共用）。

export const $ = (id: string): HTMLElement => document.getElementById(id)!;

export function setMsg(text: string, ok: boolean): void {
  const el = $("msg");
  el.textContent = text;
  el.className = ok ? "ok" : "bad";
}

/** localStorage 偏好（shell 布局偏好，不进文档/undo）。 */
export function loadPref<T>(key: string, fallback: T): T {
  try {
    const raw = window.localStorage.getItem(key);
    return raw === null ? fallback : (JSON.parse(raw) as T);
  } catch {
    return fallback;
  }
}

export function savePref(key: string, value: unknown): void {
  try {
    window.localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // 存储不可用时偏好不落盘，功能不受影响。
  }
}

export function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className?: string,
  text?: string,
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

/** 按钮：data-cmd 绑定 + title（label + shortcut）。 */
export function cmdButton(
  id: string | null,
  cmdId: string,
  label: string,
  shortcut?: string,
): HTMLButtonElement {
  const btn = document.createElement("button");
  btn.type = "button";
  if (id) btn.id = id;
  btn.dataset.cmd = cmdId;
  btn.textContent = label;
  btn.title = shortcut ? `${label} (${shortcut})` : label;
  return btn;
}
