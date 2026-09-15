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

//! B1 验收（Future Work §B1）：Paged.js 分页 + 页码边盒端到端
//! （需无头浏览器，缺席自动跳过）。

use std::path::PathBuf;

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("athanor-b1-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// 多页 HTML：40 个长段落，A4 下必然超过 1 页。
fn multipage_html() -> String {
    let paragraph = "<p>出版品质验收段落：页码边盒与运行头由 Paged.js 的 @page \
边盒渲染，分页点由其排版引擎决定。本段重复多次以产生多页文档。</p>";
    let body: String = std::iter::repeat_n(paragraph, 40).collect();
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
<title>B1 验收文档</title>\
<style>@page {{ size: A4; margin: 20mm; }}</style>\
</head><body><h1>B1 验收</h1>{body}</body></html>"
    )
}

#[test]
fn paged_print_footer_has_page_numbers() {
    if std::env::var_os("AZODOC_BROWSER_PATH").is_none() && azodoc_pdf::find_browser().is_err() {
        eprintln!("跳过：本机未找到 Chromium/Edge");
        return;
    }

    let dir = tmpdir("footer");
    let html = azodoc_pdf::augment_print_html(&multipage_html(), Some("B1 验收文档"));
    let html_path = dir.join("print.html");
    std::fs::write(&html_path, html).unwrap();
    azodoc_pdf::write_polyfill_assets(&dir).unwrap();
    let pdf_path = dir.join("out.pdf");

    let info = azodoc_pdf::print_html_to_pdf_paged(
        &azodoc_pdf::find_browser().unwrap(),
        &html_path,
        &pdf_path,
        azodoc_pdf::DEFAULT_TIMEOUT,
    )
    .expect("Paged.js 打印应成功");

    // 验收：多页文档；页脚页码边盒真实渲染（Paged.js 以 hasContent 标记
    // 已配置内容的边盒，页码经 CSS content 在打印时生成）
    assert!(
        info.page_count >= 2,
        "40 段长文应产生多页，实际 {} 页",
        info.page_count
    );
    let footer = info.footer_sample.as_deref().unwrap_or("");
    assert!(
        footer.contains("counter") || footer.contains("hasContent"),
        "首页页脚页码边盒应已渲染，实际: {footer:?}"
    );

    let pdf = std::fs::read(&pdf_path).unwrap();
    assert!(pdf.starts_with(b"%PDF"), "产出必须是 PDF");
    assert!(pdf.len() > 10_000, "多页 PDF 不应为空白产物");
    std::fs::remove_dir_all(&dir).ok();
}
