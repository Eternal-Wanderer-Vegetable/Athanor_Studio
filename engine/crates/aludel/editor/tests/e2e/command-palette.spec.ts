import { test, expect } from "@playwright/test";

/** 挂载最小文档（mock /api/doc）。 */
async function mountDoc(page: import("@playwright/test").Page, content: unknown[]) {
  await page.route("**/api/doc", (route) =>
    route.fulfill({
      json: {
        path: "cmdk.azodoc",
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

test("Ctrl+K opens palette, search filters, Enter executes, Escape restores focus", async ({ page }) => {
  await mountDoc(page, [PARA]);
  await page.locator(".ProseMirror").click();
  await page.keyboard.press("Control+k");
  const overlay = page.locator("#cmdk-overlay");
  await expect(overlay).toBeVisible();
  await expect(page.locator("#cmdk-search")).toBeFocused();
  // 搜索“保存” → 候选含 file.save，Enter 执行（HTTP mock 会收 /api/save）
  await page.locator("#cmdk-search").fill("保存");
  await expect(page.locator("#cmdk-list li").first()).toContainText("保存");
  await page.keyboard.press("Enter");
  await expect(overlay).toHaveCount(0);
  await expect(page.locator("#sb-save")).toHaveText("已保存");
});

test("palette hides capability-absent commands and explains disabled ones", async ({ page }) => {
  await mountDoc(page, [PARA]);
  await page.locator("#cmdk-input").click();
  await expect(page.locator("#cmdk-overlay")).toBeVisible();
  await page.locator("#cmdk-search").fill("另存为");
  // HTTP 模式无文件对话框能力：桌面专属命令不渲染入口（不出现死按钮）
  await expect(page.locator("#cmdk-list li")).toHaveCount(0);
  // 上下文命令仍出现但禁用并给出原因
  await page.locator("#cmdk-search").fill("合并单元格");
  const merge = page.locator("#cmdk-list li").first();
  await expect(merge).toHaveClass(/disabled/);
  await expect(merge).toContainText("光标需在表格内");
  await page.keyboard.press("Escape");
  await expect(page.locator("#cmdk-overlay")).toHaveCount(0);
});

test("topbar search box opens palette on click", async ({ page }) => {
  await mountDoc(page, [PARA]);
  await page.locator("#cmdk-input").click();
  await expect(page.locator("#cmdk-overlay")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.locator("#cmdk-overlay")).toHaveCount(0);
});

test("application menu: arrow-key navigation, Enter executes, Escape restores focus", async ({ page }) => {
  await mountDoc(page, [PARA]);
  await page.locator("#menu-edit").click();
  const menu = page.locator(".az-menu");
  await expect(menu).toBeVisible();
  // 菜单项带 role=menuitem（与 button 区分），首项已聚焦
  await expect(menu.locator("[role=menuitem]").first()).toHaveClass(/focus/);
  // ArrowDown 移动到“重做”，Enter 执行
  await page.keyboard.press("ArrowDown");
  await expect(menu.locator("[role=menuitem]").nth(1)).toHaveClass(/focus/);
  await page.keyboard.press("Escape");
  await expect(menu).toHaveCount(0);
  // 焦点回触发按钮
  await expect(page.locator("#menu-edit")).toBeFocused();
  // 重开菜单执行命令（撤销仍走 registry，焦点回正文）
  await page.locator("#menu-edit").click();
  await expect(page.locator(".az-menu")).toBeVisible();
  await page.locator(".az-menu [data-cmd='edit.undo']").click();
  await expect(page.locator(".az-menu")).toHaveCount(0);
  await expect(page.locator(".ProseMirror")).toBeFocused();
});

test("shortcut Ctrl+P triggers preview command via registry projection", async ({ page }) => {
  await mountDoc(page, [PARA]);
  await page.route("**/api/preview", (route) =>
    route.fulfill({
      json: {
        html: `<!doctype html><html><head><script>window.__azodocPagedDone=true;</script></head><body><div class="pagedjs_page"><p data-block-id="blk_1">x</p></div></body></html>`,
        print_html: "<!doctype html><html><body>p</body></html>",
        page_size: "A4 portrait",
        snapshot: { content_hash: "a".repeat(64), theme_hash: "b".repeat(64), layout_hash: "c".repeat(64), mode: "pagedjs", css_version: "print-css-v3" },
      },
    }),
  );
  await page.locator(".ProseMirror").click();
  await page.keyboard.press("Control+p");
  await expect(page.locator(".az-preview-overlay")).toBeVisible();
  await expect(page.locator(".az-preview-status")).toContainText("1 页");
  // 状态栏同步最近预览页数（快照未失效；光标块在映射内 → 显示当前页）
  await expect(page.locator("#sb-page")).toContainText("第 1 / 1 页");
  await page.keyboard.press("Escape");
  await expect(page.locator(".az-preview-overlay")).toHaveCount(0);
});
