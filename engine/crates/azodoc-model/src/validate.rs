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

//! Value 层结构校验（规范级，供 `athanor verify` 使用）。
//!
//! 与类型层的分工：类型层保证「往返无损」（宽容），本模块按
//! spec/azodoc-model.md / spec/azodoc-package.md 做严格检查并产出问题清单。
//! 未知节点类型不判错（R3：包装处理并提示），未知字段不判错（R2）。

use crate::content::{KNOWN_NODE_TYPES, KNOWN_SPAN_TYPES};
use crate::id;
use serde_json::Value;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// verify 失败
    Error,
    /// verify 警告（不影响退出码）
    Warning,
    /// 未知内容盘点等信息
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub level: Level,
    pub code: String,
    pub path: String,
    pub message: String,
}

impl Issue {
    fn error(code: &str, path: impl Into<String>, message: impl Into<String>) -> Self {
        Issue {
            level: Level::Error,
            code: code.to_string(),
            path: path.into(),
            message: message.into(),
        }
    }
    fn info(code: &str, path: impl Into<String>, message: impl Into<String>) -> Self {
        Issue {
            level: Level::Info,
            code: code.to_string(),
            path: path.into(),
            message: message.into(),
        }
    }
    fn warning(code: &str, path: impl Into<String>, message: impl Into<String>) -> Self {
        Issue {
            level: Level::Warning,
            code: code.to_string(),
            path: path.into(),
            message: message.into(),
        }
    }
}

/// 校验上下文：条目存在性回调（由容器层提供，用于 payload_ref / 资产路径检查）。
pub struct Ctx<'a> {
    pub entry_exists: &'a dyn Fn(&str) -> bool,
}

/// 校验 content.json 的 Value 表示。
/// 返回问题清单；资产引用集合一并返回，供调用方对照 assets/registry.json。
pub fn check_content(content_file: &Value, ctx: &Ctx) -> (Vec<Issue>, Vec<(String, String)>) {
    let mut issues = Vec::new();
    let mut asset_refs = Vec::new();

    let Some(arr) = content_file.get("content").and_then(Value::as_array) else {
        issues.push(Issue::error(
            "content.missing",
            "<root>",
            "缺少 content 数组",
        ));
        return (issues, asset_refs);
    };

    // 第一遍：收集脚注定义与脚注引用
    let mut footnote_defs = HashSet::new();
    let mut footnote_refs: Vec<(String, String)> = Vec::new();
    collect_refs(arr, "content", &mut footnote_defs, &mut footnote_refs);

    // 第二遍：结构检查（同时收集资产引用）
    for (i, node) in arr.iter().enumerate() {
        check_node(
            node,
            &format!("content[{i}]"),
            ctx,
            &footnote_defs,
            &mut issues,
            &mut asset_refs,
        );
    }
    for (path, target) in &footnote_refs {
        if !footnote_defs.contains(target) {
            issues.push(Issue::error(
                "footnote_ref.dangling",
                path.clone(),
                format!("footnote_ref 指向不存在的脚注块: {target}"),
            ));
        }
    }
    (issues, asset_refs)
}

fn type_of(node: &Value) -> Option<&str> {
    node.get("type").and_then(Value::as_str)
}

fn id_of(node: &Value) -> Option<&str> {
    node.get("id").and_then(Value::as_str)
}

fn collect_refs(
    nodes: &[Value],
    base: &str,
    footnote_defs: &mut HashSet<String>,
    footnote_refs: &mut Vec<(String, String)>,
) {
    for (i, node) in nodes.iter().enumerate() {
        let path = format!("{base}[{i}]");
        match type_of(node) {
            Some("footnote") => {
                if let Some(id) = id_of(node) {
                    footnote_defs.insert(id.to_string());
                }
            }
            Some("footnote_ref") => {
                if let Some(id) = node.get("id").and_then(Value::as_str) {
                    footnote_refs.push((format!("{path}.id"), id.to_string()));
                }
            }
            _ => {}
        }
        // 递归所有可能的子节点容器
        for key in ["children", "items"] {
            if let Some(children) = node.get(key).and_then(Value::as_array) {
                collect_refs(
                    children,
                    &format!("{path}.{key}"),
                    footnote_defs,
                    footnote_refs,
                );
            }
        }
        if let Some(cells) = node.get("cells").and_then(Value::as_array) {
            collect_refs(
                cells,
                &format!("{path}.cells"),
                footnote_defs,
                footnote_refs,
            );
        }
        if let Some(content) = node.get("content").and_then(Value::as_array) {
            collect_span_refs(content, &format!("{path}.content"), footnote_refs);
        }
        if let Some(caption) = node.get("caption").and_then(Value::as_array) {
            collect_span_refs(caption, &format!("{path}.caption"), footnote_refs);
        }
    }
}

