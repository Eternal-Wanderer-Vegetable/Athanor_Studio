//! azodoc-md — Markdown ↔ Azodoc Prima（映射表见落地方案 §7.2）。
//!
//! 导入：comrak（CommonMark + GFM 表格/任务列表/删除线/脚注/数学/GFM 提示块/
//! YAML front-matter）→ Prima。标题按层级包装 section。
//! 导出：Prima → GFM。往返恒等的判定标准是**模型层恒等**（归一化比较），不是字节恒等。

use azodoc_convert::{
    merge_text_spans, wrap_sections, ExportDoc, IdGen, ImportJob, ImportOutput, LossClass, LossLog,
};
use comrak::nodes::{AstNode, NodeValue};
use comrak::{parse_document, Arena, Options};
use serde_json::{json, Map, Value};
use std::collections::HashMap;

/// Markdown 解析选项（启用的扩展 = M2 映射表的范围）。
pub fn md_options() -> Options<'static> {
    let mut o = Options::default();
    o.extension.table = true;
    o.extension.tasklist = true;
    o.extension.strikethrough = true;
    o.extension.autolink = true;
    o.extension.footnotes = true;
    o.extension.alerts = true;
    o.extension.math_dollars = true;
    o.extension.front_matter_delimiter = Some("---".to_string());
    o
}

// ---------------------------------------------------------------- 导入

pub fn import(md: &str, job: &mut ImportJob) -> ImportOutput {
    let arena = Arena::new();
    let root = parse_document(&arena, md, &md_options());
    let mut log = LossLog::new();

    let mut fn_map: HashMap<String, String> = HashMap::new();
    for child in root.children() {
        if let NodeValue::FootnoteDefinition(fd) = &child.data.borrow().value {
            fn_map.insert(fd.name.clone(), job.idgen.uid("blk"));
        }
    }

    let mut title: Option<String> = None;
    let mut language: Option<String> = None;
    let mut doc_extra: Map<String, Value> = Map::new();
    let mut blocks: Vec<Value> = Vec::new();

    for child in root.children() {
        let value = &child.data.borrow().value;
        if let NodeValue::FrontMatter(text) = value {
            parse_front_matter(text, &mut title, &mut language, &mut doc_extra);
            continue;
        }
        if let Some(b) = block_node(child, job, &fn_map, &mut log) {
            blocks.push(b);
        }
    }

    let content = json!({
        "schema_version": "1.0",
        "content": wrap_sections(&mut job.idgen, blocks),
    });
    let mut content = content;
    merge_text_spans(&mut content);
    ImportOutput {
        content,
        title,
        language,
        doc_extra,
        log,
    }
}

fn parse_front_matter(
    text: &str,
    title: &mut Option<String>,
    language: &mut Option<String>,
    doc_extra: &mut Map<String, Value>,
) {
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line == "---" {
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim().to_string();
            let val = v.trim().trim_matches('"').to_string();
            match key.as_str() {
                "title" => *title = Some(val),
                "language" | "lang" => *language = Some(val),
                _ => {
                    doc_extra.insert(key, Value::String(val));
                }
            }
        }
    }
}

