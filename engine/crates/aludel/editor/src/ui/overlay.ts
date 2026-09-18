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

//! 统一覆盖层控制：应用菜单、命令面板等瞬时浮层的
//! Escape 关闭、外部 pointer-down 关闭、焦点恢复只有一个实现。
//! （打印预览浮层自带 openSeq/焦点管理，不经过本控制器。）

export interface OverlayOptions {
  /** 关闭时恢复焦点的元素（缺省为打开时的 document.activeElement）。 */
  restoreFocus?: HTMLElement | null;
  /** 关闭回调（移除 DOM 之外的状态清理）。 */
  onClose?: () => void;
  /** 点击这些元素不算“外部”（例如菜单触发按钮）。 */
  anchors?: HTMLElement[];
}

interface OpenOverlay {
  el: HTMLElement;
  restoreFocus: HTMLElement | null;
  onClose?: () => void;
  anchors: HTMLElement[];
}

export class OverlayController {
  private open: OpenOverlay | null = null;

  get active(): HTMLElement | null {
    return this.open?.el ?? null;
  }

  /** 打开覆盖层；已有覆盖层先关闭（菜单互斥）。 */
  show(el: HTMLElement, opts: OverlayOptions = {}): void {
    this.close();
    this.open = {
      el,
      restoreFocus:
        opts.restoreFocus === undefined
          ? (document.activeElement as HTMLElement | null)
          : opts.restoreFocus,
      onClose: opts.onClose,
      anchors: opts.anchors ?? [],
    };
    document.body.appendChild(el);
  }

  /** 关闭并恢复焦点（restoreFocus 不可聚焦时跳过）。 */
  close(restore = true): void {
    const cur = this.open;
    if (!cur) return;
    this.open = null;
    cur.el.remove();
    cur.onClose?.();
    if (restore) cur.restoreFocus?.focus?.();
  }

  /** document pointerdown 路由：点在覆盖层或锚点外 → 关闭。 */
  handlePointerDown(target: HTMLElement | null): void {
    const cur = this.open;
    if (!cur || !target) return;
    if (cur.el.contains(target)) return;
    if (cur.anchors.some((a) => a.contains(target))) return;
    this.close();
  }

  /** document keydown 路由：Escape 关闭（返回是否已处理）。 */
  handleKeydown(e: KeyboardEvent): boolean {
    if (e.key !== "Escape" || !this.open) return false;
    e.preventDefault();
    e.stopPropagation();
    this.close();
    return true;
  }
}
