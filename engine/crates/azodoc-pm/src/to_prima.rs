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

//! save 方向：ProseMirror 文档 JSON → Prima content.json。
//!
//! - **ID 稳定规则**：节点的 `id` attr 缺失/空/非法（非 `<前缀>_<26 位 Crockford>`）
//!   或与文档中已有 ID 重复时，经注入的 `new_id` 闭包按 [`IdKind`] 补发
//!   （section=sec、list_item=li、table_row=row、table_cell=cel、其余块=blk）；
//!   `footnote_ref.id` 是引用，不参与补发。
//! - **嵌套重建**：marks 按 rank 升序为从外到内；`code` 是 text 叶（施加于非文本
//!   节点报错）；`span_extra` 是 Text.extra 的隐形载体，不参与嵌套。
//! - 产出的 content.json 字段序 = Prima 声明序（type → id → 声明字段 → extra 展开），
//!   结构性字段（content/children/items/rows/cells/text）恒存在（可为空数组），
//!   可选字段为 null 时整体省略——与 serde 序列化形态一致。

use std::collections::HashSet;

use azodoc_model::id::{is_valid_id, IdKind};
use serde_json::{json, Map, Value};

use crate::mapping::{self, pm_attr_name, AttrDef, AttrSpec, MarkDef};
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

fn as_obj_or_empty(v: Option<&Value>) -> &Map<String, Value> {
    static EMPTY: std::sync::LazyLock<Map<String, Value>> = std::sync::LazyLock::new(Map::new);
    v.and_then(Value::as_object).unwrap_or(&EMPTY)
}

/// save 转换结果。
#[derive(Debug, Clone)]
pub struct ToPrimaResult {
    /// content.json 完整 Value（schema_version + content + 文档级 extra 展开）
    pub content_file: Value,
    /// 补发的新 ID 总数（缺失/非法 + 重复）
    pub ids_assigned: u64,
    /// 其中因重复而补发的数量
    pub ids_deduplicated: u64,
}

struct Ctx<'a> {
    new_id: &'a mut dyn FnMut(IdKind) -> String,
    seen: HashSet<String>,
    assigned: u64,
    deduplicated: u64,
}

impl Ctx<'_> {
    fn resolve_id(&mut self, want: &str, kind: IdKind) -> String {
        if is_valid_id(want) && !self.seen.contains(want) {
            self.seen.insert(want.to_string());
            return want.to_string();
        }
        let duplicate = is_valid_id(want);
        loop {
            let id = (self.new_id)(kind);
            if !self.seen.contains(&id) {
                self.seen.insert(id.clone());
                self.assigned += 1;
                if duplicate {
                    self.deduplicated += 1;
                }
                return id;
            }
        }
    }
}

// ---------------------------------------------------------------- attr 读取

fn read_text(attrs: &Map<String, Value>, name: &str, default: &str) -> String {
    attrs
        .get(name)
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_string()
}

fn read_opt_json(attrs: &Map<String, Value>, name: &str) -> Option<Value> {
    match attrs.get(name) {
        Some(v) if !v.is_null() => Some(v.clone()),
        _ => None,
    }
}

/// PM attrs 取值：PM 名优先，兼容旧快照里的 Prima 名键。
fn attr_of<'a>(attrs: &'a Map<String, Value>, pm: &str, prima: &str) -> Option<&'a Value> {
    attrs.get(pm).or_else(|| attrs.get(prima))
}

