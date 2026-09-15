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

//! Prima → Pandoc AST（导出映射）。嵌入资产物化到临时目录后以文件路径引用
//! （docx writer 对 data: URI 的支持不确定，文件路径最稳）。

use azodoc_convert::{ExportDoc, LossClass, LossLog};
use serde_json::{json, Map, Value};

/// Pandoc Attr 三元组 (id, classes, keyvals) 的空值。
const ATTRS: Value = Value::Null; // 占位（下方用函数构造）
fn attrs() -> Value {
    json!(["", [], []])
}
use std::path::Path;

pub struct ExportCtx<'a> {
    pub doc: &'a ExportDoc,
    /// 嵌入资产物化目录
    pub asset_dir: &'a Path,
    /// 脚注定义：block id → 定义块
    pub fn_defs: std::collections::HashMap<String, Value>,
}

/// Prima → pandoc AST JSON（含 meta 与 api 版本）。
pub fn prima_to_ast(doc: &ExportDoc, asset_dir: &Path, log: &mut LossLog) -> Result<Value, String> {
    std::fs::create_dir_all(asset_dir).map_err(|e| e.to_string())?;
    let mut ctx = ExportCtx {
        doc,
        asset_dir,
        fn_defs: std::collections::HashMap::new(),
    };
    // 收集脚注定义（按文档序编号，label 优先）
    let mut auto = 0usize;
    collect_fn_defs(doc.content.get("content"), &mut ctx, &mut auto);

    let mut blocks = Vec::new();
    if let Some(arr) = doc.content.get("content").and_then(Value::as_array) {
        for node in arr {
            blocks.extend(block_ast(node, &mut ctx, log));
        }
    }

    let mut meta = Map::new();
    if let Some(t) = &doc.title {
        meta.insert(
            "title".to_string(),
            json!({"t": "MetaInlines", "c": [{"t": "Str", "c": t}]}),
        );
    }
    if let Some(lang) = &doc.language {
        meta.insert(
            "language".to_string(),
            json!({"t": "MetaString", "c": lang}),
        );
    }

    Ok(json!({
        "pandoc-api-version": [1, 23, 1],
        "meta": meta,
        "blocks": blocks,
    }))
}

fn collect_fn_defs(v: Option<&Value>, ctx: &mut ExportCtx, auto: &mut usize) {
    let Some(v) = v else { return };
    if v.get("type").and_then(Value::as_str) == Some("footnote") {
        if let Some(id) = v.get("id").and_then(Value::as_str) {
            let _label = v.get("label"); // docx 导出不使用标签，仅保序
            let _ = _label;
            if !ctx.fn_defs.contains_key(id) {
                *auto += 1;
                ctx.fn_defs.insert(id.to_string(), v.clone());
            }
        }
    }
    for key in ["children", "items", "cells", "rows", "content", "caption"] {
        if let Some(arr) = v.get(key).and_then(Value::as_array) {
            for item in arr {
                collect_fn_defs(Some(item), ctx, auto);
            }
        }
    }
}

fn blocks_of(node: &Value, key: &str, ctx: &mut ExportCtx, log: &mut LossLog) -> Vec<Value> {
    let mut out = Vec::new();
    if let Some(children) = node.get(key).and_then(Value::as_array) {
        for c in children {
            out.extend(block_ast(c, ctx, log));
        }
    }
    out
}

