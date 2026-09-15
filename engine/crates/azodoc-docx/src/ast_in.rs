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

//! Pandoc AST → Prima（导入映射；映射表见落地方案 §8.1）。
//!
//! 形状依据 pandoc 3.11（API 1.23.1.2）实测：Math c=[{t:MathType}, latex]、
//! Table c=[attr, caption, colspecs, TableHead, [TableBody], TableFoot]、
//! Figure c=[attr, caption, blocks]、OrderedList c=[[start, style, delim], items]。

use azodoc_convert::{merge_text_spans, wrap_sections, ImportJob, LossClass, LossLog};
use serde_json::{json, Map, Value};
use std::path::PathBuf;

pub struct ImportCtx<'a> {
    pub job: &'a mut ImportJob,
    pub extract_dir: PathBuf,
    pub log: &'a mut LossLog,
    /// Note（脚注）展开产生的 footnote 块，追加到文档末尾
    pub pending_footnotes: Vec<Value>,
    /// 段内出现的 DisplayMath 提升为块，追加到当前段落之后
    pub pending_blocks: Vec<Value>,
}

/// pandoc AST → Prima content.json。
pub fn ast_to_prima(ast: &Value, ctx: &mut ImportCtx) -> Value {
    let empty = Vec::new();
    let blocks = ast
        .get("blocks")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let mut out = Vec::new();
    for b in blocks {
        out.extend(map_block(b, ctx));
    }
    out.extend(ctx.pending_footnotes.drain(..));
    let content = json!({
        "schema_version": "1.0",
        "content": wrap_sections(&mut ctx.job.idgen, out),
    });
    let mut content = content;
    merge_text_spans(&mut content);
    content
}

