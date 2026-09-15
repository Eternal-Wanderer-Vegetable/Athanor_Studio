// This file is part of Athanor, the Azodoc document engine.
// Copyright (C) 2026 The Athanor Studio Developers
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published
// by the Free Software Foundation, version 3 of the License only.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Paged.js / CDP 出版管线（Future Work §B1）。
//!
//! 流程：`augment_print_html` 注入 `PagedConfig`（auto + after 完成回调）与
//! polyfill 脚本 → CDP 打开页面等 load → 轮询 `window.__azodocPagedDone`
//! （从根上规避 CLI `--virtual-time-budget` 与异步分页的竞态）→ 读取分页
//! 统计 → `Page.printToPDF`。分页点由 Paged.js 决定，页码/运行头由
//! `@page` 边盒渲染。
//!
//! 第三方资产：`assets/paged.polyfill.js` —— Paged.js v0.4.3，MIT 许可
//! （https://gitlab.coko.foundation/pagedjs/pagedjs），以 include_str! 内嵌。

use serde_json::{json, Value};
use std::path::Path;
use std::time::Duration;

use crate::cdp;
use crate::{Browser, PdfError};

/// Paged.js polyfill（内嵌，运行时写到印刷 HTML 同目录）。
pub const PAGED_POLYFILL_JS: &str = include_str!("../assets/paged.polyfill.js");

/// Paged.js 模式印刷 CSS 增量：@page 边盒（页脚页码 + 运行头文档标题）。
pub const PAGED_CSS: &str = "@page { \
@bottom-center { content: counter(page); font-size: 9pt; color: #444; } \
@top-left { content: string(doctitle); font-size: 9pt; color: #444; } } \
.azodoc-doctitle { string-set: doctitle content(text); }";

/// Paged.js 渲染统计（来自分页后的 DOM 实数）。
#[derive(Debug, Clone)]
pub struct PagedRenderInfo {
    /// `.pagedjs_page` 页数
    pub page_count: u64,
    /// 首页页脚（页码边盒）文本样本，证明页码已渲染
    pub footer_sample: Option<String>,
}

/// 向印刷 HTML 注入 Paged.js 启动脚本、边盒 CSS 与运行头锚点元素。
pub fn augment_print_html(html: &str, title: Option<&str>) -> String {
    let mut inject = format!(
        "<script>window.PagedConfig = {{auto: true, after: () => {{ \
window.__azodocPagedDone = true; }}}};</script>\n\
<script src=\"./paged.polyfill.js\"></script>\n\
<style id=\"azodoc-paged\">{PAGED_CSS}</style>"
    );
    if let Some(t) = title {
        // 运行头取文档标题：隐藏锚点元素 + string-set（display:none 会被
        // Paged.js 跳过，故用 visibility:hidden 保留在布局里）
        inject.push_str(&format!(
            "\n<span class=\"azodoc-doctitle\" \
style=\"position:absolute;visibility:hidden;\">{}</span>",
            escape_html(t)
        ));
    }
    match html.rfind("</head>") {
        Some(pos) => {
            let mut out = String::with_capacity(html.len() + inject.len());
            out.push_str(&html[..pos]);
            out.push_str(&inject);
            out.push_str(&html[pos..]);
            out
        }
        None => format!("{html}\n{inject}"),
    }
}

/// 把 polyfill 写到印刷 HTML 同目录（`<script src="./paged.polyfill.js">` 需要）。
pub fn write_polyfill_assets(dir: &Path) -> std::io::Result<()> {
    std::fs::write(dir.join("paged.polyfill.js"), PAGED_POLYFILL_JS)
}

