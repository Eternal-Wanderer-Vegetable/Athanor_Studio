//! TXT 兜底导出（spec/azodoc-loss.md §5.3）：在 azodoc-model.md §9 纯文本算法之上
//! 叠加排版增强（标题下划线、列表编号、代码缩进）。增强不得改变纯文本语义。

use crate::{plain_text_of_block, plain_text_of_span, ExportDoc, LossClass, LossLog};
use serde_json::Value;
use std::collections::HashMap;

/// 导出 TXT。footnote 引用按出现顺序编号，定义集中到末尾。
pub fn export_txt(doc: &ExportDoc, log: &mut LossLog) -> String {
    let ctx = TxtCtx::build(doc);
    let mut out = String::new();
    if let Some(arr) = doc.content.get("content").and_then(Value::as_array) {
        for node in arr {
            out.push_str(&render_block(node, &ctx, 0));
        }
    }
    // 脚注定义集中到末尾
    let mut ordered: Vec<(usize, &Value)> = ctx
        .footnote_nums
        .iter()
        .filter_map(|(id, n)| ctx.footnote_defs.get(id).map(|d| (*n, d)))
        .collect();
    ordered.sort_by_key(|(n, _)| *n);
    if !ordered.is_empty() {
        out.push_str("脚注：\n");
        for (n, def) in ordered {
            out.push_str(&format!("[{n}] {}\n", plain_text_of_block(def)));
        }
    }
    count_all(&doc.content, log);
    out
}

struct TxtCtx<'a> {
    doc: &'a ExportDoc,
    footnote_defs: HashMap<String, Value>,
    footnote_nums: HashMap<String, usize>,
}