fn t_of(node: &Value) -> String {
    node.get("t")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn c_of(node: &Value) -> Vec<Value> {
    node.get("c")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn log_none(ctx: &mut ImportCtx) {
    ctx.log.node_none();
}

pub fn map_block(node: &Value, ctx: &mut ImportCtx) -> Vec<Value> {
    let t = t_of(node);
    match t.as_str() {
        "Header" => {
            let c = c_of(node);
            let level = c.first().and_then(Value::as_i64).unwrap_or(1).clamp(1, 6);
            let content = match c.get(2).and_then(Value::as_array) {
                Some(i) => map_inlines(i, ctx),
                None => Vec::new(),
            };
            log_none(ctx);
            vec![json!({
                "id": ctx.job.idgen.uid("blk"),
                "type": "heading",
                "level": level,
                "content": content,
            })]
        }
        "Para" | "Plain" => {
            let inlines = c_of(node);
            if let Some(img) = standalone_image(&inlines) {
                return vec![figure_from_image(&img, ctx)];
            }
            if let Some(m) = only_display_math(&inlines) {
                return vec![math_block_from(&m, ctx)];
            }
            let content = map_inlines(&inlines, ctx);
            let content = take_pending(content, ctx);
            if content.is_empty() {
                log_none(ctx);
                Vec::new()
            } else {
                vec![json!({
                    "id": ctx.job.idgen.uid("blk"),
                    "type": "paragraph",
                    "content": content,
                })]
            }
        }
        "BlockQuote" => {
            let children = flat_blocks(&c_of(node), ctx);
            log_none(ctx);
            vec![json!({
                "id": ctx.job.idgen.uid("blk"),
                "type": "quote",
                "children": children,
            })]
        }
        "BulletList" => vec![list_from_items(&c_of(node), false, 1, ctx)],
        "OrderedList" => {
            let c = c_of(node);
            let (start, _) = list_attrs(c.first());
            let items = c
                .get(1)
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            vec![list_from_items(&items, true, start, ctx)]
        }
        "CodeBlock" => {
            let c = c_of(node);
            let text = c.get(1).and_then(Value::as_str).unwrap_or("").to_string();
            let lang = c
                .first()
                .and_then(Value::as_array)
                .and_then(|a| a.get(1))
                .and_then(Value::as_array)
                .and_then(|classes| classes.first())
                .and_then(Value::as_str)
                .map(String::from);
            log_none(ctx);
            vec![json!({
                "id": ctx.job.idgen.uid("blk"),
                "type": "code_block",
                "language": lang,
                "text": text,
            })]
        }
        "HorizontalRule" => {
            log_none(ctx);
            vec![json!({"id": ctx.job.idgen.uid("blk"), "type": "horizontal_rule"})]
        }
        "Table" => vec![table_from(node, ctx)],
        "Figure" => vec![figure_from_figure(node, ctx)],
        "Div" => {
            let classes = node
                .pointer("/c/0/1")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let children = flat_blocks(&c_of(node), ctx);
            if !classes.is_empty() {
                ctx.log.record(
                    LossClass::Partial,
                    "div_class",
                    None,
                    "degraded",
                    &classes
                        .iter()
                        .map(|c| c.as_str().unwrap_or(""))
                        .collect::<Vec<_>>()
                        .join(" "),
                    "Div 的样式类无法映射到 v1 模型，已提升子内容".to_string(),
                );
            }
            children
        }
        "RawBlock" => {
            let c = c_of(node);
            let format = c
                .first()
                .and_then(Value::as_str)
                .unwrap_or("html")
                .to_string();
            let text = c.get(1).and_then(Value::as_str).unwrap_or("").to_string();
            let payload_ref = ctx.job.add_preserved(&format, "raw", text.into_bytes());
            ctx.log.record(
                LossClass::PreservedRaw,
                "raw_block",
                None,
                "preserved",
                &payload_ref,
                "原始块内容以原始形式保留，未参与建模".to_string(),
            );
            vec![json!({
                "id": ctx.job.idgen.uid("blk"),
                "type": "unknown",
                "origin": format,
                "loss_class": "preserved_raw",
                "summary": format!("原始 {format} 块"),
                "payload_ref": payload_ref,
                "content": [],
            })]
        }
        other => {
            ctx.log.record(
                LossClass::Partial,
                "ast_block_fallback",
                None,
                "degraded",
                other,
                format!("Pandoc 块类型 {other} 无精确映射，按子内容降级处理"),
            );
            flat_blocks(&c_of(node), ctx)
        }
    }
}

fn list_attrs(v: Option<&Value>) -> (i64, String) {
    let Some(arr) = v.and_then(Value::as_array) else {
        return (1, String::new());
    };
    let start = arr.first().and_then(Value::as_i64).unwrap_or(1);
    let style = arr
        .get(1)
        .and_then(Value::as_object)
        .and_then(|o| o.get("t"))
        .and_then(Value::as_str)
        .unwrap_or("Decimal")
        .to_string();
    (start, style)
}

fn list_from_items(items: &[Value], ordered: bool, start: i64, ctx: &mut ImportCtx) -> Value {
    let mut out = Vec::new();
    for item in items {
        // 每项是块数组
        let children = if let Some(arr) = item.as_array() {
            let mut ch = Vec::new();
            for b in arr {
                ch.extend(map_block(b, ctx));
            }
            ch
        } else {
            map_block(item, ctx)
        };
        out.push(json!({
            "id": ctx.job.idgen.uid("li"),
            "type": "list_item",
            "children": children,
        }));
    }
    log_none(ctx);
    let mut b = json!({
        "id": ctx.job.idgen.uid("blk"),
        "type": "list",
        "style": if ordered { "ordered" } else { "bullet" },
        "items": out,
    });
    if ordered {
        b.as_object_mut()
            .unwrap()
            .insert("start".to_string(), json!(start));
    }
    b
}

fn table_from(node: &Value, ctx: &mut ImportCtx) -> Value {
    let c = c_of(node);
    // [attr, caption, colspecs, TableHead, [TableBody], TableFoot]
    let colspecs = c.get(2).and_then(Value::as_array);
    let n_cols = colspecs.map(Vec::len).unwrap_or(0);
    let mut columns = Vec::new();
    for _ in 0..n_cols {
        columns.push(json!({"id": ctx.job.idgen.uid("col"), "name": ""}));
    }

    if let Some(caption) = c.get(1).and_then(Value::as_array) {
        if let Some(blocks) = caption.get(1).and_then(Value::as_array) {
            if !blocks.is_empty() {
                ctx.log.record(
                    LossClass::Partial,
                    "table_caption",
                    None,
                    "degraded",
                    "table",
                    "表格题注在 v1 模型中无对应，已丢弃并记录".to_string(),
                );
            }
        }
    }

    let mut rows: Vec<Value> = Vec::new();
    let mut header_row = false;

    if let Some(head_rows) = c.get(3).and_then(|h| h.get(1)).and_then(Value::as_array) {
        if !head_rows.is_empty() {
            header_row = true;
            for r in head_rows {
                rows.push(row_from(r, ctx));
            }
        }
    }
    if let Some(bodies) = c.get(4).and_then(Value::as_array) {
        for body in bodies {
            if let Some(rows_arr) = body.get(3).and_then(Value::as_array) {
                for r in rows_arr {
                    rows.push(row_from(r, ctx));
                }
            }
        }
    }

    if header_row {
        if let Some(first) = rows.first() {
            if let Some(cells) = first.get("cells").and_then(Value::as_array) {
                for (i, cell) in cells.iter().enumerate() {
                    let mut name = String::new();
                    if let Some(children) = cell.get("children").and_then(Value::as_array) {
                        for ch in children {
                            name.push_str(&azodoc_convert::plain_text_of_block(ch));
                        }
                    }
                    if let Some(col) = columns.get_mut(i) {
                        col.as_object_mut()
                            .unwrap()
                            .insert("name".to_string(), json!(name.trim()));
                    }
                }
            }
        }
    }
    log_none(ctx);
    json!({
        "id": ctx.job.idgen.uid("blk"),
        "type": "table",
        "header_row": header_row,
        "columns": columns,
        "rows": rows,
    })
}

fn row_from(row: &Value, ctx: &mut ImportCtx) -> Value {
    // Row = [attr, [Cell]]；Cell 是裸五元组 [attr, Align, RowSpan, ColSpan, blocks]
    let mut cells = Vec::new();
    if let Some(cell_arr) = row.get(1).and_then(Value::as_array) {
        for (i, cell) in cell_arr.iter().enumerate() {
            let c = cell.as_array().cloned().unwrap_or_default();
            let row_span = c.get(2).and_then(Value::as_i64).unwrap_or(1);
            let col_span = c.get(3).and_then(Value::as_i64).unwrap_or(1);
            let mut children = Vec::new();
            if let Some(blocks) = c.get(4).and_then(Value::as_array) {
                for b in blocks {
                    children.extend(map_block(b, ctx));
                }
            }
            if children.is_empty() {
                children.push(json!({
                    "id": ctx.job.idgen.uid("blk"),
                    "type": "paragraph",
                    "content": [],
                }));
            }
            let mut cell = json!({
                "id": ctx.job.idgen.uid("cel"),
                "column": i as i64,
                "children": children,
            });
            if col_span > 1 {
                cell.as_object_mut()
                    .unwrap()
                    .insert("colSpan".to_string(), json!(col_span));
            }
            if row_span > 1 {
                cell.as_object_mut()
                    .unwrap()
                    .insert("rowSpan".to_string(), json!(row_span));
            }
            cells.push(cell);
        }
    }
    json!({"id": ctx.job.idgen.uid("row"), "cells": cells})
}

fn figure_from_figure(node: &Value, ctx: &mut ImportCtx) -> Value {
    // Figure = [attr, caption, blocks]；blocks 内找第一个 Image
    let c = c_of(node);
    let blocks = c.get(2).cloned().unwrap_or(Value::Array(Vec::new()));
    // （Figure blocks）
    let mut imgs = Vec::new();
    find_images(&blocks, &mut imgs);

    let mut caption: Vec<Value> = Vec::new();
    if let Some(caption_val) = c.get(1) {
        if let Some(caption_blocks) = caption_val.get(1).and_then(Value::as_array) {
            for b in caption_blocks {
                if let Some(inlines) = b.get("c").and_then(Value::as_array) {
                    caption.extend(map_inlines(inlines, ctx));
                }
            }
        }
    }

    if let Some(img) = imgs.first() {
        let (asset, alt) = image_parts(img, ctx);
        log_none(ctx);
        json!({
            "id": ctx.job.idgen.uid("blk"),
            "type": "figure",
            "asset": asset,
            "alt": alt,
            "caption": caption,
        })
    } else {
        ctx.log.record(
            LossClass::Partial,
            "figure_without_image",
            None,
            "degraded",
            "figure",
            "Figure 无图片，题注降级为段落".to_string(),
        );
        json!({
            "id": ctx.job.idgen.uid("blk"),
            "type": "paragraph",
            "content": caption,
        })
    }
}

fn figure_from_image(img: &Value, ctx: &mut ImportCtx) -> Value {
    let (asset, alt) = image_parts(img, ctx);
    log_none(ctx);
    json!({
        "id": ctx.job.idgen.uid("blk"),
        "type": "figure",
        "asset": asset,
        "alt": alt,
        "caption": [],
    })
}

fn find_images(v: &Value, out: &mut Vec<Value>) {
    if let Some(obj) = v.as_object() {
        if obj.get("t").and_then(Value::as_str) == Some("Image") {
            out.push(v.clone());
        }
        for val in obj.values() {
            find_images(val, out);
        }
    } else if let Some(arr) = v.as_array() {
        for item in arr {
            find_images(item, out);
        }
    }
}

/// Image c = [attr, alt_inlines, [url, title]] → (asset:// 引用, alt)
fn image_parts(img: &Value, ctx: &mut ImportCtx) -> (String, String) {
    let c = img
        .get("c")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let url = c
        .get(2)
        .and_then(Value::as_array)
        .and_then(|t| t.first())
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let alt_inlines = c
        .get(1)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let alt = plain_of_inlines(&alt_inlines);
    let asset = resolve_media(&url, ctx);
    (asset, alt.trim().to_string())
}

fn resolve_media(url: &str, ctx: &mut ImportCtx) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        let name = url.rsplit('/').next().unwrap_or("image");
        return ctx.job.add_asset(
            name,
            azodoc_convert::guess_mime(name),
            azodoc_convert::AssetSource::External(url.to_string()),
        );
    }
    // --extract-media 的相对路径
    let path = if url.starts_with("media/") {
        ctx.extract_dir.join(url)
    } else {
        ctx.extract_dir.join(url)
    };
    if let Ok(bytes) = std::fs::read(&path) {
        let name = url
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or("image")
            .to_string();
        let mime = azodoc_convert::guess_mime(&name).to_string();
        return ctx
            .job
            .add_asset(&name, &mime, azodoc_convert::AssetSource::Embedded(bytes));
    }
    ctx.log.record(
        LossClass::Partial,
        "media_missing",
        None,
        "externalized",
        url,
        "嵌入媒体提取失败，以外链形式保留原始路径".to_string(),
    );
    ctx.job.add_asset(
        url.rsplit(['/', '\\']).next().unwrap_or("image"),
        azodoc_convert::guess_mime(url),
        azodoc_convert::AssetSource::External(url.to_string()),
    )
}

