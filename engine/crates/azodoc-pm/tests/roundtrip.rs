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

//! M6.1 验收测试：Prima ↔ ProseMirror 双向无损。
//!
//! 覆盖：全类型夹具（16 块 + 14 span + R2 未知字段）、spec 黄金样例
//! （minimal/rich/forward-compat）、M2 语料（md/html × basic/lossy）、
//! ID 补发规则、规范化恒等（幂等/合并/嵌套重建）。

// 大型 json! 夹具需要更深的宏展开上限
#![recursion_limit = "512"]

use azodoc_convert::ImportJob;
use azodoc_model::id::IdKind;
use serde_json::{json, Value};
use std::path::PathBuf;

fn repo_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// roundtrip 且断言"合法 ID 不被触碰"——补发即 panic。
fn roundtrip(file: &Value) -> Value {
    let pm = azodoc_pm::content_file_to_pm(file).expect("prima→pm");
    let mut no_alloc = |_k: IdKind| -> String { panic!("roundtrip 不应补发 ID") };
    azodoc_pm::pm_to_content_file(&pm, &mut no_alloc)
        .expect("pm→prima")
        .content_file
}

// ---------------------------------------------------------------- 夹具

/// 确定性 ID 生成（十进制数字全在 Crockford 字母表内）。
fn make_nid() -> impl FnMut(&str) -> String {
    let mut n = 0u32;
    move |prefix: &str| {
        n += 1;
        format!("{prefix}_{n:026}")
    }
}

