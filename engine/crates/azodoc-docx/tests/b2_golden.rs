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

//! B2 黄金测试：corpus/docx 语料 → 原生读取 → content.json 黄金比对（验收 ⑨）。
//! 重新生成黄金文件：`AZODOC_WRITE_GOLDEN=1 cargo test -p azodoc-docx --test b2_golden`

use azodoc_convert::ImportJob;
use std::path::PathBuf;

fn corpus() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../corpus/docx")
}

fn golden() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../corpus/docx/golden")
}

fn import_pretty(name: &str) -> (String, azodoc_convert::ImportOutput) {
    let bytes = std::fs::read(corpus().join(name)).expect("语料文件存在");
    let mut job = ImportJob::new(None);
    let out = azodoc_docx::ooxml::import_native(&bytes, &mut job).unwrap();
    let pretty = serde_json::to_string_pretty(&out.content).unwrap() + "\n";
    (pretty, out)
}

fn golden_check(name: &str) {
    let (actual, out) = import_pretty(name);
    let path = golden().join(format!("{name}.content.json"));
    if std::env::var("AZODOC_WRITE_GOLDEN").is_ok() {
        std::fs::create_dir_all(golden()).unwrap();
        std::fs::write(&path, &actual).unwrap();
        println!("golden written: {}", path.display());
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "黄金文件缺失（{}）：{e}；用 AZODOC_WRITE_GOLDEN=1 生成",
            path.display()
        )
    });
    assert_eq!(
        expected, actual,
        "content.json 黄金不一致——读取器的确定性或映射发生变化"
    );
    // 损失概要也应稳定：同输入 → 同事件序列
    let features: Vec<String> = out
        .log
        .events()
        .iter()
        .map(|e| format!("{}|{}|{}", e.feature, e.class.as_str(), e.action))
        .collect();
    let features_path = golden().join(format!("{name}.loss.txt"));
    let features_text = features.join("\n") + "\n";
    if std::env::var("AZODOC_WRITE_GOLDEN").is_ok() {
        std::fs::write(&features_path, &features_text).unwrap();
    }
    let expected_features = std::fs::read_to_string(&features_path)
        .unwrap_or_else(|e| panic!("损失黄金缺失（{}）：{e}", features_path.display()));
    assert_eq!(expected_features, features_text);
}

#[test]
fn golden_basic_docx() {
    golden_check("basic.docx");
}

#[test]
fn golden_lossy_docx() {
    golden_check("lossy.docx");
}

#[test]
fn basic_corpus_is_loss_free() {
    let (_, out) = import_pretty("basic.docx");
    assert!(!out.log.has_loss(), "basic 语料应零损失");
    assert!(out.theme.is_some(), "basic 语料有样式 → 应产出 theme");
    assert_eq!(
        out.title.as_deref(),
        Some("炼金术导论"),
        "标题来自 docProps/core.xml"
    );
}

#[test]
fn lossy_corpus_loss_profile() {
    let (_, out) = import_pretty("lossy.docx");
    let features: Vec<&str> = out
        .log
        .events()
        .iter()
        .map(|e| e.feature.as_str())
        .collect();
    for expected in [
        "docx_tracked_changes",
        "docx_omml",
        "docx_field",
        "docx_sdt",
        "docx_tab",
        "docx_page_break",
        "docx_bookmark",
        "docx_internal_link",
        "docx_run_props",
        "docx_endnotes",
        "docx_headers",
        "docx_footers",
        "docx_ole_object",
    ] {
        assert!(
            features.contains(&expected),
            "lossy 语料应报告 {expected}，实际: {features:?}"
        );
    }
    assert!(!out.annotations.is_empty(), "lossy 语料的批注应成为标注");
}
