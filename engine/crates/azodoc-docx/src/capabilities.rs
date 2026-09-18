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
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! DOCX 导出能力矩阵（版本化）。
//!
//! 导出前扫描 Prima 内容树，按节点/行内类型归类到四个状态：
//! - `supported`：有完整 DOCX 表达（ast_out 无损路径）
//! - `degraded`：导出为替代形态（callout→Div、embed→链接、mention/cite→纯文本、
//!   未知类型→子内容降级）
//! - `preserved`：不进入 DOCX 但原文保留于容器（unknown/payload_ref）
//! - `unsupported`：映射已知的结构性空缺（当前为保留状态位，矩阵版本化以容纳后续）
//!
//! 矩阵挂在转换报告的 `capabilities` 成员（schema additionalProperties 允许），
//! `matrix_version` 冻结结构契约；新增字段一律可选。

use serde_json::{json, Value};

/// 能力矩阵结构版本（结构变化才递增）。
pub const MATRIX_VERSION: &str = "1.0";

/// 块类型 → 能力状态。
fn block_status(t: &str) -> &'static str {
    match t {
        "section" | "heading" | "paragraph" | "quote" | "list" | "list_item" | "code_block"
        | "horizontal_rule" | "page_break" | "table" | "table_row" | "table_cell"
        | "table_header" | "figure" | "image" | "math_block" | "footnote" => "supported",
        "callout" | "embed" => "degraded",
        "unknown" | "unknown_block" => "preserved",
        _ => "degraded", // ast_out 兜底：未知块按 children 降级
    }
}

/// 行内类型 → 能力状态。
fn span_status(t: &str) -> &'static str {
    match t {
        "text" | "hard_break" | "code" | "em" | "strong" | "strike" | "underline" | "link"
        | "inline_math" | "inline_image" | "footnote_ref" | "line_break" => "supported",
        "mention" | "cite" => "degraded",
        "unknown" => "preserved",
        _ => "degraded", // span_ast 兜底：未知 span 按 content 展开
    }
}

/// 生成 DOCX 导出能力矩阵。`content` 为 content.json 的完整 Value
/// （含 `content` 数组）。按类型聚合，保持确定性（BTreeMap 序）。
pub fn capability_matrix(content: &Value) -> Value {
    let mut per_type: std::collections::BTreeMap<String, (String, u64)> =
        std::collections::BTreeMap::new();
    let mut counts = std::collections::BTreeMap::from([
        ("supported", 0u64),
        ("degraded", 0u64),
        ("preserved", 0u64),
        ("unsupported", 0u64),
    ]);

    fn bump(
        per_type: &mut std::collections::BTreeMap<String, (String, u64)>,
        counts: &mut std::collections::BTreeMap<&'static str, u64>,
        t: &str,
        status: &'static str,
    ) {
        *counts.entry(status).or_insert(0) += 1;
        let e = per_type
            .entry(t.to_string())
            .or_insert((status.to_string(), 0));
        if e.0 != status {
            // 同一类型只会有一个状态（静态表），防御性保留首个判定
            e.0 = status.to_string();
        }
        e.1 += 1;
    }

    fn walk(
        v: &Value,
        per_type: &mut std::collections::BTreeMap<String, (String, u64)>,
        counts: &mut std::collections::BTreeMap<&'static str, u64>,
    ) {
        match v {
            Value::Array(arr) => {
                for x in arr {
                    walk(x, per_type, counts);
                }
            }
            Value::Object(_) => {
                let t = v.get("type").and_then(Value::as_str).unwrap_or("");
                let is_block = matches!(
                    t,
                    "section"
                        | "heading"
                        | "paragraph"
                        | "quote"
                        | "list"
                        | "list_item"
                        | "code_block"
                        | "horizontal_rule"
                        | "page_break"
                        | "table"
                        | "table_row"
                        | "table_cell"
                        | "table_header"
                        | "figure"
                        | "image"
                        | "math_block"
                        | "footnote"
                        | "callout"
                        | "embed"
                        | "unknown_block"
                );
                let is_span = matches!(
                    t,
                    "text"
                        | "hard_break"
                        | "code"
                        | "em"
                        | "strong"
                        | "strike"
                        | "underline"
                        | "link"
                        | "inline_math"
                        | "inline_image"
                        | "footnote_ref"
                        | "line_break"
                        | "mention"
                        | "cite"
                );
                if !t.is_empty() && t != "doc" {
                    let status = if is_block {
                        block_status(t)
                    } else if is_span || v.get("content").is_some() || t == "unknown" {
                        span_status(t)
                    } else {
                        // 未识别的带 type 对象：按未知块处理（保守归入 preserved）
                        block_status("unknown_block")
                    };
                    bump(per_type, counts, t, status);
                }
                for (_k, val) in v.as_object().unwrap() {
                    walk(val, per_type, counts);
                }
            }
            _ => {}
        }
    }

    if let Some(arr) = content.get("content").and_then(Value::as_array) {
        for node in arr {
            walk(node, &mut per_type, &mut counts);
        }
    }

    let features: Vec<Value> = per_type
        .into_iter()
        .map(|(t, (status, count))| json!({"type": t, "status": status, "count": count}))
        .collect();

    json!({
        "matrix_version": MATRIX_VERSION,
        "target": "docx",
        "counts": counts,
        "features": features,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn matrix_classifies_supported_degraded_preserved() {
        let content = json!({
            "schema_version": "1.0",
            "content": [
                {"type": "paragraph", "content": [
                    {"type": "text", "text": "hi"},
                    {"type": "mention", "target": "@x"},
                    {"type": "footnote_ref", "id": "blk_f1"},
                ]},
                {"type": "callout", "variant": "warning", "children": [
                    {"type": "paragraph", "content": [{"type": "text", "text": "!" }]}
                ]},
                {"type": "unknown", "payload_ref": "preserved/ooxml/part-0001.bin"},
                {"type": "page_break"},
            ]
        });
        let m = capability_matrix(&content);
        assert_eq!(m["matrix_version"], "1.0");
        assert_eq!(m["target"], "docx");
        // supported: paragraph×2, text×2, footnote_ref, page_break = 6
        assert_eq!(m["counts"]["supported"], 6);
        // degraded: mention, callout = 2
        assert_eq!(m["counts"]["degraded"], 2);
        // preserved: unknown = 1
        assert_eq!(m["counts"]["preserved"], 1);
        let features = m["features"].as_array().unwrap();
        let callout = features.iter().find(|f| f["type"] == "callout").unwrap();
        assert_eq!(callout["status"], "degraded");
        assert_eq!(callout["count"], 1);
        // 确定性：同一输入字节级稳定（BTreeMap 序）
        assert_eq!(
            serde_json::to_string(&m).unwrap(),
            serde_json::to_string(&capability_matrix(&content)).unwrap()
        );
    }
}