/// 提取 inlines 的纯文本（Image alt 等）。
fn plain_of_inlines(inlines: &[Value]) -> String {
    fn s(v: &Value, out: &mut String) {
        match v.get("t").and_then(Value::as_str).unwrap_or("") {
            "Str" => out.push_str(v.get("c").and_then(Value::as_str).unwrap_or("")),
            "Space" | "SoftBreak" => out.push(' '),
            "LineBreak" => out.push('\n'),
            "Math" => {
                if let Some(latex) = v.pointer("/c/1").and_then(Value::as_str) {
                    out.push_str(latex);
                }
            }
            _ => {
                if let Some(arr) = v.get("c").and_then(Value::as_array) {
                    for x in arr {
                        s(x, out);
                    }
                }
            }
        }
    }
    let mut out = String::new();
    for v in inlines {
        s(v, &mut out);
    }
    out
}

fn standalone_image(inlines: &[Value]) -> Option<Value> {
    let mut images = Vec::new();
    for v in inlines {
        match v.get("t").and_then(Value::as_str).unwrap_or("") {
            "Image" => images.push(v.clone()),
            "Space" => {}
            "Str" => {
                let s = v.get("c").and_then(Value::as_str).unwrap_or("");
                if !s.trim().is_empty() {
                    return None;
                }
            }
            _ => return None,
        }
    }
    if images.len() == 1 {
        images.pop()
    } else {
        None
    }
}