/// 覆盖全部块/span 类型 + R2 未知字段的规范形夹具。
fn full_fixture() -> Value {
    let mut nid = make_nid();
    let heading = nid("blk");
    let para_inline = nid("blk");
    let quote = nid("blk");
    let quote_para = nid("blk");
    let list = nid("blk");
    let li1 = nid("li");
    let li1_para = nid("blk");
    let li2 = nid("li");
    let li2_para = nid("blk");
    let sublist = nid("blk");
    let li3 = nid("li");
    let li3_para = nid("blk");
    let li4 = nid("li");
    let li4_para = nid("blk");
    let olist = nid("blk");
    let oli = nid("li");
    let oli_para = nid("blk");
    let code = nid("blk");
    let empty_code = nid("blk");
    let table = nid("blk");
    let col1 = nid("col");
    let col2 = nid("col");
    let row1 = nid("row");
    let cel1 = nid("cel");
    let cel2 = nid("cel");
    let row2 = nid("row");
    let cel3 = nid("cel");
    let cel3_para = nid("blk");
    let figure = nid("blk");
    let figure_bare = nid("blk");
    let image = nid("blk");
    let hr = nid("blk");
    let math = nid("blk");
    let callout = nid("blk");
    let callout_para = nid("blk");
    let embed = nid("blk");
    let fnote = nid("blk");
    let fnote_para = nid("blk");
    let unk_block_a = nid("blk");
    let unk_block_b = nid("blk");
    let para_extra = nid("blk");
    let section = nid("sec");

    json!({
        "schema_version": "1.0",
        "content": [
            {"type": "section", "id": section, "children": [
                {"type": "heading", "id": heading, "level": 2, "content": [
                    {"type": "text", "text": "Aludel 冒烟："},
                    {"type": "strong", "content": [
                        {"type": "em", "content": [{"type": "text", "text": "嵌套"}]}
                    ]},
                    {"type": "link", "url": "https://example.com/a", "title": "示例", "content": [
                        {"type": "text", "text": "链接"}
                    ]},
                    {"type": "hard_break"},
                    {"type": "text", "text": "第二行", "x_ref": "L2"}
                ]},
                {"type": "paragraph", "id": para_inline, "content": [
                    {"type": "text", "text": "行内全家福："},
                    {"type": "code", "text": "let x = 1;"},
                    {"type": "inline_math", "latex": "E=mc^2"},
                    {"type": "footnote_ref", "id": fnote},
                    {"type": "inline_image", "asset": "asset://as_F00000000000000000000000000/pic.png", "alt": "图"},
                    {"type": "mention", "target": "doc:1"},
                    {"type": "cite", "key": "smith2020"},
                    {"type": "unknown", "origin": "html", "loss_class": "preserved_raw",
                     "summary": "<x-tip>", "payload_ref": "preserved/html/tip.html",
                     "content": [{"type": "text", "text": "保留"}]},
                    {"type": "strike", "content": [{"type": "text", "text": "旧"}]},
                    {"type": "underline", "content": [{"type": "text", "text": "新"}]},
                    {"type": "strong", "x_m": true, "content": [{"type": "text", "text": "带额外字段的加粗"}]}
                ]}
            ]},
            {"type": "quote", "id": quote, "children": [
                {"type": "paragraph", "id": quote_para, "content": [
                    {"type": "text", "text": "引用一行"}
                ]}
            ]},
            {"type": "list", "id": list, "style": "bullet", "items": [
                {"id": li1, "checked": true, "children": [
                    {"type": "paragraph", "id": li1_para, "content": [{"type": "text", "text": "已完成"}]}
                ]},
                {"id": li2, "checked": false, "children": [
                    {"type": "paragraph", "id": li2_para, "content": [{"type": "text", "text": "含嵌套列表"}]},
                    {"type": "list", "id": sublist, "style": "bullet", "items": [
                        {"id": li3, "children": [
                            {"type": "paragraph", "id": li3_para, "content": [{"type": "text", "text": "子项"}]}
                        ]}
                    ]}
                ]},
                {"id": li4, "x_note": "条目级未知字段", "children": [
                    {"type": "paragraph", "id": li4_para, "content": [{"type": "text", "text": "第三项"}]}
                ]}
            ]},
            {"type": "list", "id": olist, "style": "ordered", "start": 5, "items": [
                {"id": oli, "children": [
                    {"type": "paragraph", "id": oli_para, "content": [{"type": "text", "text": "从五开始"}]}
                ]}
            ]},
            {"type": "code_block", "id": code, "language": "rust", "text": "fn main() {\n    println!(\"hi\");\n}"},
            {"type": "code_block", "id": empty_code, "text": ""},
            {"type": "table", "id": table, "header_row": true, "columns": [
                {"id": col1, "name": "名称"},
                {"id": col2}
            ], "rows": [
                {"id": row1, "cells": [
                    {"id": cel1, "column": 0, "children": [
                        {"type": "paragraph", "id": nid("blk"), "content": [{"type": "text", "text": "甲"}]}
                    ]},
                    {"id": cel2, "column": 1, "children": [
                        {"type": "paragraph", "id": nid("blk"), "content": [{"type": "text", "text": "乙"}]}
                    ]}
                ]},
                {"id": row2, "cells": [
                    {"id": cel3, "column": 0, "rowSpan": 2, "children": [
                        {"type": "paragraph", "id": cel3_para, "content": [{"type": "text", "text": "丙"}]}
                    ]}
                ]}
            ]},
            {"type": "figure", "id": figure,
             "asset": "asset://as_G00000000000000000000000000/fig.png", "alt": "架构图",
             "caption": [
                {"type": "text", "text": "图："},
                {"type": "strong", "content": [{"type": "text", "text": "重要"}]}
             ]},
            {"type": "figure", "id": figure_bare,
             "asset": "asset://as_G00000000000000000000000001/bare.png", "alt": "无题注"},
            {"type": "image", "id": image,
             "asset": "asset://as_H00000000000000000000000000/i.png", "alt": ""},
            {"type": "horizontal_rule", "id": hr},
            {"type": "math_block", "id": math, "latex": "\\int_0^1 x^2\\,dx"},
            {"type": "callout", "id": callout, "variant": "warning", "children": [
                {"type": "paragraph", "id": callout_para, "content": [{"type": "text", "text": "注意"}]}
            ]},
            {"type": "embed", "id": embed, "asset": "asset://as_J00000000000000000000000000/frame.html"},
            {"type": "footnote", "id": fnote, "children": [
                {"type": "paragraph", "id": fnote_para, "content": [{"type": "text", "text": "脚注正文"}]}
            ]},
            {"type": "unknown", "id": unk_block_a, "origin": "docx", "loss_class": "preserved_raw",
             "summary": "ActiveX 控件", "payload_ref": "preserved/docx/c.bin", "content": []},
            {"type": "unknown", "id": unk_block_b, "origin": "html", "loss_class": "preserved_raw",
             "summary": "交互图表", "content": [{"type": "text", "text": "占位文本"}]},
            {"type": "paragraph", "id": para_extra, "x_custom": {"n": 1}, "content": [
                {"type": "text", "text": "段落级未知字段"}
            ]}
        ],
        "x_doc": {"v": 1}
    })
}

// ---------------------------------------------------------------- 恒等

#[test]
fn full_fixture_roundtrip_identical() {
    let f = full_fixture();
    assert_eq!(
        roundtrip(&f),
        f,
        "全类型夹具往返必须逐字段恒等（含 R2 未知字段）"
    );
}

