// Copyright (C) 2026 The Athanor Studio Developers
// SPDX-License-Identifier: AGPL-3.0-only

//! SessionRegistry / recovery / recent 的集成测试（不经 Tauri IPC 层）。
//!
//! 覆盖：新建草稿 → 另存为换绑；同路径打开去重；外部改动冲突拒绝；
//! 恢复草稿写读删；最近列表。

use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use studio::recent;
use studio::recovery;
use studio::sessions::SessionRegistry;

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("athanor-studio-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn make_doc(dir: &Path, name: &str) -> PathBuf {
    let doc = dir.join(name);
    assert_eq!(
        athanor_cli::cmd_new(&doc, Some("测试文档"), "zh-CN"),
        0,
        "新建文档失败"
    );
    doc
}

fn first_block_text(v: &Value) -> Value {
    let mut pm = v.clone();
    pm["content"][0]["content"] = json!([{ "type": "text", "text": "改写后的标题" }]);
    pm
}

#[test]
fn new_document_creates_draft_session() {
    let reg = SessionRegistry::default();
    let (id, doc) = reg.new_document(Some("草稿")).unwrap();
    assert!(id.starts_with("doc_"), "草稿会话键: {id}");
    assert_eq!(doc["pm_doc"]["type"], "doc");
    assert!(doc["path"].is_null(), "草稿没有路径");
    assert!(doc["fingerprint"].is_string());
    // 草稿保存只更新内存，不落盘
    let pm = doc["pm_doc"].clone();
    let resp = reg
        .save(&id, &json!({ "pm_doc": pm, "message": "草稿保存" }))
        .unwrap();
    assert_eq!(resp["draft"], true);
    assert!(resp["revision"].as_str().unwrap().starts_with("rev_"));
}

#[test]
fn open_dedupes_same_path_and_save_as_rebinds() {
    let dir = tmpdir("dedupe");
    let doc = make_doc(&dir, "a.azodoc");
    let reg = SessionRegistry::default();

    let (id1, _) = reg.open_path(&doc).unwrap();
    let (id2, _) = reg.open_path(&doc).unwrap();
    assert_eq!(id1, id2, "同一路径必须复用同一会话");
    assert_eq!(reg.session_count(), 1);

    // 另存为：会话换绑到新路径，原路径键释放
    let target = dir.join("b.azodoc");
    let session = reg.session(&id1).unwrap();
    let opened = session.open().unwrap();
    let pm = first_block_text(&opened["pm_doc"]);
    let (new_id, resp) = reg
        .save_as(&id1, target.clone(), &json!({ "pm_doc": pm }), false)
        .unwrap();
    assert_eq!(resp["saved_as"], true);
    assert!(target.is_file(), "另存目标必须真实落盘");
    assert_ne!(id1, new_id);
    assert!(reg.session(&id1).is_err(), "旧会话键应失效");
    assert_eq!(reg.session(&new_id).unwrap().doc_path().unwrap(), target);
}

#[test]
fn save_as_refuses_existing_target_without_overwrite() {
    let dir = tmpdir("saveas");
    let a = make_doc(&dir, "a.azodoc");
    let _b = make_doc(&dir, "b.azodoc");
    let before = std::fs::read(dir.join("b.azodoc")).unwrap();
    let reg = SessionRegistry::default();
    let (id, _) = reg.open_path(&a).unwrap();
    let session = reg.session(&id).unwrap();
    let pm = session.open().unwrap()["pm_doc"].clone();
    let err = reg
        .save_as(&id, dir.join("b.azodoc"), &json!({ "pm_doc": pm }), false)
        .expect_err("不覆盖已存在目标");
    assert_eq!(err.code, "target_exists");
    assert_eq!(std::fs::read(dir.join("b.azodoc")).unwrap(), before);
    // 会话仍绑定原路径
    assert_eq!(reg.session(&id).unwrap().doc_path().unwrap(), a);
}

#[test]
fn save_detects_external_modification() {
    let dir = tmpdir("conflict");
    let doc = make_doc(&dir, "c.azodoc");
    let reg = SessionRegistry::default();
    let (id, opened) = reg.open_path(&doc).unwrap();
    let fingerprint = opened["fingerprint"].as_str().unwrap().to_string();
    let pm = opened["pm_doc"].clone();

    // 外部进程改写文件
    std::fs::write(&doc, b"corrupted by external writer").unwrap();
    let err = reg
        .save(
            &id,
            &json!({ "pm_doc": pm, "expected_fingerprint": fingerprint }),
        )
        .expect_err("指纹不匹配必须拒绝");
    assert_eq!(err.code, "document_error");
    assert!(
        err.message.contains("外部"),
        "错误应说明外部修改: {}",
        err.message
    );
}

#[test]
fn recovery_draft_lifecycle() {
    let dir = tmpdir("recovery");
    let draft = json!({
        "session_path": "D:/docs/report.azodoc",
        "generation": 7,
        "saved_at": "2026-09-18T00:00:00Z",
        "pm_doc": { "type": "doc", "content": [] }
    });
    let file = recovery::write_draft(&dir, "D:/docs/report.azodoc", &draft).unwrap();
    assert!(file.is_file());

    let list = recovery::list_drafts(&dir).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].generation, 7);
    assert_eq!(
        list[0].session_path.as_deref(),
        Some("D:/docs/report.azodoc")
    );

    let read = recovery::read_draft(&dir, &list[0].file).unwrap();
    assert_eq!(read["pm_doc"]["type"], "doc");

    recovery::delete_draft(&dir, "D:/docs/report.azodoc").unwrap();
    assert!(recovery::list_drafts(&dir).unwrap().is_empty());
    // 幂等删除
    recovery::delete_draft(&dir, "D:/docs/report.azodoc").unwrap();
}

#[test]
fn recent_list_dedupes_and_truncates() {
    let dir = tmpdir("recent");
    for i in 0..25 {
        recent::touch(&dir, &PathBuf::from(format!("D:/doc/{i}.azodoc")));
    }
    let items = recent::list(&dir);
    assert_eq!(items.len(), 20);
    assert_eq!(items[0], "D:/doc/24.azodoc");
    recent::touch(&dir, &PathBuf::from("D:/doc/24.azodoc"));
    recent::touch(&dir, &PathBuf::from("D:/doc/5.azodoc"));
    let items = recent::list(&dir);
    assert_eq!(items[0], "D:/doc/5.azodoc");
    assert_eq!(items.iter().filter(|p| *p == "D:/doc/5.azodoc").count(), 1);
    recent::remove(&dir, "D:/doc/5.azodoc").unwrap();
    assert!(!recent::list(&dir).contains(&"D:/doc/5.azodoc".to_string()));
}