/// CDP + Paged.js 打印：等分页完成后再 `Page.printToPDF`。
pub fn print_html_to_pdf_paged(
    browser: &Browser,
    html_path: &Path,
    pdf_path: &Path,
    timeout: Duration,
) -> Result<PagedRenderInfo, PdfError> {
    let deadline = std::time::Instant::now() + timeout;

    let mut proc = cdp::launch_cdp(browser)?;
    let ws_url = cdp::wait_ws_url(&proc, deadline)?;
    let mut cdp = cdp::Cdp::connect(&ws_url)?;

    // 页面会话（flatten：同一 WS 上以 sessionId 路由）
    let target = cdp.call("Target.createTarget", json!({"url": "about:blank"}), None)?;
    let target_id = target
        .get("targetId")
        .and_then(Value::as_str)
        .ok_or_else(|| PdfError::Cdp("Target.createTarget 未返回 targetId".to_string()))?
        .to_string();
    let attached = cdp.call(
        "Target.attachToTarget",
        json!({"targetId": target_id, "flatten": true}),
        None,
    )?;
    let session = attached
        .get("sessionId")
        .and_then(Value::as_str)
        .ok_or_else(|| PdfError::Cdp("Target.attachToTarget 未返回 sessionId".to_string()))?
        .to_string();

    cdp.call("Page.enable", json!({}), Some(&session))?;
    cdp.call(
        "Page.navigate",
        json!({"url": cdp::url_from_path(html_path)}),
        Some(&session),
    )?;
    cdp.wait_event("Page.loadEventFired", &session, deadline)?;

    // 等 Paged.js 异步分页完成（规避 CLI 直印的 virtual-time-budget 竞态）
    cdp.wait_until_true(&session, "window.__azodocPagedDone === true", deadline)?;

    // 渲染统计（页数取 DOM 实数，页脚样本证明页码边盒已渲染——
    // 页码由 CSS content 生成，不在 textContent 里，取 ::before 计算值）
    let page_count = cdp
        .evaluate(
            &session,
            "document.querySelectorAll('.pagedjs_page').length",
        )?
        .as_u64()
        .unwrap_or(0);
    let footer_sample = cdp
        .evaluate(
            &session,
            "(() => { \
const el = document.querySelector('.pagedjs_page .pagedjs_margin-bottom-center.hasContent .pagedjs_margin-content'); \
if (!el) return ''; \
const c = getComputedStyle(el, '::before').content; \
return c && c !== 'none' && c !== 'normal' ? c : 'hasContent'; })()",
        )?
        .as_str()
        .map(String::from);
    if page_count == 0 {
        return Err(PdfError::Paged(
            "分页完成标志为真但 .pagedjs_page 为 0（polyfill 未生效？）".to_string(),
        ));
    }

    let result = cdp.call(
        "Page.printToPDF",
        json!({"printBackground": true, "preferCSSPageSize": true}),
        Some(&session),
    )?;
    let data = result
        .get("data")
        .and_then(Value::as_str)
        .ok_or_else(|| PdfError::BadOutput("printToPDF 未返回 PDF 数据".to_string()))?;
    use base64::Engine;
    let pdf = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|e| PdfError::BadOutput(format!("PDF base64 解码失败: {e}")))?;
    std::fs::write(pdf_path, &pdf)?;

    // 优雅关闭（失败不阻断成功路径；Drop 兜底 kill + 清理临时目录）
    let _ = cdp.call("Browser.close", json!({}), None);
    let _ = proc.wait();

    Ok(PagedRenderInfo {
        page_count,
        footer_sample,
    })
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn augment_injects_config_polyfill_css_and_title_anchor() {
        let html = "<html><head><title>t</title></head><body><p>x</p></body></html>";
        let out = augment_print_html(html, Some("炼金术 <手册> &\"指南\""));
        assert!(out.contains("window.PagedConfig"), "应注入 PagedConfig");
        assert!(out.contains("window.__azodocPagedDone"), "应注入完成回调");
        assert!(out.contains("<script src=\"./paged.polyfill.js\"></script>"));
        assert!(out.contains(PAGED_CSS), "应注入边盒 CSS");
        assert!(
            out.contains("炼金术 &lt;手册&gt; &amp;&quot;指南&quot;"),
            "标题应 HTML 转义"
        );
        // 注入点在 </head> 之前，原文其余部分原样保留
        assert!(out.find("window.PagedConfig").unwrap() < out.find("</head>").unwrap());
        assert!(out.ends_with("</body></html>"));
    }

    #[test]
    fn augment_without_head_appends() {
        let out = augment_print_html("<p>x</p>", None);
        assert!(out.contains("paged.polyfill.js"));
        assert!(
            !out.contains("<span class=\"azodoc-doctitle\""),
            "无标题不注入运行头锚点元素"
        );
    }

    #[test]
    fn parse_ws_url_from_stderr_line() {
        let line =
            "[xyz:123:456] DevTools listening on ws://127.0.0.1:52341/devtools/browser/abc-def";
        assert_eq!(
            cdp::parse_ws_url(line).as_deref(),
            Some("ws://127.0.0.1:52341/devtools/browser/abc-def")
        );
        assert_eq!(cdp::parse_ws_url("no url here"), None);
        assert_eq!(
            cdp::parse_ws_url("url \"ws://127.0.0.1:1/devtools/browser/x\" tail").as_deref(),
            Some("ws://127.0.0.1:1/devtools/browser/x")
        );
    }

    #[test]
    fn file_urls_escape_reserved_chars() {
        assert_eq!(
            cdp::url_from_path(Path::new(r"C:\tmp dir\a b.html")),
            "file:///C:/tmp%20dir/a%20b.html"
        );
        assert_eq!(
            cdp::url_from_path(Path::new("/tmp/x.html")),
            "file:///tmp/x.html"
        );
    }

    #[test]
    fn polyfill_asset_is_vendored() {
        assert!(PAGED_POLYFILL_JS.contains("Paged.js v0.4.3"));
        assert!(PAGED_POLYFILL_JS.len() > 500_000, "polyfill 不应为空壳");
    }
}