fn collect_span_refs(spans: &[Value], base: &str, footnote_refs: &mut Vec<(String, String)>) {
    for (i, span) in spans.iter().enumerate() {
        let path = format!("{base}[{i}]");
        if type_of(span) == Some("footnote_ref") {
            if let Some(id) = span.get("id").and_then(Value::as_str) {
                footnote_refs.push((format!("{path}.id"), id.to_string()));
            }
        }
        if let Some(content) = span.get("content").and_then(Value::as_array) {
            collect_span_refs(content, &format!("{path}.content"), footnote_refs);
        }
    }
}

fn check_node(
    node: &Value,
    path: &str,
    ctx: &Ctx,
    footnote_defs: &HashSet<String>,
    issues: &mut Vec<Issue>,
    asset_refs: &mut Vec<(String, String)>,
) {
    let Some(obj) = node.as_object() else {
        issues.push(Issue::error("node.not_object", path, "节点必须是对象"));
        return;
    };
    let Some(type_name) = type_of(node) else {
        issues.push(Issue::error(
            "node.missing_type",
            path,
            "节点缺少 type 字段",
        ));
        return;
    };
    if !KNOWN_NODE_TYPES.contains(&type_name) {
        issues.push(Issue::info(
            "node.unknown_type",
            path,
            format!("未知节点类型 `{type_name}`（按 R3 处理，原样保留）"),
        ));
    }

    // id 检查
    let want_prefix: Option<&str> = match type_name {
        "section" => Some("sec"),
        "list_item" => Some("li"),
        t if KNOWN_NODE_TYPES.contains(&t) => Some("blk"),
        _ => None,
    };
    match id_of(node) {
        None => issues.push(Issue::error("node.missing_id", path, "节点缺少 id")),
        Some(id_str) => match id::split_id(id_str) {
            Ok((kind, _)) => {
                if let Some(want) = want_prefix {
                    if kind.prefix() != want {
                        issues.push(Issue::error(
                            "id.wrong_prefix",
                            format!("{path}.id"),
                            format!("类型 `{type_name}` 的 id 前缀应为 {want}_，实际为 `{id_str}`"),
                        ));
                    }
                }
            }
            Err(e) => {
                issues.push(Issue::error(
                    "id.invalid",
                    format!("{path}.id"),
                    e.to_string(),
                ));
            }
        },
    }

    // 类型专属检查
    match type_name {
        "heading" => match node.get("level").and_then(Value::as_i64) {
            Some(l) if (1..=6).contains(&l) => {}
            _ => issues.push(Issue::error(
                "heading.level",
                path,
                "heading.level 必须是 1–6 的整数",
            )),
        },
        "list" => {
            match node.get("style").and_then(Value::as_str) {
                Some(s) if s == "ordered" || s == "bullet" => {}
                _ => issues.push(Issue::error(
                    "list.style",
                    path,
                    "list.style 必须是 ordered|bullet",
                )),
            }
            if let Some(start) = node.get("start") {
                if start.as_i64().map(|s| s < 1).unwrap_or(true) {
                    issues.push(Issue::error("list.start", path, "list.start 必须 ≥ 1"));
                }
            }
        }
        "code_block" => {
            if node.get("text").and_then(Value::as_str).is_none() {
                issues.push(Issue::error(
                    "code_block.text",
                    path,
                    "code_block 缺少 text",
                ));
            }
        }
        "table" => {
            let col_count = node
                .get("columns")
                .and_then(Value::as_array)
                .map(|a| a.len());
            if col_count.is_none() {
                issues.push(Issue::error("table.columns", path, "table 缺少 columns"));
            }
            if node.get("rows").and_then(Value::as_array).is_none() {
                issues.push(Issue::error("table.rows", path, "table 缺少 rows"));
            }
            check_table_grid(node, path, col_count, issues);
        }
        "callout" => match node.get("variant").and_then(Value::as_str) {
            Some("note" | "tip" | "warning" | "important") => {}
            _ => issues.push(Issue::error(
                "callout.variant",
                path,
                "callout.variant 必须是 note|tip|warning|important",
            )),
        },
        "math_block" => {
            if node.get("latex").and_then(Value::as_str).is_none() {
                issues.push(Issue::error(
                    "math_block.latex",
                    path,
                    "math_block 缺少 latex",
                ));
            }
        }
        "unknown" => check_unknown_fields(node, path, ctx, issues),
        _ => {}
    }

    // 块级资产引用（figure / image / embed）
    if let Some(asset) = obj.get("asset").and_then(Value::as_str) {
        asset_refs.push((path.to_string(), asset.to_string()));
    }

    // 格式扩展（x-athanor-format）：受控键值校验，未知键忽略
    check_format_extension(node, path, issues);

    // 递归子节点
    for key in ["children", "items"] {
        if let Some(children) = obj.get(key).and_then(Value::as_array) {
            for (i, child) in children.iter().enumerate() {
                check_node(
                    child,
                    &format!("{path}.{key}[{i}]"),
                    ctx,
                    footnote_defs,
                    issues,
                    asset_refs,
                );
            }
        }
    }
    if let Some(rows) = obj.get("rows").and_then(Value::as_array) {
        for (ri, row) in rows.iter().enumerate() {
            let row_path = format!("{path}.rows[{ri}]");
            if id_of(row).is_none() {
                issues.push(Issue::error("node.missing_id", &row_path, "表格行缺少 id"));
            } else if !id::is_valid_id(id_of(row).unwrap_or("")) {
                issues.push(Issue::error(
                    "id.invalid",
                    format!("{row_path}.id"),
                    "行 id 非法",
                ));
            }
            if let Some(cells) = row.get("cells").and_then(Value::as_array) {
                for (ci, cell) in cells.iter().enumerate() {
                    let cell_path = format!("{row_path}.cells[{ci}]");
                    if id_of(cell).is_none() {
                        issues.push(Issue::error("node.missing_id", &cell_path, "单元格缺少 id"));
                    } else if !id::is_valid_id(id_of(cell).unwrap_or("")) {
                        issues.push(Issue::error(
                            "id.invalid",
                            format!("{cell_path}.id"),
                            "单元格 id 非法",
                        ));
                    }
                    if cell.get("column").and_then(Value::as_i64).is_none() {
                        issues.push(Issue::error(
                            "cell.column",
                            &cell_path,
                            "单元格缺少 column 下标",
                        ));
                    }
                    if let Some(children) = cell.get("children").and_then(Value::as_array) {
                        for (i, child) in children.iter().enumerate() {
                            check_node(
                                child,
                                &format!("{cell_path}.children[{i}]"),
                                ctx,
                                footnote_defs,
                                issues,
                                asset_refs,
                            );
                        }
                    }
                }
            }
        }
    }
    if let Some(items) = obj.get("items").and_then(Value::as_array) {
        for (i, item) in items.iter().enumerate() {
            let item_path = format!("{path}.items[{i}]");
            if type_of(item) == Some("list_item") {
                check_node(item, &item_path, ctx, footnote_defs, issues, asset_refs);
            }
        }
    }

    // 行内 content / caption
    for key in ["content", "caption"] {
        if let Some(spans) = obj.get(key).and_then(Value::as_array) {
            check_spans(
                spans,
                &format!("{path}.{key}"),
                ctx,
                footnote_defs,
                issues,
                asset_refs,
            );
        }
    }
}

