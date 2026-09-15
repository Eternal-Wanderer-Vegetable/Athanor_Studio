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

//! M2 验收 ①：语料库 `md → azodoc → md → azodoc` 的**模型层恒等**测试。
//!
//! 恒等的判定标准：两次导入的内容树在「资产 ID 序号化」之后完全一致。
//! 字节级差异（转义风格、列表标记）不算漂移。

use azodoc_convert::{canonicalize_assets, IdGen, ImportJob};

fn import_md(text: &str, base: Option<std::path::PathBuf>) -> azodoc_convert::ImportOutput {
    let mut job = ImportJob::new(base);
    azodoc_md::import(text, &mut job)
}

fn asset_ids(content: &serde_json::Value) -> Vec<String> {
    fn collect(v: &serde_json::Value, ids: &mut Vec<String>) {
        if let Some(s) = v.as_str() {
            if let Some(rest) = s.strip_prefix("asset://") {
                let id = rest.split('/').next().unwrap_or(rest);
                if !ids.iter().any(|x| x == id) {
                    ids.push(id.to_string());
                }
            }
            return;
        }
        if let Some(obj) = v.as_object() {
            for val in obj.values() {
                collect(val, ids);
            }
        } else if let Some(arr) = v.as_array() {
            for item in arr {
                collect(item, ids);
            }
        }
    }
    let mut ids = Vec::new();
    collect(content, &mut ids);
    ids
}

/// md → azodoc → md → azodoc：内容树恒等。
fn assert_roundtrip_identity(md_text: &str) {
    let first = import_md(md_text, None);
    let mut doc = doc_from(&first.content, &first, &Vec::new());
    let mut export_log = azodoc_convert::LossLog::new();
    let md2 = azodoc_md::export_markdown(&mut doc, &mut export_log);

    let second = import_md(&md2, None);

    let mut a = first.content.clone();
    let mut b = second.content.clone();
    let ids_a = asset_ids(&a);
    let ids_b = asset_ids(&b);
    canonicalize_assets(&mut a, &ids_a);
    canonicalize_assets(&mut b, &ids_b);
    assert_eq!(
        a, b,
        "模型层往返恒等失败。\n--- 二次导出的 Markdown ---\n{md2}"
    );
}

fn doc_from(
    content: &serde_json::Value,
    out: &azodoc_convert::ImportOutput,
    _preserved: &[(String, Vec<u8>)],
) -> azodoc_convert::ExportDoc {
    use azodoc_convert::{ExportAsset, ExportDoc};
    // 从 content 收集资产顺序，并生成确定性的占位字节（恒等测试只比较内容树）
    let ids = asset_ids(content);
    let assets = ids
        .iter()
        .enumerate()
        .map(|(i, id)| ExportAsset {
            id: id.clone(),
            filename: format!("asset-{i}.png"),
            mime: "image/png".into(),
            storage: "external".into(),
            url: Some(format!("https://example.com/a/{i}.png")),
            bytes: None,
        })
        .collect();
    let _ = out;
    ExportDoc {
        content: content.clone(),
        assets,
        preserved: Vec::new(),
        title: None,
        language: None,
        document_id: None,
    }
}

#[test]
fn corpus_basic_roundtrip_identity() {
    let md = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../corpus/markdown/basic.md"
    ))
    .unwrap();
    assert_roundtrip_identity(&md);
}

#[test]
fn footnote_roundtrip_identity() {
    let md = "# T\n\n正文使用[^1]。\n\n[^1]: 脚注内容。\n";
    assert_roundtrip_identity(md);
}

#[test]
fn section_wrapping_is_deterministic() {
    let g1 = wrap(&["# A", "text a", "## B", "text b", "# C", "text c"]);
    let g2 = wrap(&["# A", "text a", "## B", "text b", "# C", "text c"]);
    assert_eq!(g1, g2);
}

fn wrap(lines: &[&str]) -> serde_json::Value {
    let md = lines.join("\n\n");
    let mut job = ImportJob::new(None);
    job.idgen = IdGen::new();
    let out = azodoc_md::import(&md, &mut job);
    out.content
}
