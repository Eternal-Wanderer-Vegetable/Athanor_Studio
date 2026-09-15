// This file is part of Athanor, the Azodoc document engine.
// Copyright (C) 2026 Athanor Studio
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

//! HTML 导出：自包含单文件（资产 data URI 化；含血统 meta）。
//! 脚注引用统一编号并集中到文档末尾；preserved 载荷内联前必须消毒。

use super::{escape_text, sanitize_fragment};
use azodoc_convert::{ExportDoc, LossClass, LossLog};
use serde_json::Value;

const EMBED_CSS: &str = "body{max-width:44em;margin:2rem auto;padding:0 1rem;font-family:serif;line-height:1.7;color:#222}h1,h2,h3,h4,h5,h6{line-height:1.3}.callout{border-left:4px solid #36c;background:#f0f6ff;padding:.5rem 1rem;margin:1rem 0}code,pre{background:#f5f5f5;border-radius:4px}pre{padding:.75rem;overflow:auto}figure{margin:1.5rem 0;text-align:center}figcaption{font-size:.9em;color:#666}table{border-collapse:collapse}td,th{border:1px solid #ccc;padding:.35rem .6rem}.footnotes{font-size:.9em;color:#555;border-top:1px solid #ddd;margin-top:2rem;padding-top:.5rem}.azodoc-unknown{border:1px dashed #c66;padding:.5rem 1rem;margin:1rem 0;color:#933;background:#fdf3f3}";

/// 导出上下文：文档数据 + 文档级脚注编号。
struct HtmlCtx<'a> {
    doc: &'a ExportDoc,
    /// (块 id, 序号, footnote 块)
    fn_defs: Vec<(String, usize, Value)>,
}

impl<'a> HtmlCtx<'a> {
    fn fn_num(&self, id: &str) -> Option<usize> {
        self.fn_defs
            .iter()
            .find(|(i, _, _)| i == id)
            .map(|(_, n, _)| *n)
    }
}

fn escape_attr(s: &str) -> String {
    escape_text(s).replace('"', "&quot;")
}

/// 导出为自包含单文件 HTML。
pub fn export_html(doc: &ExportDoc, log: &mut LossLog) -> String {
    // 文档级脚注编号（按定义在文档中的出现顺序）
    let mut fn_ids: Vec<String> = Vec::new();
    collect_fn_ids(doc.content.get("content"), &mut fn_ids);
    let fn_defs: Vec<(String, usize, Value)> = fn_ids
        .into_iter()
        .enumerate()
        .map(|(i, id)| {
            let def = find_fn_def(doc.content.get("content"), &id).unwrap_or(Value::Null);
            (id, i + 1, def)
        })
        .collect();
    let ctx = HtmlCtx { doc, fn_defs };

    let lang = doc.language.clone().unwrap_or_else(|| "zh-CN".into());
    let title = doc.title.clone().unwrap_or_else(|| "未命名文档".into());
    let mut out = String::new();
    out.push_str("<!doctype html>\n<html lang=\"");
    out.push_str(&escape_attr(&lang));
    out.push_str("\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    if let Some(id) = &doc.document_id {
        out.push_str(&format!(
            "<meta name=\"azodoc.document-id\" content=\"{}\">\n",
            escape_attr(id)
        ));
    }
    out.push_str(&format!("<title>{}</title>\n", escape_text(&title)));
    out.push_str("<style>");
    out.push_str(EMBED_CSS);
    out.push_str("</style>\n</head>\n<body>\n<article>\n");

    if let Some(arr) = doc.content.get("content").and_then(Value::as_array) {
        for node in arr {
            out.push_str(&block_html(node, &ctx, log, 0));
        }
    }

    if !ctx.fn_defs.is_empty() {
        out.push_str("<section class=\"footnotes\">\n<ol>\n");
        for (id, _n, def) in &ctx.fn_defs {
            let mut inner = String::new();
            if let Some(children) = def.get("children").and_then(Value::as_array) {
                for c in children {
                    inner.push_str(&block_html(c, &ctx, log, 1));
                }
            }
            out.push_str(&format!(
                "<li id=\"fn-{id}\">{} <a href=\"#fnref-{id}\">&#8617;</a></li>\n",
                inner.trim_end()
            ));
        }
        out.push_str("</ol>\n</section>\n");
    }

    out.push_str("</article>\n</body>\n</html>\n");
    out
}

