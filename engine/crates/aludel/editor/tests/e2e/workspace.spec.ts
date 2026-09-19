import { test, expect } from "@playwright/test";

test("page settings apply, save and preserve imported theme fields", async ({ page }) => {
  const theme = { schema_version: "1.0", theme: "original", styles: [{ name: "heading" }] };
  await page.route("**/api/doc", (route) => route.fulfill({ json: {
    path: "report.azodoc", fingerprint: "initial", revision: null,
    pm_doc: { type: "doc", content: [{ type: "paragraph", content: [{ type: "text", text: "报告" }] }] },
    theme, annotations: { annotations: [] }, history: [], warnings: [],
    loss_summary: { unknown_blocks: [], detached_annotations: 0 },
  } }));
  let saved: Record<string, unknown> | undefined;
  await page.route("**/api/save", (route) => {
    saved = route.request().postDataJSON();
    return route.fulfill({ json: { ok: true, fingerprint: "saved", revision: "revision" } });
  });
  await page.goto("/");
  await expect(page.locator(".ProseMirror")).toContainText("报告");
  await page.locator("#tab-layout").click();
  await page.getByRole("button", { name: "页面设置…", exact: true }).click();
  await expect(page.getByRole("dialog")).toBeVisible();
  const dialog = page.getByRole("dialog");
  await dialog.locator("select").nth(0).selectOption("Letter");
  await dialog.locator("select").nth(1).selectOption("landscape");
  await dialog.locator("input[type=number]").nth(0).fill("30");
  await page.getByRole("button", { name: "应用", exact: true }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await page.getByRole("button", { name: "保存", exact: true }).click();
  await expect.poll(() => saved?.theme).toMatchObject({
    ...theme, defaults: { page: { size: "Letter landscape", margin: "30mm 18mm 22mm 18mm" } },
  });
  await expect(page.locator("#sb-save")).not.toHaveClass(/dirty/);
});
