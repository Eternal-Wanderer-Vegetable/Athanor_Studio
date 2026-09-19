import { test, expect } from "@playwright/test";

/** 挂载最小文档（mock /api/doc）。 */
async function mountDoc(page: import("@playwright/test").Page, content: unknown[]) {
  await page.route("**/api/doc", (route) =>
    route.fulfill({
      json: {
        path: "nav.azodoc",
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

const PARA = { type: "paragraph", attrs: { id: "blk_1", extra: null }, content: [{ type: "text", text: "x" }] };

test("nav panel: comments section shows honest empty state, not fake list", async ({ page }) => {
  await mountDoc(page, [PARA]);
  await page.locator("#rail-comments").click();
  await expect(page.locator("#nav-section-comments")).toBeVisible();
  await expect(page.locator("#nav-section-comments")).toContainText("批注");
  // 切回大纲：outline 区仍可见、无不可达焦点残留
  await page.locator("#nav-tab-outline").click();
  await expect(page.locator("#nav-section-outline")).toBeVisible();
  await expect(page.locator("#nav-section-comments")).toBeHidden();
});

test("nav panel: pages section shows empty state without preview, list after preview", async ({ page }) => {
  await mountDoc(page, [PARA]);
  await page.locator("#rail-pages").click();
  await expect(page.locator("#nav-section-pages")).toBeVisible();
  await expect(page.locator("#nav-pages")).toContainText("暂无分页数据");
  // 预览一次（mock Paged.js 分页结果）后页面列表出现真实页码
  await page.route("**/api/preview", (route) =>
    route.fulfill({
      json: {
        html: `<!doctype html><html><head><script>window.__azodocPagedDone=true;</script></head><body><div class="pagedjs_page"><p data-block-id="blk_1">x</p></div><div class="pagedjs_page"><p data-block-id="blk_2">y</p></div></body></html>`,
        print_html: "<!doctype html><html><body>p</body></html>",
        page_size: "A4 portrait",
        snapshot: { content_hash: "a".repeat(64), theme_hash: "b".repeat(64), layout_hash: "c".repeat(64), mode: "pagedjs", css_version: "print-css-v3" },
      },
    }),
  );
  await page.locator("#sb-view-page").click();
  await expect(page.locator(".az-preview-overlay")).toBeVisible();
  // 等分页完成（否则 Escape 关闭会让迟到的布局结果作废）
  await expect(page.locator(".az-preview-status")).toContainText("2 页");
  await page.keyboard.press("Escape");
  await expect(page.locator(".az-preview-overlay")).toHaveCount(0);
  await expect(page.locator("#sb-page")).toContainText("页");
  await expect(page.locator("#nav-pages")).toContainText("第 1 页");
  await expect(page.locator("#nav-pages")).toContainText("第 2 页");
});