#[test]
fn roundtrip_is_idempotent() {
    let once = roundtrip(&full_fixture());
    let twice = roundtrip(&once);
    assert_eq!(once, twice, "二次往返必须是动点（规范形稳定）");
}

#[test]
fn empty_document_roundtrip() {
    let empty = json!({"schema_version": "1.0", "content": []});
    assert_eq!(roundtrip(&empty), empty);
}

// ---------------------------------------------------------------- E2 表格

#[test]
fn table_header_and_colwidth_roundtrip() {
    // cell role="header" ↔ PM table_header 节点；columns[].width ↔ cell colwidth
    let mut nid = make_nid();
    let f = json!({"schema_version": "1.0", "content": [
        {"type": "table", "id": nid("blk"), "header_row": true, "columns": [
            {"id": nid("col"), "width": 2},
            {"id": nid("col"), "width": 1}
        ], "rows": [
            {"id": nid("row"), "cells": [
                {"id": nid("cel"), "column": 0, "role": "header", "children": [
                    {"type": "paragraph", "id": nid("blk"), "content": [{"type": "text", "text": "名"}]}
                ]},
                {"id": nid("cel"), "column": 1, "role": "header", "children": [
                    {"type": "paragraph", "id": nid("blk"), "content": [{"type": "text", "text": "值"}]}
                ]}
            ]},
            {"id": nid("row"), "cells": [
                {"id": nid("cel"), "column": 0, "children": [
                    {"type": "paragraph", "id": nid("blk"), "content": [{"type": "text", "text": "a"}]}
                ]},
                {"id": nid("cel"), "column": 1, "children": [
                    {"type": "paragraph", "id": nid("blk"), "content": [{"type": "text", "text": "b"}]}
                ]}
            ]}
        ]}
    ]});

    // 正向：role="header" → table_header 节点；width → colwidth
    let pm = azodoc_pm::content_file_to_pm(&f).unwrap();
    let table = &pm["content"][0];
    let row0 = &table["content"][0];
    assert_eq!(
        row0["content"][0]["type"], "table_header",
        "首行 cell → table_header"
    );
    assert_eq!(row0["content"][0]["attrs"]["colwidth"], json!([2]));
    assert_eq!(row0["content"][1]["attrs"]["colwidth"], json!([1]));
    let row1 = &table["content"][1];
    assert_eq!(row1["content"][0]["type"], "table_cell");
    assert_eq!(row1["content"][0]["attrs"]["colwidth"], json!([2]));

    // 往返恒等（colwidth 归并回 width；role 由节点类型重写）
    assert_eq!(roundtrip(&f), f, "表头/列宽往返必须恒等");
}

#[test]
fn table_legacy_prima_attr_names_accepted() {
    // 旧快照里 PM attrs 键是 Prima 名（colSpan/rowSpan）——to_prima 兼容读取
    let mut nid = make_nid();
    let pm_doc = json!({"type": "doc", "attrs": {"schema_version": "1.0", "extra": null}, "content": [
        {"type": "table", "attrs": {"id": nid("blk"), "columns": [{"id": nid("col")}], "extra": null}, "content": [
            {"type": "table_row", "attrs": {"id": nid("row"), "extra": null}, "content": [
                {"type": "table_cell", "attrs": {
                    "id": nid("cel"), "column": 0,
                    "colSpan": 2, "extra": null
                }, "content": [
                    {"type": "paragraph", "attrs": {"id": nid("blk"), "extra": null}, "content": [
                        {"type": "text", "text": "x"}
                    ]}
                ]}
            ]}
        ]}
    ]});
    let mut no_alloc = |_k: IdKind| -> String { panic!("不应补发 ID") };
    let back = azodoc_pm::pm_to_content_file(&pm_doc, &mut no_alloc).unwrap();
    let cell = &back.content_file["content"][0]["rows"][0]["cells"][0];
    assert_eq!(cell["colSpan"], 2, "旧键名 colSpan 必须被识别");
}

// ---------------------------------------------------------------- 规范化恒等