/// 按 attr 规格把 PM attr 值写为 Prima 字段（可选规格的 null/缺失 → 整体省略）。
/// PM attr 名经 [`pm_attr_name`]（colSpan→colspan），Prima 键名仍是 `a.name`。
/// 兼容读取：旧快照里 PM 键是 Prima 名（colSpan）时也能取到。
fn push_attr_field(out: &mut Map<String, Value>, a: &AttrDef, attrs: &Map<String, Value>) {
    let v = attr_of(attrs, pm_attr_name(a.name), a.name);
    match a.spec {
        AttrSpec::Text(d) => {
            out.insert(
                a.name.to_string(),
                json!(v.and_then(Value::as_str).unwrap_or(d)),
            );
        }
        AttrSpec::Int(d) => {
            out.insert(
                a.name.to_string(),
                json!(v.and_then(Value::as_i64).unwrap_or(d)),
            );
        }
        AttrSpec::OptText => {
            if let Some(v) = v.and_then(Value::as_str) {
                out.insert(a.name.to_string(), json!(v));
            }
        }
        AttrSpec::OptInt => {
            if let Some(v) = v.and_then(Value::as_i64) {
                out.insert(a.name.to_string(), json!(v));
            }
        }
        // 跨度：1 是默认值不落字段（PM 侧 attr 恒存在且默认 1，
        // 无法区分"显式 1"与"缺失"，规范形统一省略默认）
        AttrSpec::Span => {
            if let Some(v) = v.and_then(Value::as_i64) {
                if v != 1 {
                    out.insert(a.name.to_string(), json!(v));
                }
            }
        }
        AttrSpec::OptBool => {
            if let Some(v) = v.and_then(Value::as_bool) {
                out.insert(a.name.to_string(), json!(v));
            }
        }
        AttrSpec::OptJson => {
            if let Some(v) = v {
                if !v.is_null() {
                    out.insert(a.name.to_string(), v.clone());
                }
            }
        }
    }
}

/// 把 attr `extra`（未知字段袋）平铺展开到节点对象末尾（serde flatten 的逆运算）。
fn spread_extra(out: &mut Map<String, Value>, attrs: &Map<String, Value>) {
    if let Some(extra) = read_opt_json(attrs, "extra") {
        if let Some(map) = extra.as_object() {
            for (k, v) in map {
                out.insert(k.clone(), v.clone());
            }
        }
    }
}

fn new_prima_obj(prima_type: &str, id: &str) -> Map<String, Value> {
    let mut out = Map::new();
    out.insert("type".into(), json!(prima_type));
    out.insert("id".into(), json!(id));
    out
}

