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

//! 映射表 ↔ `azodoc-model` 类型的一致性锁定（M6.1 的"生成而非漂移"保证）。
//!
//! 断言映射表的 `prima_fields` 与 schemars 从真实 Rust 类型生成的 JSON Schema
//! 枚举变体字段**逐一相等**：`azodoc-model` 增删字段而映射表未跟随时，本测试
//! 直接失败——这是 RFC §决策 3 的锁死机制。

use azodoc_model::content::{KNOWN_NODE_TYPES, KNOWN_SPAN_TYPES};
use azodoc_model::schema_gen::content_schema;
use azodoc_pm::mapping::{MARKS, NODES};
use serde_json::Value;

/// 在 schema 树中递归查找 `properties.type.const == type_name` 的变体 schema
/// （`oneOf` 是数组，对象与数组都要下钻）。
fn find_tagged_variant<'a>(v: &'a Value, type_name: &str) -> Option<&'a Value> {
    if let Some(obj) = v.as_object() {
        let tag = obj
            .get("properties")
            .and_then(|p| p.get("type"))
            .and_then(|t| t.get("const"))
            .and_then(Value::as_str);
        if tag == Some(type_name) {
            return Some(v);
        }
        for child in obj.values() {
            if let Some(found) = find_tagged_variant(child, type_name) {
                return Some(found);
            }
        }
        return None;
    }
    if let Some(arr) = v.as_array() {
        for item in arr {
            if let Some(found) = find_tagged_variant(item, type_name) {
                return Some(found);
            }
        }
    }
    None
}

/// 在 `$defs` 中按名查找结构体 schema（list_item/table_row/table_cell 的 Prima 侧
/// 是不带 type 标签的普通结构体）。
fn find_struct_def<'a>(schema: &'a Value, name: &str) -> Option<&'a Value> {
    let defs = schema.get("$defs")?.as_object()?;
    defs.get(name).filter(|d| d.get("properties").is_some())
}

fn prop_names(variant: &Value) -> Vec<String> {
    let mut names: Vec<String> = variant
        .get("properties")
        .and_then(Value::as_object)
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();
    names.sort();
    names
}

fn assert_fields_match(what: &str, variant: &Value, prima_fields: &[&str]) {
    let mut expected: Vec<String> = prima_fields.iter().map(|s| s.to_string()).collect();
    expected.sort();
    let actual = prop_names(variant);
    // type（serde tag）与 id（映射表统一单列，不进 prima_fields）豁免
    let actual_no_tag: Vec<String> = actual
        .iter()
        .filter(|n| n.as_str() != "type" && n.as_str() != "id")
        .cloned()
        .collect();
    assert_eq!(
        actual_no_tag, expected,
        "{what}: 映射表 prima_fields 与 schemars 变体字段不一致（schema properties: {actual:?}）"
    );
}

#[test]
fn node_mapping_matches_schemars_exactly() {
    let schema = content_schema();

    // ① 每个 KNOWN 块类型都有映射，且字段逐一锁定
    // （list_item 在 Prima 中是不带 type 的结构体，按 PM 名对齐）
    for t in KNOWN_NODE_TYPES {
        let def = NODES
            .iter()
            .find(|d| d.prima_type == Some(*t) || d.pm_name == *t)
            .unwrap_or_else(|| panic!("KNOWN 块类型 `{t}` 缺少映射"));
        // list_item 无 type 标签，在 $defs 中按结构体名查找
        let variant = if *t == "list_item" {
            find_struct_def(&schema, "ListItem")
                .unwrap_or_else(|| panic!("schemars $defs 中找不到 `ListItem`"))
        } else {
            find_tagged_variant(&schema, t)
                .unwrap_or_else(|| panic!("schemars 输出中找不到块类型 `{t}` 的变体"))
        };
        assert_fields_match(&format!("块 `{t}`"), variant, def.prima_fields);
    }

    // ② 每个有 Prima 类型的映射都能在 schema 中找到（含 "unknown" 双重身份：
    //    unknown_block 与 unknown_span 共享同一变体，字段必须都锁上）
    for def in NODES {
        if let Some(t) = def.prima_type {
            let variant = find_tagged_variant(&schema, t)
                .unwrap_or_else(|| panic!("schemars 输出中找不到类型 `{t}`"));
            assert_fields_match(
                &format!("PM 节点 `{}`", def.pm_name),
                variant,
                def.prima_fields,
            );
        }
    }
}

#[test]
fn span_mapping_matches_schemars_exactly() {
    let schema = content_schema();
    for t in KNOWN_SPAN_TYPES {
        // span 的落点三选一：mark（容器叶）、inline 节点、text
        let fields = MARKS
            .iter()
            .find(|d| d.prima_type == Some(*t))
            .map(|d| d.prima_fields)
            .or_else(|| {
                NODES
                    .iter()
                    .find(|d| d.prima_type == Some(*t))
                    .map(|d| d.prima_fields)
            });
        let fields = fields.unwrap_or_else(|| panic!("KNOWN span 类型 `{t}` 缺少映射"));
        let variant = find_tagged_variant(&schema, t)
            .unwrap_or_else(|| panic!("schemars 输出中找不到 span 类型 `{t}`"));
        assert_fields_match(&format!("span `{t}`"), variant, fields);
    }
}

#[test]
fn untagged_structs_match_schemars() {
    let schema = content_schema();
    for (name, pm_name) in [
        ("ListItem", "list_item"),
        ("TableRow", "table_row"),
        ("TableCell", "table_cell"),
    ] {
        let def = NODES
            .iter()
            .find(|d| d.pm_name == pm_name)
            .unwrap_or_else(|| panic!("缺少 {pm_name} 映射"));
        let struct_def = find_struct_def(&schema, name)
            .unwrap_or_else(|| panic!("schemars $defs 中找不到 `{name}`"));
        assert_fields_match(&format!("结构体 `{name}`"), struct_def, def.prima_fields);
    }

    // TableColumn 不映射为 PM 节点（整列打包进 table.columns attr）——锁定其形状
    let col = find_struct_def(&schema, "TableColumn").expect("schemars $defs 找不到 TableColumn");
    assert_eq!(
        prop_names(col),
        vec!["id", "name"],
        "TableColumn 形状变化时须同步 table.columns attr 约定"
    );
    let table = NODES
        .iter()
        .find(|d| d.pm_name == "table")
        .expect("table 映射");
    assert!(
        table.attrs.iter().any(|a| a.name == "columns"),
        "table 必须有 columns attr（TableColumn 的载体）"
    );
}

#[test]
fn content_expressions_reference_defined_names() {
    let node_names: Vec<&str> = NODES.iter().map(|d| d.pm_name).collect();
    let mut group_members: Vec<&str> = Vec::new();
    for d in NODES {
        if let Some(expr) = d.content {
            let token = expr.trim_end_matches(['*', '+']);
            assert!(
                node_names.contains(&token) || token == "block" || token == "inline",
                "`{}` 的 content 表达式引用了未定义的 `{token}`",
                d.pm_name
            );
        }
        if !d.group.is_empty() {
            group_members.push(d.group);
        }
    }
    assert!(group_members.contains(&"block"), "block 组必须有成员");
    assert!(group_members.contains(&"inline"), "inline 组必须有成员");
}