#[test]
fn non_canonical_nesting_normalizes() {
    // em[strong[x]] 语义等价于 strong[em[x]]（rank 序）；二次往返是动点
    let non_canonical = json!({"schema_version": "1.0", "content": [
        {"type": "paragraph", "id": make_nid()("blk"), "content": [
            {"type": "em", "content": [
                {"type": "strong", "content": [{"type": "text", "text": "x"}]}
            ]},
            {"type": "strong", "content": [{"type": "text", "text": "y"}]}
        ]}
    ]});
    let once = roundtrip(&non_canonical);
    let expected = json!({"schema_version": "1.0", "content": [
        {"type": "paragraph", "id": once["content"][0]["id"], "content": [
            {"type": "strong", "content": [
                {"type": "em", "content": [{"type": "text", "text": "x"}]}
            ]},
            {"type": "strong", "content": [{"type": "text", "text": "y"}]}
        ]}
    ]});
    assert_eq!(once, expected, "非规范嵌套应重建为 rank 序");
    assert_eq!(roundtrip(&once), once, "规范化后必须幂等");
}

#[test]
fn duplicate_marks_collapse() {
    // strong[strong[x]]：PM 的 marks 是集合，重复折叠后不还原（语义等价，可接受）
    let doubled = json!({"schema_version": "1.0", "content": [
        {"type": "paragraph", "id": make_nid()("blk"), "content": [
            {"type": "strong", "content": [
                {"type": "strong", "content": [{"type": "text", "text": "x"}]}
            ]}
        ]}
    ]});
    let once = roundtrip(&doubled);
    assert_eq!(
        once["content"][0]["content"][0]["type"], "strong",
        "重复 mark 折叠为单个"
    );
    assert_eq!(
        once["content"][0]["content"][0]["content"][0]["type"], "text",
        "不应保留两层嵌套"
    );
}

#[test]
fn adjacent_text_merges_on_both_sides() {
    let mut nid = make_nid();
    // Prima 侧相邻同形 span → 单个 PM text 节点
    let prima = json!({"schema_version": "1.0", "content": [
        {"type": "paragraph", "id": nid("blk"), "content": [
            {"type": "text", "text": "a"},
            {"type": "text", "text": "b"},
            {"type": "strong", "content": [{"type": "text", "text": "亮"}]},
            {"type": "strong", "content": [{"type": "text", "text": "度"}]}
        ]}
    ]});
    let pm = azodoc_pm::content_file_to_pm(&prima).unwrap();
    let para_content = pm["content"][0]["content"].as_array().unwrap();
    assert_eq!(
        para_content.len(),
        2,
        "PM 侧应合并为 2 个 text 节点（普通 + strong）"
    );
    assert_eq!(para_content[0]["text"], "ab");

    // PM 侧相邻同 marks text → 单个 span
    let pm_doc = json!({"type": "doc", "attrs": {"schema_version": "1.0", "extra": null}, "content": [
        {"type": "paragraph", "attrs": {"id": nid("blk"), "extra": null}, "content": [
            {"type": "text", "text": "a", "marks": [{"type": "strong", "attrs": {"extra": null}}]},
            {"type": "text", "text": "b", "marks": [{"type": "strong", "attrs": {"extra": null}}]}
        ]}
    ]});
    let mut no_alloc = |_k: IdKind| -> String { panic!("不应补发 ID") };
    let back = azodoc_pm::pm_to_content_file(&pm_doc, &mut no_alloc).unwrap();
    let spans = back.content_file["content"][0]["content"]
        .as_array()
        .unwrap();
    assert_eq!(spans.len(), 1, "PM 相邻同 marks 应合并为单个 span");
    assert_eq!(spans[0]["type"], "strong");
    assert_eq!(spans[0]["content"][0]["text"], "ab");
}

// ---------------------------------------------------------------- spec 黄金样例

fn read_content_of(sample: &str) -> Value {
    let path = repo_dir().join("spec/examples").join(sample);
    let f =
        std::fs::File::open(&path).unwrap_or_else(|e| panic!("打开 {} 失败: {e}", path.display()));
    let mut zf = zip::ZipArchive::new(f).expect("打开 ZIP 失败");
    let mut bytes = Vec::new();
    use std::io::Read;
    zf.by_name("document/content.json")
        .expect("条目不存在")
        .read_to_end(&mut bytes)
        .expect("读取失败");
    serde_json::from_slice(&bytes).expect("content.json 解析失败")
}

#[test]
fn spec_golden_samples_roundtrip() {
    for name in ["minimal.azodoc", "rich.azodoc", "forward-compat.azodoc"] {
        let content = read_content_of(name);
        assert_eq!(
            roundtrip(&content),
            content,
            "{name} 往返必须恒等（forward-compat 覆盖 R2：unknown 块/span + payload_ref）"
        );
    }
}