fn check_spans(
    spans: &[Value],
    base: &str,
    ctx: &Ctx,
    footnote_defs: &HashSet<String>,
    issues: &mut Vec<Issue>,
    asset_refs: &mut Vec<(String, String)>,
) {
    for (i, span) in spans.iter().enumerate() {
        let path = format!("{base}[{i}]");
        let Some(type_name) = type_of(span) else {
            issues.push(Issue::error("span.missing_type", path, "span 缺少 type"));
            continue;
        };
        if !KNOWN_SPAN_TYPES.contains(&type_name) {
            issues.push(Issue::info(
                "span.unknown_type",
                &path,
                format!("未知行内类型 `{type_name}`（按 R3 处理，原样保留）"),
            ));
        }
        match type_name {
            "text" | "code" => {
                if span.get("text").and_then(Value::as_str).is_none() {
                    issues.push(Issue::error(
                        "span.text",
                        &path,
                        format!("{type_name} 缺少 text"),
                    ));
                }
            }
            "link" => {
                if span.get("url").and_then(Value::as_str).is_none() {
                    issues.push(Issue::error("link.url", &path, "link 缺少 url"));
                }
            }
            "footnote_ref" => {
                let Some(target) = span.get("id").and_then(Value::as_str) else {
                    issues.push(Issue::error(
                        "footnote_ref.id",
                        &path,
                        "footnote_ref 缺少 id",
                    ));
                    continue;
                };
                if !footnote_defs.contains(target) {
                    issues.push(Issue::error(
                        "footnote_ref.dangling",
                        path.clone(),
                        format!("footnote_ref 指向不存在的脚注块: {target}"),
                    ));
                }
            }
            "inline_math" => {
                if span.get("latex").and_then(Value::as_str).is_none() {
                    issues.push(Issue::error(
                        "inline_math.latex",
                        &path,
                        "inline_math 缺少 latex",
                    ));
                }
            }
            "unknown" => check_unknown_fields(span, &path, ctx, issues),
            _ => {}
        }
        if let Some(asset) = span.get("asset").and_then(Value::as_str) {
            asset_refs.push((path.clone(), asset.to_string()));
        }
        check_format_extension(span, &path, issues);
        if let Some(content) = span.get("content").and_then(Value::as_array) {
            check_spans(
                content,
                &format!("{path}.content"),
                ctx,
                footnote_defs,
                issues,
                asset_refs,
            );
        }
    }
}