fn block_ast(node: &Value, ctx: &mut ExportCtx, log: &mut LossLog) -> Vec<Value> {
    let t = node.get("type").and_then(Value::as_str).unwrap_or("");
    match t {
        "section" => {
            // 组织性糖：透明展开
            log.node_none();
            blocks_of(node, "children", ctx, log)
        }
        "heading" => {
            let level = node.get("level").and_then(Value::as_i64).unwrap_or(1);
            let inlines = inline_ast(node.get("content"), ctx, log);
            log.node_none();
            vec![json!({"t": "Header", "c": [level, attrs(), inlines]})]
        }
        "paragraph" => {
            let inlines = inline_ast(node.get("content"), ctx, log);
            if inlines.is_empty() {
                log.node_none();
                Vec::new()
            } else {
                vec![json!({"t": "Para", "c": inlines})]
            }
        }
        "quote" => {
            let children = blocks_of(node, "children", ctx, log);
            log.node_none();
            vec![json!({"t": "BlockQuote", "c": children})]
        }
        "list" => {
            let style = node
                .get("style")
                .and_then(Value::as_str)
                .unwrap_or("bullet");
            let items_ast: Vec<Value> = node
                .get("items")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .map(|item| json!(blocks_of(item, "children", ctx, log)))
                        .collect()
                })
                .unwrap_or_default();
            log.node_none();
            if style == "ordered" {
                let start = node.get("start").and_then(Value::as_i64).unwrap_or(1);
                vec![json!({"t": "OrderedList", "c": [
                    [start, {"t": "Decimal"}, {"t": "Period"}],
                    items_ast
                ]})]
            } else {
                vec![json!({"t": "BulletList", "c": items_ast})]
            }
        }
        "code_block" => {
            let text = node.get("text").and_then(Value::as_str).unwrap_or("");
            let lang = node.get("language").and_then(Value::as_str).unwrap_or("");
            let classes = if lang.is_empty() {
                vec![]
            } else {
                vec![json!(lang)]
            };
            log.node_none();
            vec![json!({"t": "CodeBlock", "c": [["", classes, []], text]})]
        }
        "horizontal_rule" => {
            log.node_none();
            vec![json!({"t": "HorizontalRule"})]
        }
        "table" => table_ast(node, ctx, log),
        "figure" | "image" => figure_ast(node, ctx, log),
        "math_block" => {
            let latex = node.get("latex").and_then(Value::as_str).unwrap_or("");
            log.node_none();
            vec![json!({"t": "Para", "c": [
                {"t": "Math", "c": [{"t": "DisplayMath"}, latex]}
            ]})]
        }
        "callout" => {
            let variant = node
                .get("variant")
                .and_then(Value::as_str)
                .unwrap_or("note");
            let mut children = Vec::new();
            if let Some(arr) = node.get("children").and_then(Value::as_array) {
                for c in arr {
                    children.extend(block_ast(c, ctx, log));
                }
            }
            log.record(
                LossClass::Partial,
                "callout_docx",
                None,
                "degraded",
                variant,
                "提示块在 DOCX 导出中以 Div(类) 呈现，样式取决于阅读器".to_string(),
            );
            let classes = vec![json!(variant)];
            vec![json!({"t": "Div", "c": [["", classes, []], children]})]
        }
        "footnote" => Vec::new(), // 由 footnote_ref 内联为 Note
        "embed" => {
            let asset = node.get("asset").and_then(Value::as_str).unwrap_or("");
            let name = ctx.doc.asset_filename(asset).unwrap_or(asset).to_string();
            let url = resolve_asset_path(ctx, asset);
            log.record(
                LossClass::Partial,
                "embed_docx",
                None,
                "degraded",
                asset,
                "内嵌资源在 DOCX 中以链接呈现".to_string(),
            );
            vec![json!({"t": "Para", "c": [
                {"t": "Link", "c": [attrs(), [{"t": "Str", "c": name}], [url, ""]]}
            ]})]
        }
        "unknown" => {
            log.record(
                LossClass::PreservedRaw,
                "unknown_block_docx",
                None,
                "preserved",
                node.get("payload_ref")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
                "未知内容在 DOCX 导出中被省略，原文保留于容器".to_string(),
            );
            Vec::new()
        }
        _ => {
            log.record(
                LossClass::Partial,
                "docx_export_fallback",
                None,
                "degraded",
                t,
                "未知块类型按子内容降级导出".to_string(),
            );
            blocks_of(node, "children", ctx, log)
        }
    }
}