/// 把 def 声明的全部 attr 字段按 prima_fields 顺序写入（结构性字段由调用方后置）。
fn push_declared_attr_fields(
    out: &mut Map<String, Value>,
    attrs: &Map<String, Value>,
    prima_fields: &[&'static str],
    attr_defs: &'static [AttrDef],
) {
    for f in prima_fields {
        if let Some(a) = attr_defs.iter().find(|a| a.name == *f) {
            push_attr_field(out, a, attrs);
        }
    }
}

// ---------------------------------------------------------------- marks 解析

/// 已解析的 mark：def + 全量 attrs（缺失补默认值）。
#[derive(Debug, Clone)]
struct ParsedMark {
    def: &'static MarkDef,
    attrs: Map<String, Value>,
}

impl ParsedMark {
    fn canonical(&self) -> String {
        Value::Object(self.attrs.clone()).to_string()
    }

    /// 用该 mark 包裹一个 span（container 类型；code/span_extra 由调用方处理）。
    fn wrap(&self, inner: Value) -> Value {
        let mut w = Map::new();
        w.insert(
            "type".into(),
            json!(self.def.prima_type.expect("容器 mark 必有 Prima 类型")),
        );
        for a in self.def.attrs {
            if a.name == "extra" {
                continue;
            }
            push_attr_field(&mut w, a, &self.attrs);
        }
        w.insert("content".into(), Value::Array(vec![inner]));
        spread_extra(&mut w, &self.attrs);
        Value::Object(w)
    }
}

/// 解析 PM 节点的 marks 数组：排序（rank, name, attrs）、去重。
fn parse_marks(node: &Map<String, Value>) -> Result<Vec<ParsedMark>, PmError> {
    let mut out: Vec<ParsedMark> = Vec::new();
    for m in as_array_or_empty(node.get("marks")) {
        let mo = m.as_object().ok_or_else(|| bad("marks 成员必须是对象"))?;
        let t = mo.get("type").and_then(Value::as_str).unwrap_or("");
        let def = mapping::mark_by_pm(t).ok_or_else(|| PmError::UnknownPmMark(t.to_string()))?;
        let raw = as_obj_or_empty(mo.get("attrs"));
        let mut attrs = Map::new();
        for a in def.attrs {
            let get = raw.get(pm_attr_name(a.name)).or_else(|| raw.get(a.name));
            match a.spec {
                AttrSpec::Text(d) => {
                    attrs.insert(
                        a.name.to_string(),
                        json!(get.and_then(Value::as_str).unwrap_or(d)),
                    );
                }
                AttrSpec::Int(d) => {
                    attrs.insert(
                        a.name.to_string(),
                        json!(get.and_then(Value::as_i64).unwrap_or(d)),
                    );
                }
                AttrSpec::OptText => {
                    attrs.insert(
                        a.name.to_string(),
                        get.and_then(Value::as_str)
                            .map(|s| Value::String(s.to_string()))
                            .unwrap_or(Value::Null),
                    );
                }
                AttrSpec::OptInt => {
                    attrs.insert(
                        a.name.to_string(),
                        get.and_then(Value::as_i64)
                            .map(|i| json!(i))
                            .unwrap_or(Value::Null),
                    );
                }
                AttrSpec::Span => {
                    // marks 侧无 Span 用法；保持与 OptInt 相同的空值语义
                    attrs.insert(
                        a.name.to_string(),
                        get.and_then(Value::as_i64)
                            .map(|i| json!(i))
                            .unwrap_or(Value::Null),
                    );
                }
                AttrSpec::OptBool => {
                    attrs.insert(
                        a.name.to_string(),
                        get.and_then(Value::as_bool)
                            .map(Value::Bool)
                            .unwrap_or(Value::Null),
                    );
                }
                AttrSpec::OptJson => {
                    attrs.insert(
                        a.name.to_string(),
                        match get {
                            Some(v) if !v.is_null() => v.clone(),
                            _ => Value::Null,
                        },
                    );
                }
            }
        }
        out.push(ParsedMark { def, attrs });
    }
    out.sort_by(|a, b| {
        (a.def.rank, a.def.pm_name, a.canonical()).cmp(&(b.def.rank, b.def.pm_name, b.canonical()))
    });
    out.dedup_by(|a, b| a.def.pm_name == b.def.pm_name && a.canonical() == b.canonical());
    Ok(out)
}

fn same_marks(a: &[ParsedMark], b: &[ParsedMark]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b.iter())
            .all(|(x, y)| x.def.pm_name == y.def.pm_name && x.canonical() == y.canonical())
}

// ---------------------------------------------------------------- 顶层

/// PM doc 节点 → content.json。
///
/// `new_id` 在需要补发 ID 时被调用（M6.2 服务器注入 `AzodocId::generate`；
/// 测试注入确定性生成器以断言"合法 ID 不被触碰"）。
pub fn pm_to_content_file(
    doc: &Value,
    new_id: &mut dyn FnMut(IdKind) -> String,
) -> Result<ToPrimaResult, PmError> {
    let obj = doc
        .as_object()
        .ok_or_else(|| bad("PM 文档顶层必须是对象"))?;
    if obj.get("type").and_then(Value::as_str) != Some("doc") {
        return Err(bad("PM 文档顶层节点必须是 `doc`"));
    }
    let attrs = as_obj_or_empty(obj.get("attrs"));
    let mut ctx = Ctx {
        new_id,
        seen: HashSet::new(),
        assigned: 0,
        deduplicated: 0,
    };

    let mut content = Vec::new();
    for b in as_array_or_empty(obj.get("content")) {
        content.push(build_block(b, &mut ctx)?);
    }

    let mut file = Map::new();
    file.insert(
        "schema_version".into(),
        json!(read_text(attrs, "schema_version", "1.0")),
    );
    file.insert("content".into(), Value::Array(content));
    if let Some(extra) = read_opt_json(attrs, "extra") {
        if let Some(map) = extra.as_object() {
            for (k, v) in map {
                file.insert(k.clone(), v.clone());
            }
        }
    }
    Ok(ToPrimaResult {
        content_file: Value::Object(file),
        ids_assigned: ctx.assigned,
        ids_deduplicated: ctx.deduplicated,
    })
}