fn collect_fn_ids(v: Option<&Value>, ids: &mut Vec<String>) {
    let Some(v) = v else { return };
    if v.get("type").and_then(Value::as_str) == Some("footnote") {
        if let Some(id) = v.get("id").and_then(Value::as_str) {
            if !ids.iter().any(|x| x == id) {
                ids.push(id.to_string());
            }
        }
    }
    for key in ["children", "items", "cells", "rows", "content", "caption"] {
        if let Some(arr) = v.get(key).and_then(Value::as_array) {
            for item in arr {
                collect_fn_ids(Some(item), ids);
            }
        }
    }
}

fn find_fn_def(v: Option<&Value>, id: &str) -> Option<Value> {
    let v = v?;
    if v.get("type").and_then(Value::as_str) == Some("footnote")
        && v.get("id").and_then(Value::as_str) == Some(id)
    {
        return Some(v.clone());
    }
    for key in ["children", "items", "cells", "rows", "content", "caption"] {
        if let Some(arr) = v.get(key).and_then(Value::as_array) {
            for item in arr {
                if let Some(found) = find_fn_def(Some(item), id) {
                    return Some(found);
                }
            }
        }
    }
    None
}

fn block_html(node: &Value, ctx: &HtmlCtx, log: &mut LossLog, depth: usize) -> String {
    let t = node.get("type").and_then(Value::as_str).unwrap_or("");
    match t {
        "section" => {
            // section 是组织性糖（azodoc-model.md §5）：透明展开避免双层化
            log.node_none();
            let mut inner = String::new();
            if let Some(children) = node.get("children").and_then(Value::as_array) {
                for c in children {
                    inner.push_str(&block_html(c, ctx, log, depth + 1));
                }
            }
            inner
        }
        "heading" => {
            let level = node.get("level").and_then(Value::as_i64).unwrap_or(1);
            let id = node.get("id").and_then(Value::as_str).unwrap_or("");
            let text = inline_html(node.get("content"), ctx, log);
            log.node_none();
            format!("<h{level} id=\"{id}\">{text}</h{level}>\n")
        }
        "paragraph" => {
            let text = inline_html(node.get("content"), ctx, log);
            if text.trim().is_empty() {
                log.node_none();
                String::new()
            } else {
                format!("<p>{text}</p>\n")
            }
        }
        "quote" => {
            let mut inner = String::new();
            if let Some(children) = node.get("children").and_then(Value::as_array) {
                for c in children {
                    inner.push_str(&block_html(c, ctx, log, depth + 1));
                }
            }
            format!("<blockquote>\n{inner}</blockquote>\n")
        }
        "list" => {
            let style = node
                .get("style")
                .and_then(Value::as_str)
                .unwrap_or("bullet");
            let tag = if style == "ordered" { "ol" } else { "ul" };
            let start_attr = match node.get("start").and_then(Value::as_i64) {
                Some(s) if s != 1 => format!(" start=\"{s}\""),
                _ => String::new(),
            };
            let mut inner = String::new();
            if let Some(items) = node.get("items").and_then(Value::as_array) {
                for item in items {
                    let mut li_inner = String::new();
                    if let Some(c) = item.get("checked").and_then(Value::as_bool) {
                        let state = if c { " checked" } else { "" };
                        li_inner.push_str(&format!("<input type=\"checkbox\" disabled{state}> "));
                    }
                    if let Some(children) = item.get("children").and_then(Value::as_array) {
                        for (i, c) in children.iter().enumerate() {
                            let rendered = block_html(c, ctx, log, depth + 1);
                            if i == 0 && c.get("type").and_then(Value::as_str) == Some("paragraph")
                            {
                                li_inner.push_str(
                                    rendered
                                        .trim_start_matches("<p>")
                                        .trim_end_matches("</p>\n"),
                                );
                                li_inner.push('\n');
                            } else {
                                li_inner.push_str(&rendered);
                            }
                        }
                    }
                    inner.push_str(&format!("<li>{}</li>\n", li_inner.trim_end()));
                }
            }
            log.node_none();
            format!("<{tag}{start_attr}>\n{inner}</{tag}>\n")
        }
        "code_block" => {
            let text = node.get("text").and_then(Value::as_str).unwrap_or("");
            let lang = node
                .get("language")
                .and_then(Value::as_str)
                .map(|l| format!(" class=\"language-{l}\""))
                .unwrap_or_default();
            log.node_none();
            format!("<pre><code{lang}>{}</code></pre>\n", escape_text(text))
        }
        "table" => {
            let header_row = node.get("header_row").and_then(Value::as_bool) == Some(true);
            let mut thead = String::new();
            let mut tbody = String::new();
            if let Some(rows) = node.get("rows").and_then(Value::as_array) {
                for (ri, row) in rows.iter().enumerate() {
                    let is_header = header_row && ri == 0;
                    let mut row_html = String::new();
                    if let Some(cells) = row.get("cells").and_then(Value::as_array) {
                        for c in cells {
                            let mut attrs = String::new();
                            let cs = c.get("colSpan").and_then(Value::as_i64).unwrap_or(1);
                            let rs = c.get("rowSpan").and_then(Value::as_i64).unwrap_or(1);
                            if cs > 1 {
                                attrs.push_str(&format!(" colspan=\"{cs}\""));
                            }
                            if rs > 1 {
                                attrs.push_str(&format!(" rowspan=\"{rs}\""));
                            }
                            let tag = if is_header { "th" } else { "td" };
                            let mut cell_inner = String::new();
                            if let Some(children) = c.get("children").and_then(Value::as_array) {
                                for ch in children {
                                    cell_inner.push_str(&block_html(ch, ctx, log, depth + 1));
                                }
                            }
                            row_html.push_str(&format!(
                                "<{tag}{attrs}>{}</{tag}>",
                                cell_inner.trim_end()
                            ));
                        }
                    }
                    if is_header {
                        thead.push_str(&format!("<thead>\n<tr>{row_html}</tr>\n</thead>\n"));
                    } else {
                        tbody.push_str(&format!("<tr>{row_html}</tr>\n"));
                    }
                }
            }
            log.node_none();
            format!("<table>\n{thead}{tbody}</table>\n")
        }
        "figure" => {
            let asset = node.get("asset").and_then(Value::as_str).unwrap_or("");
            let alt = node.get("alt").and_then(Value::as_str).unwrap_or("");
            let url = ctx.doc.asset_url(asset);
            log.node_none();
            let cap = inline_html(node.get("caption"), ctx, log);
            let cap_html = if cap.is_empty() {
                String::new()
            } else {
                format!("<figcaption>{cap}</figcaption>\n")
            };
            format!(
                "<figure><img src=\"{}\" alt=\"{}\">{cap_html}</figure>\n",
                escape_attr(&url),
                escape_attr(alt)
            )
        }
        "image" => {
            let asset = node.get("asset").and_then(Value::as_str).unwrap_or("");
            let alt = node.get("alt").and_then(Value::as_str).unwrap_or("");
            let url = ctx.doc.asset_url(asset);
            log.node_none();
            format!(
                "<p><img src=\"{}\" alt=\"{}\"></p>\n",
                escape_attr(&url),
                escape_attr(alt)
            )
        }
        "horizontal_rule" => {
            log.node_none();
            "<hr>\n".to_string()
        }
        "math_block" => {
            let latex = node.get("latex").and_then(Value::as_str).unwrap_or("");
            log.record(
                LossClass::Degraded,
                "math_export",
                None,
                "degraded",
                latex,
                "公式以 data-latex 文本呈现（KaTeX 内嵌为后续 RFC）".to_string(),
            );
            format!(
                "<p class=\"math\" data-latex=\"{}\">{}</p>\n",
                escape_attr(latex),
                escape_text(latex)
            )
        }
        "callout" => {
            let variant = node
                .get("variant")
                .and_then(Value::as_str)
                .unwrap_or("note");
            let mut inner = String::new();
            if let Some(children) = node.get("children").and_then(Value::as_array) {
                for c in children {
                    inner.push_str(&block_html(c, ctx, log, depth + 1));
                }
            }
            log.node_none();
            format!("<div class=\"callout\" data-variant=\"{variant}\">{inner}</div>\n")
        }
        "footnote" => String::new(), // 集中到末尾
        "embed" => {
            let asset = node.get("asset").and_then(Value::as_str).unwrap_or("");
            let url = ctx.doc.asset_url(asset);
            let mime = ctx
                .doc
                .assets
                .iter()
                .find(|a| asset.contains(&a.id))
                .map(|a| a.mime.clone())
                .unwrap_or_default();
            if mime.starts_with("image/") {
                log.node_none();
                format!("<p><img src=\"{}\" alt=\"\"></p>\n", escape_attr(&url))
            } else if mime.starts_with("video/") {
                log.node_none();
                format!(
                    "<p><video controls src=\"{}\"></video></p>\n",
                    escape_attr(&url)
                )
            } else if mime.starts_with("audio/") {
                log.node_none();
                format!(
                    "<p><audio controls src=\"{}\"></audio></p>\n",
                    escape_attr(&url)
                )
            } else {
                let name = ctx.doc.asset_filename(asset).unwrap_or(asset);
                log.record(
                    LossClass::Partial,
                    "embed_html_export",
                    None,
                    "degraded",
                    asset,
                    "内嵌资源以链接呈现".to_string(),
                );
                format!(
                    "<p><a href=\"{}\">{}</a></p>\n",
                    escape_attr(&url),
                    escape_text(name)
                )
            }
        }
        "unknown" => {
            let summary = node.get("summary").and_then(Value::as_str).unwrap_or("");
            let payload_ref = node
                .get("payload_ref")
                .and_then(Value::as_str)
                .unwrap_or("");
            if let Some(bytes) = ctx.doc.preserved_bytes(payload_ref) {
                let raw = String::from_utf8_lossy(bytes);
                let (clean, removed) = sanitize_fragment(&raw);
                if removed {
                    log.record(
                        LossClass::Unsupported,
                        "active_content",
                        None,
                        "removed",
                        payload_ref,
                        "preserved 载荷内联前已消毒（移除活动内容）".to_string(),
                    );
                }
                log.node_none();
                format!(
                    "<div data-azodoc-unknown=\"{}\">{clean}</div>\n",
                    escape_attr(summary)
                )
            } else {
                log.record(
                    LossClass::PreservedRaw,
                    "unknown_block_html",
                    None,
                    "preserved",
                    payload_ref,
                    "未知内容以占位框呈现".to_string(),
                );
                format!(
                    "<div class=\"azodoc-unknown\" data-azodoc-unknown=\"{}\">[未能显示: {}]</div>\n",
                    escape_attr(summary),
                    escape_text(summary)
                )
            }
        }
        _ => {
            log.record(
                LossClass::Partial,
                "html_export_fallback",
                None,
                "degraded",
                t,
                "未知块类型按子内容降级导出".to_string(),
            );
            let mut s = String::new();
            for key in ["children", "items"] {
                if let Some(children) = node.get(key).and_then(Value::as_array) {
                    for c in children {
                        s.push_str(&block_html(c, ctx, log, depth + 1));
                    }
                }
            }
            s
        }
    }
}

