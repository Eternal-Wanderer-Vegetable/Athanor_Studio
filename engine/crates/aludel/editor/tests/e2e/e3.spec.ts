import { test, expect } from "@playwright/test";

/** 挂载一个含公式+文本的文档（mock /api/doc）。 */
async function mountDoc(page: import("@playwright/test").Page, content: unknown[]) {
  await page.route("**/api/doc", (route) =>
    route.fulfill({
      json: {
        path: "e3.azodoc",
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

/** 向编辑器派发合成 paste 事件（构造 DataTransfer + ClipboardEvent）。 */
async function paste(page: import("@playwright/test").Page, html: string, text = "") {
  await page.locator(".ProseMirror").click();
  await page.evaluate(
    ([h, t]) => {
      const dt = new DataTransfer();
      if (h) dt.setData("text/html", h);
      if (t) dt.setData("text/plain", t);
      const ev = new ClipboardEvent("paste", { clipboardData: dt, bubbles: true, cancelable: true });
      document.querySelector(".ProseMirror")!.dispatchEvent(ev);
    },
    [html, text],
  );
}

test("math_block renders via KaTeX, invalid latex falls back to source", async ({ page }) => {
  await mountDoc(page, [
    {
      type: "math_block",
      attrs: { id: "blk_1", latex: "E=mc^2", extra: null },
    },
    {
      type: "paragraph",
      attrs: { id: "blk_2", extra: null },
      content: [
        { type: "text", text: "前 " },
        { type: "inline_math", attrs: { latex: "\\alpha+1", extra: null } },
        { type: "text", text: " 后" },
      ],
    },
    {
      type: "math_block",
      attrs: { id: "blk_3", latex: "\\broken{", extra: null },
    },
  ]);
  // 合法公式 → KaTeX 节点；非法 → 源码回退（不消失、不抛错）
  await expect(page.locator(".az-math .katex")).toHaveCount(1);
  await expect(page.locator(".az-inline-math .katex")).toHaveCount(1);
  await expect(page.locator(".az-math[data-error]")).toContainText("\\broken{");
});

test("paste sanitizes scripts/handlers/dangerous urls, keeps whitelist", async ({ page }) => {
  await mountDoc(page, [
    { type: "paragraph", attrs: { id: "blk_1", extra: null }, content: [{ type: "text", text: "X" }] },
  ]);
  await paste(
    page,
    `<p onclick="evil()">hi <strong>b</strong></p>
     <script>alert(1)</script>
     <iframe src="https://evil"></iframe>
     <a href="javascript:alert(1)">bad</a>
     <a href="https://ok">good</a>`,
    "fallback",
  );
  const pm = page.locator(".ProseMirror");
  await expect(pm).toContainText("hi");
  await expect(pm).toContainText("good");
  await expect(pm.locator("strong")).toHaveText("b");
  // script/iframe/javascript: 不得出现
  await expect(pm.locator("script")).toHaveCount(0);
  await expect(pm.locator("iframe")).toHaveCount(0);
  await expect(pm.locator("a[href^='javascript:']")).toHaveCount(0);
  // 危险 href 被剥（文本留）
  await expect(pm.locator("a")).not.toHaveCount(0);
});

test("footnote ref click jumps to definition", async ({ page }) => {
  page.on("console", (m) => {
    if (m.type() === "error") console.log("[browser]", m.text());
  });
  page.on("pageerror", (e) => console.log("[pageerror]", e.message));
  await mountDoc(page, [
    {
      type: "paragraph",
      attrs: { id: "blk_1", extra: null },
      content: [
        { type: "text", text: "正文" },
        { type: "footnote_ref", attrs: { id: "blk_N", extra: null } },
      ],
    },
    {
      type: "footnote",
      attrs: { id: "blk_N", extra: null },
      content: [
        {
          type: "paragraph",
          attrs: { id: "blk_2", extra: null },
          content: [{ type: "text", text: "注文" }],
        },
      ],
    },
  ]);
  // 引用可点 → 跳到定义（定义块含回跳符）
  await page.locator(".az-footnote-ref").click();
  await expect(page.locator(".az-footnote")).toContainText("注文");
  await expect(page.locator(".az-footnote-back")).toBeVisible();
});
