import { test, expect } from "@playwright/test";

/** 挂载最小文档（mock /api/doc）。 */
async function mountDoc(page: import("@playwright/test").Page, content: unknown[]) {
  await page.route("**/api/doc", (route) =>
    route.fulfill({
      json: {
        path: "ribbon.azodoc",
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

const TABLE_DOC = [
  {
    type: "table",
    attrs: {
      id: "blk_t",
      extra: null,
      columns: [
        { id: "col_1", name: "列 1" },
        { id: "col_2", name: "列 2" },
      ],
    },
    content: [
      {
        type: "table_row",
        attrs: { id: "row_1", extra: null },
        content: [
          {
            type: "table_cell",
            attrs: { id: "cel_1", column: 0, extra: null },
            content: [{ type: "paragraph", attrs: { id: "blk_a", extra: null }, content: [{ type: "text", text: "a" }] }],
          },
          {
            type: "table_cell",
            attrs: { id: "cel_2", column: 1, extra: null },
            content: [{ type: "paragraph", attrs: { id: "blk_b", extra: null }, content: [{ type: "text", text: "b" }] }],
          },
        ],
      },
    ],
  },
];

test("ribbon tabs switch panels: home/insert/view/review", async ({ page }) => {
  await mountDoc(page, [PARA]);
  // 默认开始页签：加粗可见
  await expect(page.locator("#ribbon-tabs [role=tab][aria-selected=true]")).toHaveText("开始");
  await expect(page.locator("#tb-strong")).toBeVisible();
  await expect(page.locator("#tb-page-break")).toHaveCount(0);
  // 插入页签：分页符出现，加粗消失
  await page.locator("#tab-insert").click();
  await expect(page.locator("#tb-page-break")).toBeVisible();
  await expect(page.locator("#tb-strong")).toHaveCount(0);
  // 视图页签：预览/页面设置
  await page.locator("#tab-view").click();
  await expect(page.locator("#m-preview")).toBeVisible();
  // 文档检查页签：检查文档命令
  await page.locator("#tab-review").click();
  await expect(page.locator("[data-cmd='file.verify']")).toBeVisible();
});

test("contextual Table tab appears only when selection is inside a table", async ({ page }) => {
  await mountDoc(page, [PARA, ...TABLE_DOC]);
  const tab = page.locator("#tab-table");
  // 光标在普通段落：上下文页签不可见
  await page.locator(".ProseMirror p").first().click();
  await expect(tab).toBeHidden();
  // 光标进入表格单元格：页签出现并被选中
  await page.locator(".ProseMirror td").first().click();
  await expect(tab).toBeVisible();
  await expect(tab).toHaveAttribute("aria-selected", "true");
  await expect(page.locator("#tb-row-add")).toBeVisible();
  // 离开表格：页签消失，回到之前页签
  await page.locator(".ProseMirror > p").first().click();
  await expect(tab).toBeHidden();
  await expect(page.locator("#ribbon-tabs [role=tab][aria-selected=true]")).not.toHaveText("表格");
});

test("ribbon collapse toggles panel and persists preference", async ({ page }) => {
  await mountDoc(page, [PARA]);
  const panel = page.locator("#ribbon-panel");
  await expect(panel).toBeVisible();
  await page.locator("#ribbon-collapse").click();
  await expect(panel).toHaveClass(/collapsed/);
  // 偏好持久化：重载后仍折叠
  await page.reload();
  await expect(page.locator("#ribbon-panel")).toHaveClass(/collapsed/);
  // 点页签自动展开（Word 行为）
  await page.locator("#tab-insert").click();
  await expect(panel).not.toHaveClass(/collapsed/);
  await expect(page.locator("#tb-page-break")).toBeVisible();
});

test("ribbon overflow collects low-priority groups into 更多 menu without horizontal scroll", async ({ page }) => {
  await page.setViewportSize({ width: 760, height: 700 });
  await mountDoc(page, [PARA]);
  // 溢出按钮出现，功能区无横向滚动
  await expect(page.locator("#ribbon-overflow")).toBeVisible();
  const noScroll = await page.evaluate(() => {
    const r = document.getElementById("ribbon-panel")!;
    return r.scrollWidth <= r.clientWidth + 1;
  });
  expect(noScroll).toBe(true);
  // 溢出组里的命令仍可达
  await page.locator("#ribbon-overflow").click();
  const menu = page.locator(".ribbon-overflow-menu");
  await expect(menu).toBeVisible();
  await expect(menu.locator("[data-cmd]").first()).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(menu).toHaveCount(0);
});

test("floating formatting bar appears on text selection and hides on collapse", async ({ page }) => {
  await mountDoc(page, [PARA, { type: "paragraph", attrs: { id: "blk_2", extra: null }, content: [{ type: "text", text: "选择我" }] }]);
  const bar = page.locator("#floating-bar");
  await expect(bar).toHaveCount(0);
  const pm = page.locator(".ProseMirror");
  await pm.click();
  await page.keyboard.press("Control+a");
  await expect(bar).toBeVisible();
  await expect(bar.locator("[data-cmd='format.strong']")).toBeVisible();
  // 点击浮动条加粗 → 正文 mark 生效，选区不丢（两段选区 → 两个 strong run）
  await bar.locator("[data-cmd='format.strong']").click();
  await expect(pm.locator("strong")).toHaveCount(2);
  // 折叠选区 → 浮动条消失
  await page.keyboard.press("ArrowRight");
  await expect(bar).toHaveCount(0);
});

test("panel toggles: outline rail button and inspector toggle", async ({ page }) => {
  await mountDoc(page, [PARA]);
  const left = page.locator("#left-panel");
  const right = page.locator("#right-panel");
  await expect(left).toBeVisible();
  await expect(right).toBeVisible();
  await page.locator("#rail-outline").click();
  await expect(left).toHaveClass(/closed/);
  await page.locator("#tab-view").click();
  await page.locator("[data-cmd='view.toggleInspector']").first().click();
  await expect(right).toHaveClass(/closed/);
});
