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

//! B2 端到端：原生 OOXML 读取 → 容器装配 → verify（验收 ①③⑦⑧）。
//! 全部不依赖 Pandoc；`--reader pandoc` 回归用例在 Pandoc 缺席时自动跳过。

use serde_json::Value;
use std::path::{Path, PathBuf};

fn corpus() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../corpus/docx")
}

fn tmpdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("athanor-b2-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn manifest_of(doc: &Path) -> Value {
    let (c, _) = azodoc_container::open(std::fs::read(doc).unwrap()).unwrap();
    c.manifest_value().clone()
}

#[test]
fn native_import_basic_verify_and_layers() {
    let dir = tmpdir("basic");
    let doc = dir.join("basic.azodoc");
    assert_eq!(
        athanor_cli::cmd_import_reader(
            &corpus().join("basic.docx"),
            &doc,
            Some("docx"),
            "zh-CN",
            None,
            false,
            "native",
        ),
        0,
        "native 导入失败"
    );
    assert_eq!(athanor_cli::verify_cmd::run(&doc), 0, "verify 应通过");

    let m = manifest_of(&doc);
    // 样式 → 表现层
    assert!(
        m["layers"].get("presentation").is_some(),
        "basic 语料有样式引用，应登记 presentation 层"
    );
    // 批注缺失 → 不写语义层
    assert!(m["layers"].get("semantics").is_none());
    // converter 名修复：原生读取 = athanor-docx
    let report_path = m["reports"][0]["path"].as_str().unwrap();
    let (mut c, _) = azodoc_container::open(std::fs::read(&doc).unwrap()).unwrap();
    let report: Value = serde_json::from_slice(&c.read_entry(report_path).unwrap()).unwrap();
    assert_eq!(report["engine"]["converter"], "athanor-docx");
    assert_eq!(report["status"], "complete", "basic 语料应零损失");
    // 元数据：creator 走 document.extra（R2）
    assert_eq!(m["document"]["creator"], "Athanor");
    // 修订落链 author_id = converter 名
    let chain: Value =
        serde_json::from_slice(&c.read_entry("revisions/chain.json").unwrap()).unwrap();
    assert_eq!(chain["revisions"][0]["author"]["id"], "athanor-docx");
    assert_eq!(chain["revisions"][0]["author"]["type"], "importer");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn native_import_lossy_semantics_and_preserved() {
    let dir = tmpdir("lossy");
    let doc = dir.join("lossy.azodoc");
    assert_eq!(
        athanor_cli::cmd_import_reader(
            &corpus().join("lossy.docx"),
            &doc,
            Some("docx"),
            "zh-CN",
            None,
            false,
            "native",
        ),
        0,
        "lossy 导入失败"
    );
    assert_eq!(athanor_cli::verify_cmd::run(&doc), 0, "verify 应通过");

    let m = manifest_of(&doc);
    assert!(
        m["layers"].get("semantics").is_some(),
        "批注应登记 semantics 层"
    );
    let (mut c, _) = azodoc_container::open(std::fs::read(&doc).unwrap()).unwrap();
    let ann: Value =
        serde_json::from_slice(&c.read_entry("semantics/annotations.json").unwrap()).unwrap();
    assert!(
        !ann["annotations"].as_array().unwrap().is_empty(),
        "至少一条批注标注"
    );
    let theme: Value =
        serde_json::from_slice(&c.read_entry("presentation/theme.json").unwrap()).unwrap();
    assert_eq!(theme["schema_version"], "1.0");

    // preserved 载荷（页眉/页脚/OLE/OMML 原件）真实落容器
    let names = c.entry_names();
    assert!(
        names.iter().any(|n| n.starts_with("preserved/docx/")),
        "preserved/docx/ 应有载荷，条目: {names:?}"
    );

    // 标注可被重定位体系识别：relocate 后无 detached（导入即锚定）
    let content: Value =
        serde_json::from_slice(&c.read_entry("document/content.json").unwrap()).unwrap();
    let text = serde_json::to_string(&content).unwrap();
    assert!(text.contains("被批注的句子"), "正文应含批注锚定文本");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn auto_reader_matches_native_without_pandoc_dependency() {
    let dir = tmpdir("auto");
    let doc = dir.join("auto.azodoc");
    assert_eq!(
        athanor_cli::cmd_import_reader(
            &corpus().join("basic.docx"),
            &doc,
            Some("docx"),
            "zh-CN",
            None,
            false,
            "auto",
        ),
        0,
        "auto（原生优先）应成功"
    );
    assert_eq!(athanor_cli::verify_cmd::run(&doc), 0);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn invalid_reader_value_is_rejected() {
    let dir = tmpdir("bogus");
    let doc = dir.join("bogus.azodoc");
    assert_eq!(
        athanor_cli::cmd_import_reader(
            &corpus().join("basic.docx"),
            &doc,
            Some("docx"),
            "zh-CN",
            None,
            false,
            "bogus",
        ),
        1,
        "未知 reader 取值应报错退出"
    );
    assert!(!doc.exists(), "失败时不应产出容器");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn pandoc_reader_path_still_works() {
    let Some(_pandoc) = azodoc_docx::bridge::find_pandoc().ok() else {
        eprintln!("跳过：本机未找到 Pandoc");
        return;
    };
    let dir = tmpdir("pandoc");
    let doc = dir.join("pandoc.azodoc");
    assert_eq!(
        athanor_cli::cmd_import_reader(
            &corpus().join("basic.docx"),
            &doc,
            Some("docx"),
            "zh-CN",
            None,
            false,
            "pandoc",
        ),
        0,
        "pandoc 回退路径导入失败"
    );
    assert_eq!(athanor_cli::verify_cmd::run(&doc), 0);
    // 回退路径 converter 名区分
    let m = manifest_of(&doc);
    let report_path = m["reports"][0]["path"].as_str().unwrap();
    let (mut c, _) = azodoc_container::open(std::fs::read(&doc).unwrap()).unwrap();
    let report: Value = serde_json::from_slice(&c.read_entry(report_path).unwrap()).unwrap();
    assert_eq!(report["engine"]["converter"], "athanor-docx-pandoc");
    std::fs::remove_dir_all(&dir).ok();
}