impl<'a> TxtCtx<'a> {
    fn build(doc: &'a ExportDoc) -> TxtCtx<'a> {
        let mut ctx = TxtCtx {
            doc,
            footnote_defs: HashMap::new(),
            footnote_nums: HashMap::new(),
        };
        if let Some(arr) = doc.content.get("content").and_then(Value::as_array) {
            for node in arr {
                collect_defs(Some(node), &mut ctx.footnote_defs);
                number_refs(Some(node), &mut ctx.footnote_nums);
            }
        }
        ctx
    }
}

fn collect_defs(v: Option<&Value>, defs: &mut HashMap<String, Value>) {
    let Some(v) = v else { return };
    if v.get("type").and_then(Value::as_str) == Some("footnote") {
        if let Some(id) = v.get("id").and_then(Value::as_str) {
            defs.insert(id.to_string(), v.clone());
        }
    }
    for key in ["children", "items", "cells", "rows", "content", "caption"] {
        walk_all(v.get(key), defs);
    }
}

fn walk_all(v: Option<&Value>, defs: &mut HashMap<String, Value>) {
    if let Some(arr) = v.and_then(Value::as_array) {
        for item in arr {
            collect_defs(Some(item), defs);
        }
    }
}

fn number_refs(v: Option<&Value>, nums: &mut HashMap<String, usize>) {
    let Some(v) = v else { return };
    if v.get("type").and_then(Value::as_str) == Some("footnote_ref") {
        if let Some(id) = v.get("id").and_then(Value::as_str) {
            let next = nums.len() + 1;
            nums.entry(id.to_string()).or_insert(next);
        }
    }
    for key in ["children", "items", "cells", "rows", "content", "caption"] {
        if let Some(arr) = v.get(key).and_then(Value::as_array) {
            for item in arr {
                number_refs(Some(item), nums);
            }
        }
    }
}

fn render_block(node: &Value, ctx: &TxtCtx, depth: usize) -> String {
    let t = node.get("type").and_then(Value::as_str).unwrap_or("");
    match t {
        "section" => {
            let mut s = String::new();
            if let Some(children) = node.get("children").and_then(Value::as_array) {
                for c in children {
                    s.push_str(&render_block(c, ctx, depth));
                }
            }
            s
        }
        "heading" => {
            let text = spans_text(node.get("content"));
            let level = node.get("level").and_then(Value::as_i64).unwrap_or(1);
            let ch = if level <= 1 { '=' } else { '-' };
            let width = visual_width(&text).max(1);
            let underline: String = std::iter::repeat(ch).take(width).collect();
            format!("{text}\n{underline}\n\n")
        }
        "paragraph" => {
            let text = spans_text(node.get("content"));
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
                    inner.push_str(&render_block(c, ctx, depth));
                }
            }
            inner
                .lines()
                .map(|l| format!("  {l}\n"))
                .collect::<String>()
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
                    let mut first = true;
                    if let Some(children) = item.get("children").and_then(Value::as_array) {
                        for c in children {
                            let rendered = render_block(c, ctx, depth + 1);
                            for (li, line) in rendered.lines().enumerate() {
                                if first {
                                    s.push_str(&format!("{marker}{check}"));
                                    first = false;
                                } else if li == 0 {
                                    s.push_str(&cont);
                                }
                                s.push_str(line);
                                s.push('\n');
                            }
                            // 渲染块以 "\n\n" 结尾时最后一行后有空行：lines() 已消化
                        }
                    }
                    if first {
                        s.push_str(&format!("{marker}{check}\n"));
                    }
                }
            }
            s.push('\n');
            s
        }
        "code_block" => {
            let text = node.get("text").and_then(Value::as_str).unwrap_or("");
            let mut s = String::new();
            for line in text.lines() {
                s.push_str("  ");
                s.push_str(line);
                s.push('\n');
            }
            s.push('\n');
            s
        }
        "table" => {
            let mut lines: Vec<String> = Vec::new();
            if node.get("header_row").and_then(Value::as_bool) == Some(true) {
                let header: Vec<String> = column_names(node);
                lines.push(header.join(" / "));
            }
            let data_rows: &[Value] = node
                .get("rows")
                .and_then(Value::as_array)
                .map(|a| a.as_slice())
                .unwrap_or(&[]);
            // header_row 时 rows[0] 即表头数据，已由列名行呈现，跳过避免重复
            let skip = if node.get("header_row").and_then(Value::as_bool) == Some(true)
                && !lines.is_empty()
            {
                1
            } else {
                0
            };
            for row in data_rows.iter().skip(skip) {
                let mut cells = Vec::new();
                if let Some(cs) = row.get("cells").and_then(Value::as_array) {
                    for c in cs {
                        let mut parts = Vec::new();
                        if let Some(children) = c.get("children").and_then(Value::as_array) {
                            for ch in children {
                                parts.push(plain_text_of_block(ch));
                            }
                        }
                        cells.push(parts.join(" "));
                    }
                }
                lines.push(cells.join(" / "));
            }
            let mut s = lines
                .into_iter()
                .fold(String::new(), |acc, l| format!("{acc}{l}\n"));
            s.push('\n');
            s
        }
        "figure" | "image" => {
            let alt = node.get("alt").and_then(Value::as_str).unwrap_or("");
            let mut s = format!("[图: {alt}]\n");
            if t == "figure" {
                let cap = spans_text(node.get("caption"));
                if !cap.is_empty() {
                    s.push_str(&cap);
                    s.push('\n');
                }
            }
            s.push('\n');
            s
        }
        "callout" => {
            let variant = node
                .get("variant")
                .and_then(Value::as_str)
                .unwrap_or("note")
                .to_uppercase();
            let mut inner = String::new();
            if let Some(children) = node.get("children").and_then(Value::as_array) {
                for c in children {
                    let rendered = render_block(c, ctx, depth);
                    inner.push_str(rendered.trim_end_matches('\n'));
                }
            }
            format!("[{variant}] {inner}\n\n")
        }
        "math_block" => format!(
            "{}\n\n",
            node.get("latex").and_then(Value::as_str).unwrap_or("")
        ),
        "horizontal_rule" => "──────────\n\n".to_string(),
        "embed" => {
            let asset = node.get("asset").and_then(Value::as_str).unwrap_or("");
            let name = ctx.doc.asset_filename(asset).unwrap_or(asset);
            format!("[附件: {name}]\n\n")
        }
        "unknown" => {
            let summary = node
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or("未知内容");
            format!("[未能显示的内容: {summary}]\n\n")
        }
        "footnote" => String::new(), // 定义集中到文档末尾
        _ => String::new(),
    }
}

