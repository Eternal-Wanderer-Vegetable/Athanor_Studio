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

//! B1 验收（Future Work §B1）：publish 默认走 CDP + Paged.js 分页
//! （页码/运行头），`--no-paged` 回退 CLI 直印；layout_hash 确定性沿用 M5
//! （需无头浏览器，缺席自动跳过）。

use std::path::PathBuf;

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("athanor-b1-cli-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// 多页 Markdown 夹具（A4 下 > 1 页）。
fn write_multipage_md(dir: &std::path::Path) -> PathBuf {
    let md = dir.join("long.md");
    let paragraph = "出版品质验收段落：分页点由 Paged.js 排版引擎决定，页码由 @page \
边盒渲染为页脚。段落之间保持足够的文本量以确保文档超过一页。";
    let mut text = String::from("# 多页验收文档\n\n");
    for i in 1..=40 {
        text.push_str(&format!("{paragraph}（第 {i} 段）\n\n"));
    }
    std::fs::write(&md, text).unwrap();
    md
}

fn publication_of(doc: &std::path::Path) -> serde_json::Value {
    let data = std::fs::read(doc).unwrap();
    let (mut c, _) = azodoc_container::open(data).unwrap();
    let layer: serde_json::Value =
        serde_json::from_slice(&c.read_entry("publication/publication.json").unwrap()).unwrap();
    layer["publications"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone()
}

#[test]
fn publish_paged_multiline_deterministic() {
    if std::env::var_os("AZODOC_BROWSER_PATH").is_none() && azodoc_pdf::find_browser().is_err() {
        eprintln!("跳过：本机未找到 Chromium/Edge");
        return;
    }

    let dir = tmpdir("paged");
    let md = write_multipage_md(&dir);
    let doc = dir.join("d.azodoc");
    assert_eq!(
        athanor_cli::cmd_import(&md, &doc, None, "zh-CN", None, false),
        0
    );

    // 默认出版：走 CDP + Paged.js
    assert_eq!(
        athanor_cli::cmd_publish(
            &doc,
            &athanor_cli::PublishArgs {
                out: Some(&dir.join("paged.pdf")),
                browser: None,
                no_paged: false,
            }
        ),
        0
    );
    let pub1 = publication_of(&doc);
    assert!(
        pub1["renderer"]["engine"]
            .as_str()
            .unwrap()
            .contains("chromium-cdp/pagedjs"),
        "默认应记录 pagedjs 引擎，实际: {}",
        pub1["renderer"]["engine"]
    );
    let pages = pub1["page"]["count"].as_u64().unwrap();
    assert!(pages >= 2, "多页文档页数应 >= 2，实际 {pages}");
    let pdf = std::fs::read(dir.join("paged.pdf")).unwrap();
    assert!(pdf.starts_with(b"%PDF") && pdf.len() > 10_000, "产物非空白");

    // 确定性验收：同输入同渲染器 → layout_hash 稳定
    assert_eq!(
        athanor_cli::cmd_publish(
            &doc,
            &athanor_cli::PublishArgs {
                out: None,
                browser: None,
                no_paged: false
            }
        ),
        0
    );
    let pub2 = publication_of(&doc);
    assert_eq!(pub1["layout_hash"], pub2["layout_hash"]);
    assert_eq!(pub1["page"]["count"], pub2["page"]["count"]);

    assert_eq!(athanor_cli::verify_cmd::run(&doc), 0);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn publish_no_paged_falls_back_plain() {
    if std::env::var_os("AZODOC_BROWSER_PATH").is_none() && azodoc_pdf::find_browser().is_err() {
        eprintln!("跳过：本机未找到 Chromium/Edge");
        return;
    }

    let dir = tmpdir("plain");
    let md = write_multipage_md(&dir);
    let doc = dir.join("d.azodoc");
    assert_eq!(
        athanor_cli::cmd_import(&md, &doc, None, "zh-CN", None, false),
        0
    );

    assert_eq!(
        athanor_cli::cmd_publish(
            &doc,
            &athanor_cli::PublishArgs {
                out: None,
                browser: None,
                no_paged: true,
            }
        ),
        0
    );
    let record = publication_of(&doc);
    assert!(
        record["renderer"]["engine"]
            .as_str()
            .unwrap()
            .contains("chromium-print"),
        "--no-paged 应记录直印引擎，实际: {}",
        record["renderer"]["engine"]
    );
    assert_ne!(record["layout_hash"], serde_json::Value::Null);

    assert_eq!(athanor_cli::verify_cmd::run(&doc), 0);
    std::fs::remove_dir_all(&dir).ok();
}