// ---------------------------------------------------------------- M2 语料

fn corpus_dir() -> PathBuf {
    repo_dir().join("corpus")
}

fn read_text_file(p: &std::path::Path) -> String {
    std::fs::read_to_string(p).unwrap_or_else(|e| panic!("读取 {} 失败: {e}", p.display()))
}

#[test]
fn m2_corpus_roundtrip_identical() {
    for (file, import_fn) in [
        ("markdown/basic.md", 0),
        ("markdown/lossy.md", 0),
        ("html/basic.html", 1),
        ("html/lossy.html", 1),
    ] {
        let path = corpus_dir().join(file);
        let src = read_text_file(&path);
        let mut job = ImportJob::new(None);
        let out = if import_fn == 0 {
            azodoc_md::import(&src, &mut job)
        } else {
            azodoc_html::import(&src, &mut job)
        };
        let prima = out.content;
        assert_eq!(
            roundtrip(&prima),
            prima,
            "{file} 导入产物经 PM 往返必须恒等（M6 验收①：以 M2 语料为准）"
        );
    }
}

// ---------------------------------------------------------------- ID 规则

#[test]
fn id_allocation_rules() {
    let pm = json!({"type": "doc", "attrs": {"schema_version": "1.0", "extra": null}, "content": [
        {"type": "paragraph", "attrs": {"id": "", "extra": null}, "content": []},
        {"type": "paragraph", "attrs": {"id": "blk_00000000000000000000000001", "extra": null}, "content": []},
        {"type": "paragraph", "attrs": {"id": "blk_00000000000000000000000001", "extra": null}, "content": []},
        {"type": "paragraph", "attrs": {"id": "not-an-id", "extra": null}, "content": []},
        {"type": "section", "attrs": {"id": "", "extra": null}, "content": []}
    ]});
    let kinds = std::cell::RefCell::new(Vec::new());
    let mut seq = 0u32;
    let mut gen = |k: IdKind| -> String {
        kinds.borrow_mut().push(k);
        seq += 1;
        format!("blk_{:026}", 1000 + seq)
    };
    let r = azodoc_pm::pm_to_content_file(&pm, &mut gen).unwrap();

    assert_eq!(r.ids_assigned, 4, "缺失/重复/非法/section 共补发 4 个");
    assert_eq!(r.ids_deduplicated, 1, "其中重复 1 个");
    assert_eq!(
        kinds.into_inner(),
        vec![
            IdKind::Blk, // 空 id
            IdKind::Blk, // 重复
            IdKind::Blk, // 非法
            IdKind::Sec, // section
        ],
        "补发 ID 的种类必须跟随节点类型"
    );

    let ids: Vec<&str> = r.content_file["content"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids[1], "blk_00000000000000000000000001",
        "合法且未重复的 ID 必须保留"
    );
    let mut sorted = ids.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), ids.len(), "补发后全文档 ID 唯一");
    for id in &ids {
        assert!(
            azodoc_model::id::is_valid_id(id),
            "补发/保留的 ID `{id}` 必须合法"
        );
    }
}

#[test]
fn marks_rebuild_in_rank_order() {
    // PM 乱序 marks 输入 → 嵌套按 rank 重建：link(0) 在最外，code(5) 在最内
    let pm = json!({"type": "doc", "attrs": {"schema_version": "1.0", "extra": null}, "content": [
        {"type": "paragraph", "attrs": {"id": "blk_00000000000000000000000002", "extra": null}, "content": [
            {"type": "text", "text": "x", "marks": [
                {"type": "code", "attrs": {"extra": null}},
                {"type": "em", "attrs": {"extra": null}},
                {"type": "link", "attrs": {"url": "https://e.com", "title": null, "extra": null}},
                {"type": "strong", "attrs": {"extra": null}}
            ]}
        ]}
    ]});
    let mut no_alloc = |_k: IdKind| -> String { panic!("不应补发 ID") };
    let r = azodoc_pm::pm_to_content_file(&pm, &mut no_alloc).unwrap();
    let span = &r.content_file["content"][0]["content"][0];
    assert_eq!(span["type"], "link", "rank 0 在最外");
    assert_eq!(span["content"][0]["type"], "strong");
    assert_eq!(span["content"][0]["content"][0]["type"], "em");
    assert_eq!(
        span["content"][0]["content"][0]["content"][0]["type"], "code",
        "code 为最内叶"
    );
    assert_eq!(span["content"][0]["content"][0]["content"][0]["text"], "x");
}
