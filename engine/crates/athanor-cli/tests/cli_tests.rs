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

//! CLI 端到端冒烟测试（M1 验收的命令行形态）。

use std::path::PathBuf;
use std::process::Command;

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_athanor"))
        .args(args)
        .output()
        .expect("启动 athanor 失败")
}

fn ok_or_dump(out: &std::process::Output) -> bool {
    out.status.success()
}

fn stdout(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn stderr(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

fn spec_example(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../spec/examples")
        .join(name)
}

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("athanor-cli-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn new_verify_info_cycle() {
    let dir = tmpdir("cycle");
    let doc = dir.join("t.azodoc");

    let out = run(&["new", doc.to_str().unwrap(), "--title", "测试文档"]);
    assert!(ok_or_dump(&out), "new 失败: {}", stderr(&out));
    assert!(doc.exists());

    let v = run(&["verify", doc.to_str().unwrap()]);
    assert!(ok_or_dump(&v), "verify 失败:\n{}{}", stdout(&v), stderr(&v));
    assert!(stdout(&v).contains("校验通过"));

    let i = run(&["info", doc.to_str().unwrap()]);
    assert!(stdout(&i).contains("测试文档"));
    assert!(stdout(&i).contains("prefixed"));

    // 不覆盖已有文件
    let again = run(&["new", doc.to_str().unwrap(), "--title", "另一个"]);
    assert!(!ok_or_dump(&again));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn verify_golden_samples_pass() {
    for name in ["minimal.azodoc", "rich.azodoc", "forward-compat.azodoc"] {
        let p = spec_example(name);
        let v = run(&["verify", p.to_str().unwrap()]);
        assert!(
            ok_or_dump(&v),
            "{name} verify 失败:\n{}{}",
            stdout(&v),
            stderr(&v)
        );
        let s = stdout(&v);
        assert!(!s.contains("✗"), "{name} 出现错误行:\n{s}");
        // rich 的唯一资产被 figure 引用，不得误报孤儿（回归：块级 asset 引用必须被收集）
        if name == "rich.azodoc" {
            assert!(!s.contains("孤儿资产"), "figure 的资产引用未被收集:\n{s}");
        }
    }
}

#[test]
fn info_json_on_golden() {
    let p = spec_example("rich.azodoc");
    let i = run(&["info", p.to_str().unwrap(), "--json"]);
    assert!(ok_or_dump(&i));
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout(&i)).expect("info --json 应输出合法 JSON");
    assert!(parsed["document"]["id"]
        .as_str()
        .unwrap()
        .starts_with("doc_"));
}

#[test]
fn verify_friendly_failure_on_bad_magic() {
    let dir = tmpdir("badmagic");
    let mut data = std::fs::read(spec_example("minimal.azodoc")).unwrap();
    data[0] = b'X';
    let p = dir.join("bad.azodoc");
    std::fs::write(&p, &data).unwrap();

    let v = run(&["verify", p.to_str().unwrap()]);
    assert_eq!(v.status.code(), Some(1));
    let e = stderr(&v);
    assert!(e.contains("错误："), "应含四段式错误头:\n{e}");
    assert!(e.contains("建议："), "应含恢复建议:\n{e}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn verify_rejects_future_major_version() {
    let dir = tmpdir("future");
    let mut data = std::fs::read(spec_example("minimal.azodoc")).unwrap();
    data[8] = 2;
    let p = dir.join("v2.azodoc");
    std::fs::write(&p, &data).unwrap();

    let v = run(&["verify", p.to_str().unwrap()]);
    assert_eq!(v.status.code(), Some(1));
    let e = stderr(&v);
    assert!(e.contains("版本过新"), "{e}");
    assert!(e.contains("recover"), "版本拒绝必须给恢复指引: {e}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn recover_cli_salvages_and_reports() {
    let dir = tmpdir("recover");
    let src = dir.join("broken.azodoc");
    std::fs::copy(spec_example("forward-compat.azodoc"), &src).unwrap();
    let out = dir.join("out");

    let r = run(&[
        "recover",
        src.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ]);
    assert!(ok_or_dump(&r), "recover 失败: {}", stderr(&r));

    assert_eq!(
        std::fs::read(out.join("future/feature.bin")).unwrap(),
        b"FUTURE-FEATURE\x00\x01\x02\x03"
    );
    assert!(out.join("manifest.json").exists());
    assert!(out.join("recovery-report.txt").exists());
    assert!(stdout(&r).contains("中央目录"));

    // 源文件未被修改
    assert_eq!(
        std::fs::read(&src).unwrap(),
        std::fs::read(spec_example("forward-compat.azodoc")).unwrap()
    );
    std::fs::remove_dir_all(&dir).ok();
}
