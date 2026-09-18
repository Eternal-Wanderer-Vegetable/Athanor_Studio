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

//! load 方向：Prima content.json → ProseMirror 文档 JSON。
//!
//! 规范形约定（与 [`crate::to_prima`] 对偶）：
//! - attrs 全量显式（含默认值），键序 = [`crate::mapping`] 声明序；
//! - marks 数组按 (rank, name, attrs) 排序并去重（PM 的 marks 是集合）；
//! - 相邻同 marks 的文本合并为单个 text 节点；
//! - 行内嵌套 span 展开为 marks；原子节点继承祖先 marks（忠实往返
//!   `Strong[InlineMath]` 类结构）；`Text.extra` 非空时落 `span_extra` 隐形标记。

use serde_json::{json, Map, Value};

use crate::mapping::{self, pm_attr_name, AttrDef, AttrSpec, MarkDef, NodeDef};
use crate::PmError;

fn bad(msg: impl std::fmt::Display) -> PmError {
    PmError::BadStructure(msg.to_string())
}

fn as_array_or_empty(v: Option<&Value>) -> &[Value] {
    static EMPTY: &[Value] = &[];
    v.and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(EMPTY)
}

/// Prima 字段值 → PM attr 值（缺省/类型不符时落规范默认）。
fn field_to_attr(v: Option<&Value>, spec: AttrSpec) -> Value {
    match spec {
        AttrSpec::Text(d) => json!(v.and_then(Value::as_str).unwrap_or(d)),
        AttrSpec::Int(d) => json!(v.and_then(Value::as_i64).unwrap_or(d)),
        AttrSpec::OptText => v
            .and_then(Value::as_str)
            .map(|s| Value::String(s.to_string()))
            .unwrap_or(Value::Null),
        AttrSpec::OptInt => v
            .and_then(Value::as_i64)
            .map(|i| json!(i))
            .unwrap_or(Value::Null),
        // 跨度：缺失即 1（与 schema default 对齐；无效值回退 1）
        AttrSpec::Span => json!(v.and_then(Value::as_i64).unwrap_or(1).max(1)),
        AttrSpec::OptBool => v
            .and_then(Value::as_bool)
            .map(Value::Bool)
            .unwrap_or(Value::Null),
        AttrSpec::OptJson => match v {
            Some(x) if !x.is_null() => x.clone(),
            _ => Value::Null,
        },
    }
}

