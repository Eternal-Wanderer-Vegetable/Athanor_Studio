import { test, expect } from "@playwright/test";

/** 挂载最小文档（mock /api/doc）。宽视口：段落/样式组不落入“更多”溢出，
 *  picker 控件保持可见可交互。 */
async function mountDoc(page: import("@playwright/test").Page, content: unknown[]) {
  await page.setViewportSize({ width: 1920, height: 900 });
  await page.route("**/api/doc", (route) =>
    route.fulfill({
      json: {
        path: "editing.azodoc",
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

const PARA = { type: "paragraph", attrs: { id: "blk_1", extra: null }, content: [{ type: "text", text: "abc" }] };

/** 样式选择器的可达路径：组在 ribbon 里可见 → tb-style；被溢出收走 →
 *  “更多”菜单里的 ov-style（picker 必须有溢出路径，不能消失）。 */
async function stylePicker(page: import("@playwright/test").Page) {
  const direct = page.locator("#tb-style");
  if (await direct.isVisible()) return direct;
  await page.locator("#ribbon-overflow").click();
  const ov = page.locator("#ov-style");
  await expect(ov).toBeVisible();
  return ov;
}

test("style picker applies heading and reflects block type", async ({ page }) => {
  await mountDoc(page, [PARA]);
  await page.locator(".ProseMirror").click();
  const picker = await stylePicker(page);
  await picker.selectOption("heading2");
  await expect(page.locator(".ProseMirror h2")).toHaveCount(1);
  // 反射回读：picker 现在显示 heading2（同一 SelectionContext）
  await expect(page.locator("#tb-style")).toHaveValue("heading2");
  const picker2 = await stylePicker(page);
  await picker2.selectOption("paragraph");
  await expect(page.locator(".ProseMirror h2")).toHaveCount(0);
  await expect(page.locator("#tb-style")).toHaveValue("paragraph");
});

test("line-height picker writes paragraph format through one transaction path", async ({ page }) => {
  await mountDoc(page, [PARA]);
  await page.locator(".ProseMirror").click();
  await page.locator("#tb-lineheight").selectOption("1.5");
  // 反射回读证明写入进入 SelectionContext.paragraph.values.lineHeight
  await expect(page.locator("#tb-lineheight")).toHaveValue("1.5");
  // DOM 落点：段落行高样式
  await expect(page.locator(".ProseMirror [style*='line-height']").first()).toBeVisible();
});

test("superscript command toggles via ribbon button", async ({ page }) => {
  await mountDoc(page, [PARA]);
  await page.locator(".ProseMirror").click();
  await page.keyboard.press("Control+a");
  await page.locator("[data-cmd='format.superscript']").first().click();
  await expect(page.locator(".ProseMirror [style*='vertical-align']").first()).toBeVisible();
  // 再点一次撤销切换（active 态读 SelectionContext.character）
  await page.locator("[data-cmd='format.superscript']").first().click();
  await expect(page.locator(".ProseMirror [style*='vertical-align']")).toHaveCount(0);
});

test("status bar view-mode switch: pageWidth activates and restores", async ({ page }) => {
  await mountDoc(page, [PARA]);
  await expect(page.locator("#sb-mode")).toHaveText("连续编辑");
  await page.locator("#sb-view-pagewidth").click();
  await expect(page.locator("#sb-mode")).toHaveText("页宽视图");
  await expect(page.locator("#sb-view-pagewidth")).toHaveAttribute("aria-pressed", "true");
  await page.locator("#sb-view-continuous").click();
  await expect(page.locator("#sb-mode")).toHaveText("连续编辑");
  await expect(page.locator("#sb-view-continuous")).toHaveAttribute("aria-pressed", "true");
});