fn block_node<'a>(
    node: &'a AstNode<'a>,
    job: &mut ImportJob,
    fn_map: &HashMap<String, String>,
    log: &mut LossLog,
) -> Option<Value> {
    let value = &node.data.borrow().value;
    match value {
        NodeValue::Document | NodeValue::FrontMatter(_) => None,
        NodeValue::Paragraph => {
            if let Some(img) = only_image(node) {
                Some(figure_from_image(img, job, log))
            } else {
                let content = inlines_of_children(node, job, fn_map, log);
                Some(json!({
                    "id": job.idgen.uid("blk"),
                    "type": "paragraph",
                    "content": content,
                }))
            }
        }
        NodeValue::Heading(h) => {
            log.node_none();
            Some(json!({
                "id": job.idgen.uid("blk"),
                "type": "heading",
                "level": h.level as i64,
                "content": inlines_of_children(node, job, fn_map, log),
            }))
        }
        NodeValue::BlockQuote => {
            let children = child_blocks(node, job, fn_map, log);
            Some(json!({
                "id": job.idgen.uid("blk"),
                "type": "quote",
                "children": children,
            }))
        }
        NodeValue::List(_) => Some(list_node(node, job, fn_map, log)),
        NodeValue::CodeBlock(cb) => {
            let lang = cb.info.split_whitespace().next().unwrap_or("").to_string();
            let language = if lang.is_empty() { None } else { Some(lang) };
            Some(json!({
                "id": job.idgen.uid("blk"),
                "type": "code_block",
                "language": language,
                "text": cb.literal.clone(),
            }))
        }
        NodeValue::ThematicBreak => {
            log.node_none();
            Some(json!({
                "id": job.idgen.uid("blk"),
                "type": "horizontal_rule",
            }))
        }
        NodeValue::HtmlBlock(hb) => {
            let payload = hb.literal.clone().into_bytes();
            let payload_ref = job.add_preserved("html", "html", payload);
            log.record(
                LossClass::PreservedRaw,
                "html_block",
                Some(source_line(node)),
                "preserved",
                &payload_ref,
                "HTML 块以原始形式保留，未参与建模".to_string(),
            );
            Some(unknown_block(job, "html", &payload_ref, "HTML 块"))
        }
        NodeValue::Table(_) => Some(table_node(node, job, fn_map, log)),
        NodeValue::FootnoteDefinition(fd) => {
            let id = fn_map
                .get(&fd.name)
                .cloned()
                .unwrap_or_else(|| job.idgen.uid("blk"));
            let children = child_blocks(node, job, fn_map, log);
            let mut b = json!({
                "id": id,
                "type": "footnote",
                "children": children,
            });
            // 保留原始 MD 标签，供导出还原 [^label]
            b.as_object_mut()
                .unwrap()
                .insert("label".to_string(), Value::String(fd.name.clone()));
            Some(b)
        }
        NodeValue::Alert(a) => {
            let (variant, exact) = match a.alert_type {
                comrak::nodes::AlertType::Note => ("note", true),
                comrak::nodes::AlertType::Tip => ("tip", true),
                comrak::nodes::AlertType::Important => ("important", true),
                comrak::nodes::AlertType::Warning => ("warning", true),
                comrak::nodes::AlertType::Caution => ("warning", false),
            };
            if !exact {
                log.record(
                    LossClass::Partial,
                    "alert_variant",
                    Some(source_line(node)),
                    "degraded",
                    "caution → warning",
                    "GFM 提示块 Caution 归并为 warning".to_string(),
                );
            } else {
                log.node_none();
            }
            let mut children = Vec::new();
            if let Some(t) = &a.title {
                children.push(json!({
                    "id": job.idgen.uid("blk"),
                    "type": "paragraph",
                    "content": [
                        {"type": "strong", "content": [{"type": "text", "text": t}]},
                    ],
                }));
            }
            for child in node.children() {
                if let Some(b) = block_node(child, job, fn_map, log) {
                    children.push(b);
                }
            }
            Some(json!({
                "id": job.idgen.uid("blk"),
                "type": "callout",
                "variant": variant,
                "children": children,
            }))
        }
        NodeValue::Math(m) if m.display_math => Some(json!({
            "id": job.idgen.uid("blk"),
            "type": "math_block",
            "latex": m.literal.clone(),
        })),
        other => {
            log.record(
                LossClass::Partial,
                "md_block_fallback",
                Some(source_line(node)),
                "degraded",
                &format!("{other:?}"),
                "Markdown 块类型按子内容降级处理".to_string(),
            );
            let mut children = Vec::new();
            for child in node.children() {
                if let Some(b) = block_node(child, job, fn_map, log) {
                    children.push(b);
                }
            }
            match children.len() {
                0 => None,
                1 => children.into_iter().next(),
                _ => Some(json!({
                    "id": job.idgen.uid("sec"),
                    "type": "section",
                    "children": children,
                })),
            }
        }
    }
}

fn list_node<'a>(
    node: &'a AstNode<'a>,
    job: &mut ImportJob,
    fn_map: &HashMap<String, String>,
    log: &mut LossLog,
) -> Value {
    let (list_type, start) = match &node.data.borrow().value {
        NodeValue::List(nl) => (nl.list_type.clone(), nl.start),
        _ => (comrak::nodes::ListType::Bullet, 1),
    };
    let ordered = matches!(list_type, comrak::nodes::ListType::Ordered);
    let mut items = Vec::new();
    for item in node.children() {
        let checked = match &item.data.borrow().value {
            NodeValue::TaskItem(ti) => Some(ti.symbol.is_some()),
            _ => None,
        };
        let children = child_blocks(item, job, fn_map, log);
        let mut li = json!({
            "id": job.idgen.uid("li"),
            "type": "list_item",
            "children": children,
        });
        if let Some(c) = checked {
            li.as_object_mut()
                .unwrap()
                .insert("checked".to_string(), json!(c));
        }
        items.push(li);
    }
    log.node_none();
    let mut b = json!({
        "id": job.idgen.uid("blk"),
        "type": "list",
        "style": if ordered { "ordered" } else { "bullet" },
        "items": items,
    });
    if ordered {
        b.as_object_mut()
            .unwrap()
            .insert("start".to_string(), json!(start as i64));
    }
    b
}