/// 表格网格诊断（azodoc-model.md §6.5 不规则表规则）：
/// 按 column/colSpan/rowSpan 重建占位矩阵，重叠或越界输出 Warning
/// （table.irregular，不拒绝保存/打开；修复只在用户显式操作下发生）。
fn check_table_grid(node: &Value, path: &str, col_count: Option<usize>, issues: &mut Vec<Issue>) {
    let Some(rows) = node.get("rows").and_then(Value::as_array) else {
        return;
    };
    let mut occupied: HashSet<(u64, u64)> = HashSet::new();
    for (ri, row) in rows.iter().enumerate() {
        let Some(cells) = row.get("cells").and_then(Value::as_array) else {
            continue;
        };
        for (ci, cell) in cells.iter().enumerate() {
            let Some(col) = cell.get("column").and_then(Value::as_u64) else {
                continue; // 缺 column 已由 cell.column 错误覆盖
            };
            let cspan = cell
                .get("colSpan")
                .and_then(Value::as_u64)
                .unwrap_or(1)
                .max(1);
            let rspan = cell
                .get("rowSpan")
                .and_then(Value::as_u64)
                .unwrap_or(1)
                .max(1);
            let cell_path = format!("{path}.rows[{ri}].cells[{ci}]");

            if let Some(ncols) = col_count {
                if col.saturating_add(cspan) > ncols as u64 {
                    issues.push(Issue::warning(
                        "table.irregular",
                        cell_path.clone(),
                        format!(
                            "单元格 column({col}) + colSpan({cspan}) 超出声明列数({ncols})——不规则表，仅诊断不自动重写"
                        ),
                    ));
                }
            }
            if (ri as u64).saturating_add(rspan) > rows.len() as u64 {
                issues.push(Issue::warning(
                    "table.irregular",
                    cell_path.clone(),
                    format!(
                        "单元格 rowSpan({rspan}) 超出剩余行数({})——不规则表，仅诊断不自动重写",
                        rows.len() as u64 - ri as u64
                    ),
                ));
            }
            let mut overlap = false;
            for dr in 0..rspan.min(64) {
                for dc in 0..cspan.min(64) {
                    let pos = (ri as u64 + dr, col + dc);
                    if !occupied.insert(pos) {
                        overlap = true;
                    }
                }
            }
            if overlap {
                issues.push(Issue::warning(
                    "table.irregular",
                    cell_path,
                    "单元格与既有占位重叠——不规则表，仅诊断不自动重写".to_string(),
                ));
            }
        }
    }
}

