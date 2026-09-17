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

//! M1 验收 ④：schemars 生成 Schema 与 spec/json-schema 的双向接受性比对。
//!
//! 断言：黄金样例的每个 manifest/content 层文件在【生成 Schema】与【spec Schema】
//! 下都被接受。任何一侧拒绝合法文件 → 规范与代码发生漂移，测试失败。

use azodoc_model::schema_gen::{content_schema, manifest_schema};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

fn spec_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../spec")
}

fn samples() -> Vec<PathBuf> {
    ["minimal.azodoc", "rich.azodoc", "forward-compat.azodoc"]
        .iter()
        .map(|n| spec_dir().join("examples").join(n))
        .collect()
}

fn read_zip_entry(path: &std::path::Path, entry: &str) -> Value {
    let bytes = read_entry_bytes(path, entry);
    serde_json::from_slice(&bytes).expect("JSON 解析失败")
}

fn read_entry_bytes(path: &std::path::Path, entry: &str) -> Vec<u8> {
    let file = std::fs::File::open(path).expect("打开样例失败");
    let mut zf = zip::ZipArchive::new(file).expect("打开 ZIP 失败");
    let mut out = Vec::new();
    use std::io::Read;
    zf.by_name(entry)
        .expect("条目不存在")
        .read_to_end(&mut out)
        .expect("读取失败");
    out
}

/// jsonschema 0.56 API 封装（隔离可能的版本差异）。
mod validator {
    use serde_json::Value;

    pub fn validates(schema: &Value, instance: &Value) -> Result<(), String> {
        jsonschema::validate(schema, instance).map_err(|e| e.to_string())
    }
}