fn table_node<'a>(
    node: &'a AstNode<'a>,
    job: &mut ImportJob,
    fn_map: &HashMap<String, String>,
    log: &mut LossLog,
) -> Value {
    let num_columns = match &node.data.borrow().value {
        NodeValue::Table(t) => t.num_columns,
        _ => 0,
    };
    let mut columns = Vec::new();
    for _i in 0..num_columns {
        columns.push(json!({"id": job.idgen.uid("col"), "name": ""}));
    }
    let mut rows = Vec::new();
    let mut header_row = false;
    for row in node.children() {
        let is_header = matches!(&row.data.borrow().value, NodeValue::TableRow(h) if *h);
        if is_header {
            header_row = true;
            // 表头文本填入列名
            for (i, cell) in row.children().enumerate() {
                let text = inline_text_of(cell, job, fn_map, log);
                if let Some(col) = columns.get_mut(i) {
                    col.as_object_mut()
                        .unwrap()
                        .insert("name".to_string(), json!(text));
                }
            }
            continue;
        }
        let mut cells = Vec::new();
        for (i, cell) in row.children().enumerate() {
            cells.push(json!({
                "id": job.idgen.uid("cel"),
                "column": i as i64,
                "children": [{
                    "id": job.idgen.uid("blk"),
                    "type": "paragraph",
                    "content": inlines_of_children(cell, job, fn_map, log),
                }],
            }));
        }
        rows.push(json!({"id": job.idgen.uid("row"), "cells": cells}));
    }
    log.node_none();
    json!({
        "id": job.idgen.uid("blk"),
        "type": "table",
        "header_row": header_row,
        "columns": columns,
        "rows": rows,
    })
}

fn figure_from_image<'a>(img: &'a AstNode<'a>, job: &mut ImportJob, log: &mut LossLog) -> Value {
    let url = match &img.data.borrow().value {
        NodeValue::Image(l) => l.url.clone(),
        _ => String::new(),
    };
    let alt = inline_text_of(img, job, &HashMap::new(), log);
    let asset = resolve_image(&url, job, log);
    json!({
        "id": job.idgen.uid("blk"),
        "type": "figure",
        "asset": asset,
        "alt": alt.trim(),
        "caption": [],
    })
}

/// 段落是否只含一张图（忽略空白与软换行）。
fn only_image<'b>(node: &'b AstNode<'b>) -> Option<&'b AstNode<'b>> {
    let children: Vec<&AstNode<'b>> = node.children().collect();
    let mut images = Vec::new();
    for c in &children {
        match &c.data.borrow().value {
            NodeValue::Image(_) => images.push(*c),
            NodeValue::Text(t) if t.trim().is_empty() => {}
            NodeValue::SoftBreak => {}
            _ => return None,
        }
    }
    if images.len() == 1 {
        images.pop()
    } else {
        None
    }
}

fn resolve_image(url: &str, job: &mut ImportJob, log: &mut LossLog) -> String {
    if let Some(rest) = url.strip_prefix("data:") {
        // data:[mime];base64,....
        if let Some((meta, b64)) = rest.split_once(",") {
            let mime = meta
                .strip_suffix(";base64")
                .unwrap_or(meta)
                .trim_start_matches("data:")
                .to_string();
            use base64::Engine as _;
            if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64.trim()) {
                let ext = mime_to_ext(&mime);
                let name = format!("embedded.{ext}");
                return job.add_asset(&name, &mime, azodoc_convert::AssetSource::Embedded(bytes));
            }
        }
        log.record(
            LossClass::Partial,
            "data_uri_invalid",
            None,
            "externalized",
            url,
            "data URI 无法解码，以外链形式保留".to_string(),
        );
        return job.add_asset(
            "image",
            "image",
            azodoc_convert::AssetSource::External(url.to_string()),
        );
    }
    if url.starts_with("http://") || url.starts_with("https://") {
        let name = url.rsplit('/').next().unwrap_or("image");
        return job.add_asset(
            name,
            azodoc_convert::guess_mime(name),
            azodoc_convert::AssetSource::External(url.to_string()),
        );
    }
    // 本地路径：能读到就内嵌，读不到保留原始引用并报告
    if let Some(base) = &job.base_dir {
        let path = base.join(url);
        if let Ok(bytes) = std::fs::read(&path) {
            let name = url
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or("image")
                .to_string();
            let mime = azodoc_convert::guess_mime(&name).to_string();
            return job.add_asset(&name, &mime, azodoc_convert::AssetSource::Embedded(bytes));
        }
    }
    log.record(
        LossClass::Partial,
        "local_image_missing",
        None,
        "externalized",
        url,
        "本地图片文件不存在，以外链形式保留原始路径".to_string(),
    );
    job.add_asset(
        url.rsplit(['/', '\\']).next().unwrap_or("image"),
        azodoc_convert::guess_mime(url),
        azodoc_convert::AssetSource::External(url.to_string()),
    )
}

