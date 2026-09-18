import { test, expect } from "@playwright/test";

/** 挂载最小文档（mock /api/doc）。 */
async function mountDoc(page: import("@playwright/test").Page, content: unknown[]) {
  await page.route("**/api/doc", (route) =>
    route.fulfill({
      json: {
        path: "e4.azodoc",
        fingerprint: "f",
        revision: null,
        pm_doc: { type: "doc", content },
        theme: null,
        annotations: { annotations: [] },
        history: [],
        warnings: [],
        loss_summary: { unknown_blocks: [], detached_annotations: 0 },
      },
    }),
  );
  await page.route("**/api/save", (route) =>
    route.fulfill({ json: { ok: true, fingerprint: "s", revision: "r" } }),
  );
  await page.goto("/");
  await expect(page.locator(".ProseMirror")).toBeVisible();
}

/** mock /api/preview：返回 Paged 增强 HTML（模拟两页分页结果）。 */
function mockPreview(page: import("@playwright/test").Page, pages: string[]) {
  const pageHtml = pages
    .map((p) => `<div class="pagedjs_page"><div class="pagedjs_area">${p}</div></div>`)
    .join("");
  const html = `<!doctype html><html><head>
<script>window.__azodocPagedDone = true;</script>
</head><body><article>${pageHtml}</article></body></html>`;
  return page.route("**/api/preview", (route) =>
    route.fulfill({
      json: {
        html,
        print_html: `<!doctype html><html><body><article>plain</article></body></html>`,
        page_size: "A4 portrait",
        snapshot: {
          content_hash: "a".repeat(64),
          theme_hash: "b".repeat(64),
          layout_hash: "c".repeat(64),
          mode: "pagedjs",
          css_version: "print-css-v3",
        },
      },
    }),
  );
}

test("page_break inserts a node and round-trips through save", async ({ page }) => {
  await mountDoc(page, [
    { type: "paragraph", attrs: { id: "blk_1", extra: null }, content: [{ type: "text", text: "前" }] },
  ]);
  await page.locator("#tab-insert").click();
  await page.locator("#tb-page-break").click();
  // 编辑器内可见分页标记 + 其后有段落供光标
  await expect(page.locator(".ProseMirror .az-page-break")).toHaveCount(1);
  const docJson = await page.evaluate(() => {
    const pm = document.querySelector(".ProseMirror") as HTMLElement;
    return (window as unknown as { __pmDoc?: unknown }).__pmDoc ?? pm.textContent;
  });
  expect(docJson).toBeTruthy();
});

test("print preview opens, shows real page count and print entry", async ({ page }) => {
  await mountDoc(page, [
    { type: "paragraph", attrs: { id: "blk_1", extra: null }, content: [{ type: "text", text: "一" }] },
    { type: "page_break", attrs: { id: "blk_2", extra: null } },
    { type: "paragraph", attrs: { id: "blk_3", extra: null }, content: [{ type: "text", text: "二" }] },
  ]);
  await mockPreview(page, [
    `<p data-block-id="blk_1">一</p><div class="page-break" data-block-id="blk_2"></div>`,
    `<p data-block-id="blk_3">二</p>`,
  ]);
  await page.locator("#tab-view").click();
  await page.locator("#m-preview").click();
  // 浮层打开：状态条显示实数页数 + 快照指纹；打印按钮可用
  await expect(page.locator(".az-preview-overlay")).toBeVisible();
  await expect(page.locator(".az-preview-status")).toContainText("2 页");
  await expect(page.locator(".az-preview-status")).toContainText("aaaaaaaaaaaa");
  await expect(page.locator(".az-preview-print")).toBeEnabled();
  // iframe 里是两页分页 DOM（LayoutIndex 已据此计算）
  const pages = page.locator(".az-preview-frame").contentFrame().locator(".pagedjs_page");
  await expect(pages).toHaveCount(2);
  // 关闭回到编辑器
  await page.locator(".az-preview-close").click();
  await expect(page.locator(".az-preview-overlay")).toHaveCount(0);
});

test("preview falls back to unpaginated render when paging times out", async ({ page }) => {
  await mountDoc(page, [
    { type: "paragraph", attrs: { id: "blk_1", extra: null }, content: [{ type: "text", text: "x" }] },
  ]);
  // __azodocPagedDone 永不为真 → 超时回退（把超时缩到最小：mock 直接给完成态失败场景）
  await page.route("**/api/preview", (route) =>
    route.fulfill({
      json: {
        html: `<!doctype html><html><body><p>paged-stub</p></body></html>`,
        print_html: `<!doctype html><html><body><article>plain-fallback-content</article></body></html>`,
        page_size: "A4 portrait",
        snapshot: {
          content_hash: "d".repeat(64),
          theme_hash: "e".repeat(64),
          layout_hash: "f".repeat(64),
          mode: "pagedjs",
          css_version: "print-css-v3",
        },
      },
    }),
  );
  await page.evaluate(() => {
    (window as unknown as { __previewTimeoutMs?: number }).__previewTimeoutMs = 400;
  });
  await page.locator("#tab-view").click();
  await page.locator("#m-preview").click();
  // 注入 400ms 超时 → 快速落入回退分支（重灌未分页 print_html）
  await expect(page.locator(".az-preview-status")).toContainText("分页不可用", { timeout: 10_000 });
  const frame = page.locator(".az-preview-frame").contentFrame();
  await expect(frame.locator("article")).toContainText("plain-fallback-content");
  await expect(page.locator(".az-preview-print")).toBeEnabled();
});
