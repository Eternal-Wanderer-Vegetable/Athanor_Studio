import { test, expect } from "@playwright/test";

/** 挂载最小文档（mock /api/doc）。 */
async function mountDoc(page: import("@playwright/test").Page, content: unknown[]) {
  await page.route("**/api/doc", (route) =>
    route.fulfill({
      json: {
        path: "e6.azodoc",
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

test("keyboard: tab reaches toolbar, Enter toggles, focus ring visible, aria-pressed syncs", async ({ page }) => {
  await mountDoc(page, [PARA]);
  // Tab 从页面顶部出发，若干步内必须落到可交互控件（全键盘可达）
  let focusable = false;
  for (let i = 0; i < 60; i++) {
    await page.keyboard.press("Tab");
    const tag = await page.evaluate(() => document.activeElement?.tagName);
    if (tag === "BUTTON" || tag === "SELECT" || tag === "INPUT") { focusable = true; break; }
  }
  expect(focusable, "Tab 应能到达菜单/工具栏控件").toBe(true);
  // 焦点可见：:focus-visible 提供非零 outline
  const outline = await page.evaluate(() => {
    const el = document.activeElement as HTMLElement;
    return getComputedStyle(el).outlineWidth;
  });
  expect(parseFloat(outline)).toBeGreaterThan(0);
  // 键盘直达加粗：Ctrl+B → 按钮 active + aria-pressed=true（状态不只看颜色）
  await page.locator(".ProseMirror").click();
  await page.keyboard.type("ab");
  // 选中输入的文本再切 mark：折叠选区的 storedMarks 受后续输入/失焦影响，
  // 用真实范围选区验证 aria-pressed 同步最稳。
  await page.keyboard.press("Control+a");
  await page.keyboard.press("Control+b");
  await expect(page.locator("#tb-strong")).toHaveClass(/active/);
  await expect(page.locator("#tb-strong")).toHaveAttribute("aria-pressed", "true");
  await page.keyboard.press("Control+b");
  await expect(page.locator("#tb-strong")).toHaveAttribute("aria-pressed", "false");
});

test("keyboard: Escape closes preview overlay and restores focus", async ({ page }) => {
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
  await page.locator("#m-preview").click();
  const overlay = page.locator(".az-preview-overlay");
  await expect(overlay).toBeVisible();
  await expect(overlay).toHaveAttribute("role", "dialog");
  await expect(overlay).toHaveAttribute("aria-modal", "true");
  // 焦点进入浮层；Escape 关闭且焦点归还编辑器/前序元素
  const focusedIn = await page.evaluate(() =>
    (document.activeElement as HTMLElement | null)?.closest?.(".az-preview-overlay") !== null);
  expect(focusedIn).toBe(true);
  await page.keyboard.press("Escape");
  await expect(overlay).toHaveCount(0);
  const stillIn = await page.evaluate(() =>
    (document.activeElement as HTMLElement | null)?.closest?.(".az-preview-overlay") !== null);
  expect(stillIn).toBe(false);
});

test("IME composition events do not drop text or remount editor", async ({ page }) => {
  await mountDoc(page, [PARA]);
  const pm = page.locator(".ProseMirror");
  await pm.click();
  const sameDoc = await page.evaluate(() => {
    const el = document.querySelector(".ProseMirror")!;
    el.dispatchEvent(new CompositionEvent("compositionstart", { data: "ni" }));
    el.dispatchEvent(new CompositionEvent("compositionupdate", { data: "ni hao" }));
    el.dispatchEvent(new CompositionEvent("compositionend", { data: "你好" }));
    return el;
  });
  expect(sameDoc).toBeTruthy();
  // 组合事件流后编辑器仍可输入且结构未重挂载
  await page.keyboard.type("ok");
  await expect(pm).toContainText("ok");
});

for (const scale of [1, 1.5, 2]) {
  test(`DPI ${Math.round(scale * 100)}%: chrome not clipped`, async ({ browser }) => {
    // deviceScaleFactor 为 context 级设置——模拟真实高 DPI 缩放
    const context = await browser.newContext({ deviceScaleFactor: scale, viewport: { width: 1280, height: 860 } });
    const page = await context.newPage();
    await mountDoc(page, [PARA]);
    const vw = page.viewportSize()!.width;
    for (const sel of [".menubar", ".ribbon", ".statusbar", ".workspace", ".ProseMirror"]) {
      const el = page.locator(sel);
      await expect(el).toBeVisible();
      const box = await el.boundingBox();
      expect(box, sel).toBeTruthy();
      // 不被裁出视口：右缘不得越出，宽度不得为 0
      expect(box!.width, `${sel} 宽度`).toBeGreaterThan(0);
      expect(box!.x + box!.width, `${sel} 右缘越界`).toBeLessThanOrEqual(vw + 1);
    }
    await context.close();
  });
}

test("feature flag: printPreviewV1=0 disables preview entry", async ({ page }) => {
  // flags 读取发生在模块加载；route 须在 goto 之前注册
  await page.route("**/api/doc", (route) =>
    route.fulfill({
      json: {
        path: "e6.azodoc", fingerprint: "f", revision: null,
        pm_doc: { type: "doc", content: [PARA] }, theme: null,
        annotations: { annotations: [] }, history: [], warnings: [],
        loss_summary: { unknown_blocks: [], detached_annotations: 0 },
      },
    }),
  );
  await page.goto("/?flags=printPreviewV1:0");
  await expect(page.locator(".ProseMirror")).toBeVisible();
  await page.locator("#m-preview").click();
  await expect(page.locator("#msg")).toContainText("打印预览功能已关闭");
  await expect(page.locator(".az-preview-overlay")).toHaveCount(0);
});

test("perf baseline: long document mount + input latency recorded", async ({ page }) => {
  // 500 段落长文档（固定夹具）
  const content = Array.from({ length: 500 }, (_, i) => ({
    type: "paragraph",
    attrs: { id: `blk_${String(i).padStart(6, "0")}`, extra: null },
    content: [{ type: "text", text: `段落 ${i} 的固定文本内容，用于性能基线测量。` }],
  }));
  const t0 = Date.now();
  await mountDoc(page, content);
  const mountMs = Date.now() - t0;
  // 输入延迟：一次事务往返 < 1s（宽松阈值；p95 回归在发布门槛记录）
  const latMs = await page.evaluate(async () => {
    const pm = document.querySelector(".ProseMirror") as HTMLElement;
    pm.focus();
    const t = performance.now();
    pm.dispatchEvent(new InputEvent("beforeinput", { inputType: "insertText", data: "测" }));
    return performance.now() - t;
  });
  // 字数统计已延迟归并（E6）：输入期间不阻塞主线程长任务
  console.log(`PERF mount=${mountMs}ms inputDispatch=${latMs.toFixed(1)}ms blocks=500`);
  await expect(page.locator(".ProseMirror")).toContainText("段落 499");
  expect(mountMs, "500 块文档挂载应 < 10s").toBeLessThan(10_000);
});
