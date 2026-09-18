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

//! M2 验收 ②（HTML 侧）：映射表损失级别断言（落地方案 §7.3）+ 安全例外条款。

use azodoc_convert::{ImportJob, LossClass};

fn import(html: &str) -> (azodoc_convert::ImportOutput, azodoc_convert::ImportJob) {
    let mut job = ImportJob::new(None);
    let out = azodoc_html::import(html, &mut job);
    (out, job)
}

fn event(out: &azodoc_convert::ImportOutput, feature: &str) -> Option<azodoc_convert::LossEvent> {
    out.log
        .events()
        .iter()
        .find(|e| e.feature == feature)
        .cloned()
}

#[test]
fn core_html_features_are_lossless() {
    let (out, _) = import(
        "<h1>T</h1><p><strong>b</strong><em>i</em><code>c</code>\
        <a href='https://e.x'>l</a></p>\
        <ul><li>a</li><li>b</li></ul>\
        <blockquote><p>q</p></blockquote>\
        <pre><code class='language-rs'>fn x() {}</code></pre>\
        <hr><p>行<br>断</p>",
    );
    assert!(!out.log.has_loss(), "{:?}", out.log.events());
}

#[test]
fn script_is_removed_and_reported_not_preserved() {
    let (out, job) = import("<p>前</p><script>alert(1)</script><p>后</p>");
    let e = event(&out, "active_content").expect("script 必须报告");
    assert_eq!(e.class, LossClass::Unsupported);
    assert_eq!(e.action, "removed");
    // 安全例外：活动内容不进入 preserved/
    assert!(
        job.preserved
            .iter()
            .all(|(_, b)| !String::from_utf8_lossy(b).contains("alert")),
        "活动内容不得入 preserved/"
    );
}

#[test]
fn event_handler_attributes_are_removed() {
    let (out, _) = import("<p onclick=\"bad()\">文字</p>");
    let e = event(&out, "active_content").expect("onclick 必须报告");
    assert_eq!(e.class, LossClass::Unsupported);
    // 段落内容仍在
    let s = serde_json::to_string(&out.content).unwrap();
    assert!(s.contains("文字"));
    assert!(!s.contains("bad()"));
}

#[test]
fn javascript_urls_are_removed() {
    let (out, _) = import("<p><a href=\"javascript:alert(1)\">点我</a></p>");
    assert!(event(&out, "active_content").is_some());
    let s = serde_json::to_string(&out.content).unwrap();
    assert!(!s.contains("javascript:"), "javascript: URL 不得保留");
    assert!(s.contains("点我"));
}

#[test]
fn custom_elements_are_preserved_raw() {
    let (out, job) = import("<p>前</p><x-chart data-series=\"1,2\">内</x-chart>");
    let e = event(&out, "custom_element").expect("自定义元素必须报告");
    assert_eq!(e.class, LossClass::PreservedRaw);
    // payload 落盘
    assert!(job
        .preserved
        .iter()
        .any(|(_, b)| String::from_utf8_lossy(b).contains("<x-chart")));
    // content 中存在 unknown 块，且 payload_ref 指向 preserved
    let s = serde_json::to_string(&out.content).unwrap();
    assert!(s.contains("preserved/html/"));
}

#[test]
fn comments_are_preserved_raw() {
    let (out, _) = import("<!-- 注释 -->");
    assert!(event(&out, "html_comment").is_some());
}

#[test]
fn span_class_is_partial() {
    let (out, _) = import("<p>带<span class=\"hl\">类</span>的片段</p>");
    let e = event(&out, "style_attribute").expect("span class 应记 PARTIAL");
    assert_eq!(e.class, LossClass::Partial);
}

#[test]
fn style_element_is_partial() {
    let (out, _) = import("<style>p{color:red}</style><p>x</p>");
    let e = event(&out, "style_element").expect("style 应记 PARTIAL");
    assert_eq!(e.class, LossClass::Partial);
}

#[test]
fn data_uri_image_becomes_embedded_asset() {
    // 1x1 红色 PNG
    let png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
    let (out, job) =
        import(format!("<p><img src=\"data:image/png;base64,{png}\" alt=\"点\"></p>").as_str());
    assert_eq!(job.assets.len(), 1);
    match &job.assets[0].source {
        azodoc_convert::AssetSource::Embedded(bytes) => {
            assert_eq!(bytes[0], 0x89); // PNG magic
            assert_eq!(job.assets[0].mime, "image/png");
        }
        _ => panic!("data URI 应生成内嵌资产"),
    }
    let s = serde_json::to_string(&out.content).unwrap();
    assert!(s.contains("asset://"));
}

#[test]
fn page_break_marker_import_and_export() {
    // 自家产物 <div class="page-break"> → page_break 块（E4 手动分页）。
    let (out, _) = import("<p>前</p><div class=\"page-break\"></div><p>后</p>");
    let s = serde_json::to_string(&out.content).unwrap();
    assert!(
        s.contains("\"page_break\""),
        "div.page-break 应映射为 page_break"
    );

    // 导出：page_break → <div class="page-break">（印刷 CSS 分页点）。
    let mut nid = 0u32;
    let mut nid = move |_: &str| -> String {
        nid += 1;
        format!("blk_{nid:026}")
    };
    let mut idgen = |k: azodoc_model::id::IdKind| -> String {
        let _ = k;
        nid("blk")
    };
    let content = serde_json::json!({"schema_version": "1.0", "content": [
        {"type": "paragraph", "id": idgen(azodoc_model::id::IdKind::Blk),
         "content": [{"type": "text", "text": "a"}]},
        {"type": "page_break", "id": idgen(azodoc_model::id::IdKind::Blk)},
    ]});
    let doc = azodoc_convert::ExportDoc {
        content,
        assets: vec![],
        preserved: vec![],
        title: None,
        language: None,
        document_id: None,
    };
    let mut log = azodoc_convert::LossLog::new();
    let html = azodoc_html::export_html(&doc, &mut log);
    assert!(
        html.contains("<div class=\"page-break\""),
        "导出应含分页标记"
    );

    // marked 变体：块带 data-block-id（LayoutIndex 用）。
    let mut log2 = azodoc_convert::LossLog::new();
    let marked = azodoc_html::export_html_marked(&doc, &mut log2);
    assert!(
        marked.contains("data-block-id="),
        "marked 导出应带 data-block-id"
    );
    assert!(!html.contains("data-block-id"), "默认导出不得带标记");
}

#[test]
fn sanitized_fragment_removes_active_content() {
    let (clean, removed) =
        azodoc_html::sanitize_fragment("<p onclick=\"x()\">好<script>bad()</script></p>");
    assert!(removed);
    assert!(clean.contains("好"));
    assert!(!clean.contains("script"));
    assert!(!clean.contains("onclick"));
}
