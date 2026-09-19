import { test, expect } from "@playwright/test";

/** 挂载最小文档（mock /api/doc）。 */
async function mountDoc(page: import("@playwright/test").Page, content: unknown[]) {
  await page.route("**/api/doc", (route) =>
    route.fulfill({
      json: {
        path: "shell.azodoc",
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
  await page.goto("/");
  await expect(page.locator(".ProseMirror")).toBeVisible();
}

const PARA = { type: "paragraph", attrs: { id: "blk_1", extra: null }, content: [{ type: "text", text: "x" }] };

// 视觉断点矩阵（计划 §8.2）：不断言像素，断言结构完整性 + 留截图基线工件。
for (const [width, height] of [
  [900, 600],
  [1280, 820],
  [1440, 900],
  [1920, 1080],
] as const) {
  test(`breakpoint ${width}x${height}: chrome intact, no horizontal scroll`, async ({ page }) => {
    await page.setViewportSize({ width, height });
    await mountDoc(page, [PARA]);
    const noScrollX = await page.evaluate(() => {
      const de = document.documentElement;
      return de.scrollWidth <= de.clientWidth + 1;
    });
    expect(noScrollX, `${width}x${height} 不得整页横向滚动`).toBe(true);
    // 功能区/菜单/状态栏/正文在所有断点都可达
    await expect(page.locator("#ribbon-panel")).toBeVisible();
    await expect(page.locator(".menubar")).toBeVisible();
    await expect(page.locator(".statusbar")).toBeVisible();
    await expect(page.locator("#editor")).toBeVisible();
    // 截图基线：存为测试工件，供人工差异审阅（不做像素盲比）
    await page.screenshot({ path: `test-results/baseline-${width}x${height}.png`, fullPage: false });
  });
}

test("breakpoint 1280x820 dark theme: chrome intact", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 820 });
  await mountDoc(page, [PARA]);
  await page.locator("#tab-view").click();
  await page.locator("[data-cmd='view.theme']").first().click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  const noScrollX = await page.evaluate(() => {
    const de = document.documentElement;
    return de.scrollWidth <= de.clientWidth + 1;
  });
  expect(noScrollX).toBe(true);
  await page.screenshot({ path: "test-results/baseline-1280x820-dark.png" });
});

test("DPR 2 at 1280x820: chrome intact, no horizontal scroll", async ({ browser }) => {
  const context = await browser.newContext({ viewport: { width: 1280, height: 820 }, deviceScaleFactor: 2 });
  const page = await context.newPage();
  await mountDoc(page, [PARA]);
  const noScrollX = await page.evaluate(() => {
    const de = document.documentElement;
    return de.scrollWidth <= de.clientWidth + 1;
  });
  expect(noScrollX).toBe(true);
  await expect(page.locator("#ribbon-panel")).toBeVisible();
  await page.screenshot({ path: "test-results/baseline-1280x820-dpr2.png" });
  await context.close();
});

test("ARIA contract: tablist/tab/tabpanel, menubar/menuitem, status, dialog", async ({ page }) => {
  await mountDoc(page, [PARA]);
  // ribbon: tablist + tab + tabpanel 关联
  await expect(page.locator("#ribbon-tabs")).toHaveAttribute("role", "tablist");
  await expect(page.locator("#tab-home")).toHaveAttribute("role", "tab");
  await expect(page.locator("#tab-home")).toHaveAttribute("aria-controls", "ribbon-panel");
  await expect(page.locator("#ribbon-panel")).toHaveAttribute("role", "tabpanel");
  // 应用菜单：menubar 容器；打开后有 menu/menuitem
  await expect(page.locator("#app-menus")).toHaveAttribute("role", "menubar");
  await page.locator("#menu-format").click();
  const menu = page.locator(".az-menu[role='menu']").first();
  await expect(menu).toBeVisible();
  await expect(menu.locator("[role='menuitem']").first()).toBeVisible();
  await page.keyboard.press("Escape");
  // 状态栏语义
  await expect(page.locator(".statusbar")).toHaveAttribute("role", "status");
  // 编辑器角色
  await expect(page.locator("#editor")).toHaveAttribute("role", "textbox");
});

test("keyboard: arrow keys move focus without executing; Enter executes", async ({ page }) => {
  await mountDoc(page, [PARA]);
  const tab = page.locator("#tab-home");
  await tab.focus();
  // ArrowRight 只移动焦点，不执行任何命令（文档不变、无菜单弹出）
  await page.keyboard.press("ArrowRight");
  const focused = await page.evaluate(() => document.activeElement?.id);
  expect(focused).toBe("tab-insert");
  await expect(page.locator("#ribbon-panel")).toHaveAttribute("aria-labelledby", "tab-insert");
  await expect(page.locator(".az-menu")).toHaveCount(0);
  // Enter 才执行：在聚焦的菜单触发器上 Enter 打开菜单
  await page.locator("#menu-format").focus();
  await page.keyboard.press("Enter");
  await expect(page.locator(".az-menu[role='menu']")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.locator(".az-menu")).toHaveCount(0);
});
