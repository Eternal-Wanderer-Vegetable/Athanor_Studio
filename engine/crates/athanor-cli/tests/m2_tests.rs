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

//! M2 端到端测试：import → verify → transmute →（模拟编辑）→ stale → upgrade。
//! 含 TXT 黄金文件与 strict-loss 行为。

use athanor_cli::{cmd_import, cmd_transmute, cmd_upgrade};
use std::path::{Path, PathBuf};

fn corpus(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../corpus")
        .join(name)
}

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("athanor-m2-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn read_container(p: &Path) -> azodoc_container::Container {
    let data = std::fs::read(p).unwrap();
    azodoc_container::open(data).unwrap().0
}

// ---------------------------------------------------------- TXT 黄金文件

#[test]
fn txt_golden_markdown_basic() {
    let md = std::fs::read_to_string(corpus("markdown/basic.md")).unwrap();
    let mut job = azodoc_convert::ImportJob::new(None);
    let out = azodoc_md::import(&md, &mut job);
    let doc = azodoc_convert::ExportDoc {
        content: out.content.clone(),
        assets: vec![],
        preserved: Vec::new(),
        title: out.title.clone(),
        language: None,
        document_id: None,
    };
    let mut log = azodoc_convert::LossLog::new();
    let actual = azodoc_convert::txt::export_txt(&doc, &mut log);

    let golden = corpus("golden/markdown-basic.txt");
    if std::env::var("AZODOC_WRITE_GOLDEN").is_ok() {
        std::fs::write(&golden, &actual).unwrap();
    }
    let expected = std::fs::read_to_string(&golden).unwrap();
    assert_eq!(
        actual, expected,
        "TXT 黄金文件不匹配（重新生成请设 AZODOC_WRITE_GOLDEN=1）"
    );
}

#[test]
fn txt_golden_html_basic() {
    let html = std::fs::read_to_string(corpus("html/basic.html")).unwrap();
    let mut job = azodoc_convert::ImportJob::new(None);
    let out = azodoc_html::import(&html, &mut job);
    let doc = azodoc_convert::ExportDoc {
        content: out.content.clone(),
        assets: vec![],
        preserved: Vec::new(),
        title: out.title.clone(),
        language: None,
        document_id: None,
    };
    let mut log = azodoc_convert::LossLog::new();
    let actual = azodoc_convert::txt::export_txt(&doc, &mut log);

    let golden = corpus("golden/html-basic.txt");
    if std::env::var("AZODOC_WRITE_GOLDEN").is_ok() {
        std::fs::write(&golden, &actual).unwrap();
    }
    let expected = std::fs::read_to_string(&golden).unwrap();
    assert_eq!(actual, expected);
}

// ---------------------------------------------------------- import → verify → transmute

#[test]
fn import_verify_transmute_cycle() {
    let dir = tmpdir("cycle");
    let doc = dir.join("basic.azodoc");

    let code = cmd_import(
        &corpus("markdown/basic.md"),
        &doc,
        None,
        "zh-CN",
        None,
        false,
    );
    assert_eq!(code, 0, "import 失败");
    assert!(doc.exists());

    assert_eq!(
        athanor_cli::verify_cmd::run(&doc),
        0,
        "导入产物应通过 verify"
    );

    // transmute → txt / html / md
    for to in ["txt", "html", "md"] {
        let code = cmd_transmute(&doc, to, Some(&dir.join(format!("out.{to}"))), false, false);
        assert_eq!(code, 0, "transmute {to} 失败");
        assert!(dir.join(format!("out.{to}")).exists());
    }
    assert_eq!(
        athanor_cli::verify_cmd::run(&doc),
        0,
        "缓存刷新后 verify 应仍通过"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn strict_loss_returns_three() {
    let dir = tmpdir("strict");
    let doc = dir.join("lossy.azodoc");
    // lossy.md 含 preserved_raw 内容 → --strict-loss 应返回 3
    let code = cmd_import(
        &corpus("markdown/lossy.md"),
        &doc,
        None,
        "zh-CN",
        None,
        true,
    );
    assert_eq!(code, 3);
    std::fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------- ④ stale → upgrade

#[test]
fn upgrade_rebuilds_stale_caches() {
    let dir = tmpdir("upgrade");
    let doc = dir.join("doc.azodoc");

    // 1. 导入（含 txt + markdown 缓存种子，均 fresh）
    assert_eq!(
        cmd_import(
            &corpus("markdown/basic.md"),
            &doc,
            None,
            "zh-CN",
            None,
            false
        ),
        0
    );

    // 2. 模拟编辑：向 content.json 追加一个段落（层 sha256 自动同步，缓存变 stale）
    let mut c = read_container(&doc);
    let mut content: serde_json::Value =
        serde_json::from_slice(&c.read_entry("document/content.json").unwrap()).unwrap();
    content["content"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "id": "blk_10000000000000000000000000",
            "type": "paragraph",
            "content": [{"type": "text", "text": "编辑后新增的段落。"}]
        }));
    let mut bytes = serde_json::to_vec_pretty(&content).unwrap();
    bytes.push(b'\n');
    c.set_entry("document/content.json", bytes).unwrap();
    std::fs::write(&doc, c.write().unwrap()).unwrap();

    // 3. verify 应提示缓存 stale
    let v = athanor_cli::verify_cmd::run(&doc);
    assert_eq!(v, 0, "stale 是警告不是错误");
    // 4. upgrade 重建
    assert_eq!(cmd_upgrade(&doc), 0);
    // 5. verify 回到全绿，且新缓存包含编辑内容
    assert_eq!(athanor_cli::verify_cmd::run(&doc), 0);
    let mut c2 = read_container(&doc);
    let txt = String::from_utf8(c2.read_entry("compatibility/text/document.txt").unwrap()).unwrap();
    assert!(
        txt.contains("编辑后新增的段落。"),
        "重建的 TXT 缓存应包含编辑内容"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn html_import_e2e() {
    let dir = tmpdir("html");
    let doc = dir.join("h.azodoc");
    assert_eq!(
        cmd_import(&corpus("html/basic.html"), &doc, None, "zh-CN", None, false),
        0
    );
    assert_eq!(athanor_cli::verify_cmd::run(&doc), 0);
    // 网络图片 → 外链资产登记在注册表
    let mut c = read_container(&doc);
    let reg = String::from_utf8(c.read_entry("assets/registry.json").unwrap()).unwrap();
    assert!(reg.contains("example.com/img.png"));
    assert!(reg.contains("\"external\""));
    std::fs::remove_dir_all(&dir).ok();
}
