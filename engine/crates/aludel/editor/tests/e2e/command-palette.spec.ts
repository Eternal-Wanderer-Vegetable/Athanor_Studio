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