// ---------------------------------------------------------------- 块

fn def_of_block(
    node: &Map<String, Value>,
) -> Result<(&str, &'static mapping::NodeDef, Map<String, Value>), PmError> {
    let pm_name = node
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| bad("PM 节点缺少 type"))?;
    let def =
        mapping::node_by_pm(pm_name).ok_or_else(|| PmError::UnknownPmNode(pm_name.to_string()))?;
    if def.group != "block" {
        return Err(bad(format!("PM 节点 `{pm_name}` 不能出现在块内容中")));
    }
    Ok((pm_name, def, as_obj_or_empty(node.get("attrs")).clone()))
}

fn content_children(node: &Map<String, Value>) -> &[Value] {
    as_array_or_empty(node.get("content"))
}

fn build_block(node: &Value, ctx: &mut Ctx) -> Result<Value, PmError> {
    let obj = node.as_object().ok_or_else(|| bad("块节点必须是对象"))?;
    let (pm_name, def, attrs) = def_of_block(obj)?;
    let want_id = read_text(&attrs, "id", "");
    let id = ctx.resolve_id(&want_id, def.id_kind.unwrap_or(IdKind::Blk));
    let mut out = new_prima_obj(def.prima_type.expect("块 def 必有 Prima 类型"), &id);

    match pm_name {
        "section" | "quote" | "callout" | "footnote" => {
            push_declared_attr_fields(&mut out, &attrs, def.prima_fields, def.attrs);
            let mut children = Vec::new();
            for c in content_children(obj) {
                children.push(build_block(c, ctx)?);
            }
            out.insert("children".into(), Value::Array(children));
        }
        "paragraph" | "heading" => {
            push_declared_attr_fields(&mut out, &attrs, def.prima_fields, def.attrs);
            out.insert(
                "content".into(),
                Value::Array(build_spans(content_children(obj), ctx)?),
            );
        }
        "list" => {
            push_declared_attr_fields(&mut out, &attrs, def.prima_fields, def.attrs);
            let mut items = Vec::new();
            for item in content_children(obj) {
                items.push(build_list_item(item, ctx)?);
            }
            out.insert("items".into(), Value::Array(items));
        }
        "code_block" => {
            push_declared_attr_fields(&mut out, &attrs, def.prima_fields, def.attrs);
            let mut text = String::new();
            for t in content_children(obj) {
                let to = t
                    .as_object()
                    .ok_or_else(|| bad("code_block 子节点必须是对象"))?;
                if to.get("type").and_then(Value::as_str) != Some("text") {
                    return Err(bad("code_block 只能包含 text 子节点"));
                }
                text.push_str(to.get("text").and_then(Value::as_str).unwrap_or(""));
            }
            out.insert("text".into(), json!(text));
        }
        "table" => {
            let mut rows = Vec::new();
            for row in content_children(obj) {
                rows.push(build_table_row(row, ctx)?);
            }
            // 列宽：PM cell colwidth（像素，prosemirror-tables 惯例）归并进
            // columns[].width（spec §6.5 相对单位——沿用像素值，仅比例有效）；
            // PM 侧缺 columns 或列数不足时按网格宽补齐（新列发 col_ ID）。
            let columns = table_columns(content_children(obj), &attrs, ctx);
            push_declared_attr_fields(&mut out, &attrs, def.prima_fields, def.attrs);
            out.insert("columns".into(), Value::Array(columns));
            out.insert("rows".into(), Value::Array(rows));
        }
        "figure" | "image" | "horizontal_rule" | "math_block" | "embed" => {
            push_declared_attr_fields(&mut out, &attrs, def.prima_fields, def.attrs);
        }
        "unknown_block" => {
            push_declared_attr_fields(&mut out, &attrs, def.prima_fields, def.attrs);
            out.insert(
                "content".into(),
                Value::Array(build_spans(content_children(obj), ctx)?),
            );
        }
        other => return Err(bad(format!("PM 节点 `{other}` 不能出现在块内容中"))),
    }
    spread_extra(&mut out, &attrs);
    Ok(Value::Object(out))
}

