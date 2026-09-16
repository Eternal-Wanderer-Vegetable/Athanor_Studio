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

//! 生成物黄金比对（复刻 azodoc-model golden_schema.rs 模式）。
//!
//! `gen/` 下的两个工件入库管理；映射表任何改动导致产物变化时本测试失败，
//! 重新生成：`cargo run -p azodoc-pm --bin generate`。

use azodoc_pm::gen;
use serde_json::Value;
use std::path::PathBuf;

fn gen_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("gen")
}

/// 黄金比对忽略换行差异（gen/** 已在 .gitattributes 豁免转换，但旧克隆或
/// 未知 checkout 配置下仍可能是 CRLF——生成器恒为 LF）。
fn normalize_eol(s: &str) -> String {
    s.replace("\r\n", "\n")
}

#[test]
fn generated_artifacts_match_committed_golden() {
    for (name, body) in [
        ("aludel-schema.json", gen::aludel_schema_json()),
        ("schema.mjs", gen::schema_mjs()),
    ] {
        let path = gen_dir().join(name);
        let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "读取 {} 失败: {e}（先运行 cargo run -p azodoc-pm --bin generate）",
                path.display()
            )
        });
        assert_eq!(
            normalize_eol(&committed),
            normalize_eol(&body),
            "gen/{name} 与重新生成产物不一致——运行 cargo run -p azodoc-pm --bin generate 更新"
        );
    }
}

#[test]
fn schema_spec_is_well_formed() {
    let spec: Value =
        serde_json::from_str(&gen::aludel_schema_json()).expect("schema spec 必须是合法 JSON");
    let nodes = spec
        .get("nodes")
        .and_then(Value::as_object)
        .expect("nodes 对象");
    let marks = spec
        .get("marks")
        .and_then(Value::as_object)
        .expect("marks 对象");

    for required in [
        "doc",
        "text",
        "paragraph",
        "heading",
        "list_item",
        "table_row",
        "table_cell",
        "unknown_block",
        "unknown_span",
    ] {
        assert!(nodes.contains_key(required), "nodes 缺少 `{required}`");
    }
    for required in [
        "link",
        "strong",
        "em",
        "underline",
        "strike",
        "code",
        "span_extra",
    ] {
        assert!(marks.contains_key(required), "marks 缺少 `{required}`");
    }

    let doc = &nodes["doc"];
    assert_eq!(
        doc["content"], "block*",
        "doc.content 必须是 block*（Prima 允许空文档）"
    );
    let para = &nodes["paragraph"];
    assert_eq!(para["content"], "inline*");
    let code = &nodes["code_block"];
    assert_eq!(
        code["code"],
        Value::Bool(true),
        "code_block 必须带 code 标志"
    );
}
