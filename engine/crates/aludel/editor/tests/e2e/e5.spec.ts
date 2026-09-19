import { test, expect } from "@playwright/test";

const SHA = "0".repeat(64);

/** 挂载最小文档（mock /api/doc），history 可注入。 */
async function mountDoc(
  page: import("@playwright/test").Page,
  opts: { content?: unknown[]; history?: unknown[]; theme?: unknown } = {},
) {
  await page.route("**/api/doc", (route) =>
    route.fulfill({
      json: {
        path: "e5.azodoc",
        fingerprint: "f",
        revision: "rev_B",
        pm_doc: { type: "doc", content: opts.content ?? [
          { type: "paragraph", attrs: { id: "blk_1", extra: null }, content: [{ type: "text", text: "一" }] },
        ] },
        theme: opts.theme ?? { schema_version: "1.0", pageSize: "A4" },
        annotations: { annotations: [] },
        history: opts.history ?? [],
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

test("theme-only change marks document dirty and enters save flow", async ({ page }) => {
  await mountDoc(page);
  await expect(page.locator("#sb-save")).toHaveText("已保存");
  // 页面设置 → 纸张改 Letter → 应用
  await page.locator("#tab-layout").click();
  await page.locator("#m-page-settings").click();
  const dialog = page.locator(".page-settings-dialog");
  await expect(dialog).toBeVisible();
  await dialog.locator("select").first().selectOption("Letter");
  await dialog.locator("button[type=submit]").click();
  // theme-only 修改与正文同一 dirty 语义（E5）
  await expect(page.locator("#sb-save")).toHaveText("未保存");
  await expect(page.locator("#sb-save")).toHaveClass("dirty");
});

test("history sidebar offers restore for non-current revisions and marks theme snapshots", async ({ page }) => {
  await mountDoc(page, {
    history: [
      { id: "rev_A", author_type: "importer", author_id: "x", message: "导入", timestamp: null, is_current: false, is_head: false, theme_sha256: null },
      { id: "rev_B", author_type: "human", author_id: "u", message: "第二版", timestamp: null, is_current: true, is_head: true, theme_sha256: SHA },
    ],
  });
  const items = page.locator("#history li");
  await expect(items).toHaveCount(2);
  // 主题快照标记 ◈ 只出现在带 theme_sha256 的条目上
  await expect(items.nth(0)).toContainText("rev_B");
  await expect(items.nth(0)).toContainText("◈");
  await expect(items.nth(1)).toContainText("rev_A");
  await expect(items.nth(1)).not.toContainText("◈");
  // 当前修订无还原按钮；非当前有
  await expect(items.nth(0).locator(".hist-restore")).toHaveCount(0);
  await expect(items.nth(1).locator(".hist-restore")).toHaveCount(1);

  // 点击还原 → POST /api/checkout，返回带主题快照的文档并重挂载
  await page.route("**/api/checkout", (route) =>
    route.fulfill({
      json: {
        path: "e5.azodoc",
        fingerprint: "f2",
        revision: "rev_A",
        theme_restored: true,
        pm_doc: { type: "doc", content: [
          { type: "paragraph", attrs: { id: "blk_old", extra: null }, content: [{ type: "text", text: "旧内容" }] },
        ] },
        theme: { schema_version: "1.0", pageSize: "Letter" },
        annotations: { annotations: [] },
        history: [],
        warnings: [],
        loss_summary: { unknown_blocks: [], detached_annotations: 0 },
      },
    }),
  );
  await items.nth(1).locator(".hist-restore").click();
  await expect(page.locator("#msg")).toContainText("已还原修订 rev_A");
  await expect(page.locator(".ProseMirror")).toContainText("旧内容");
});

test("checkout without theme snapshot reports body-only history", async ({ page }) => {
  await mountDoc(page, {
    history: [
      { id: "rev_A", author_type: "importer", author_id: "x", message: "导入", timestamp: null, is_current: false, is_head: false, theme_sha256: null },
      { id: "rev_B", author_type: "human", author_id: "u", message: "第二版", timestamp: null, is_current: true, is_head: true, theme_sha256: SHA },
    ],
  });
  await page.route("**/api/checkout", (route) =>
    route.fulfill({
      json: {
        path: "e5.azodoc",
        fingerprint: "f2",
        revision: "rev_A",
        theme_restored: false,
        pm_doc: { type: "doc", content: [
          { type: "paragraph", attrs: { id: "blk_old", extra: null }, content: [{ type: "text", text: "旧" }] },
        ] },
        theme: { schema_version: "1.0", pageSize: "A4" },
        annotations: { annotations: [] },
        history: [],
        warnings: ["当前修订未记录表现层主题快照（仅正文历史）；主题继承自最近状态"],
        loss_summary: { unknown_blocks: [], detached_annotations: 0 },
      },
    }),
  );
  await page.locator("#history li").nth(1).locator(".hist-restore").click();
  await expect(page.locator("#msg")).toContainText("仅正文历史");
});