fn table_ast(node: &Value, ctx: &mut ExportCtx, log: &mut LossLog) -> Vec<Value> {
    let header_row = node.get("header_row").and_then(Value::as_bool) == Some(true);
    let cols = node
        .get("columns")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let rows = node
        .get("rows")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    // 列名：header_row=false 时列名无处安放
    if !header_row
        && cols.iter().any(|c| {
            !c.get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .is_empty()
        })
    {
        log.record(
            LossClass::Partial,
            "column_names",
            None,
            "degraded",
            "table",
            "无表头行的列名在 DOCX 导出中被丢弃".to_string(),
        );
    }

    let align_default = json!({"t": "AlignDefault"});
    let colspecs: Vec<Value> = cols
        .iter()
        .map(|_| json!([align_default, {"t": "ColWidth", "c": 0.0}]))
        .collect();

    let row_ast = |row: &Value, ctx: &mut ExportCtx, log: &mut LossLog| -> Value {
        let cells: Vec<Value> = row
            .get("cells")
            .and_then(Value::as_array)
            .map(|cs| {
                cs.iter()
                    .map(|c| {
                        let children = blocks_of(c, "children", ctx, log);
                        let rs = c.get("rowSpan").and_then(Value::as_i64).unwrap_or(1);
                        let cs2 = c.get("colSpan").and_then(Value::as_i64).unwrap_or(1);
                        // Row/Cell 在 pandoc JSON 中是裸元组（非构造器包装）
                        json!([attrs(), {"t": "AlignDefault"}, rs, cs2, children])
                    })
                    .collect()
            })
            .unwrap_or_default();
        json!([attrs(), cells])
    };

    let (head, body_rows): (Vec<Value>, Vec<Value>) = if header_row && !rows.is_empty() {
        (
            vec![row_ast(&rows[0], ctx, log)],
            rows[1..].iter().map(|r| row_ast(r, ctx, log)).collect(),
        )
    } else {
        (
            Vec::new(),
            rows.iter().map(|r| row_ast(r, ctx, log)).collect(),
        )
    };

    log.node_none();
    vec![json!({"t": "Table", "c": [
        attrs(),
        [null, []],
        colspecs,
        [attrs(), head],
        [[attrs(), 0, [], body_rows]],
        [attrs(), []]
    ]})]
}

fn figure_ast(node: &Value, ctx: &mut ExportCtx, log: &mut LossLog) -> Vec<Value> {
    let asset = node.get("asset").and_then(Value::as_str).unwrap_or("");
    let alt = node.get("alt").and_then(Value::as_str).unwrap_or("");
    let url = resolve_asset_path(ctx, asset);
    let img = json!({"t": "Image", "c": [
        attrs(),
        [{"t": "Str", "c": alt}],
        [url, ""]
    ]});
    log.node_none();
    if t_of_block(node) == "figure" {
        let caption_inlines = inline_ast(node.get("caption"), ctx, log);
        let caption = if caption_inlines.is_empty() {
            json!([null, []])
        } else {
            json!([null, [{"t": "Para", "c": caption_inlines}]])
        };
        vec![json!({"t": "Figure", "c": [attrs(), caption, [
            {"t": "Plain", "c": [img]}
        ]]})]
    } else {
        vec![json!({"t": "Para", "c": [img]})]
    }
}

