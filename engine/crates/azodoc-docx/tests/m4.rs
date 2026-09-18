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

//! M4 测试：AST 映射单元（无需 Pandoc）+ docx 清点 + Pandoc 端到端（可用时）。

use azodoc_convert::{ImportJob, LossClass};
use serde_json::{json, Value};

// ---------------------------------------------------------------- AST 映射单元

fn import_ast(ast: &Value) -> azodoc_convert::ImportOutput {
    let mut job = ImportJob::new(None);
    let mut log = azodoc_convert::LossLog::new();
    let mut ctx = azodoc_docx::ast_in::ImportCtx {
        job: &mut job,
        extract_dir: std::env::temp_dir(),
        log: &mut log,
        pending_footnotes: Vec::new(),
        pending_blocks: Vec::new(),
    };
    let content = azodoc_docx::ast_in::ast_to_prima(ast, &mut ctx);
    azodoc_convert::ImportOutput {
        content,
        title: azodoc_docx::ast_in::meta_title(ast),
        language: None,
        doc_extra: Default::default(),
        annotations: Vec::new(),
        theme: None,
        log,
    }
}

#[test]
fn ast_fixture_maps_to_prima() {
    // 形状按 pandoc 3.11 实测
    let ast = json!({
        "pandoc-api-version": [1, 23, 1],
        "meta": {"title": {"t": "MetaInlines", "c": [{"t": "Str", "c": "标题来自元数据"}]}},
        "blocks": [
            {"t": "Header", "c": [1, ["", [], []], [{"t": "Str", "c": "标题一"}]]},
            {"t": "Para", "c": [
                {"t": "Strong", "c": [{"t": "Str", "c": "粗"}]},
                {"t": "Space"},
                {"t": "Link", "c": [["", [], []], [{"t": "Str", "c": "链"}], ["https://e.x", ""]]},
                {"t": "Math", "c": [{"t": "InlineMath"}, "a^2"]},
                {"t": "Note", "c": [{"t": "Para", "c": [{"t": "Str", "c": "脚注正文"}]}]}
            ]},
            {"t": "OrderedList", "c": [[3, {"t": "Decimal"}, {"t": "Period"}], [
                [{"t": "Plain", "c": [{"t": "Str", "c": "甲"}]}]
            ]]},
            {"t": "Table", "c": [
                ["", [], []],
                [null, []],
                [[{"t": "AlignDefault"}, {"t": "ColWidth", "c": 0.5}]],
                [["", [], []], [
                    [["", [], []], [
                        [["", [], []], {"t": "AlignLeft"}, 1, 1, [
                            {"t": "Plain", "c": [{"t": "Str", "c": "表头"}]}
                        ]]
                    ]]
                ]],
                [[["", [], []], 0, [], [
                    [["", [], []], [
                        [["", [], []], {"t": "AlignDefault"}, 1, 2, [
                            {"t": "Plain", "c": [{"t": "Str", "c": "跨列"}]}
                        ]]
                    ]]
                ]]],
                [["", [], []], []]
            ]},
            {"t": "RawBlock", "c": ["html", "<div>原始</div>"]}
        ]
    });
    let out = import_ast(&ast);

    // 标题来自 meta
    assert_eq!(out.title.as_deref(), Some("标题来自元数据"));
    let s = serde_json::to_string(&out.content).unwrap();
    assert!(s.contains("\"heading\""));
    assert!(s.contains("标题一"));
    assert!(s.contains("\"strong\""));
    assert!(s.contains("https://e.x"));
    assert!(s.contains("inline_math"));
    assert!(s.contains("footnote_ref") && s.contains("footnote"));
    assert!(s.contains("\"start\":3"), "有序列表 start 应保留");
    assert!(s.contains("\"colSpan\":2"), "合并单元格应保留");
    // E2：TableHead → header_row + 首行 cell role:"header"；
    // ColWidth 0.5 → columns[].width 500（分数 ×1000 取整）
    assert!(
        s.contains("\"header_row\":true"),
        "TableHead 应产生 header_row"
    );
    assert!(
        s.contains("\"role\":\"header\""),
        "首行 cell 应有 role:header"
    );
    assert!(s.contains("\"width\":500"), "ColWidth 0.5 → width 500");
    // RawBlock → preserved_raw
    let e = out
        .log
        .events()
        .iter()
        .find(|e| e.feature == "raw_block")
        .expect("RawBlock 应记 preserved_raw");
    assert_eq!(e.class, LossClass::PreservedRaw);
    // 脚注定义挂在文档末尾
    assert!(s.contains("脚注正文"));
}

#[test]
fn empty_exact_map_is_handled() {
    let ast = json!({
        "pandoc-api-version": [1, 23, 1],
        "meta": {},
        "blocks": [
            {"t": "Para", "c": [
                {"t": "SmallCaps", "c": [{"t": "Str", "c": "小型大写"}]},
                {"t": "Span", "c": [["", ["hl"], []], [{"t": "Str", "c": "类"}]]}
            ]}
        ]
    });
    let out = import_ast(&ast);
    assert!(out
        .log
        .events()
        .iter()
        .any(|e| e.feature == "inline_semantic" && e.class == LossClass::Partial));
    assert!(out.log.events().iter().any(|e| e.feature == "span_class"));
}

// ---------------------------------------------------------------- docx 清点

fn make_docx_zip(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
    use std::io::Write;
    let buf = std::io::Cursor::new(Vec::new());
    let mut z = zip::ZipWriter::new(buf);
    for (name, data) in entries {
        z.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(data).unwrap();
    }
    z.finish().unwrap().into_inner()
}

#[test]
fn inventory_detects_dropped_parts() {
    let docx = make_docx_zip(&[
        (
            "docProps/core.xml",
            r#"<cp:coreProperties><dc:title>清点标题</dc:title></cp:coreProperties>"#
                .as_bytes()
                .to_vec(),
        ),
        ("word/header1.xml", b"<w:ftr/>".to_vec()),
        ("word/footer1.xml", b"<w:ftr/>".to_vec()),
        ("word/comments.xml", b"<w:comments/>".to_vec()),
        ("word/media/image1.png", vec![0x89, b'P', b'N', b'G']),
    ]);
    let inv = azodoc_docx::inventory::inventory(&docx).unwrap();
    assert_eq!(inv.title.as_deref(), Some("清点标题"));
    assert!(inv.has_headers && inv.has_footers && inv.has_comments);
    assert_eq!(inv.media_count, 1);
    let issues = azodoc_docx::inventory::loss_issues(&inv);
    assert_eq!(issues.len(), 3, "页眉/页脚/批注各一条");
}

#[test]
fn inventory_on_plain_document_is_clean() {
    let docx = make_docx_zip(&[("word/document.xml", b"<w:document/>".to_vec())]);
    let inv = azodoc_docx::inventory::inventory(&docx).unwrap();
    assert_eq!(inv.title, None);
    assert!(!inv.has_headers);
    assert_eq!(azodoc_docx::inventory::loss_issues(&inv).len(), 0);
}