fn only_display_math(inlines: &[Value]) -> Option<Value> {
    let mut math = Vec::new();
    for v in inlines {
        match v.get("t").and_then(Value::as_str).unwrap_or("") {
            "Math" => math.push(v.clone()),
            "Space" => {}
            "Str" => {
                let s = v.get("c").and_then(Value::as_str).unwrap_or("");
                if !s.trim().is_empty() {
                    return None;
                }
            }
            _ => return None,
        }
    }
    if math.len() == 1 {
        math.pop()
    } else {
        None
    }
}

fn math_block_from(math: &Value, ctx: &mut ImportCtx) -> Value {
    let latex = math
        .get("c")
        .and_then(Value::as_array)
        .and_then(|c| c.get(1))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    log_none(ctx);
    json!({
        "id": ctx.job.idgen.uid("blk"),
        "type": "math_block",
        "latex": latex,
    })
}

fn flat_blocks(blocks: &[Value], ctx: &mut ImportCtx) -> Vec<Value> {
    let mut out = Vec::new();
    for b in blocks {
        out.extend(map_block(b, ctx));
    }
    out
}

fn take_pending(mut content: Vec<Value>, ctx: &mut ImportCtx) -> Vec<Value> {
    if !ctx.pending_blocks.is_empty() {
        content.extend(ctx.pending_blocks.drain(..));
    }
    content
}