/// x-athanor-format 扩展校验：只校验受控键的取值域；未知键/其他 extra 原样放行。
fn check_format_extension(node: &Value, path: &str, issues: &mut Vec<Issue>) {
    let Some(fmt) = node
        .as_object()
        .and_then(|o| o.get("x-athanor-format"))
        .and_then(Value::as_object)
    else {
        return;
    };
    let fpath = format!("{path}.x-athanor-format");

    if let Some(para) = fmt.get("paragraph").and_then(Value::as_object) {
        if let Some(align) = para.get("align").and_then(Value::as_str) {
            if !matches!(align, "left" | "center" | "right" | "justify") {
                issues.push(Issue::error(
                    "format.align",
                    format!("{fpath}.paragraph.align"),
                    format!("align 必须是 left|center|right|justify，实际为 `{align}`"),
                ));
            }
        }
        for key in [
            "indentStartPt",
            "indentEndPt",
            "firstLinePt",
            "spaceBeforePt",
            "spaceAfterPt",
        ] {
            if let Some(v) = para.get(key) {
                match v.as_f64() {
                    Some(n) if (0.0..=999.0).contains(&n) => {}
                    _ => issues.push(Issue::error(
                        "format.range",
                        format!("{fpath}.paragraph.{key}"),
                        format!("{key} 必须是 0–999 的数值（pt）"),
                    )),
                }
            }
        }
        if let Some(v) = para.get("lineHeight") {
            match v.as_f64() {
                Some(n) if (0.5..=5.0).contains(&n) => {}
                _ => issues.push(Issue::error(
                    "format.lineHeight",
                    format!("{fpath}.paragraph.lineHeight"),
                    "lineHeight 必须是 0.5–5.0 的倍数",
                )),
            }
        }
    }

    if let Some(ch) = fmt.get("character").and_then(Value::as_object) {
        if let Some(v) = ch.get("fontSizePt") {
            match v.as_f64() {
                Some(n) if (1.0..=999.0).contains(&n) => {}
                _ => issues.push(Issue::error(
                    "format.fontSizePt",
                    format!("{fpath}.character.fontSizePt"),
                    "fontSizePt 必须是 1–999 的数值",
                )),
            }
        }
        for key in ["color", "highlight"] {
            if let Some(s) = ch.get(key).and_then(Value::as_str) {
                let ok = s.len() == 7
                    && s.starts_with('#')
                    && s[1..].bytes().all(|b| b.is_ascii_hexdigit());
                if !ok {
                    issues.push(Issue::error(
                        "format.color",
                        format!("{fpath}.character.{key}"),
                        format!("{key} 必须是 #RRGGBB 形式，实际为 `{s}`"),
                    ));
                }
            }
        }
        if let Some(fam) = ch.get("fontFamily").and_then(Value::as_str) {
            if fam.is_empty()
                || fam.len() > 200
                || fam
                    .bytes()
                    .any(|b| matches!(b, b'\'' | b'"' | b';' | b'{' | b'}' | b'<' | b'>'))
            {
                issues.push(Issue::error(
                    "format.fontFamily",
                    format!("{fpath}.character.fontFamily"),
                    "fontFamily 含非法字符或超长",
                ));
            }
        }
        if let Some(va) = ch.get("verticalAlign").and_then(Value::as_str) {
            if !matches!(va, "sub" | "super") {
                issues.push(Issue::error(
                    "format.verticalAlign",
                    format!("{fpath}.character.verticalAlign"),
                    "verticalAlign 必须是 sub|super",
                ));
            }
        }
    }
}

fn check_unknown_fields(node: &Value, path: &str, ctx: &Ctx, issues: &mut Vec<Issue>) {
    let loss_class = node.get("loss_class").and_then(Value::as_str);
    match loss_class {
        Some("preserved_raw") => match node.get("payload_ref").and_then(Value::as_str) {
            Some(p) if !p.is_empty() => {
                if !(ctx.entry_exists)(p) {
                    issues.push(Issue::error(
                        "unknown.payload_missing",
                        path,
                        format!("payload_ref 指向不存在的条目: {p}"),
                    ));
                }
            }
            _ => issues.push(Issue::error(
                "unknown.payload_required",
                path,
                "loss_class=preserved_raw 必须携带 payload_ref",
            )),
        },
        Some("unsupported") => {}
        _ => issues.push(Issue::error(
            "unknown.loss_class",
            path,
            "unknown 节点的 loss_class 必须是 preserved_raw|unsupported",
        )),
    }
    if node
        .get("summary")
        .and_then(Value::as_str)
        .map(str::is_empty)
        .unwrap_or(true)
    {
        issues.push(Issue::error(
            "unknown.summary",
            path,
            "unknown 节点缺少非空 summary",
        ));
    }
}
