import { test, expect } from "@playwright/test";

/** 挂载最小文档（mock /api/doc）。 */
async function mountDoc(page: import("@playwright/test").Page, content: unknown[]) {
  await page.route("**/api/doc", (route) =>
    route.fulfill({
      json: {
        path: "resp.azodoc",
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

for (const width of [1280, 900, 768]) {
  test(`viewport ${width}px: no horizontal scroll, chrome reachable`, async ({ page }) => {
    await page.setViewportSize({ width, height: 760 });
    await mountDoc(page, [PARA]);
    const noScrollX = await page.evaluate(() => {
      const de = document.documentElement;
      return de.scrollWidth <= de.clientWidth + 1;
    });
    expect(noScrollX, `${width}px 整页不得横向滚动`).toBe(true);
    const ribbonScroll = await page.evaluate(() => {
      const r = document.getElementById("ribbon-panel")!;
      return r.scrollWidth <= r.clientWidth + 1;
    });
    expect(ribbonScroll, `${width}px 功能区不得横向滚动`).toBe(true);
    // 所有页签与命令面板可达
    for (const tab of ["home", "insert", "view", "review"]) {
      await expect(page.locator(`#tab-${tab}`)).toBeVisible();
    }
    await page.keyboard.press("Control+k");
    await expect(page.locator("#cmdk-overlay")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.locator("#cmdk-overlay")).toHaveCount(0);
  });
}

test("narrow viewport: panels become overlay drawers, not vanished", async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 760 });
  await mountDoc(page, [PARA]);
  // 窄屏下面板不消失：默认开着的大纲面板覆盖在工作区上（absolute 抽屉）
  const left = page.locator("#left-panel");
  await expect(left).toBeVisible();
  const pos = await left.evaluate((el) => getComputedStyle(el).position);
  expect(pos).toBe("absolute");
  // rail 按钮仍可开合它
  await page.locator("#rail-outline").click();
  await expect(left).toHaveClass(/closed/);
  await page.locator("#rail-outline").click();
  await expect(left).not.toHaveClass(/closed/);
});

test("theme toggle applies dark chrome and persists", async ({ page }) => {
  await mountDoc(page, [PARA]);
  await page.locator("#tab-view").click();
  await page.locator("[data-cmd='view.theme']").first().click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await page.reload();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await page.locator("#tab-view").click();
  await page.locator("[data-cmd='view.theme']").first().click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
});

test("status bar shows chars/save/zoom; page count only after successful preview", async ({ page }) => {
  await mountDoc(page, [PARA]);
  await expect(page.locator("#sb-chars")).toContainText("字");
  await expect(page.locator("#sb-zoom")).toHaveText("100%");
  // 未预览过：不显示页数（不把连续布局当 Word 分页）
  await expect(page.locator("#sb-page")).toHaveText("");
});