fn mime_to_ext(mime: &str) -> &str {
    match mime {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/svg+xml" => "svg",
        "image/webp" => "webp",
        _ => "bin",
    }
}

fn unknown_block(job: &mut ImportJob, origin: &str, payload_ref: &str, summary: &str) -> Value {
    json!({
        "id": job.idgen.uid("blk"),
        "type": "unknown",
        "origin": origin,
        "loss_class": "preserved_raw",
        "summary": summary,
        "payload_ref": payload_ref,
        "content": [],
    })
}

fn child_blocks<'a>(
    node: &'a AstNode<'a>,
    job: &mut ImportJob,
    fn_map: &HashMap<String, String>,
    log: &mut LossLog,
) -> Vec<Value> {
    let mut out = Vec::new();
    for child in node.children() {
        if let Some(b) = block_node(child, job, fn_map, log) {
            out.push(b);
        }
    }
    out
}

fn inlines_of_children<'a>(
    node: &'a AstNode<'a>,
    job: &mut ImportJob,
    fn_map: &HashMap<String, String>,
    log: &mut LossLog,
) -> Vec<Value> {
    let mut out = Vec::new();
    for child in node.children() {
        out.extend(inline_nodes(child, job, fn_map, log));
    }
    out
}

fn inline_nodes<'a>(
    node: &'a AstNode<'a>,
    job: &mut ImportJob,
    fn_map: &HashMap<String, String>,
    log: &mut LossLog,
) -> Vec<Value> {
    let value = &node.data.borrow().value;
    match value {
        NodeValue::Text(t) => {
            if t.is_empty() {
                Vec::new()
            } else {
                log.node_none();
                vec![json!({"type": "text", "text": t.to_string()})]
            }
        }
        NodeValue::SoftBreak => {
            log.node_none();
            vec![json!({"type": "text", "text": " "})]
        }
        NodeValue::LineBreak => {
            log.node_none();
            vec![json!({"type": "hard_break"})]
        }
        NodeValue::Code(c) => {
            log.node_none();
            vec![json!({"type": "code", "text": c.literal.clone()})]
        }
        NodeValue::Emph => {
            vec![json!({
                "type": "em",
                "content": inlines_of_children(node, job, fn_map, log),
            })]
        }
        NodeValue::Strong => {
            vec![json!({
                "type": "strong",
                "content": inlines_of_children(node, job, fn_map, log),
            })]
        }
        NodeValue::Strikethrough => {
            vec![json!({
                "type": "strike",
                "content": inlines_of_children(node, job, fn_map, log),
            })]
        }
        NodeValue::Link(l) => {
            log.node_none();
            let mut span = json!({
                "type": "link",
                "url": l.url.clone(),
                "content": inlines_of_children(node, job, fn_map, log),
            });
            if !l.title.is_empty() {
                span.as_object_mut()
                    .unwrap()
                    .insert("title".to_string(), json!(l.title));
            }
            vec![span]
        }
        NodeValue::Image(l) => {
            let alt = inline_text_of(node, job, fn_map, log);
            let asset = resolve_image(&l.url, job, log);
            log.node_none();
            vec![json!({
                "type": "inline_image",
                "asset": asset,
                "alt": alt.trim(),
            })]
        }
        NodeValue::HtmlInline(s) => {
            let payload_ref = job.add_preserved("html", "html", s.clone().into_bytes());
            log.record(
                LossClass::PreservedRaw,
                "inline_html",
                Some(source_line(node)),
                "preserved",
                &payload_ref,
                "内联 HTML 以原始形式保留，未参与建模".to_string(),
            );
            vec![json!({
                "type": "unknown",
                "origin": "html",
                "loss_class": "preserved_raw",
                "summary": "内联 HTML",
                "payload_ref": payload_ref,
                "content": [],
            })]
        }
        NodeValue::FootnoteReference(fr) => {
            let id = fn_map.get(&fr.name).cloned().unwrap_or_default();
            log.node_none();
            vec![json!({"type": "footnote_ref", "id": id})]
        }
        NodeValue::Math(m) => {
            log.node_none();
            vec![json!({"type": "inline_math", "latex": m.literal.clone()})]
        }
        other => {
            log.record(
                LossClass::Partial,
                "md_inline_fallback",
                Some(source_line(node)),
                "degraded",
                &format!("{other:?}"),
                "Markdown 行内类型按纯文本降级处理".to_string(),
            );
            inlines_of_children(node, job, fn_map, log)
        }
    }
}