fn spans_text(v: Option<&Value>) -> String {
    let mut s = String::new();
    if let Some(arr) = v.and_then(Value::as_array) {
        for span in arr {
            s.push_str(&render_span(span, None));
        }
    }
    s
}

fn render_span(span: &Value, nums: Option<&HashMap<String, usize>>) -> String {
    let t = span.get("type").and_then(Value::as_str).unwrap_or("");
    match t {
        "footnote_ref" => {
            let id = span.get("id").and_then(Value::as_str).unwrap_or("");
            match nums.and_then(|m| m.get(id)) {
                Some(n) => format!("[{n}]"),
                None => String::new(),
            }
        }
        "link" => {
            let text = spans_text(span.get("content"));
            let url = span.get("url").and_then(Value::as_str).unwrap_or("");
            if text.is_empty() {
                format!("<{url}>")
            } else {
                format!("{text}（{url}）")
            }
        }
        _ => plain_text_of_span(span),
    }
}

fn column_names(table: &Value) -> Vec<String> {
    table
        .get("columns")
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

/// 显示宽度：CJK 全角记 2，其余记 1（loss 规范 §5.3）。
pub fn visual_width(s: &str) -> usize {
    s.chars().map(wide_char).sum()
}

fn wide_char(c: char) -> usize {
    let cp = c as u32;
    let wide = (0x1100..=0x115F).contains(&cp)
        || (0x2E80..=0xA4CF).contains(&cp)
        || (0xAC00..=0xD7A3).contains(&cp)
        || (0xF900..=0xFAFF).contains(&cp)
        || (0xFE30..=0xFE4F).contains(&cp)
        || (0xFF00..=0xFF60).contains(&cp)
        || (0xFFE0..=0xFFE6).contains(&cp);
    if wide {
        2
    } else {
        1
    }
}

/// 所有节点计数：unknown 块按 preserved_raw 记事件，其余 none。
fn count_all(content: &Value, log: &mut LossLog) {
    fn walk_node(v: &Value, log: &mut LossLog) {
        let t = v.get("type").and_then(Value::as_str).unwrap_or("");
        match t {
            "unknown" => log.record(
                LossClass::PreservedRaw,
                "unknown_txt",
                None,
                "preserved",
                v.get("payload_ref").and_then(Value::as_str).unwrap_or(""),
                "未知内容在 TXT 导出中以占位符呈现".to_string(),
            ),
            "" => {}
            _ => log.node_none(),
        }
        for key in ["children", "items", "cells", "rows"] {
            if let Some(arr) = v.get(key).and_then(Value::as_array) {
                for c in arr {
                    walk_node(c, log);
                }
            }
        }
        for key in ["content", "caption"] {
            if let Some(arr) = v.get(key).and_then(Value::as_array) {
                for s in arr {
                    walk_span(s, log);
                }
            }
        }
    }
    fn walk_span(s: &Value, log: &mut LossLog) {
        let t = s.get("type").and_then(Value::as_str).unwrap_or("");
        match t {
            "unknown" => log.record(
                LossClass::PreservedRaw,
                "unknown_span_txt",
                None,
                "preserved",
                s.get("payload_ref").and_then(Value::as_str).unwrap_or(""),
                "未知行内内容在 TXT 导出中被省略".to_string(),
            ),
            "" => {}
            _ => log.node_none(),
        }
        if let Some(arr) = s.get("content").and_then(Value::as_array) {
            for inner in arr {
                walk_span(inner, log);
            }
        }
    }
    if let Some(arr) = content.get("content").and_then(Value::as_array) {
        for n in arr {
            walk_node(n, log);
        }
    }
}