fn t_of_block(node: &Value) -> String {
    node.get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// 嵌入资产物化为文件，返回 pandoc 可读的路径；外链原样返回。
fn resolve_asset_path(ctx: &mut ExportCtx, asset_field: &str) -> String {
    let id = asset_field
        .strip_prefix("asset://")
        .map(|s| s.split('/').next().unwrap_or(s).to_string())
        .unwrap_or_default();
    for a in &ctx.doc.assets {
        if a.id == id {
            if a.storage == "external" {
                return a.url.clone().unwrap_or_default();
            }
            if let Some(bytes) = &a.bytes {
                let path = ctx.asset_dir.join(format!(
                    "{}_{}",
                    &id[id.len().saturating_sub(6)..],
                    a.filename
                ));
                if std::fs::write(&path, bytes).is_ok() {
                    return path.to_string_lossy().to_string();
                }
            }
            return a.url.clone().unwrap_or_default();
        }
    }
    asset_field.to_string()
}

// ---------------------------------------------------------------- 行内

fn inline_ast(spans: Option<&Value>, ctx: &mut ExportCtx, log: &mut LossLog) -> Vec<Value> {
    let mut out = Vec::new();
    if let Some(arr) = spans.and_then(Value::as_array) {
        for s in arr {
            out.extend(span_ast(s, ctx, log));
        }
    }
    out
}

/// 文本拆分为 Str/Space 序列（pandoc 需要 Space 节点）。
fn text_ast(text: &str) -> Vec<Value> {
    let mut out = Vec::new();
    let mut word = String::new();
    for c in text.chars() {
        if c == ' ' {
            if !word.is_empty() {
                out.push(json!({"t": "Str", "c": word}));
                word = String::new();
            }
            out.push(json!({"t": "Space"}));
        } else {
            word.push(c);
        }
    }
    if !word.is_empty() {
        out.push(json!({"t": "Str", "c": word}));
    }
    out
}

fn span_ast(span: &Value, ctx: &mut ExportCtx, log: &mut LossLog) -> Vec<Value> {
    let t = span.get("type").and_then(Value::as_str).unwrap_or("");
    match t {
        "text" => {
            let text = span.get("text").and_then(Value::as_str).unwrap_or("");
            let nodes = text_ast(text);
            if nodes.is_empty() {
                Vec::new()
            } else {
                log.node_none();
                nodes
            }
        }
        "hard_break" => {
            log.node_none();
            vec![json!({"t": "LineBreak"})]
        }
        "code" => {
            let text = span.get("text").and_then(Value::as_str).unwrap_or("");
            log.node_none();
            vec![json!({"t": "Code", "c": [attrs(), text]})]
        }
        "em" => wrap("Emph", span, ctx, log),
        "strong" => wrap("Strong", span, ctx, log),
        "strike" => wrap("Strikeout", span, ctx, log),
        "underline" => wrap("Underline", span, ctx, log),
        "link" => {
            let url = span.get("url").and_then(Value::as_str).unwrap_or("");
            let title = span.get("title").and_then(Value::as_str).unwrap_or("");
            let inlines = inline_ast(span.get("content"), ctx, log);
            log.node_none();
            vec![json!({"t": "Link", "c": [attrs(), inlines, [url, title]]})]
        }
        "inline_math" => {
            let latex = span.get("latex").and_then(Value::as_str).unwrap_or("");
            log.node_none();
            vec![json!({"t": "Math", "c": [{"t": "InlineMath"}, latex]})]
        }
        "inline_image" => {
            let asset = span.get("asset").and_then(Value::as_str).unwrap_or("");
            let alt = span.get("alt").and_then(Value::as_str).unwrap_or("");
            let url = resolve_asset_path(ctx, asset);
            log.node_none();
            vec![json!({"t": "Image", "c": [attrs(), [{"t": "Str", "c": alt}], [url, ""]]})]
        }
        "footnote_ref" => {
            let id = span.get("id").and_then(Value::as_str).unwrap_or("");
            let def = ctx.fn_defs.get(id).cloned();
            match def {
                Some(def) => {
                    let blocks = blocks_of(&def, "children", ctx, log);
                    log.node_none();
                    vec![json!({"t": "Note", "c": blocks})]
                }
                None => Vec::new(),
            }
        }
        "unknown" => {
            log.record(
                LossClass::PreservedRaw,
                "unknown_span_docx",
                None,
                "preserved",
                span.get("payload_ref")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
                "未知行内内容在 DOCX 导出中被省略，原文保留于容器".to_string(),
            );
            Vec::new()
        }
        "mention" => {
            log.record(
                LossClass::Partial,
                "mention_docx",
                None,
                "degraded",
                span.get("target").and_then(Value::as_str).unwrap_or(""),
                "提及在 DOCX 中以纯文本呈现".to_string(),
            );
            text_ast(span.get("target").and_then(Value::as_str).unwrap_or(""))
        }
        "cite" => {
            log.record(
                LossClass::Partial,
                "cite_docx",
                None,
                "degraded",
                span.get("key").and_then(Value::as_str).unwrap_or(""),
                "引用键在 DOCX 中以纯文本呈现".to_string(),
            );
            text_ast(span.get("key").and_then(Value::as_str).unwrap_or(""))
        }
        _ => inline_ast(span.get("content"), ctx, log),
    }
}

fn wrap(kind: &str, span: &Value, ctx: &mut ExportCtx, log: &mut LossLog) -> Vec<Value> {
    let inlines = inline_ast(span.get("content"), ctx, log);
    log.node_none();
    vec![json!({"t": kind, "c": inlines})]
}
