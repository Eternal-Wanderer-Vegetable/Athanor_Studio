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

//! M3 验收（语义层）：「编辑后重定位」存活场景。
//! 策略对应 azodoc-model.md §10：上下文匹配 → exact 重锚 → 迁移 → detached。

use azodoc_convert::semantics::relocate_annotations;
use serde_json::{json, Value};

fn blk(id: &str, text: &str) -> Value {
    json!({
        "id": format!("blk_{id}"),
        "type": "paragraph",
        "content": [{"type": "text", "text": text}]
    })
}

fn quote_annotation(id: &str, block: &str, exact: &str, prefix: &str, suffix: &str) -> Value {
    json!({
        "id": format!("ann_{id}"),
        "type": "Concept",
        "target": {
            "kind": "text_quote",
            "block": format!("blk_{block}"),
            "selector": {"type": "text_quote", "exact": exact, "prefix": prefix, "suffix": suffix}
        },
        "value": {"label": "x"},
        "author": {"type": "human"},
        "created_at": "2026-09-15T00:00:00Z"
    })
}

fn run(content: &Value, annotations: &mut Value) -> azodoc_convert::semantics::RelocateStats {
    relocate_annotations(content, annotations)
}

#[test]
fn untouched_document_keeps_annotations() {
    let content = json!({"schema_version": "1.0", "content": [
        blk("A", "Rust 是一门系统级编程语言。"),
        blk("B", "第二段文字。"),
    ]});
    let mut anns = json!({"schema_version": "1.0", "annotations": [
        quote_annotation("1", "A", "系统级", "Rust 是一门", "编程语言。")
    ]});
    let s = run(&content, &mut anns);
    assert_eq!(
        (s.unchanged, s.reanchored, s.moved, s.detached),
        (1, 0, 0, 0)
    );
    assert!(anns["annotations"][0].get("detached").is_none());
}

#[test]
fn edit_elsewhere_reanchors_context() {
    // 引用文字仍在，但前缀上下文因同块内其它文字被改而失配 → 重锚
    let content = json!({"schema_version": "1.0", "content": [
        blk("A", "Rust 是一门系统级编程语言。"),
    ]});
    let mut anns = json!({"schema_version": "1.0", "annotations": [
        quote_annotation("1", "A", "系统级", "前缀已失效", "后缀也失效")
    ]});
    let s = run(&content, &mut anns);
    assert_eq!((s.unchanged, s.reanchored), (0, 1));
    let sel = &anns["annotations"][0]["target"]["selector"];
    assert_eq!(sel["exact"], "系统级");
    assert_eq!(sel["prefix"], "Rust 是一门");
    assert_eq!(sel["suffix"], "编程语言。");
    assert!(anns["annotations"][0].get("detached").is_none());
}

#[test]
fn quote_moved_to_another_block_migrates() {
    // 原块已删掉该句；唯一出现在别的块 → 迁移
    let content = json!({"schema_version": "1.0", "content": [
        blk("A", "这段已经重写完毕。"),
        blk("B", "关键句子迁移到了这里，系统级很重要。"),
    ]});
    let mut anns = json!({"schema_version": "1.0", "annotations": [
        quote_annotation("1", "A", "系统级", "", "")
    ]});
    let s = run(&content, &mut anns);
    assert_eq!(s.moved, 1);
    assert_eq!(anns["annotations"][0]["target"]["block"], "blk_B");
    // 迁移后上下文被重写
    assert!(anns["annotations"][0]["target"]["selector"]["suffix"]
        .as_str()
        .unwrap()
        .contains("很重要"));
}

#[test]
fn deleted_text_is_detached_never_deleted() {
    let content = json!({"schema_version": "1.0", "content": [
        blk("A", "这段文字完全重写了。"),
    ]});
    let mut anns = json!({"schema_version": "1.0", "annotations": [
        quote_annotation("1", "A", "系统级", "", "")
    ]});
    let s = run(&content, &mut anns);
    assert_eq!(s.detached, 1);
    assert_eq!(
        anns["annotations"][0]["detached"],
        json!(true),
        "失配标注必须保留"
    );
}

#[test]
fn ambiguous_exact_is_detached() {
    // exact 在两个其它块都出现 → 无法唯一迁移
    let content = json!({"schema_version": "1.0", "content": [
        blk("A", "无关。"),
        blk("B", "系统级一。"),
        blk("C", "系统级二。"),
    ]});
    let mut anns = json!({"schema_version": "1.0", "annotations": [
        quote_annotation("1", "A", "系统级", "", "")
    ]});
    let s = run(&content, &mut anns);
    assert_eq!(s.detached, 1);
}

#[test]
fn block_target_missing_is_detached() {
    let content = json!({"schema_version": "1.0", "content": [
        blk("A", "文字。"),
    ]});
    let mut anns = json!({"schema_version": "1.0", "annotations": [
        json!({
            "id": "ann_1", "type": "ReviewStatus",
            "target": {"kind": "block", "block": "blk_GONE"},
            "value": {"status": "approved"},
            "author": {"type": "human"},
            "created_at": "2026-09-15T00:00:00Z"
        })
    ]});
    let s = run(&content, &mut anns);
    assert_eq!(s.detached, 1);
    assert_eq!(anns["annotations"][0]["detached"], json!(true));
}

#[test]
fn survival_rate_on_typical_edit() {
    // 典型 AI 编辑场景：5 条标注，1 条被删文本、4 条存活（含 1 条重锚）
    let _content = json!({"schema_version": "1.0", "content": [
        blk("A", "Azodoc 是结构化文档容器，支持安全降级。"),
        blk("B", "修订层记录 AI 作者。"),
    ]});
    let mut anns = json!({"schema_version": "1.0", "annotations": [
        quote_annotation("1", "A", "结构化文档容器", "Azodoc 是", "，支持"),
        quote_annotation("2", "A", "安全降级", "支持", "。"),
        quote_annotation("3", "B", "AI 作者", "记录", "。"),
        quote_annotation("4", "A", "不存在的引用", "", ""),
        quote_annotation("5", "B", "修订层", "", ""),
    ]});
    // 模拟编辑：A 段落措辞微调（引用仍在），B 不变；引用 4 的文字被删
    let edited = json!({"schema_version": "1.0", "content": [
        blk("A", "Azodoc 是一种结构化文档容器，可安全降级。"),
        blk("B", "修订层记录 AI 作者。"),
    ]});
    let s = run(&edited, &mut anns);
    // ann1/ann2/ann3：exact 仍在 → 重锚（ann3 的前缀上下文因措辞调整而失配）
    // ann5：完全未变；ann4：原文已删 → detached（保留不删）
    let survived = s.unchanged + s.reanchored + s.moved;
    assert_eq!(
        (s.unchanged, s.reanchored, s.moved, s.detached),
        (1, 3, 0, 1)
    );
    assert!(
        survived * 100 / (survived + s.detached) >= 60,
        "存活率应 ≥ 60%"
    );
}