/// list.items 成员（Prima 中是不带 type 的普通对象）。
fn build_list_item(node: &Value, ctx: &mut Ctx) -> Result<Value, PmError> {
    let obj = node
        .as_object()
        .ok_or_else(|| bad("list.items 成员必须是对象"))?;
    if obj.get("type").and_then(Value::as_str) != Some("list_item") {
        return Err(bad("list 的直接子节点必须是 list_item"));
    }
    let def = mapping::node_by_pm("list_item").expect("list_item 定义存在");
    let attrs = as_obj_or_empty(obj.get("attrs")).clone();
    let want_id = read_text(&attrs, "id", "");
    let id = ctx.resolve_id(&want_id, def.id_kind.expect("list_item 有 ID 身份"));
    let mut out = Map::new();
    out.insert("id".into(), json!(id));
    push_declared_attr_fields(&mut out, &attrs, def.prima_fields, def.attrs);
    let mut children = Vec::new();
    for c in content_children(obj) {
        children.push(build_block(c, ctx)?);
    }
    out.insert("children".into(), Value::Array(children));
    spread_extra(&mut out, &attrs);
    Ok(Value::Object(out))
}

fn build_table_row(node: &Value, ctx: &mut Ctx) -> Result<Value, PmError> {
    let obj = node
        .as_object()
        .ok_or_else(|| bad("table.rows 成员必须是对象"))?;
    if obj.get("type").and_then(Value::as_str) != Some("table_row") {
        return Err(bad("table 的直接子节点必须是 table_row"));
    }
    let def = mapping::node_by_pm("table_row").expect("table_row 定义存在");
    let attrs = as_obj_or_empty(obj.get("attrs")).clone();
    let want_id = read_text(&attrs, "id", "");
    let id = ctx.resolve_id(&want_id, def.id_kind.expect("table_row 有 ID 身份"));
    let mut cells = Vec::new();
    for cell in content_children(obj) {
        cells.push(build_table_cell(cell, ctx)?);
    }
    let mut out = Map::new();
    out.insert("id".into(), json!(id));
    out.insert("cells".into(), Value::Array(cells));
    spread_extra(&mut out, &attrs);
    Ok(Value::Object(out))
}

fn build_table_cell(node: &Value, ctx: &mut Ctx) -> Result<Value, PmError> {
    let obj = node
        .as_object()
        .ok_or_else(|| bad("table_row.cells 成员必须是对象"))?;
    let pm_type = obj.get("type").and_then(Value::as_str).unwrap_or("");
    let (pm_name, is_header) = match pm_type {
        "table_cell" => ("table_cell", false),
        "table_header" => ("table_header", true),
        _ => return Err(bad("table_row 的直接子节点必须是 table_cell/table_header")),
    };
    let def = mapping::node_by_pm(pm_name).expect("table_cell/table_header 定义存在");
    let attrs = as_obj_or_empty(obj.get("attrs")).clone();
    let want_id = read_text(&attrs, "id", "");
    let id = ctx.resolve_id(&want_id, def.id_kind.expect("cell 有 ID 身份"));
    let mut out = Map::new();
    out.insert("id".into(), json!(id));
    push_declared_attr_fields(&mut out, &attrs, def.prima_fields, def.attrs);
    if is_header {
        // 节点类型即角色：th → Prima role="header"（不依赖 PM attr）
        out.insert("role".into(), json!("header"));
    }
    let mut children = Vec::new();
    for c in content_children(obj) {
        children.push(build_block(c, ctx)?);
    }
    out.insert("children".into(), Value::Array(children));
    spread_extra(&mut out, &attrs);
    Ok(Value::Object(out))
}