fn inline_html(spans: Option<&Value>, ctx: &HtmlCtx, log: &mut LossLog) -> String {
    let mut s = String::new();
    if let Some(arr) = spans.and_then(Value::as_array) {
        for span in arr {
            s.push_str(&span_html(span, ctx, log));
        }
    }
    s
}

fn span_html(span: &Value, ctx: &HtmlCtx, log: &mut LossLog) -> String {
    let t = span.get("type").and_then(Value::as_str).unwrap_or("");
    match t {
        "text" => escape_text(span.get("text").and_then(Value::as_str).unwrap_or("")),
        "hard_break" => "<br>\n".to_string(),
        "code" => format!(
            "<code>{}</code>",
            escape_text(span.get("text").and_then(Value::as_str).unwrap_or(""))
        ),
        "em" => format!("<em>{}</em>", inline_html(span.get("content"), ctx, log)),
        "strong" => format!(
            "<strong>{}</strong>",
            inline_html(span.get("content"), ctx, log)
        ),
        "underline" => format!("<u>{}</u>", inline_html(span.get("content"), ctx, log)),
        "strike" => format!("<s>{}</s>", inline_html(span.get("content"), ctx, log)),
        "link" => {
            let url = span.get("url").and_then(Value::as_str).unwrap_or("");
            let title = span
                .get("title")
                .and_then(Value::as_str)
                .map(|t| format!(" title=\"{}\"", escape_attr(t)))
                .unwrap_or_default();
            log.node_none();
            format!(
                "<a href=\"{}\"{title}>{}</a>",
                escape_attr(url),
                inline_html(span.get("content"), ctx, log)
            )
        }
        "inline_math" => {
            let latex = span.get("latex").and_then(Value::as_str).unwrap_or("");
            log.record(
                LossClass::Degraded,
                "math_export",
                None,
                "degraded",
                latex,
                "公式以 data-latex 文本呈现".to_string(),
            );
            format!(
                "<span class=\"math\" data-latex=\"{}\">{}</span>",
                escape_attr(latex),
                escape_text(latex)
            )
        }
        "inline_image" => {
            let asset = span.get("asset").and_then(Value::as_str).unwrap_or("");
            let alt = span.get("alt").and_then(Value::as_str).unwrap_or("");
            log.node_none();
            format!(
                "<img src=\"{}\" alt=\"{}\">",
                escape_attr(&ctx.doc.asset_url(asset)),
                escape_attr(alt)
            )
        }
        "footnote_ref" => {
            let id = span.get("id").and_then(Value::as_str).unwrap_or("");
            log.node_none();
            match ctx.fn_num(id) {
                Some(n) => format!("<sup><a href=\"#fn-{id}\">[{n}]</a></sup>"),
                None => String::new(),
            }
        }
        "unknown" => {
            let payload_ref = span
                .get("payload_ref")
                .and_then(Value::as_str)
                .unwrap_or("");
            if let Some(bytes) = ctx.doc.preserved_bytes(payload_ref) {
                let raw = String::from_utf8_lossy(bytes);
                let (clean, removed) = sanitize_fragment(&raw);
                if removed {
                    log.record(
                        LossClass::Unsupported,
                        "active_content",
                        None,
                        "removed",
                        payload_ref,
                        "preserved 载荷内联前已消毒".to_string(),
                    );
                }
                log.node_none();
                clean
            } else {
                log.record(
                    LossClass::PreservedRaw,
                    "unknown_span_html",
                    None,
                    "preserved",
                    payload_ref,
                    "未知行内内容以占位呈现".to_string(),
                );
                format!(
                    "<span class=\"azodoc-unknown\">[{}]</span>",
                    escape_text(span.get("summary").and_then(Value::as_str).unwrap_or(""))
                )
            }
        }
        "mention" => {
            log.node_none();
            format!(
                "<span class=\"mention\">@{}</span>",
                escape_text(span.get("target").and_then(Value::as_str).unwrap_or(""))
            )
        }
        "cite" => {
            log.node_none();
            format!(
                "<cite>{}</cite>",
                escape_text(span.get("key").and_then(Value::as_str).unwrap_or(""))
            )
        }
        _ => inline_html(span.get("content"), ctx, log),
    }
}

pub fn converter_name() -> &'static str {
    "athanor-html"
}