/// 源对象中不属于已声明字段的部分（`#[serde(flatten)] extra` 的逆运算）。
fn extra_of(src: &Map<String, Value>, known: &[&str]) -> Map<String, Value> {
    src.iter()
        .filter(|(k, _)| !known.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// 已声明字段清单（extra 收集用）：全部非 extra attr + prima_fields；
/// 仅**有标签**的类型（prima_type = Some）额外排除 `type`——无标签结构
/// （list_item/table_row/table_cell）输入中的 `type` 是普通未知字段，必须保真。
fn known_keys(
    attr_defs: &[AttrDef],
    prima_fields: &[&'static str],
    tagged: bool,
    out: &mut Vec<&str>,
) {
    if tagged {
        out.push("type");
    }
    for a in attr_defs {
        if a.name != "extra" {
            out.push(a.name);
        }
    }
    out.extend_from_slice(prima_fields);
}

fn node_def_of(prima_type: &str) -> Result<&'static NodeDef, PmError> {
    mapping::NODES
        .iter()
        .find(|d| d.prima_type == Some(prima_type))
        .ok_or_else(|| {
            bad(format!(
                "未知的 Prima 节点类型 `{prima_type}`（读取方应先经 validate 盘点）"
            ))
        })
}

fn mark_def_of(prima_type: &str) -> Result<&'static MarkDef, PmError> {
    mapping::MARKS
        .iter()
        .find(|d| d.prima_type == Some(prima_type))
        .ok_or_else(|| bad(format!("未知的 Prima span 类型 `{prima_type}`")))
}

/// 组装 PM 节点 JSON（键序 type → attrs → content → text → marks，同 PM toJSON）。
fn pm_node(
    pm_name: &str,
    attrs: Map<String, Value>,
    content: Vec<Value>,
    text: Option<String>,
    marks: Vec<Value>,
) -> Value {
    let mut m = Map::new();
    m.insert("type".into(), json!(pm_name));
    if !attrs.is_empty() {
        m.insert("attrs".into(), Value::Object(attrs));
    }
    if !content.is_empty() {
        m.insert("content".into(), Value::Array(content));
    }
    if let Some(t) = text {
        m.insert("text".into(), json!(t));
    }
    if !marks.is_empty() {
        m.insert("marks".into(), Value::Array(marks));
    }
    Value::Object(m)
}

// ---------------------------------------------------------------- 块

/// content.json 全文件 → PM doc 节点。
pub fn content_file_to_pm(file: &Value) -> Result<Value, PmError> {
    let obj = file
        .as_object()
        .ok_or_else(|| bad("content.json 顶层必须是对象"))?;
    let content = as_array_or_empty(obj.get("content"));
    let mut blocks = Vec::with_capacity(content.len());
    for n in content {
        blocks.push(block_to_pm(n)?);
    }
    let schema_version = obj
        .get("schema_version")
        .and_then(Value::as_str)
        .unwrap_or("1.0");
    let extra = extra_of(obj, &["schema_version", "content"]);
    let mut attrs = Map::new();
    attrs.insert("schema_version".into(), json!(schema_version));
    attrs.insert(
        "extra".into(),
        if extra.is_empty() {
            Value::Null
        } else {
            Value::Object(extra)
        },
    );
    Ok(pm_node("doc", attrs, blocks, None, Vec::new()))
}

fn block_to_pm(n: &Value) -> Result<Value, PmError> {
    let obj = n.as_object().ok_or_else(|| bad("块节点必须是对象"))?;
    let t = obj.get("type").and_then(Value::as_str).unwrap_or("");
    match t {
        "section" | "quote" | "callout" | "footnote" => container_to_pm(obj, t),
        "paragraph" | "heading" => inline_block_to_pm(obj, t),
        "list" => list_to_pm(obj),
        "code_block" => code_block_to_pm(obj),
        "table" => table_to_pm(obj),
        "figure" | "image" | "horizontal_rule" | "page_break" | "math_block" | "embed" => {
            atom_block_to_pm(obj, t)
        }
        "unknown" => unknown_block_to_pm(obj),
        other => Err(bad(format!(
            "未知的 Prima 节点类型 `{other}`（读取方应先经 validate 盘点）"
        ))),
    }
}

fn block_attrs(def: &NodeDef, obj: &Map<String, Value>) -> Map<String, Value> {
    let mut known = Vec::new();
    known_keys(
        def.attrs,
        def.prima_fields,
        def.prima_type.is_some(),
        &mut known,
    );
    let mut attrs = Map::new();
    for a in def.attrs {
        if a.name == "extra" {
            continue;
        }
        // PM 键名（colSpan→colspan 等）；known/extra 仍按 Prima 名计算
        attrs.insert(
            pm_attr_name(a.name).to_string(),
            field_to_attr(obj.get(a.name), a.spec),
        );
    }
    let extra = extra_of(obj, &known);
    attrs.insert(
        "extra".into(),
        if extra.is_empty() {
            Value::Null
        } else {
            Value::Object(extra)
        },
    );
    attrs
}

fn container_to_pm(obj: &Map<String, Value>, t: &str) -> Result<Value, PmError> {
    let def = node_def_of(t)?;
    let children = as_array_or_empty(obj.get("children"));
    let mut content = Vec::with_capacity(children.len());
    for c in children {
        content.push(block_to_pm(c)?);
    }
    Ok(pm_node(
        def.pm_name,
        block_attrs(def, obj),
        content,
        None,
        Vec::new(),
    ))
}

fn inline_block_to_pm(obj: &Map<String, Value>, t: &str) -> Result<Value, PmError> {
    let def = node_def_of(t)?;
    let content = spans_to_pm(as_array_or_empty(obj.get("content")))?;
    Ok(pm_node(
        def.pm_name,
        block_attrs(def, obj),
        content,
        None,
        Vec::new(),
    ))
}

fn atom_block_to_pm(obj: &Map<String, Value>, t: &str) -> Result<Value, PmError> {
    let def = node_def_of(t)?;
    Ok(pm_node(
        def.pm_name,
        block_attrs(def, obj),
        Vec::new(),
        None,
        Vec::new(),
    ))
}

fn unknown_block_to_pm(obj: &Map<String, Value>) -> Result<Value, PmError> {
    let def = node_def_of("unknown")?;
    let content = spans_to_pm(as_array_or_empty(obj.get("content")))?;
    Ok(pm_node(
        def.pm_name,
        block_attrs(def, obj),
        content,
        None,
        Vec::new(),
    ))
}

fn list_to_pm(obj: &Map<String, Value>) -> Result<Value, PmError> {
    let def = node_def_of("list")?;
    let item_def = mapping::node_by_pm("list_item").expect("list_item 定义存在");
    let mut content = Vec::new();
    for item in as_array_or_empty(obj.get("items")) {
        let io = item
            .as_object()
            .ok_or_else(|| bad("list.items 成员必须是对象"))?;
        let children = as_array_or_empty(io.get("children"));
        let mut sub = Vec::with_capacity(children.len());
        for c in children {
            sub.push(block_to_pm(c)?);
        }
        content.push(pm_node(
            item_def.pm_name,
            block_attrs(item_def, io),
            sub,
            None,
            Vec::new(),
        ));
    }
    Ok(pm_node(
        def.pm_name,
        block_attrs(def, obj),
        content,
        None,
        Vec::new(),
    ))
}

fn code_block_to_pm(obj: &Map<String, Value>) -> Result<Value, PmError> {
    let def = node_def_of("code_block")?;
    let text = obj.get("text").and_then(Value::as_str).unwrap_or("");
    let content = if text.is_empty() {
        Vec::new()
    } else {
        vec![pm_node(
            "text",
            Map::new(),
            Vec::new(),
            Some(text.to_string()),
            Vec::new(),
        )]
    };
    Ok(pm_node(
        def.pm_name,
        block_attrs(def, obj),
        content,
        None,
        Vec::new(),
    ))
}

fn table_to_pm(obj: &Map<String, Value>) -> Result<Value, PmError> {
    let def = node_def_of("table")?;
    let row_def = mapping::node_by_pm("table_row").expect("table_row 定义存在");
    let cell_def = mapping::node_by_pm("table_cell").expect("table_cell 定义存在");
    let header_def = mapping::node_by_pm("table_header").expect("table_header 定义存在");
    // columns[].width（相对单位）→ 每列 colwidth（PM 像素宽数组的单元素）
    let col_widths: Vec<i64> = as_array_or_empty(obj.get("columns"))
        .iter()
        .map(|c| c.get("width").and_then(Value::as_i64).unwrap_or(0))
        .collect();
    let mut content = Vec::new();
    for row in as_array_or_empty(obj.get("rows")) {
        let ro = row
            .as_object()
            .ok_or_else(|| bad("table.rows 成员必须是对象"))?;
        let mut row_content = Vec::new();
        for cell in as_array_or_empty(ro.get("cells")) {
            let co = cell
                .as_object()
                .ok_or_else(|| bad("table_row.cells 成员必须是对象"))?;
            let children = as_array_or_empty(co.get("children"));
            let mut cell_content = Vec::with_capacity(children.len());
            for c in children {
                cell_content.push(block_to_pm(c)?);
            }
            let is_header = co.get("role").and_then(Value::as_str) == Some("header");
            let cell_def = if is_header { header_def } else { cell_def };
            let mut attrs = block_attrs(cell_def, co);
            // colwidth 分发：按 column + colSpan 取覆盖列的首个宽度
            let col = co.get("column").and_then(Value::as_i64).unwrap_or(0).max(0) as usize;
            let span = co
                .get("colSpan")
                .and_then(Value::as_i64)
                .unwrap_or(1)
                .max(1) as usize;
            let widths: Vec<Value> = (0..span)
                .map(|w| match col_widths.get(col + w).copied().unwrap_or(0) {
                    v if v > 0 => json!(v),
                    _ => Value::Null,
                })
                .collect();
            if widths.iter().any(|v| !v.is_null()) {
                attrs.insert("colwidth".into(), Value::Array(widths));
            }
            row_content.push(pm_node(
                cell_def.pm_name,
                attrs,
                cell_content,
                None,
                Vec::new(),
            ));
        }
        content.push(pm_node(
            row_def.pm_name,
            block_attrs(row_def, ro),
            row_content,
            None,
            Vec::new(),
        ));
    }
    Ok(pm_node(
        def.pm_name,
        block_attrs(def, obj),
        content,
        None,
        Vec::new(),
    ))
}

// ---------------------------------------------------------------- 行内

/// 行内转换的中间产物：文本运行（带 marks）或已是节点的原子。
enum Item {
    Run { text: String, marks: Vec<Mark> },
    Node(Value),
}

/// 已解析的 mark：def 引用 + 全量 attrs（JSON）。
#[derive(Clone)]
struct Mark {
    def: &'static MarkDef,
    json: Value,
}

fn mark_sort_key(m: &Mark) -> (u8, &str, String) {
    (m.def.rank, m.def.pm_name, m.json.to_string())
}

fn sort_marks(marks: &mut Vec<Mark>) {
    marks.sort_by(|a, b| mark_sort_key(a).cmp(&mark_sort_key(b)));
    marks.dedup_by(|a, b| a.json == b.json);
}

fn marks_json(marks: &[Mark]) -> Vec<Value> {
    marks.iter().map(|m| m.json.clone()).collect()
}

/// 组装单个 mark 的 JSON（attrs 全量显式）。
fn mark_json(def: &MarkDef, src: &Map<String, Value>) -> Value {
    let mut known = Vec::new();
    known_keys(def.attrs, def.prima_fields, true, &mut known);
    let mut attrs = Map::new();
    for a in def.attrs {
        if a.name == "extra" {
            continue;
        }
        attrs.insert(a.name.to_string(), field_to_attr(src.get(a.name), a.spec));
    }
    let extra = extra_of(src, &known);
    attrs.insert(
        "extra".into(),
        if extra.is_empty() {
            Value::Null
        } else {
            Value::Object(extra)
        },
    );
    json!({ "type": def.pm_name, "attrs": Value::Object(attrs) })
}

fn inline_node_attrs(def: &NodeDef, obj: &Map<String, Value>) -> Map<String, Value> {
    block_attrs(def, obj)
}

/// Prima span 数组 → PM 行内节点数组（规范形）。
pub fn spans_to_pm(spans: &[Value]) -> Result<Vec<Value>, PmError> {
    fn flush(pending: &mut Option<(Vec<Value>, String)>, out: &mut Vec<Value>) {
        if let Some((marks, text)) = pending.take() {
            out.push(pm_node("text", Map::new(), Vec::new(), Some(text), marks));
        }
    }

    let mut items = Vec::new();
    walk_spans(spans, &mut Vec::new(), &mut items)?;

    let mut out: Vec<Value> = Vec::new();
    // pending = (规范形 marks JSON, 累积文本)；marks 相同的相邻运行合并
    let mut pending: Option<(Vec<Value>, String)> = None;
    for item in items {
        match item {
            Item::Node(v) => {
                flush(&mut pending, &mut out);
                out.push(v);
            }
            Item::Run { text, mut marks } => {
                sort_marks(&mut marks);
                let marks = marks_json(&marks);
                match &mut pending {
                    Some((pm, buf)) if *pm == marks => buf.push_str(&text),
                    _ => {
                        flush(&mut pending, &mut out);
                        pending = Some((marks, text));
                    }
                }
            }
        }
    }
    flush(&mut pending, &mut out);
    Ok(out)
}

/// 递归展开嵌套 span：容器 span 压栈为 mark，叶与原子直接产出。
fn walk_spans(spans: &[Value], acc: &mut Vec<Mark>, out: &mut Vec<Item>) -> Result<(), PmError> {
    for span in spans {
        let obj = span.as_object().ok_or_else(|| bad("span 必须是对象"))?;
        let t = obj.get("type").and_then(Value::as_str).unwrap_or("");
        match t {
            "text" => {
                let text = obj
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let mut marks = acc.clone();
                let extra = extra_of(obj, &["type", "text"]);
                if !extra.is_empty() {
                    let def = mapping::mark_by_pm("span_extra").expect("span_extra 定义存在");
                    marks.push(Mark {
                        def,
                        json: json!({ "type": "span_extra", "attrs": { "data": Value::Object(extra) } }),
                    });
                }
                out.push(Item::Run { text, marks });
            }
            "strong" | "em" | "underline" | "strike" | "link" => {
                let def = mark_def_of(t)?;
                acc.push(Mark {
                    def,
                    json: mark_json(def, obj),
                });
                let res = walk_spans(as_array_or_empty(obj.get("content")), acc, out);
                acc.pop();
                res?;
            }
            "code" => {
                let def = mark_def_of(t)?;
                let text = obj
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let mut marks = acc.clone();
                marks.push(Mark {
                    def,
                    json: mark_json(def, obj),
                });
                out.push(Item::Run { text, marks });
            }
            "hard_break" | "footnote_ref" | "inline_math" | "inline_image" | "mention" | "cite" => {
                let def = node_def_of(t)?;
                let mut marks = acc.clone();
                sort_marks(&mut marks);
                out.push(Item::Node(pm_node(
                    def.pm_name,
                    inline_node_attrs(def, obj),
                    Vec::new(),
                    None,
                    marks_json(&marks),
                )));
            }
            "unknown" => {
                // unknown span：节点本身继承祖先 marks；其 content 属于 unknown 自己的
                // 上下文，以空 acc 递归，避免双重施加。
                // 注意按 PM 名显式取 def——`node_def_of("unknown")` 会命中先注册的块定义。
                let def = mapping::node_by_pm("unknown_span").expect("unknown_span 定义存在");
                let inner = spans_to_pm(as_array_or_empty(obj.get("content")))?;
                let mut marks = acc.clone();
                sort_marks(&mut marks);
                out.push(Item::Node(pm_node(
                    def.pm_name,
                    inline_node_attrs(def, obj),
                    inner,
                    None,
                    marks_json(&marks),
                )));
            }
            other => return Err(bad(format!("未知的 Prima span 类型 `{other}`"))),
        }
    }
    Ok(())
}