// ---------------------------------------------------------------- 行内映射

pub fn map_inlines(inlines: &[Value], ctx: &mut ImportCtx) -> Vec<Value> {
    let mut out = Vec::new();
    for v in inlines {
        out.extend(map_inline(v, ctx));
    }
    out
}

fn container_span(kind: &str, children: Vec<Value>, ctx: &mut ImportCtx) -> Vec<Value> {
    log_none(ctx);
    vec![json!({"type": kind, "content": children})]
}

fn map_inline(node: &Value, ctx: &mut ImportCtx) -> Vec<Value> {
    let t = t_of(node);
    match t.as_str() {
        "Str" => {
            let s = node.get("c").and_then(Value::as_str).unwrap_or("");
            if s.is_empty() {
                Vec::new()
            } else {
                log_none(ctx);
                vec![json!({"type": "text", "text": s})]
            }
        }
        "Space" => {
            log_none(ctx);
            vec![json!({"type": "text", "text": " "})]
        }
        "SoftBreak" => {
            log_none(ctx);
            vec![json!({"type": "text", "text": " "})]
        }
        "LineBreak" => {
            log_none(ctx);
            vec![json!({"type": "hard_break"})]
        }
        "Code" => {
            let text = node
                .get("c")
                .and_then(Value::as_array)
                .and_then(|c| c.get(1))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            log_none(ctx);
            vec![json!({"type": "code", "text": text})]
        }
        "Emph" => container_span("em", map_inlines(&c_of(node), ctx), ctx),
        "Strong" => container_span("strong", map_inlines(&c_of(node), ctx), ctx),
        "Strikeout" => container_span("strike", map_inlines(&c_of(node), ctx), ctx),
        "Underline" => container_span("underline", map_inlines(&c_of(node), ctx), ctx),
        "Link" => {
            let c = c_of(node);
            let url = c
                .get(2)
                .and_then(Value::as_array)
                .and_then(|tg| tg.first())
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let title = c
                .get(2)
                .and_then(Value::as_array)
                .and_then(|tg| tg.get(1))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let content = c
                .get(1)
                .and_then(Value::as_array)
                .map(|i| map_inlines(i, ctx))
                .unwrap_or_default();
            log_none(ctx);
            let mut span = json!({"type": "link", "url": url, "content": content});
            if !title.is_empty() {
                span.as_object_mut()
                    .unwrap()
                    .insert("title".to_string(), json!(title));
            }
            vec![span]
        }
        "Image" => {
            let (asset, alt) = image_parts(node, ctx);
            log_none(ctx);
            vec![json!({"type": "inline_image", "asset": asset, "alt": alt})]
        }
        "Math" => {
            let c = c_of(node);
            let kind = c
                .first()
                .and_then(Value::as_object)
                .and_then(|o| o.get("t"))
                .and_then(Value::as_str)
                .unwrap_or("InlineMath")
                .to_string();
            let latex = c.get(1).and_then(Value::as_str).unwrap_or("").to_string();
            log_none(ctx);
            if kind == "DisplayMath" {
                // 段内展示公式提升为块（追加到段后）
                ctx.pending_blocks.push(json!({
                    "id": ctx.job.idgen.uid("blk"),
                    "type": "math_block",
                    "latex": latex,
                }));
                Vec::new()
            } else {
                vec![json!({"type": "inline_math", "latex": latex})]
            }
        }
        "Note" => {
            // 脚注：Note c = [blocks]
            let blocks = c_of(node);
            let children = flat_blocks(&blocks, ctx);
            let id = ctx.job.idgen.uid("blk");
            let label = (ctx.pending_footnotes.len() + 1).to_string();
            let mut def = json!({
                "id": id,
                "type": "footnote",
                "children": children,
            });
            def.as_object_mut()
                .unwrap()
                .insert("label".to_string(), json!(label));
            ctx.pending_footnotes.push(def);
            log_none(ctx);
            vec![json!({"type": "footnote_ref", "id": id})]
        }
        "Span" => {
            let classes = node
                .pointer("/c/0/1")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if !classes.is_empty() {
                ctx.log.record(
                    LossClass::Partial,
                    "span_class",
                    None,
                    "degraded",
                    &classes
                        .iter()
                        .map(|c| c.as_str().unwrap_or(""))
                        .collect::<Vec<_>>()
                        .join(" "),
                    "Span 的样式类无法映射到 v1 模型，已提升子内容".to_string(),
                );
            }
            map_inlines(&c_of(node), ctx)
        }
        "SmallCaps" | "Superscript" | "Subscript" => {
            ctx.log.record(
                LossClass::Partial,
                "inline_semantic",
                None,
                "degraded",
                &t,
                format!("行内类型 {t} 在 v1 模型中无对应，退化为内部内容"),
            );
            map_inlines(&c_of(node), ctx)
        }
        "RawInline" => {
            let c = c_of(node);
            let format = c
                .first()
                .and_then(Value::as_str)
                .unwrap_or("html")
                .to_string();
            let text = c.get(1).and_then(Value::as_str).unwrap_or("").to_string();
            let payload_ref = ctx.job.add_preserved(&format, "raw", text.into_bytes());
            ctx.log.record(
                LossClass::PreservedRaw,
                "raw_inline",
                None,
                "preserved",
                &payload_ref,
                "原始行内内容以原始形式保留".to_string(),
            );
            vec![json!({
                "type": "unknown",
                "origin": format,
                "loss_class": "preserved_raw",
                "summary": format!("原始 {format} 行内"),
                "payload_ref": payload_ref,
                "content": [],
            })]
        }
        "Cite" | "Quoted" => {
            ctx.log.record(
                LossClass::Partial,
                "inline_semantic",
                None,
                "degraded",
                &t,
                format!("行内类型 {t} 在 v1 模型中无对应，退化为内部内容"),
            );
            map_inlines(&c_of(node), ctx)
        }
        other => {
            ctx.log.record(
                LossClass::Partial,
                "ast_inline_fallback",
                None,
                "degraded",
                &other,
                format!("Pandoc 行内类型 {other} 无精确映射，按子内容降级处理"),
            );
            map_inlines(&c_of(node), ctx)
        }
    }
}

/// meta.title（MetaInlines / MetaBlocks / MetaString）→ 字符串。
pub fn meta_title(ast: &Value) -> Option<String> {
    let meta = ast.get("meta")?;
    let title = meta.get("title")?;
    match title.get("t").and_then(Value::as_str) {
        Some("MetaString") => title.get("c").and_then(Value::as_str).map(String::from),
        Some("MetaInlines") | Some("MetaBlocks") => {
            let inlines = title.get("c").and_then(Value::as_array)?;
            let mut s = String::new();
            for v in inlines {
                match v.get("t").and_then(Value::as_str) {
                    Some("Str") => s.push_str(v.get("c").and_then(Value::as_str).unwrap_or("")),
                    Some("Space") => s.push(' '),
                    _ => {}
                }
            }
            let s = s.trim().to_string();
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        }
        _ => None,
    }
}

/// 供测试/内部使用的空 Map 构造。
#[allow(dead_code)]
pub(crate) fn empty_map() -> Map<String, Value> {
    Map::new()
}