/// 归并 columns：只处理 PM attrs.columns 既有项（id 校验 + colwidth
/// → width 归并）。不补列、不补 name——新列的 columns 项由编辑侧
/// syncTable 与表格命令共同维护；列数少于网格的旧表保持原样
/// （spec §6.5 不规则表只诊断、不静默修复）。
fn table_columns(rows: &[Value], attrs: &Map<String, Value>, ctx: &mut Ctx) -> Vec<Value> {
    // 网格每列第一个非空 colwidth（PM cell 像素宽 → Prima 相对单位）
    let mut colwidth: Vec<Option<i64>> = Vec::new();
    for row in rows {
        let Some(cells) = row.get("content").and_then(Value::as_array) else {
            continue;
        };
        let mut col = 0usize;
        for cell in cells {
            let co = as_obj_or_empty(cell.get("attrs"));
            let span = co
                .get("colspan")
                .or_else(|| co.get("colSpan"))
                .and_then(Value::as_i64)
                .unwrap_or(1)
                .max(1) as usize;
            if let Some(widths) = co.get("colwidth").and_then(Value::as_array) {
                for (w, v) in widths.iter().enumerate().take(span) {
                    let idx = col + w;
                    if colwidth.len() <= idx {
                        colwidth.resize(idx + 1, None);
                    }
                    if colwidth[idx].is_none() {
                        colwidth[idx] = v.as_i64();
                    }
                }
            }
            col += span;
        }
    }

    let mut cols: Vec<Value> = as_array_or_empty(attrs.get("columns")).to_vec();
    for (i, c) in cols.iter_mut().enumerate() {
        if let Some(obj) = c.as_object_mut() {
            let id = obj
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let id = ctx.resolve_id(&id, IdKind::Col);
            obj.insert("id".into(), json!(id));
            // 不补 name：缺失与空串在 Prima 语义同值，规范形不写字段
            match colwidth.get(i).copied().flatten() {
                Some(w) => {
                    obj.insert("width".into(), json!(w));
                }
                None => {
                    // 拖宽清掉后不落 width（缺失=等宽，spec §6.5）
                    obj.remove("width");
                }
            }
        }
    }
    cols
}

// ---------------------------------------------------------------- 行内

/// PM 行内 children → Prima span 数组（相邻同 marks 文本合并）。
fn build_spans(children: &[Value], ctx: &mut Ctx) -> Result<Vec<Value>, PmError> {
    let mut out: Vec<Value> = Vec::new();
    let mut pending: Option<(Vec<ParsedMark>, String)> = None;

    for child in children {
        let obj = child.as_object().ok_or_else(|| bad("行内节点必须是对象"))?;
        match obj.get("type").and_then(Value::as_str) {
            Some("text") => {
                let marks = parse_marks(obj)?;
                let text = obj
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                match &mut pending {
                    Some((pm, buf)) if same_marks(pm, &marks) => buf.push_str(&text),
                    _ => {
                        flush_run(&mut pending, &mut out);
                        pending = Some((marks, text));
                    }
                }
            }
            Some(_) => {
                flush_run(&mut pending, &mut out);
                out.push(build_inline_node(obj, ctx)?);
            }
            None => return Err(bad("行内节点缺少 type")),
        }
    }
    flush_run(&mut pending, &mut out);
    Ok(out)
}

fn flush_run(pending: &mut Option<(Vec<ParsedMark>, String)>, out: &mut Vec<Value>) {
    if let Some((marks, text)) = pending.take() {
        out.push(span_from_run(&text, &marks));
    }
}

