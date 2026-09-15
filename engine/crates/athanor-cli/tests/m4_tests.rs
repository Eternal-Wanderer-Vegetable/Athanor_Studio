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

//! M4 端到端（需要 Pandoc；未安装时自动跳过）。

use std::path::PathBuf;

fn corpus(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../corpus")
        .join(name)
}

#[test]
fn docx_roundtrip_e2e() {
    let Some(_pandoc) = azodoc_docx::bridge::find_pandoc().ok() else {
        eprintln!("跳过：本机未找到 Pandoc");
        return;
    };

    let dir = std::env::temp_dir().join(format!("athanor-m4-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    // 夹具：不含远程图片（Pandoc 生成 docx 时会尝试下载远程资源并可能失败）
    let fixture_md = dir.join("fixture.md");
    std::fs::write(
        &fixture_md,
        "# 端到端标题\n\n段落含 **粗** 与 `码`。\n\n- 项一\n- 项二\n\n| 甲 | 乙 |\n| -- | -- |\n| 1 | 2 |\n",
    )
    .unwrap();
    let fixture_docx = dir.join("fixture.docx");
    let pandoc = azodoc_docx::bridge::find_pandoc().unwrap();
    let gen = std::process::Command::new(&pandoc)
        .args([
            "-f",
            "markdown",
            "-t",
            "docx",
            "-o",
            fixture_docx.to_str().unwrap(),
            fixture_md.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(gen.status.success(), "夹具 docx 生成失败");

    let doc = dir.join("e2e.azodoc");
    assert_eq!(
        athanor_cli::cmd_import(&fixture_docx, &doc, Some("docx"), "zh-CN", None, false),
        0,
        "docx 导入失败"
    );
    assert_eq!(athanor_cli::verify_cmd::run(&doc), 0);

    let out_docx = dir.join("e2e-out.docx");
    assert_eq!(
        athanor_cli::cmd_transmute(&doc, "docx", Some(&out_docx), false, false),
        0
    );
    assert!(out_docx.exists());

    // 回导入：核心内容仍在
    let reimport = dir.join("reimport.azodoc");
    assert_eq!(
        athanor_cli::cmd_import(&out_docx, &reimport, Some("docx"), "zh-CN", None, false),
        0,
        "回导入失败"
    );
    assert_eq!(athanor_cli::verify_cmd::run(&reimport), 0);
    let mut c2 = azodoc_container::open(std::fs::read(&reimport).unwrap())
        .unwrap()
        .0;
    let content2: serde_json::Value =
        serde_json::from_slice(&c2.read_entry("document/content.json").unwrap()).unwrap();
    let s2 = serde_json::to_string(&content2).unwrap();
    assert!(s2.contains("端到端标题"), "回导入丢失标题");

    std::fs::remove_dir_all(&dir).ok();
}
