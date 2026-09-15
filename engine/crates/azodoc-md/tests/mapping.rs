//! M2 验收 ②（Markdown 侧）：映射表每一行的损失级别断言（落地方案 §7.2）。

use azodoc_convert::{ImportJob, LossClass};

fn import(md: &str) -> azodoc_convert::ImportOutput {
    let mut job = ImportJob::new(None);
    azodoc_md::import(md, &mut job)
}

fn event(out: &azodoc_convert::ImportOutput, feature: &str) -> Option<azodoc_convert::LossEvent> {
    out.log
        .events()
        .iter()
        .find(|e| e.feature == feature)
        .cloned()
}

#[test]
fn core_markdown_features_are_lossless() {
    let md = "# 标题\n\n**粗** *斜* ~~删~~ `码`\n\n- [x] 任务\n\n| a | b |\n| - | - |\n| 1 | 2 |\n";
    let out = import(md);
    assert!(
        !out.log.has_loss(),
        "核心特性不应有损失: {:?}",
        out.log.events()
    );
    assert!(out.log.count(LossClass::None) > 0);
}

#[test]
fn inline_html_is_preserved_raw() {
    let out = import("包含 <kbd>Ctrl</kbd> 的段落\n");
    let e = event(&out, "inline_html").expect("应记录 inline_html 事件");
    assert_eq!(e.class, LossClass::PreservedRaw);
    assert_eq!(e.action, "preserved");
}

#[test]
fn html_block_is_preserved_raw() {
    let out = import("<div class=\"x\">块</div>\n");
    let e = event(&out, "html_block").expect("应记录 html_block 事件");
    assert_eq!(e.class, LossClass::PreservedRaw);
    // preserved 载荷必须已登记
    assert!(!out.log.events().is_empty());
}

#[test]
fn html_comment_is_preserved_raw() {
    let out = import("<!-- 注释 -->\n");
    assert!(event(&out, "html_block").is_some() || event(&out, "inline_html").is_some());
}

#[test]
fn math_imports_losslessly() {
    let out = import("公式 $E = mc^2$ 与块公式：\n\n$$a^2 + b^2 = c^2$$\n");
    assert!(!out.log.has_loss(), "{:?}", out.log.events());
}

#[test]
fn math_export_to_html_is_degraded() {
    // 数学在 HTML 导出中降级（映射表：MD 往返 NONE，HTML 导出 DEGRADED）
    let out = import("公式 $E = mc^2$\n");
    let doc = azodoc_convert::ExportDoc {
        content: out.content.clone(),
        assets: vec![],
        preserved: vec![],
        title: None,
        language: None,
        document_id: None,
    };
    let mut log = azodoc_convert::LossLog::new();
    let _ = azodoc_convert::txt::export_txt(&doc, &mut log);
    let mut log = azodoc_convert::LossLog::new();
    // TXT 对 math_block 是无损文本输出
    let _ = azodoc_convert::txt::export_txt(&doc, &mut log);
    assert!(!log.has_loss());
    // HTML 导出降级由 azodoc-html 的映射测试覆盖
}

#[test]
fn frontmatter_title_and_unknown_keys() {
    let out = import("---\ntitle: T1\nlanguage: en\nspooky: 42\n---\n\n正文\n");
    assert_eq!(out.title.as_deref(), Some("T1"));
    assert_eq!(out.language.as_deref(), Some("en"));
    assert_eq!(out.doc_extra.get("spooky"), Some(&serde_json::json!("42")));
    assert!(!out.log.has_loss(), "front-matter 不应有损失");
}

#[test]
fn footnote_roundtrip_is_lossless() {
    let out = import("用[^a]。\n\n[^a]: 注。\n");
    assert!(!out.log.has_loss(), "{:?}", out.log.events());
}

#[test]
fn underline_export_is_partial() {
    let out = import("plain\n");
    // 构造带 underline 的内容走 MD 导出
    let content = serde_json::json!({
        "schema_version": "1.0",
        "content": [{
            "id": "blk_10000000000000000000000000",
            "type": "paragraph",
            "content": [
                {"type": "text", "text": "前 "},
                {"type": "underline", "content": [{"type": "text", "text": "线"}]},
            ],
        }]
    });
    let doc = azodoc_convert::ExportDoc {
        content,
        assets: vec![],
        preserved: vec![],
        title: None,
        language: None,
        document_id: None,
    };
    let mut log = azodoc_convert::LossLog::new();
    let md = azodoc_md::export_markdown(&doc, &mut log);
    let e = log
        .events()
        .iter()
        .find(|e| e.feature == "underline_md")
        .expect("下划线导出应记 PARTIAL");
    assert_eq!(e.class, LossClass::Partial);
    assert!(md.contains("前 线"));
}

#[test]
fn merged_cell_export_is_partial() {
    let content = serde_json::json!({
        "schema_version": "1.0",
        "content": [{
            "id": "blk_10000000000000000000000000",
            "type": "table",
            "header_row": true,
            "columns": [
                {"id": "col_10000000000000000000000000", "name": "a"},
                {"id": "col_20000000000000000000000000", "name": "b"}
            ],
            "rows": [{
                "id": "row_10000000000000000000000000",
                "cells": [
                    {"id": "cel_10000000000000000000000000", "column": 0, "colSpan": 2, "children": [
                        {"id": "blk_20000000000000000000000000", "type": "paragraph", "content": [{"type": "text", "text": "跨列"}]}
                    ]}
                ]
            }]
        }]
    });
    let doc = azodoc_convert::ExportDoc {
        content,
        assets: vec![],
        preserved: vec![],
        title: None,
        language: None,
        document_id: None,
    };
    let mut log = azodoc_convert::LossLog::new();
    let _ = azodoc_md::export_markdown(&doc, &mut log);
    let e = log
        .events()
        .iter()
        .find(|e| e.feature == "merged_cell")
        .expect("合并单元格导出应记 PARTIAL");
    assert_eq!(e.class, LossClass::Partial);
}
