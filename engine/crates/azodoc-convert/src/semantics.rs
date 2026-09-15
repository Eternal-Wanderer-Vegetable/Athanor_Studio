//! 语义标注的编辑后重定位（azodoc-model.md §10 的实现）。
//!
//! 锚定基准：块的纯文本（azodoc-convert::plain_text_of_block）。
//! 策略：① `prefix + exact + suffix` 仍在原块 → 未受影响；
//!       ② 否则 `exact` 仍在原块 → 重写上下文（reanchored）；
//!       ③ 否则 `exact` 在其它块唯一出现 → 迁移目标块（moved）；
//!       ④ 都失败 → 标记 `detached`（R2 通道的额外成员，绝不删除标注）。

use crate::plain_text_of_block;
use serde_json::{json, Map, Value};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RelocateStats {
    pub unchanged: u64,
    pub reanchored: u64,
    pub moved: u64,
    pub detached: u64,
}

const CONTEXT_CHARS: usize = 8;

/// 对 annotations.json 执行重定位。原位更新，返回统计。
pub fn relocate_annotations(content: &Value, annotations: &mut Value) -> RelocateStats {
    let mut stats = RelocateStats::default();

    // 块纯文本索引与现存 ID 集合
    let mut block_texts: Map<String, Value> = Map::new();
    let mut block_ids: Vec<String> = Vec::new();
    let mut asset_ids: Vec<String> = Vec::new();
    collect_index(content, &mut block_texts, &mut block_ids, &mut asset_ids);

    let Some(arr) = annotations
        .get_mut("annotations")
        .and_then(Value::as_array_mut)
    else {
        return stats;
    };
    for ann in arr.iter_mut() {
        let kind = ann
            .pointer("/target/kind")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        match kind.as_str() {
            "text_quote" => relocate_text_quote(ann, &block_texts, &mut stats),
            "block" | "section" => {
                let id_key = if kind == "block" { "block" } else { "section" };
                let id = ann
                    .pointer(&format!("/target/{id_key}"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let exists = if kind == "block" {
                    block_ids.iter().any(|x| x == id)
                } else {
                    block_ids.iter().any(|x| x == id) // section id 也收进 block_ids
                };
                if exists {
                    clear_detached(ann);
                    stats.unchanged += 1;
                } else {
                    mark_detached(ann);
                    stats.detached += 1;
                }
            }
            "asset" => {
                let id = ann
                    .pointer("/target/asset")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if asset_ids.iter().any(|x| x == id) {
                    clear_detached(ann);
                    stats.unchanged += 1;
                } else {
                    mark_detached(ann);
                    stats.detached += 1;
                }
            }
            "document" => {
                clear_detached(ann);
                stats.unchanged += 1;
            }
            _ => {}
        }
    }
    stats
}

fn collect_index(
    v: &Value,
    block_texts: &mut Map<String, Value>,
    block_ids: &mut Vec<String>,
    asset_ids: &mut Vec<String>,
) {
    if let Some(obj) = v.as_object() {
        let t = obj.get("type").and_then(Value::as_str).unwrap_or("");
        if let Some(id) = obj.get("id").and_then(Value::as_str) {
            if matches!(
                t,
                "section"
                    | "paragraph"
                    | "heading"
                    | "quote"
                    | "list"
                    | "list_item"
                    | "code_block"
                    | "table"
                    | "figure"
                    | "image"
                    | "math_block"
                    | "callout"
                    | "embed"
                    | "footnote"
                    | "unknown"
            ) {
                block_texts.insert(id.to_string(), json!(plain_text_of_block(v)));
                block_ids.push(id.to_string());
            }
        }
        if let Some(asset) = obj.get("asset").and_then(Value::as_str) {
            if let Some(rest) = asset.strip_prefix("asset://") {
                let id = rest.split('/').next().unwrap_or(rest);
                if !asset_ids.iter().any(|x| x == id) {
                    asset_ids.push(id.to_string());
                }
            }
        }
        if t == "text_quote" {
            return;
        }
        for key in ["children", "items", "cells", "rows", "content", "caption"] {
            if let Some(arr) = obj.get(key).and_then(Value::as_array) {
                for item in arr {
                    collect_index(item, block_texts, block_ids, asset_ids);
                }
            }
        }
    }
}

fn relocate_text_quote(
    ann: &mut Value,
    block_texts: &Map<String, Value>,
    stats: &mut RelocateStats,
) {
    // 先只读决策，再原位应用（避免同源可变借用冲突）
    let action = decide(ann, block_texts);
    match action {
        QuoteAction::Unchanged => {
            clear_detached(ann);
            stats.unchanged += 1;
        }
        QuoteAction::Reanchor {
            new_block,
            prefix,
            suffix,
        } => {
            if let Some(new_block) = new_block {
                ann["target"]["block"] = json!(new_block);
                stats.moved += 1;
            } else {
                stats.reanchored += 1;
            }
            ann["target"]["selector"]["prefix"] = json!(prefix);
            ann["target"]["selector"]["suffix"] = json!(suffix);
            clear_detached(ann);
        }
        QuoteAction::Detach => {
            mark_detached(ann);
            stats.detached += 1;
        }
    }
}

enum QuoteAction {
    Unchanged,
    Reanchor {
        new_block: Option<String>,
        prefix: String,
        suffix: String,
    },
    Detach,
}

fn decide(ann: &Value, block_texts: &Map<String, Value>) -> QuoteAction {
    let target = match ann.get("target") {
        Some(t) => t,
        None => return QuoteAction::Detach,
    };
    let block_id = target
        .get("block")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let sel = target.get("selector");
    let Some(sel) = sel else {
        return QuoteAction::Detach;
    };
    let exact = sel
        .get("exact")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if exact.is_empty() {
        return QuoteAction::Detach;
    }
    let prefix = sel.get("prefix").and_then(Value::as_str).unwrap_or("");
    let suffix = sel.get("suffix").and_then(Value::as_str).unwrap_or("");

    let current_text = block_texts
        .get(&block_id)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    // ① 上下文仍完整匹配
    if !current_text.is_empty() && current_text.contains(&format!("{prefix}{exact}{suffix}")) {
        return QuoteAction::Unchanged;
    }
    // ② exact 仍在原块：重写上下文
    if let Some(pos) = current_text.find(&exact) {
        let (p, s) = context_of(&current_text, pos, exact.chars().count());
        return QuoteAction::Reanchor {
            new_block: None,
            prefix: p,
            suffix: s,
        };
    }
    // ③ exact 在其它块唯一出现：迁移
    let mut hits: Vec<String> = block_texts
        .iter()
        .filter(|(id, text)| {
            id.as_str() != block_id && text.as_str().unwrap_or("").contains(&exact)
        })
        .map(|(id, _)| id.clone())
        .collect();
    hits.sort();
    if hits.len() == 1 {
        let new_id = hits[0].clone();
        let text = block_texts
            .get(&new_id)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let pos = text.find(&exact).unwrap_or(0);
        let (p, s) = context_of(&text, pos, exact.chars().count());
        return QuoteAction::Reanchor {
            new_block: Some(new_id),
            prefix: p,
            suffix: s,
        };
    }
    // ④ 失配：detached（绝不删除）
    QuoteAction::Detach
}

fn mark_detached(ann: &mut Value) {
    if ann.get("detached") != Some(&Value::Bool(true)) {
        ann["detached"] = json!(true);
    }
}

fn clear_detached(ann: &mut Value) {
    if let Some(obj) = ann.as_object_mut() {
        obj.remove("detached");
    }
}

/// 取 exact 在 text 中 [pos, pos+len) 的前后上下文。
fn context_of(text: &str, pos: usize, len_chars: usize) -> (String, String) {
    let chars: Vec<char> = text.chars().collect();
    // pos 是字节下标；转换为字符下标
    let byte_prefix = &text[..pos];
    let start_char = byte_prefix.chars().count();
    let end_char = (start_char + len_chars).min(chars.len());
    let p_start = start_char.saturating_sub(CONTEXT_CHARS);
    let p_end = start_char;
    let s_start = end_char;
    let s_end = (end_char + CONTEXT_CHARS).min(chars.len());
    (
        chars[p_start..p_end].iter().collect(),
        chars[s_start..s_end].iter().collect(),
    )
}