#[test]
fn golden_layers_accepted_by_both_schemas() {
    let gen_manifest = manifest_schema();
    let gen_content = content_schema();
    let spec_manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(spec_dir().join("json-schema/manifest.schema.json")).unwrap(),
    )
    .unwrap();
    let spec_content: Value = serde_json::from_str(
        &std::fs::read_to_string(spec_dir().join("json-schema/content.schema.json")).unwrap(),
    )
    .unwrap();

    let mut failures = Vec::new();
    for sample in samples() {
        let name = sample.file_name().unwrap().to_string_lossy().to_string();
        // manifest + content
        let m = read_zip_entry(&sample, "manifest.json");
        for (label, schema) in [("generated", &gen_manifest), ("spec", &spec_manifest)] {
            if let Err(e) = validator::validates(schema, &m) {
                failures.push(format!("{name} manifest.json [{label}]: {e}"));
            }
        }
        let c = read_zip_entry(&sample, "document/content.json");
        for (label, schema) in [("generated", &gen_content), ("spec", &spec_content)] {
            if let Err(e) = validator::validates(schema, &c) {
                failures.push(format!("{name} content.json [{label}]: {e}"));
            }
        }
        // 修订快照（若存在）也必须通过 content Schema
        let manifest_obj = m.as_object().unwrap();
        if let Some(current) = manifest_obj.get("current_revision").and_then(Value::as_str) {
            let snap = read_entry_bytes(&sample, &format!("revisions/{current}/content.json"));
            let snap_v: Value = serde_json::from_slice(&snap).unwrap();
            for (label, schema) in [("generated", &gen_content), ("spec", &spec_content)] {
                if let Err(e) = validator::validates(schema, &snap_v) {
                    failures.push(format!("{name} revisions/{current} [{label}]: {e}"));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "Schema 漂移检测失败（黄金样例被拒绝）:\n{}",
        failures.join("\n")
    );
}

type Shape = (BTreeSet<String>, BTreeSet<String>);

#[test]
fn structural_diff_detects_field_required_and_variant_drift() {
    let original = content_schema();
    let expected = variant_shapes(&original, "Node");
    let mut changed = original.clone();
    changed["$defs"]["Node"]["oneOf"][0]["properties"]
        .as_object_mut()
        .unwrap()
        .remove("children");
    assert_ne!(
        variant_shapes(&changed, "Node"),
        expected,
        "removed field must fail"
    );
    let mut changed = original.clone();
    changed["$defs"]["Node"]["oneOf"][0]["properties"]["new_field"] =
        serde_json::json!({"type": "string"});
    assert_ne!(
        variant_shapes(&changed, "Node"),
        expected,
        "added field must fail"
    );
    let mut changed = original.clone();
    changed["$defs"]["Node"]["oneOf"][0]["required"] = serde_json::json!(["type", "id"]);
    assert_ne!(
        variant_shapes(&changed, "Node"),
        expected,
        "optionalized field must fail"
    );
    let mut changed = original.clone();
    changed["$defs"]["Node"]["oneOf"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    assert_ne!(
        variant_shapes(&changed, "Node"),
        expected,
        "removed variant must fail"
    );
    let mut changed = original.clone();
    changed["$defs"]["Node"]["oneOf"][0]["required"]
        .as_array_mut()
        .unwrap()
        .reverse();
    assert_eq!(
        variant_shapes(&changed, "Node"),
        expected,
        "ordering is immaterial"
    );

    let original = manifest_schema();
    let expected = item_shape(&original, &original["properties"]["reports"]);
    let mut changed = original.clone();
    changed["$defs"]["ReportEntry"]["properties"]
        .as_object_mut()
        .unwrap()
        .remove("sha256");
    assert_ne!(
        item_shape(&changed, &changed["properties"]["reports"]),
        expected
    );
}

#[test]
fn nested_objects_match_with_explicit_compatibility_gaps() {
    let generated = content_schema();
    let spec: Value = serde_json::from_str(
        &std::fs::read_to_string(spec_dir().join("json-schema/content.schema.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(shape(&generated, &generated), shape(&spec, &spec));
    for (name, pointer) in [
        ("ListItem", "/$defs/list/properties/items/items"),
        ("TableColumn", "/$defs/table/properties/columns/items"),
        ("TableRow", "/$defs/table/properties/rows/items"),
        (
            "TableCell",
            "/$defs/table/properties/rows/items/properties/cells/items",
        ),
    ] {
        let actual = shape(&generated, &generated["$defs"][name]);
        let mut expected = shape(&spec, spec.pointer(pointer).unwrap());
        if name == "ListItem" {
            assert!(!actual.0.contains("type"));
            assert!(!actual.1.contains("type"));
            assert!(expected.0.remove("type"));
            assert!(expected.1.remove("type"));
        }
        assert_eq!(actual, expected, "{name} nested structure drift");
    }
    for (family, spec_name) in [("Node", "unknown_block"), ("Span", "unknown_span")] {
        let unknown = generated["$defs"][family]["oneOf"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| {
                v.pointer("/properties/type/const").and_then(Value::as_str) == Some("unknown")
            })
            .unwrap();
        assert!(!shape(&generated, unknown).1.contains("content"));
        assert!(shape(&spec, &spec["$defs"][spec_name])
            .1
            .contains("content"));
    }
}

/// Resolve only the structural parts needed for the diff. `$ref` names are
/// intentionally ignored: schemars uses Rust type names while the normative
/// spec uses protocol names.
fn resolved(schema: &Value, node: &Value) -> Value {
    let Some(reference) = node.get("$ref").and_then(Value::as_str) else {
        return node.clone();
    };
    let Some(name) = reference.strip_prefix("#/$defs/") else {
        return node.clone();
    };
    let mut base = resolved(schema, &schema["$defs"][name]);
    let Some(base_obj) = base.as_object_mut() else {
        return base;
    };
    let Some(overlay) = node.as_object() else {
        return base;
    };
    for (key, value) in overlay {
        if key == "$ref" {
            continue;
        }
        if key == "properties" {
            let target = base_obj
                .entry(key.clone())
                .or_insert_with(|| Value::Object(serde_json::Map::new()));
            if let (Some(target), Some(value)) = (target.as_object_mut(), value.as_object()) {
                target.extend(value.clone());
            }
        } else if key == "required" {
            let mut required = base_obj
                .get(key)
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for item in value.as_array().cloned().unwrap_or_default() {
                if !required.contains(&item) {
                    required.push(item);
                }
            }
            base_obj.insert(key.clone(), Value::Array(required));
        } else {
            base_obj.insert(key.clone(), value.clone());
        }
    }
    base
}

fn shape(schema: &Value, node: &Value) -> Shape {
    let node = resolved(schema, node);
    let node = node
        .get("anyOf")
        .and_then(Value::as_array)
        .and_then(|branches| {
            branches.iter().find(|branch| {
                let branch = resolved(schema, branch);
                branch.get("type").and_then(Value::as_str) == Some("object")
                    || branch.get("properties").is_some()
            })
        })
        .map(|branch| resolved(schema, branch))
        .unwrap_or(node);
    let properties = node
        .get("properties")
        .and_then(Value::as_object)
        .map(|properties| properties.keys().cloned().collect())
        .unwrap_or_default();
    let required = node
        .get("required")
        .and_then(Value::as_array)
        .map(|required| {
            required
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    (properties, required)
}

fn item_shape(schema: &Value, node: &Value) -> Shape {
    let node = resolved(schema, node);
    node.get("items")
        .map(|item| shape(schema, item))
        .unwrap_or_else(|| shape(schema, &node))
}

fn collect_variants(schema: &Value, node: &Value, output: &mut BTreeMap<String, Shape>) {
    let node = resolved(schema, node);
    if let Some(branches) = node
        .get("oneOf")
        .or_else(|| node.get("anyOf"))
        .and_then(Value::as_array)
    {
        for branch in branches {
            collect_variants(schema, branch, output);
        }
        return;
    }
    let Some(kind) = node
        .pointer("/properties/type/const")
        .and_then(Value::as_str)
    else {
        return;
    };
    let mut signature = shape(schema, &node);
    // The Rust model uses serde(default) for unknown content so it can read
    // legacy files without a content array; the normative schema requires the
    // canonical empty array. This is the one documented compatibility waiver.
    if kind == "unknown" {
        signature.1.remove("content");
    }
    output.insert(kind.to_string(), signature);
}

fn variant_shapes(schema: &Value, definition: &str) -> BTreeMap<String, Shape> {
    let mut output = BTreeMap::new();
    collect_variants(schema, &schema["$defs"][definition], &mut output);
    output
}

#[test]
fn generated_and_spec_schemas_have_matching_structure() {
    let generated_manifest = manifest_schema();
    let generated_content = content_schema();
    let spec_manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(spec_dir().join("json-schema/manifest.schema.json")).unwrap(),
    )
    .unwrap();
    let spec_content: Value = serde_json::from_str(
        &std::fs::read_to_string(spec_dir().join("json-schema/content.schema.json")).unwrap(),
    )
    .unwrap();

    let mut generated = BTreeMap::new();
    let mut normative = BTreeMap::new();
    for key in ["<root>", "azodoc", "document", "compatibility", "reports"] {
        let generated_node = if key == "<root>" {
            &generated_manifest
        } else {
            &generated_manifest["properties"][key]
        };
        let spec_node = if key == "<root>" {
            &spec_manifest
        } else {
            &spec_manifest["properties"][key]
        };
        let mut generated_shape = if matches!(key, "compatibility" | "reports") {
            item_shape(&generated_manifest, generated_node)
        } else {
            shape(&generated_manifest, generated_node)
        };
        let mut spec_shape = if matches!(key, "compatibility" | "reports") {
            item_shape(&spec_manifest, spec_node)
        } else {
            shape(&spec_manifest, spec_node)
        };
        // `#[serde(flatten)] extra` is represented as additionalProperties in
        // schemars and is therefore absent from generated properties.
        generated_shape.0.remove("extra");
        spec_shape.0.remove("extra");
        if key == "reports" {
            // Older containers may omit the optional report kind; the model
            // deliberately remains readable while new writers always emit it.
            generated_shape.1.remove("kind");
            spec_shape.1.remove("kind");
        }
        generated.insert(key, generated_shape);
        normative.insert(key, spec_shape);
    }
    assert_eq!(generated, normative, "manifest schema structural drift");

    assert_eq!(
        variant_shapes(&generated_content, "Node"),
        variant_shapes(&spec_content, "node"),
        "block schema structural drift"
    );
    assert_eq!(
        variant_shapes(&generated_content, "Span"),
        variant_shapes(&spec_content, "span"),
        "span schema structural drift"
    );
}