/// 文本运行 → 嵌套 span（rank 升序 = 从外到内；code 为叶；span_extra 落 Text.extra）。
fn span_from_run(text: &str, marks: &[ParsedMark]) -> Value {
    let mut span_extra: Map<String, Value> = Map::new();
    let mut code: Option<&ParsedMark> = None;
    let mut containers: Vec<&ParsedMark> = Vec::new();
    for m in marks {
        match m.def.pm_name {
            "span_extra" => {
                if let Some(data) = read_opt_json(&m.attrs, "data") {
                    if let Some(map) = data.as_object() {
                        for (k, v) in map {
                            span_extra.insert(k.clone(), v.clone());
                        }
                    }
                }
            }
            "code" => code = Some(m),
            _ => containers.push(m),
        }
    }

    let mut cur = if let Some(code_mark) = code {
        let mut o = Map::new();
        o.insert("type".into(), json!("code"));
        o.insert("text".into(), json!(text));
        if !span_extra.is_empty() {
            for (k, v) in span_extra {
                o.insert(k, v);
            }
        }
        spread_extra(&mut o, &code_mark.attrs);
        Value::Object(o)
    } else {
        let mut o = Map::new();
        o.insert("type".into(), json!("text"));
        o.insert("text".into(), json!(text));
        for (k, v) in span_extra {
            o.insert(k, v);
        }
        Value::Object(o)
    };

    for m in containers.iter().rev() {
        cur = m.wrap(cur);
    }
    cur
}

/// 行内原子/unknown 节点 → Prima span（继承节点自身 marks 作为祖先链）。
fn build_inline_node(obj: &Map<String, Value>, ctx: &mut Ctx) -> Result<Value, PmError> {
    let pm_name = obj.get("type").and_then(Value::as_str).unwrap_or("");
    let def =
        mapping::node_by_pm(pm_name).ok_or_else(|| PmError::UnknownPmNode(pm_name.to_string()))?;
    if def.group != "inline" {
        return Err(bad(format!("PM 节点 `{pm_name}` 不能出现在行内内容中")));
    }
    let attrs = as_obj_or_empty(obj.get("attrs")).clone();
    let marks = parse_marks(obj)?;

    let base: Map<String, Value> = match pm_name {
        "hard_break" => {
            let mut o = Map::new();
            o.insert("type".into(), json!("hard_break"));
            o
        }
        "footnote_ref" => {
            let mut o = Map::new();
            o.insert("type".into(), json!("footnote_ref"));
            o.insert("id".into(), json!(read_text(&attrs, "id", "")));
            o
        }
        "inline_math" | "inline_image" | "mention" | "cite" => {
            let mut o = Map::new();
            o.insert(
                "type".into(),
                json!(def.prima_type.expect("行内 def 必有类型")),
            );
            push_declared_attr_fields(&mut o, &attrs, def.prima_fields, def.attrs);
            o
        }
        "unknown_span" => {
            let mut o = Map::new();
            o.insert("type".into(), json!("unknown"));
            push_declared_attr_fields(&mut o, &attrs, def.prima_fields, def.attrs);
            let content = build_spans(content_children(obj), ctx)?;
            o.insert("content".into(), Value::Array(content));
            o
        }
        other => return Err(bad(format!("PM 节点 `{other}` 不能出现在行内内容中"))),
    };

    wrap_with_marks(base, &attrs, &marks)
}

/// 用节点自身的 marks 重建祖先链；span_extra 并入 extra，code 施加于非文本报错。
fn wrap_with_marks(
    mut base: Map<String, Value>,
    base_attrs: &Map<String, Value>,
    marks: &[ParsedMark],
) -> Result<Value, PmError> {
    let mut containers: Vec<&ParsedMark> = Vec::new();
    for m in marks {
        match m.def.pm_name {
            "span_extra" => {
                if let Some(data) = read_opt_json(&m.attrs, "data") {
                    if let Some(map) = data.as_object() {
                        for (k, v) in map {
                            base.insert(k.clone(), v.clone());
                        }
                    }
                }
            }
            "code" => {
                return Err(bad("code 标记只能施加于文本（Prima 的 Code 是 text 叶）"));
            }
            _ => containers.push(m),
        }
    }
    spread_extra(&mut base, base_attrs);

    let mut cur = Value::Object(base);
    for m in containers.iter().rev() {
        cur = m.wrap(cur);
    }
    Ok(cur)
}
