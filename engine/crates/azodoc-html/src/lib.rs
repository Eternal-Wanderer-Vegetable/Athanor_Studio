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

//! azodoc-html — HTML ↔ Azodoc Prima（映射表见落地方案 §7.3）。
//!
//! 导入：html5ever（浏览器级容错解析）→ Prima。安全例外（loss 规范 §3）：
//! `<script>`、事件属性、`javascript:` URL 一律移除并报告，不入 preserved/。
//! 导出：自包含单文件 HTML——资产 data URI 化，嵌入血统 meta。
//! [`sanitize_fragment`] 供导出 preserved 载荷时复用。

use azodoc_convert::{merge_text_spans, ImportJob, ImportOutput, LossClass, LossLog};
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use serde_json::{json, Value};
use tendril::TendrilSink;

// ---------------------------------------------------------------- 导入

pub fn import(html: &str, job: &mut ImportJob) -> ImportOutput {
    let mut log = LossLog::new();
    let dom = {
        let mut input = html.as_bytes();
        html5ever::parse_document(RcDom::default(), html5ever::ParseOpts::default())
            .from_utf8()
            .read_from(&mut input)
            .unwrap_or_else(|_| RcDom::default())
    };

    let mut title = None;
    let mut blocks: Vec<Value> = Vec::new();

    // 遍历 document 子节点：<html> 走结构分支，其余（顶层注释等）按块处理
    for child in dom.document.children.borrow().iter() {
        match element_name(child).as_deref() {
            Some("html") => {
                for hc in child.children.borrow().iter() {
                    match element_name(hc).as_deref() {
                        Some("head") => {
                            title = extract_title(hc, job, &mut log);
                        }
                        Some("body") => {
                            // body 本身是透明容器：展开其子节点
                            for b in flat_blocks(hc, job, &mut log) {
                                blocks.push(b);
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {
                for b in body_blocks(child, job, &mut log) {
                    blocks.push(b);
                }
            }
        }
    }

    // HTML 导入不做标题包装（section 仅来自显式 <section>/<article>/<main>；
    // 大纲由 heading 序列决定，见 azodoc-model.md §5）
    let content = json!({
        "schema_version": "1.0",
        "content": blocks,
    });
    let mut content = content;
    merge_text_spans(&mut content);
    ImportOutput {
        content,
        title,
        language: None,
        doc_extra: Default::default(),
        log,
    }
}

fn element_name(h: &Handle) -> Option<String> {
    match &h.data {
        NodeData::Element { name, .. } => Some(name.local.to_string().to_ascii_lowercase()),
        _ => None,
    }
}

fn attr(h: &Handle, key: &str) -> Option<String> {
    if let NodeData::Element { attrs, .. } = &h.data {
        for a in attrs.borrow().iter() {
            if a.name.local.to_string().eq_ignore_ascii_case(key) {
                return Some(a.value.to_string());
            }
        }
    }
    None
}

fn extract_title(head: &Handle, job: &mut ImportJob, log: &mut LossLog) -> Option<String> {
    let mut title = None;
    for child in head.children.borrow().iter() {
        match element_name(child).as_deref() {
            Some("title") => {
                title = Some(text_content(child));
                log.node_none();
            }
            Some("script") => active_content_event(log, "script 元素（head）"),
            Some("style") => style_dropped(log, "head"),
            _ => {}
        }
    }
    let _ = job;
    title
}

/// 安全例外（loss 规范 §3）：活动内容不保留、不执行，必须报告。
fn active_content_event(log: &mut LossLog, what: &str) {
    log.record(
        LossClass::Unsupported,
        "active_content",
        None,
        "removed",
        what,
        "活动内容被移除（安全政策；--preserve-raw-active 于 M3 提供）".to_string(),
    );
}

fn style_dropped(log: &mut LossLog, where_: &str) {
    log.record(
        LossClass::Partial,
        "style_element",
        None,
        "dropped",
        where_,
        "样式表未提取到主题层（v1 降级政策）".to_string(),
    );
}

/// 块级遍历。返回 0..n 个块节点。
fn body_blocks(h: &Handle, job: &mut ImportJob, log: &mut LossLog) -> Vec<Value> {
    let mut out = Vec::new();
    match &h.data {
        NodeData::Text { contents } => {
            let text = contents.borrow().to_string();
            if !text.trim().is_empty() {
                let spans = vec![json!({"type": "text", "text": collapse_ws(&text)})];
                log.node_none();
                out.push(json!({
                    "id": job.idgen.uid("blk"),
                    "type": "paragraph",
                    "content": spans,
                }));
            }
        }
        NodeData::Comment { contents } => {
            let payload = format!("<!--{}-->", contents);
            let payload_ref = job.add_preserved("html", "html", payload.into_bytes());
            log.record(
                LossClass::PreservedRaw,
                "html_comment",
                None,
                "preserved",
                &payload_ref,
                "HTML 注释以原始形式保留".to_string(),
            );
            out.push(unknown_block(job, "HTML 注释", &payload_ref));
        }
        NodeData::Element { .. } => {
            strip_active_attrs(h, log);
            let name = element_name(h).unwrap_or_default();
            match name.as_str() {
                "script" => active_content_event(log, "script 元素"),
                "style" => style_dropped(log, "body"),
                "p" => {
                    let content = inline_children(h, job, log);
                    if content.is_empty() {
                        log.node_none();
                    } else {
                        out.push(json!({
                            "id": job.idgen.uid("blk"),
                            "type": "paragraph",
                            "content": content,
                        }));
                    }
                }
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                    let level: i64 = name[1..].parse().unwrap_or(1);
                    let content = inline_children(h, job, log);
                    log.node_none();
                    out.push(json!({
                        "id": job.idgen.uid("blk"),
                        "type": "heading",
                        "level": level,
                        "content": content,
                    }));
                }
                "blockquote" => {
                    let children = flat_blocks(h, job, log);
                    log.node_none();
                    out.push(json!({
                        "id": job.idgen.uid("blk"),
                        "type": "quote",
                        "children": children,
                    }));
                }
                "ul" | "ol" => out.push(list_block(h, job, log, false)),
                "pre" => out.push(code_block(h, job, log)),
                "table" => out.push(table_block(h, job, log)),
                "hr" => {
                    log.node_none();
                    out.push(json!({"id": job.idgen.uid("blk"), "type": "horizontal_rule"}));
                }
                "figure" => {
                    if let Some(f) = figure_block(h, job, log) {
                        out.push(f);
                    }
                }
                "img" => {
                    if let Some(f) = standalone_image(h, job, log) {
                        out.push(f);
                    }
                }
                "section" | "article" | "main" => {
                    let children = flat_blocks(h, job, log);
                    log.node_none();
                    out.push(json!({
                        "id": job.idgen.uid("sec"),
                        "type": "section",
                        "children": children,
                    }));
                }
                "iframe" | "video" | "audio" | "object" | "embed" => {
                    out.push(embed_block(h, &name, job, log));
                }
                "template" => {
                    log.record(
                        LossClass::Partial,
                        "template_element",
                        None,
                        "dropped",
                        "template",
                        "template 元素内容未导入".to_string(),
                    );
                }
                n if is_inline_tag(n) => {
                    // 块级上下文里出现裸行内元素：聚合成段落
                    let spans = inline_node(h, job, log);
                    if !spans.is_empty() {
                        out.push(json!({
                            "id": job.idgen.uid("blk"),
                            "type": "paragraph",
                            "content": spans,
                        }));
                    }
                }
                _ => {
                    // div / 未知元素 / 自定义元素
                    if is_custom_element(&name) {
                        let serialized = serialize_element(h);
                        let payload_ref =
                            job.add_preserved("html", "html", serialized.into_bytes());
                        log.record(
                            LossClass::PreservedRaw,
                            "custom_element",
                            None,
                            "preserved",
                            &payload_ref,
                            format!("自定义元素 <{name}> 以原始形式保留"),
                        );
                        out.push(unknown_block(job, &format!("<{name}>"), &payload_ref));
                    } else if KNOWN_TAGS.contains(&name.as_str()) {
                        // div 等透明容器
                        for b in flat_blocks(h, job, log) {
                            out.push(b);
                        }
                    } else {
                        let serialized = serialize_element(h);
                        let payload_ref =
                            job.add_preserved("html", "html", serialized.into_bytes());
                        log.record(
                            LossClass::PreservedRaw,
                            "unknown_element",
                            None,
                            "preserved",
                            &payload_ref,
                            format!("未知元素 <{name}> 以原始形式保留"),
                        );
                        out.push(unknown_block(job, &format!("<{name}>"), &payload_ref));
                    }
                }
            }
        }
        _ => {}
    }
    out
}

/// div 的直接文本子节点需要聚合成段落；此处把子节点序列整体按块处理。
fn flat_blocks(h: &Handle, job: &mut ImportJob, log: &mut LossLog) -> Vec<Value> {
    let mut out = Vec::new();
    let mut pending_text: Vec<Value> = Vec::new();
    for child in h.children.borrow().iter() {
        let is_blockish = match &child.data {
            NodeData::Element { .. } => !is_inline_tag(&element_name(child).unwrap_or_default()),
            NodeData::Text { contents } => contents.borrow().trim().is_empty(), // 纯空白忽略
            _ => false,
        };
        if is_blockish {
            flush_text(&mut pending_text, job, log, &mut out);
            out.extend(body_blocks(child, job, log));
        } else {
            pending_text.extend(inline_node(child, job, log));
        }
    }
    flush_text(&mut pending_text, job, log, &mut out);
    out
}

fn flush_text(
    pending: &mut Vec<Value>,
    job: &mut ImportJob,
    log: &mut LossLog,
    out: &mut Vec<Value>,
) {
    let non_empty: Vec<Value> = pending
        .drain(..)
        .filter(|s| {
            s.get("type").and_then(Value::as_str) != Some("text")
                || !s
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .unwrap_or("")
                    .is_empty()
        })
        .collect();
    if !non_empty.is_empty() {
        log.node_none();
        out.push(json!({
            "id": job.idgen.uid("blk"),
            "type": "paragraph",
            "content": non_empty,
        }));
    }
}

fn list_block(h: &Handle, job: &mut ImportJob, log: &mut LossLog, tight: bool) -> Value {
    let _ = tight;
    let ordered = element_name(h).as_deref() == Some("ol");
    let start = attr(h, "start")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(1);
    let mut items = Vec::new();
    for child in h.children.borrow().iter() {
        if element_name(child).as_deref() == Some("li") {
            let checked = attr(child, "data-azodoc-checked").and_then(|s| s.parse::<bool>().ok());
            let children = flat_blocks(child, job, log);
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
        } else if element_name(child).is_some() {
            // ul/ol 直接嵌套（不规范但常见）：作为子列表并入上一项
            if let Some(last) = items.last_mut() {
                if let Some(children) = last.get_mut("children").and_then(Value::as_array_mut) {
                    if element_name(child)
                        .as_deref()
                        .map(|n| n == "ul" || n == "ol")
                        .unwrap_or(false)
                    {
                        children.push(list_block(child, job, log, true));
                    }
                }
            }
        }
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
            .insert("start".to_string(), json!(start));
    }
    b
}

fn code_block(h: &Handle, job: &mut ImportJob, log: &mut LossLog) -> Value {
    let mut language = None;
    let text = match h
        .children
        .borrow()
        .first()
        .map(|c| (element_name(c), c.clone()))
    {
        Some((Some(n), c)) if n == "code" => {
            language = class_language(&c);
            text_content(&c)
        }
        _ => text_content(h),
    };
    log.node_none();
    json!({
        "id": job.idgen.uid("blk"),
        "type": "code_block",
        "language": language,
        "text": text,
    })
}

fn class_language(h: &Handle) -> Option<String> {
    let classes = attr(h, "class")?;
    classes
        .split_whitespace()
        .find(|c| c.starts_with("language-"))
        .map(|c| c["language-".len()..].to_string())
}

fn table_block(h: &Handle, job: &mut ImportJob, log: &mut LossLog) -> Value {
    let mut columns: Vec<Value> = Vec::new();
    let mut rows: Vec<Value> = Vec::new();
    let mut header_row = false;

    let mut sections: Vec<Handle> = Vec::new();
    for child in h.children.borrow().iter() {
        match element_name(child).as_deref() {
            Some("thead") | Some("tbody") | Some("tfoot") => sections.push(child.clone()),
            Some("tr") => sections.push(child.clone()),
            _ => {}
        }
    }
    for section in &sections {
        let is_head = element_name(section).as_deref() == Some("thead");
        let rows_here: Vec<Handle> = if is_head {
            section.children.borrow().clone()
        } else if element_name(section).as_deref() == Some("tr") {
            vec![section.clone()]
        } else {
            section.children.borrow().clone()
        };
        for tr in rows_here {
            if element_name(&tr).as_deref() != Some("tr") {
                continue;
            }
            let mut cells = Vec::new();
            for (i, td) in tr.children.borrow().iter().enumerate() {
                let name = element_name(td).unwrap_or_default();
                if name != "td" && name != "th" {
                    continue;
                }
                if is_head || name == "th" {
                    header_row = true;
                }
                let col_span = attr(td, "colspan")
                    .and_then(|s| s.parse::<i64>().ok())
                    .unwrap_or(1);
                let row_span = attr(td, "rowspan")
                    .and_then(|s| s.parse::<i64>().ok())
                    .unwrap_or(1);
                let mut children = flat_blocks(td, job, log);
                if children.is_empty() {
                    children.push(json!({
                        "id": job.idgen.uid("blk"),
                        "type": "paragraph",
                        "content": [],
                    }));
                }
                let mut cell = json!({
                    "id": job.idgen.uid("cel"),
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
            if !cells.is_empty() {
                rows.push(json!({"id": job.idgen.uid("row"), "cells": cells}));
            }
        }
    }
    // 列名取表头第一行文本
    if header_row {
        if let Some(first) = rows.first() {
            if let Some(cells) = first.get("cells").and_then(Value::as_array) {
                for c in cells {
                    let mut name = String::new();
                    if let Some(children) = c.get("children").and_then(Value::as_array) {
                        for ch in children {
                            name.push_str(&azodoc_convert::plain_text_of_block(ch));
                        }
                    }
                    columns.push(json!({"id": job.idgen.uid("col"), "name": name.trim()}));
                }
            }
        }
    } else {
        let width = rows
            .iter()
            .map(|r| {
                r.get("cells")
                    .and_then(Value::as_array)
                    .map(|a| a.len())
                    .unwrap_or(0)
            })
            .max()
            .unwrap_or(0);
        for _ in 0..width {
            columns.push(json!({"id": job.idgen.uid("col"), "name": ""}));
        }
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

fn figure_block(h: &Handle, job: &mut ImportJob, log: &mut LossLog) -> Option<Value> {
    let mut asset = None;
    let mut alt = String::new();
    let mut caption: Vec<Value> = Vec::new();
    for child in h.children.borrow().iter() {
        match element_name(child).as_deref() {
            Some("img") => {
                asset = Some(img_asset(child, job, log));
                alt = attr(child, "alt").unwrap_or_default();
            }
            Some("figcaption") => {
                caption = inline_children(child, job, log);
            }
            _ => {}
        }
    }
    let asset = asset?;
    log.node_none();
    Some(json!({
        "id": job.idgen.uid("blk"),
        "type": "figure",
        "asset": asset,
        "alt": alt,
        "caption": caption,
    }))
}

fn standalone_image(h: &Handle, job: &mut ImportJob, log: &mut LossLog) -> Option<Value> {
    let asset = img_asset(h, job, log);
    let alt = attr(h, "alt").unwrap_or_default();
    log.node_none();
    Some(json!({
        "id": job.idgen.uid("blk"),
        "type": "figure",
        "asset": asset,
        "alt": alt,
        "caption": [],
    }))
}

fn img_asset(h: &Handle, job: &mut ImportJob, log: &mut LossLog) -> String {
    let src = attr(h, "src").unwrap_or_default();
    resolve_media(&src, job, log)
}

fn resolve_media(src: &str, job: &mut ImportJob, log: &mut LossLog) -> String {
    if let Some(rest) = src.strip_prefix("data:") {
        if let Some((meta, b64)) = rest.split_once(",") {
            let mime = meta.strip_suffix(";base64").unwrap_or(meta).to_string();
            use base64::Engine as _;
            if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64.trim()) {
                let ext = mime_ext(&mime);
                return job.add_asset(
                    &format!("embedded.{ext}"),
                    &mime,
                    azodoc_convert::AssetSource::Embedded(bytes),
                );
            }
        }
        log.record(
            LossClass::Partial,
            "data_uri_invalid",
            None,
            "externalized",
            src,
            "data URI 无法解码，以外链形式保留".to_string(),
        );
        return job.add_asset(
            "asset",
            "application/octet-stream",
            azodoc_convert::AssetSource::External(src.to_string()),
        );
    }
    if src.starts_with("http://") || src.starts_with("https://") {
        let name = src.rsplit('/').next().unwrap_or("asset");
        return job.add_asset(
            name,
            azodoc_convert::guess_mime(name),
            azodoc_convert::AssetSource::External(src.to_string()),
        );
    }
    if let Some(base) = &job.base_dir {
        if let Ok(bytes) = std::fs::read(base.join(src)) {
            let name = src
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or("asset")
                .to_string();
            let mime = azodoc_convert::guess_mime(&name).to_string();
            return job.add_asset(&name, &mime, azodoc_convert::AssetSource::Embedded(bytes));
        }
    }
    log.record(
        LossClass::Partial,
        "local_media_missing",
        None,
        "externalized",
        src,
        "本地资源不存在，以外链形式保留原始路径".to_string(),
    );
    job.add_asset(
        src.rsplit(['/', '\\']).next().unwrap_or("asset"),
        azodoc_convert::guess_mime(src),
        azodoc_convert::AssetSource::External(src.to_string()),
    )
}

fn mime_ext(mime: &str) -> &str {
    match mime {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/svg+xml" => "svg",
        "image/webp" => "webp",
        "video/mp4" => "mp4",
        "audio/mpeg" => "mp3",
        _ => "bin",
    }
}

fn embed_block(h: &Handle, name: &str, job: &mut ImportJob, log: &mut LossLog) -> Value {
    let src = attr(h, "src").unwrap_or_default();
    let asset = if src.is_empty() {
        job.add_asset(
            "embedded",
            "application/octet-stream",
            azodoc_convert::AssetSource::External(String::new()),
        )
    } else {
        resolve_media(&src, job, log)
    };
    log.record(
        LossClass::Partial,
        "embed_html",
        None,
        "degraded",
        name,
        format!("<{name}> 以 embed 块导入，呈现细节降级"),
    );
    json!({
        "id": job.idgen.uid("blk"),
        "type": "embed",
        "asset": asset,
    })
}

fn unknown_block(job: &mut ImportJob, summary: &str, payload_ref: &str) -> Value {
    json!({
        "id": job.idgen.uid("blk"),
        "type": "unknown",
        "origin": "html",
        "loss_class": "preserved_raw",
        "summary": summary,
        "payload_ref": payload_ref,
        "content": [],
    })
}

/// 事件属性移除（安全例外）。
fn strip_active_attrs(h: &Handle, log: &mut LossLog) {
    if let NodeData::Element { attrs, .. } = &h.data {
        let mut to_remove = Vec::new();
        for (i, a) in attrs.borrow().iter().enumerate() {
            let name = a.name.local.to_string().to_ascii_lowercase();
            let value = a.value.to_string();
            if name.starts_with("on") {
                to_remove.push(i);
                active_content_event(log, &format!("事件属性 {name}"));
            } else if (name == "href" || name == "src" || name == "xlink:href")
                && value
                    .trim_start()
                    .to_ascii_lowercase()
                    .starts_with("javascript:")
            {
                to_remove.push(i);
                active_content_event(log, "javascript: URL");
            }
        }
        for i in to_remove.into_iter().rev() {
            attrs.borrow_mut().remove(i);
        }
    }
}

const INLINE_TAGS: &[&str] = &[
    "em", "i", "strong", "b", "u", "ins", "s", "strike", "del", "code", "a", "img", "br", "span",
    "mark", "sub", "sup", "small", "q", "abbr", "cite", "var", "kbd", "samp", "time", "wbr",
    "font", "big",
];

fn is_inline_tag(name: &str) -> bool {
    INLINE_TAGS.contains(&name)
}

const KNOWN_TAGS: &[&str] = &["div", "center", "p", "h1", "h2", "h3", "h4", "h5", "h6"];

fn is_custom_element(name: &str) -> bool {
    name.contains('-')
}

/// 行内遍历。返回 0..n 个 span。
fn inline_children(h: &Handle, job: &mut ImportJob, log: &mut LossLog) -> Vec<Value> {
    let mut out = Vec::new();
    for child in h.children.borrow().iter() {
        out.extend(inline_node(child, job, log));
    }
    out
}

fn inline_node(h: &Handle, job: &mut ImportJob, log: &mut LossLog) -> Vec<Value> {
    match &h.data {
        NodeData::Text { contents } => {
            let raw = contents.borrow().to_string();
            if raw.trim().is_empty() && !raw.contains('\u{00A0}') {
                Vec::new()
            } else {
                log.node_none();
                vec![json!({"type": "text", "text": collapse_ws(&raw)})]
            }
        }
        NodeData::Comment { contents } => {
            let payload = format!("<!--{}-->", contents);
            let payload_ref = job.add_preserved("html", "html", payload.into_bytes());
            log.record(
                LossClass::PreservedRaw,
                "html_comment",
                None,
                "preserved",
                &payload_ref,
                "HTML 注释以原始形式保留".to_string(),
            );
            vec![unknown_span(job, "HTML 注释", &payload_ref)]
        }
        NodeData::Element { .. } => {
            strip_active_attrs(h, log);
            let name = element_name(h).unwrap_or_default();
            match name.as_str() {
                "script" => {
                    active_content_event(log, "script 元素");
                    Vec::new()
                }
                "br" => {
                    log.node_none();
                    vec![json!({"type": "hard_break"})]
                }
                "em" | "i" => vec![wrap_span("em", h, job, log)],
                "strong" | "b" => vec![wrap_span("strong", h, job, log)],
                "u" | "ins" => vec![wrap_span("underline", h, job, log)],
                "s" | "strike" | "del" => vec![wrap_span("strike", h, job, log)],
                "code" | "kbd" | "samp" => {
                    let text = text_content(h);
                    log.node_none();
                    vec![json!({"type": "code", "text": text})]
                }
                "a" => {
                    let url = attr(h, "href").unwrap_or_default();
                    let content = inline_children(h, job, log);
                    log.node_none();
                    let mut span = json!({
                        "type": "link",
                        "url": url,
                        "content": content,
                    });
                    if let Some(t) = attr(h, "title") {
                        span.as_object_mut()
                            .unwrap()
                            .insert("title".to_string(), json!(t));
                    }
                    vec![span]
                }
                "img" => {
                    let asset = img_asset(h, job, log);
                    let alt = attr(h, "alt").unwrap_or_default();
                    log.node_none();
                    vec![json!({"type": "inline_image", "asset": asset, "alt": alt})]
                }
                "span" => {
                    if attr(h, "class").is_some() || attr(h, "style").is_some() {
                        log.record(
                            LossClass::Partial,
                            "style_attribute",
                            None,
                            "degraded",
                            "span",
                            "span 的样式类无法映射到 v1 主题，已丢弃".to_string(),
                        );
                    }
                    inline_children(h, job, log)
                }
                "mark" => {
                    log.record(
                        LossClass::Partial,
                        "mark_element",
                        None,
                        "degraded",
                        "mark",
                        "<mark> 映射为强调".to_string(),
                    );
                    vec![wrap_span("strong", h, job, log)]
                }
                "sub" | "sup" | "small" | "q" | "abbr" | "cite" | "var" | "time" | "font"
                | "big" | "wbr" => {
                    log.record(
                        LossClass::Partial,
                        "inline_semantic",
                        None,
                        "degraded",
                        &name,
                        format!("<{name}> 的语义在 v1 模型中无对应，退化为内部内容"),
                    );
                    inline_children(h, job, log)
                }
                _ if is_custom_element(&name) || !is_inline_tag(&name) => {
                    let serialized = serialize_element(h);
                    let payload_ref = job.add_preserved("html", "html", serialized.into_bytes());
                    log.record(
                        LossClass::PreservedRaw,
                        "custom_element",
                        None,
                        "preserved",
                        &payload_ref,
                        format!("<{name}> 以原始形式保留"),
                    );
                    vec![unknown_span(job, &format!("<{name}>"), &payload_ref)]
                }
                _ => inline_children(h, job, log),
            }
        }
        _ => Vec::new(),
    }
}

fn wrap_span(kind: &str, h: &Handle, job: &mut ImportJob, log: &mut LossLog) -> Value {
    log.node_none();
    json!({
        "type": kind,
        "content": inline_children(h, job, log),
    })
}

fn unknown_span(_job: &mut ImportJob, summary: &str, payload_ref: &str) -> Value {
    json!({
        "type": "unknown",
        "origin": "html",
        "loss_class": "preserved_raw",
        "summary": summary,
        "payload_ref": payload_ref,
        "content": [],
    })
}

fn text_content(h: &Handle) -> String {
    match &h.data {
        NodeData::Text { contents } => contents.borrow().to_string(),
        NodeData::Document | NodeData::Doctype { .. } => String::new(),
        NodeData::Comment { .. } | NodeData::ProcessingInstruction { .. } => String::new(),
        NodeData::Element { .. } => {
            let mut s = String::new();
            for child in h.children.borrow().iter() {
                s.push_str(&text_content(child));
            }
            s
        }
    }
}

fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_ws = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !in_ws {
                out.push(' ');
                in_ws = true;
            }
        } else {
            out.push(c);
            in_ws = false;
        }
    }
    out.trim().to_string()
}

/// 手工序列化元素（preserved 载荷；文本转义，结构保真）。
pub fn serialize_element(h: &Handle) -> String {
    match &h.data {
        NodeData::Text { contents } => escape_text(&contents.borrow()),
        NodeData::Comment { contents } => format!("<!--{}-->", contents),
        NodeData::Element { name, attrs, .. } => {
            let tag = name.local.to_string();
            let mut s = format!("<{tag}");
            for a in attrs.borrow().iter() {
                s.push_str(&format!(
                    " {}=\"{}\"",
                    a.name.local,
                    escape_text(&a.value).replace('"', "&quot;")
                ));
            }
            s.push('>');
            for child in h.children.borrow().iter() {
                s.push_str(&serialize_element(child));
            }
            s.push_str(&format!("</{tag}>"));
            s
        }
        _ => String::new(),
    }
}

pub(crate) fn escape_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

// ---------------------------------------------------------------- 净化

/// 解析并净化 HTML 片段：移除 script/style/iframe/object、事件属性、javascript: URL。
/// 返回净化后的 HTML 与「是否发生了移除」。
pub fn sanitize_fragment(html: &str) -> (String, bool) {
    let mut removed = false;
    let mut input = html.as_bytes();
    let parser = html5ever::parse_fragment(
        RcDom::default(),
        html5ever::ParseOpts::default(),
        html5ever::QualName::new(None, html5ever::ns!(), html5ever::local_name!("html")),
        vec![],
        false,
    );
    let dom = parser
        .from_utf8()
        .read_from(&mut input)
        .unwrap_or_else(|_| RcDom::default());
    for child in dom.document.children.borrow().iter() {
        sanitize_node(child, &mut removed);
    }
    let mut out = String::new();
    for child in dom.document.children.borrow().iter() {
        out.push_str(&serialize_element(child));
    }
    (out, removed)
}

fn sanitize_node(h: &Handle, removed: &mut bool) {
    // 父层负责移除活动子元素（Rc 内字段不可变）
    let mut keep: Vec<Handle> = Vec::new();
    for child in h.children.borrow().iter() {
        if is_active_element(child) {
            *removed = true;
            continue;
        }
        sanitize_node(child, removed);
        keep.push(child.clone());
    }
    *h.children.borrow_mut() = keep;

    if let NodeData::Element { attrs, .. } = &h.data {
        let mut to_remove = Vec::new();
        for (i, a) in attrs.borrow().iter().enumerate() {
            let an = a.name.local.to_string().to_ascii_lowercase();
            let av = a.value.to_string();
            if an.starts_with("on")
                || ((an == "href" || an == "src")
                    && av
                        .trim_start()
                        .to_ascii_lowercase()
                        .starts_with("javascript:"))
            {
                to_remove.push(i);
                *removed = true;
            }
        }
        for i in to_remove.into_iter().rev() {
            attrs.borrow_mut().remove(i);
        }
    }
}

fn is_active_element(h: &Handle) -> bool {
    match &h.data {
        NodeData::Element { name, .. } => {
            let tag = name.local.to_string().to_ascii_lowercase();
            matches!(
                tag.as_str(),
                "script" | "style" | "iframe" | "object" | "embed" | "link" | "meta"
            )
        }
        _ => false,
    }
}

mod export;
pub use export::{converter_name, export_html};
