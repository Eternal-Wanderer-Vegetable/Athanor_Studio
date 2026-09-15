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

//! M1 验收 ④：schemars 生成 Schema 与 spec/json-schema 的双向接受性比对。
//!
//! 断言：黄金样例的每个 manifest/content 层文件在【生成 Schema】与【spec Schema】
//! 下都被接受。任何一侧拒绝合法文件 → 规范与代码发生漂移，测试失败。

use azodoc_model::schema_gen::{content_schema, manifest_schema};
use serde_json::Value;
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