/// 收集节点的纯文本（图片 alt 等）。
fn inline_text_of<'a>(
    node: &'a AstNode<'a>,
    job: &mut ImportJob,
    fn_map: &HashMap<String, String>,
    log: &mut LossLog,
) -> String {
    let mut s = String::new();
    for child in node.children() {
        for span in inline_nodes(child, job, fn_map, log) {
            s.push_str(&azodoc_convert::plain_text_of_span(&span));
        }
    }
    s
}

fn source_line(node: &AstNode) -> String {
    format!("line {}", node.data.borrow().sourcepos.start.line)
}

// ---------------------------------------------------------------- 导出

struct MdCtx<'a> {
    doc: &'a ExportDoc,
    fn_labels: HashMap<String, String>,
    fn_defs: Vec<(String, Value)>,
}

/// 导出为 GFM。损失按节点计数并写入 log。
pub fn export_markdown(doc: &ExportDoc, log: &mut LossLog) -> String {
    let mut ctx = MdCtx {
        doc,
        fn_labels: HashMap::new(),
        fn_defs: Vec::new(),
    };
    // 脚注定义标签：优先 extra.label，否则按文档序生成
    let mut auto = 0usize;
    if let Some(arr) = doc.content.get("content").and_then(Value::as_array) {
        for node in arr {
            collect_fn_defs(Some(node), &mut ctx, &mut auto);
        }
    }
    let mut out = String::new();
    if let Some(arr) = doc.content.get("content").and_then(Value::as_array) {
        for node in arr {
            out.push_str(&block_md(node, &ctx, log, 0));
        }
    }
    for (label, def) in &ctx.fn_defs {
        let body = footnote_body(def, &ctx, log);
        out.push_str(&format!("[^{label}]: {body}\n"));
    }
    out
}

