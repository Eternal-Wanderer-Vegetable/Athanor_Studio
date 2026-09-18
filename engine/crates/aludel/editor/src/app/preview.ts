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

//! 打印预览（E4）：服务端产出的 Paged.js 增强 HTML 灌进 iframe，
//! 等 `__azodocPagedDone` 后从 `.pagedjs_page` 实数取页数并从
//! `data-block-id` 克隆体反查块→页映射（LayoutIndex）。
//! 分页超时/失败回退未分页呈现，不改文档、不假造页数。

import type { PreviewResult } from "../platform/gateway";

/** 块 → 页索引（Paged.js 分页后的实数结果；块跨页记首页）。 */
export interface LayoutIndex {
  page_count: number;
  /** block id → 1-based 页码（克隆体首次出现的页）。 */
  page_of_block: Record<string, number>;
  /** 手动分页块落在哪一页之前（page_break 的块 id 列表）。 */
  page_breaks: { id: string; page: number }[];
}

export interface PreviewOutcome {
  layout: LayoutIndex | null;
  /** 分页不可用（超时/异常）时的未分页呈现标记。 */
  fallback: boolean;
  page_count: number | null;
}

const DEFAULT_PAGED_TIMEOUT_MS = 20_000;
const POLL_MS = 120;

/** 从分页完成的 iframe 文档提取 LayoutIndex。 */
export function buildLayoutIndex(doc: Document): LayoutIndex {
  const pages = Array.from(doc.querySelectorAll<HTMLElement>(".pagedjs_page"));
  const page_of_block: Record<string, number> = {};
  const page_breaks: { id: string; page: number }[] = [];
  pages.forEach((page, i) => {
    const pageNo = i + 1;
    page.querySelectorAll<HTMLElement>("[data-block-id]").forEach((el) => {
      const id = el.dataset.blockId ?? "";
      if (!id) return;
      if (page_of_block[id] === undefined) page_of_block[id] = pageNo;
      if (el.classList.contains("page-break")) page_breaks.push({ id, page: pageNo });
    });
  });
  return { page_count: pages.length, page_of_block, page_breaks };
}

function waitPagedDone(win: Window, timeoutMs: number): Promise<void> {
  return new Promise((resolve, reject) => {
    const start = Date.now();
    const w = win as unknown as { __azodocPagedDone?: boolean };
    const timer = setInterval(() => {
      if (w.__azodocPagedDone === true) {
        clearInterval(timer);
        resolve();
      } else if (Date.now() - start > timeoutMs) {
        clearInterval(timer);
        reject(new Error("分页超时"));
      }
    }, POLL_MS);
  });
}

export class PreviewSurface {
  private overlay: HTMLElement | null = null;
  private iframe: HTMLIFrameElement | null = null;
  private statusEl: HTMLElement | null = null;
  private printBtn: HTMLButtonElement | null = null;
  private pagedReady = false;
  private readonly timeoutMs: number;
  /** 打开代际：close/重开递增，迟到的异步结果作废（取消旧布局任务）。 */
  private openSeq = 0;
  private restoreFocus: HTMLElement | null = null;

  constructor(timeoutMs = DEFAULT_PAGED_TIMEOUT_MS) {
    this.timeoutMs = timeoutMs;
  }

  /** 打开预览浮层：载入 HTML → 等分页 → 展示页数与 LayoutIndex。 */
  async open(result: PreviewResult): Promise<PreviewOutcome> {
    this.close(); // 幂等：重开先清场
    const seq = ++this.openSeq;
    const stale = () => seq !== this.openSeq;
    this.restoreFocus = document.activeElement as HTMLElement | null;
    const overlay = document.createElement("div");
    overlay.className = "az-preview-overlay";
    overlay.setAttribute("role", "dialog");
    overlay.setAttribute("aria-modal", "true");
    overlay.setAttribute("aria-label", "打印预览");
    overlay.innerHTML = `
      <div class="az-preview-bar">
        <span class="az-preview-status">分页中…</span>
        <span class="az-preview-spacer"></span>
        <button class="az-preview-print" disabled>打印…</button>
        <button class="az-preview-close">关闭</button>
      </div>
      <div class="az-preview-stage"></div>`;
    document.body.appendChild(overlay);
    this.overlay = overlay;
    this.statusEl = overlay.querySelector(".az-preview-status");
    this.printBtn = overlay.querySelector(".az-preview-print");
    overlay.querySelector(".az-preview-close")?.addEventListener("click", () => this.close());
    overlay.addEventListener("keydown", (e) => {
      if (e.key === "Escape") this.close();
    });

    const stage = overlay.querySelector(".az-preview-stage")!;
    const iframe = document.createElement("iframe");
    iframe.className = "az-preview-frame";
    iframe.title = "打印预览";
    stage.appendChild(iframe);
    this.iframe = iframe;
    (overlay.querySelector(".az-preview-close") as HTMLElement | null)?.focus();

    const snap = result.snapshot;
    const hashShort = snap.content_hash.slice(0, 12);
    let outcome: PreviewOutcome = { layout: null, fallback: false, page_count: null };
    try {
      iframe.srcdoc = result.html;
      const win = iframe.contentWindow;
      if (!win) throw new Error("预览窗口不可用");
      await waitPagedDone(win, this.timeoutMs);
      if (stale()) return outcome; // 关闭/重开后迟到的分页完成不入局
      this.pagedReady = true;
      const doc = iframe.contentDocument;
      if (doc) {
        const layout = buildLayoutIndex(doc);
        outcome = { layout, fallback: false, page_count: layout.page_count };
        this.setStatus(
          `${layout.page_count} 页 · ${result.page_size} · 快照 ${hashShort}…（${snap.mode}）`,
        );
      }
    } catch {
      if (stale()) return outcome;
      // 分页不可用：未分页回退呈现（print_html 不含 Paged 增量，
      // 同一份印刷管线产物，DOM 未被部分分页污染）。
      iframe.srcdoc = result.print_html;
      outcome = { layout: null, fallback: true, page_count: null };
      this.setStatus(`分页不可用，按未分页预览 · 快照 ${hashShort}…`);
    }
    if (this.printBtn) {
      this.printBtn.disabled = false;
      this.printBtn.addEventListener("click", () => {
        // 系统打印入口：分页完成 → 打印 paged DOM；回退 → 打印原 DOM。
        this.iframe?.contentWindow?.print();
      });
    }
    return outcome;
  }

  private setStatus(text: string): void {
    if (this.statusEl) this.statusEl.textContent = text;
  }

  get isPaged(): boolean {
    return this.pagedReady;
  }

  close(): void {
    this.openSeq += 1; // 在途分页结果作废
    this.overlay?.remove();
    this.overlay = null;
    this.iframe = null;
    this.statusEl = null;
    this.printBtn = null;
    this.pagedReady = false;
    // 焦点还给打开前的元素（模态关闭的键盘可达性）
    const restore = this.restoreFocus;
    this.restoreFocus = null;
    restore?.focus?.();
  }
}