fn collect_fn_defs(v: Option<&Value>, ctx: &mut MdCtx, auto: &mut usize) {
    let Some(v) = v else { return };
    if v.get("type").and_then(Value::as_str) == Some("footnote") {
        if let Some(id) = v.get("id").and_then(Value::as_str) {
            let label = v
                .get("label")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| {
                    *auto += 1;
                    auto.to_string()
                });
            ctx.fn_labels.insert(id.to_string(), label.clone());
            ctx.fn_defs.push((label, v.clone()));
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

fn footnote_body(def: &Value, ctx: &MdCtx, log: &mut LossLog) -> String {
    let mut parts = Vec::new();
    if let Some(children) = def.get("children").and_then(Value::as_array) {
        for c in children {
            let rendered = block_md(c, ctx, log, 0);
            parts.push(rendered.trim_end().to_string());
        }
    }
    parts.join(" ")
}

fn indent_block(s: &str, prefix: &str, skip_first: bool) -> String {
    let mut out = String::new();
    for (i, line) in s.split_inclusive('\n').enumerate() {
        if !(skip_first && i == 0) {
            if line == "\n" {
                out.push('\n');
                continue;
            }
            out.push_str(prefix);
        }
        out.push_str(line);
    }
    out
}

fn block_md(node: &Value, ctx: &MdCtx, log: &mut LossLog, depth: usize) -> String {
    let t = node.get("type").and_then(Value::as_str).unwrap_or("");
    match t {
        "section" => {
            let mut s = String::new();
            if let Some(children) = node.get("children").and_then(Value::as_array) {
                for c in children {
                    s.push_str(&block_md(c, ctx, log, depth));
                }
            }
            s
        }
        "heading" => {
            let level = node.get("level").and_then(Value::as_i64).unwrap_or(1);
            let hashes: String = "#".repeat(level as usize);
            let text = inline_md(node.get("content"), ctx, log);
            format!("{hashes} {text}\n\n")
        }
        "paragraph" => {
            let text = inline_md(node.get("content"), ctx, log);
            if text.trim().is_empty() {
                String::new()
            } else {
                format!("{text}\n\n")
            }
        }
        "quote" => {
            let mut inner = String::new();
            if let Some(children) = node.get("children").and_then(Value::as_array) {
                for c in children {
                    inner.push_str(&block_md(c, ctx, log, depth));
                }
            }
            inner
                .lines()
                .map(|l| {
                    if l.is_empty() {
                        ">\n".to_string()
                    } else {
                        format!("> {l}\n")
                    }
                })
                .collect::<String>()
                + "\n"
        }
        "list" => {
            let style = node
                .get("style")
                .and_then(Value::as_str)
                .unwrap_or("bullet");
            let start = node.get("start").and_then(Value::as_i64).unwrap_or(1);
            let mut s = String::new();
            if let Some(items) = node.get("items").and_then(Value::as_array) {
                for (i, item) in items.iter().enumerate() {
                    let marker = match style {
                        "ordered" => format!("{}. ", start + i as i64),
                        _ => "- ".to_string(),
                    };
                    let check = match item.get("checked").and_then(Value::as_bool) {
                        Some(true) => "[x] ",
                        Some(false) => "[ ] ",
                        None => "",
                    };
                    let cont: String = " ".repeat(marker.len() + check.len());
                    let mut parts: Vec<String> = Vec::new();
                    if let Some(children) = item.get("children").and_then(Value::as_array) {
                        for (ci, c) in children.iter().enumerate() {
                            let mut rendered = block_md(c, ctx, log, depth + 1)
                                .trim_end_matches('\n')
                                .to_string();
                            if ci == 0 && c.get("type").and_then(Value::as_str) == Some("paragraph")
                            {
                                rendered = rendered.trim_start().to_string();
                                s.push_str(&format!("{marker}{check}{rendered}\n"));
                            } else {
                                let indented = indent_block(&rendered, &cont, false);
                                parts.push(indented);
                            }
                        }
                    }
                    for p in parts {
                        s.push_str(&p);
                        s.push('\n');
                    }
                }
            }
            s.push('\n');
            s
        }
        "code_block" => {
            let text = node.get("text").and_then(Value::as_str).unwrap_or("");
            let lang = node.get("language").and_then(Value::as_str).unwrap_or("");
            let max_run = text
                .lines()
                .map(|l| {
                    let mut run = 0;
                    let mut best = 0;
                    for c in l.chars() {
                        if c == '`' {
                            run += 1;
                            best = best.max(run);
                        } else {
                            run = 0;
                        }
                    }
                    best
                })
                .max()
                .unwrap_or(0);
            let fence = "`".repeat((max_run + 1).max(3));
            let body = if text.ends_with('\n') {
                text.to_string()
            } else {
                format!("{text}\n")
            };
            format!(
                "{fence}{lang}
{body}{fence}

"
            )
        }
        "table" => emit_table_md(node, ctx, log),
        "figure" => {
            let asset = node.get("asset").and_then(Value::as_str).unwrap_or("");
            let alt = node.get("alt").and_then(Value::as_str).unwrap_or("");
            let url = ctx.doc.asset_url(asset);
            let mut s = format!(
                "![{alt}]({})\n",
                comrak::escape_commonmark_link_destination(&url)
            );
            let cap = inline_md(node.get("caption"), ctx, log);
            if !cap.trim().is_empty() {
                log.record(
                    LossClass::Partial,
                    "figure_caption_md",
                    None,
                    "degraded",
                    &cap,
                    "图题在 Markdown 中以斜体段落呈现".to_string(),
                );
                s.push_str(&format!("*{cap}*\n"));
            }
            s.push('\n');
            s
        }
        "image" => {
            let asset = node.get("asset").and_then(Value::as_str).unwrap_or("");
            let alt = node.get("alt").and_then(Value::as_str).unwrap_or("");
            let url = ctx.doc.asset_url(asset);
            log.node_none();
            format!(
                "![{alt}]({})\n\n",
                comrak::escape_commonmark_link_destination(&url)
            )
        }
        "horizontal_rule" => "---\n\n".to_string(),
        "math_block" => {
            let latex = node.get("latex").and_then(Value::as_str).unwrap_or("");
            log.node_none();
            format!("$${latex}$$\n\n")
        }
        "callout" => emit_callout_md(node, ctx, log),
        "footnote" => String::new(), // 已在末尾输出
        "embed" => {
            let asset = node.get("asset").and_then(Value::as_str).unwrap_or("");
            let name = ctx.doc.asset_filename(asset).unwrap_or(asset);
            log.record(
                LossClass::Partial,
                "embed_md",
                None,
                "degraded",
                asset,
                "内嵌资源在 Markdown 中以链接呈现".to_string(),
            );
            format!(
                "[{name}]({})\n\n",
                comrak::escape_commonmark_link_destination(&ctx.doc.asset_url(asset))
            )
        }
        "unknown" => {
            let summary = node.get("summary").and_then(Value::as_str).unwrap_or("");
            let payload_ref = node
                .get("payload_ref")
                .and_then(Value::as_str)
                .unwrap_or("");
            log.record(
                LossClass::PreservedRaw,
                "unknown_block_md",
                None,
                "preserved",
                payload_ref,
                "未知内容以占位注释呈现，原文保留于容器".to_string(),
            );
            format!("<!-- azodoc: unsupported {summary}，原文见 {payload_ref} -->\n\n")
        }
        _ => {
            log.record(
                LossClass::Partial,
                "md_export_fallback",
                None,
                "degraded",
                t,
                "未知块类型按子内容降级导出".to_string(),
            );
            let mut s = String::new();
            for key in ["children", "items"] {
                if let Some(children) = node.get(key).and_then(Value::as_array) {
                    for c in children {
                        s.push_str(&block_md(c, ctx, log, depth + 1));
                    }
                }
            }
            s
        }
    }
}

fn emit_table_md(node: &Value, ctx: &MdCtx, log: &mut LossLog) -> String {
    // 合并单元格无法用 GFM 表达
    let merged = table_has_spans(node);
    if merged {
        log.record(
            LossClass::Partial,
            "merged_cell",
            None,
            "degraded",
            "table",
            "表格含合并单元格，GFM 表格按普通单元格展开".to_string(),
        );
    } else {
        log.node_none();
    }
    let header_names = column_names(node);
    let mut rows_md: Vec<Vec<String>> = Vec::new();
    if node.get("header_row").and_then(Value::as_bool) == Some(true) {
        if let Some(rows) = node.get("rows").and_then(Value::as_array) {
            if let Some(first) = rows.first() {
                let cells = cell_texts(first, ctx, log);
                if !header_names.iter().all(String::is_empty)
                    && header_names.join("") != cells.join("")
                {
                    rows_md.push(header_names);
                }
                rows_md.push(cells);
                for row in rows.iter().skip(1) {
                    rows_md.push(cell_texts(row, ctx, log));
                }
            }
        }
    } else {
        rows_md.push(header_names);
        if let Some(rows) = node.get("rows").and_then(Value::as_array) {
            for row in rows {
                rows_md.push(cell_texts(row, ctx, log));
            }
        }
    }
    if rows_md.is_empty() {
        return String::new();
    }
    let width = rows_md.iter().map(Vec::len).max().unwrap_or(1);
    let mut out = String::from("| ");
    out.push_str(
        &rows_md[0]
            .iter()
            .map(|c| c.replace('|', "\\|"))
            .collect::<Vec<_>>()
            .join(" | "),
    );
    for _ in rows_md[0].len()..width {
        out.push_str(" | ");
    }
    out.push_str(" |\n|");
    for _ in 0..width {
        out.push_str(" --- |");
    }
    out.push('\n');
    for row in rows_md.iter().skip(1) {
        out.push_str("| ");
        out.push_str(
            &row.iter()
                .map(|c| c.replace('|', "\\|"))
                .collect::<Vec<_>>()
                .join(" | "),
        );
        for _ in row.len()..width {
            out.push_str(" | ");
        }
        out.push_str(" |\n");
    }
    out.push('\n');
    out
}

fn table_has_spans(node: &Value) -> bool {
    node.get("rows")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter().any(|r| {
                r.get("cells")
                    .and_then(Value::as_array)
                    .map(|cs| {
                        cs.iter().any(|c| {
                            c.get("colSpan").and_then(Value::as_i64).unwrap_or(1) > 1
                                || c.get("rowSpan").and_then(Value::as_i64).unwrap_or(1) > 1
                        })
                    })
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn column_names(node: &Value) -> Vec<String> {
    node.get("columns")
        .and_then(Value::as_array)
        .map(|cols| {
            cols.iter()
                .map(|c| {
                    c.get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string()
                })
                .collect()
        })
        .unwrap_or_default()
}

fn cell_texts(row: &Value, ctx: &MdCtx, log: &mut LossLog) -> Vec<String> {
    row.get("cells")
        .and_then(Value::as_array)
        .map(|cs| {
            cs.iter()
                .map(|c| {
                    let mut s = String::new();
                    if let Some(children) = c.get("children").and_then(Value::as_array) {
                        for ch in children {
                            s.push_str(block_md(ch, ctx, log, 0).trim());
                        }
                    }
                    s.replace('\n', " ")
                })
                .collect()
        })
        .unwrap_or_default()
}

fn emit_callout_md(node: &Value, ctx: &MdCtx, log: &mut LossLog) -> String {
    let variant = node
        .get("variant")
        .and_then(Value::as_str)
        .unwrap_or("note");
    let gfm = match variant {
        "note" => "NOTE",
        "tip" => "TIP",
        "important" => "IMPORTANT",
        "warning" => "WARNING",
        _ => "NOTE",
    };
    log.node_none();
    let mut inner = String::new();
    if let Some(children) = node.get("children").and_then(Value::as_array) {
        for c in children {
            inner.push_str(&block_md(c, ctx, log, 0));
        }
    }
    let mut s = format!("> [!{gfm}]\n");
    for line in inner.lines() {
        if line.is_empty() {
            s.push_str(">\n");
        } else {
            s.push_str(&format!("> {line}\n"));
        }
    }
    s.push('\n');
    s
}

fn inline_md(spans: Option<&Value>, ctx: &MdCtx, log: &mut LossLog) -> String {
    let mut s = String::new();
    if let Some(arr) = spans.and_then(Value::as_array) {
        for span in arr {
            s.push_str(&span_md(span, ctx, log));
        }
    }
    s
}

fn span_md(span: &Value, ctx: &MdCtx, log: &mut LossLog) -> String {
    let t = span.get("type").and_then(Value::as_str).unwrap_or("");
    match t {
        "text" => {
            comrak::escape_commonmark_inline(span.get("text").and_then(Value::as_str).unwrap_or(""))
        }
        "hard_break" => "\\\n".to_string(),
        "code" => {
            let text = span.get("text").and_then(Value::as_str).unwrap_or("");
            let ticks = "`".repeat(text.matches('`').count().max(0) + 1);
            let pad = if text.starts_with('`') || text.ends_with('`') {
                " "
            } else {
                ""
            };
            format!("{ticks}{pad}{text}{pad}{ticks}")
        }
        "em" => format!("*{}*", inline_md(span.get("content"), ctx, log)),
        "strong" => format!("**{}**", inline_md(span.get("content"), ctx, log)),
        "strike" => format!("~~{}~~", inline_md(span.get("content"), ctx, log)),
        "underline" => {
            log.record(
                LossClass::Partial,
                "underline_md",
                None,
                "degraded",
                "underline",
                "下划线在 Markdown 中无法表达，退化为普通文本".to_string(),
            );
            inline_md(span.get("content"), ctx, log)
        }
        "link" => {
            let url = span.get("url").and_then(Value::as_str).unwrap_or("");
            let title = span.get("title").and_then(Value::as_str);
            let text = inline_md(span.get("content"), ctx, log);
            log.node_none();
            match title {
                Some(t) => format!(
                    "[{}]({} \"{}\")",
                    text,
                    comrak::escape_commonmark_link_destination(url),
                    t.replace('"', "'")
                ),
                None => format!(
                    "[{}]({})",
                    text,
                    comrak::escape_commonmark_link_destination(url)
                ),
            }
        }
        "inline_math" => {
            log.node_none();
            format!(
                "${}$",
                span.get("latex").and_then(Value::as_str).unwrap_or("")
            )
        }
        "inline_image" => {
            let asset = span.get("asset").and_then(Value::as_str).unwrap_or("");
            let alt = span.get("alt").and_then(Value::as_str).unwrap_or("");
            log.node_none();
            format!(
                "![{alt}]({})",
                comrak::escape_commonmark_link_destination(&ctx.doc.asset_url(asset))
            )
        }
        "footnote_ref" => {
            let id = span.get("id").and_then(Value::as_str).unwrap_or("");
            log.node_none();
            match ctx.fn_labels.get(id) {
                Some(label) => format!("[^{label}]"),
                None => String::new(),
            }
        }
        "unknown" => {
            log.record(
                LossClass::PreservedRaw,
                "unknown_span_md",
                None,
                "preserved",
                span.get("payload_ref")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
                "未知行内内容在 Markdown 导出中被省略，原文保留于容器".to_string(),
            );
            String::new()
        }
        "mention" => {
            log.record(
                LossClass::Partial,
                "mention_md",
                None,
                "degraded",
                span.get("target").and_then(Value::as_str).unwrap_or(""),
                "提及在 Markdown 中以纯文本呈现".to_string(),
            );
            span.get("target")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        }
        "cite" => {
            log.record(
                LossClass::Partial,
                "cite_md",
                None,
                "degraded",
                span.get("key").and_then(Value::as_str).unwrap_or(""),
                "引用键在 Markdown 中以纯文本呈现".to_string(),
            );
            span.get("key")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        }
        _ => inline_md(span.get("content"), ctx, log),
    }
}

/// 供 CLI 报告使用的转换器标识。
pub fn converter_name() -> &'static str {
    "athanor-md"
}

/// 未使用的占位（保持 IdGen 引用），M3 修订层接入后移除。
#[allow(dead_code)]
fn _idgen_marker() -> IdGen {
    IdGen::new()
}
