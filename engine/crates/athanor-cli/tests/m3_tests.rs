//! M3 端到端：修订链（importer/AI/人工作者）+ checkout 哈希一致 + 标注读写与重定位。

use athanor_cli::{cmd_annotate, cmd_checkout, cmd_commit, cmd_history, AnnotateArgs};
use std::path::{Path, PathBuf};

fn corpus(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../corpus")
        .join(name)
}

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("athanor-m3-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn read_container(p: &Path) -> azodoc_container::Container {
    let data = std::fs::read(p).unwrap();
    azodoc_container::open(data).unwrap().0
}

fn sha(data: &[u8]) -> String {
    use sha2::Digest;
    let d = sha2::Sha256::digest(data);
    d.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn import_creates_initial_revision() {
    let dir = tmpdir("import");
    let doc = dir.join("d.azodoc");
    assert_eq!(
        athanor_cli::cmd_import(
            &corpus("markdown/basic.md"),
            &doc,
            None,
            "zh-CN",
            None,
            false
        ),
        0
    );
    // history 应显示 importer 落的初始修订
    let mut c = read_container(&doc);
    let h = c.history().unwrap();
    assert_eq!(h.len(), 1);
    assert_eq!(h[0].author_type, "importer");
    assert!(h[0].is_head && h[0].is_current);
    // manifest.current_revision 已推进
    assert_eq!(
        c.manifest_typed().current_revision.as_deref(),
        Some(h[0].id.as_str())
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn ai_and_human_commits_land_on_chain() {
    let dir = tmpdir("commits");
    let doc = dir.join("d.azodoc");
    assert_eq!(
        athanor_cli::cmd_import(
            &corpus("markdown/basic.md"),
            &doc,
            None,
            "zh-CN",
            None,
            false
        ),
        0
    );

    // AI 作者提交
    assert_eq!(cmd_commit(&doc, "ai:model-x", "AI 批量改写"), 0);
    // 人工作者提交
    assert_eq!(cmd_commit(&doc, "human:vegetable", "手工润色"), 0);

    let mut c = read_container(&doc);
    let h = c.history().unwrap();
    assert_eq!(h.len(), 3);
    assert_eq!(h[1].author_type, "ai");
    assert_eq!(h[1].author_id.as_deref(), Some("model-x"));
    assert_eq!(h[2].author_type, "human");
    assert_eq!(h[2].author_id.as_deref(), Some("vegetable"));
    // 链式 parent
    assert_eq!(h[2].parent.as_deref(), Some(h[1].id.as_str()));
    assert_eq!(h[1].parent.as_deref(), Some(h[0].id.as_str()));
    assert_eq!(
        athanor_cli::verify_cmd::run(&doc),
        0,
        "落链后 verify 应通过"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn checkout_restores_snapshot_hash_and_ids() {
    let dir = tmpdir("checkout");
    let doc = dir.join("d.azodoc");
    assert_eq!(
        athanor_cli::cmd_import(
            &corpus("markdown/basic.md"),
            &doc,
            None,
            "zh-CN",
            None,
            false
        ),
        0
    );
    // 记录初始修订与 content 哈希
    let mut c = read_container(&doc);
    let rev_a = c.history().unwrap()[0].id.clone();
    drop(c);

    // 编辑 + 人工提交（内容变化）
    {
        let mut c = read_container(&doc);
        let mut content: serde_json::Value =
            serde_json::from_slice(&c.read_entry("document/content.json").unwrap()).unwrap();
        content["content"].as_array_mut().unwrap()[0]["children"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "id": "blk_10000000000000000000000000",
                "type": "paragraph",
                "content": [{"type": "text", "text": "新增段落。"}]
            }));
        let mut bytes = serde_json::to_vec_pretty(&content).unwrap();
        bytes.push(b'\n');
        c.set_entry("document/content.json", bytes).unwrap();
        std::fs::write(&doc, c.write().unwrap()).unwrap();
    }
    assert_eq!(cmd_commit(&doc, "human:vegetable", "第二版"), 0);

    // checkout 回初始修订
    assert_eq!(cmd_checkout(&doc, &rev_a, None), 0);

    // 验收：content 与快照哈希一致
    let mut c = read_container(&doc);
    let content_now = c.read_entry("document/content.json").unwrap();
    let snapshot = c
        .read_entry(&format!("revisions/{rev_a}/content.json"))
        .unwrap();
    assert_eq!(sha(&content_now), sha(&snapshot));
    // current = rev_a，head 在第二版
    assert_eq!(
        c.manifest_typed().current_revision.as_deref(),
        Some(rev_a.as_str())
    );
    assert_eq!(
        athanor_cli::verify_cmd::run(&doc),
        0,
        "checkout 后 verify 应通过"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn annotate_and_relocate_flow() {
    let dir = tmpdir("annotate");
    let doc = dir.join("d.azodoc");
    assert_eq!(
        athanor_cli::cmd_import(
            &corpus("markdown/basic.md"),
            &doc,
            None,
            "zh-CN",
            None,
            false
        ),
        0
    );

    // 找一个真实存在的块：heading 块（标题「基础语料」）
    let mut c = read_container(&doc);
    let content: serde_json::Value =
        serde_json::from_slice(&c.read_entry("document/content.json").unwrap()).unwrap();
    let heading_id =
        find_heading_with(&content, "基础语料").expect("语料中应存在含「基础语料」的标题块");
    let quote = "基础语料";
    drop(c);

    // 添加 text_quote 标注
    let args = AnnotateArgs {
        block: &heading_id,
        ann_type: "Concept",
        value_json: r#"{"label": "标题概念"}"#,
        exact: Some(quote),
        prefix: Some(""),
        suffix: Some(""),
        confidence: Some(0.99),
        author: Some("ai:model-x"),
        source: Some("athanor-test"),
    };
    assert_eq!(cmd_annotate(&doc, &args), 0);
    assert_eq!(athanor_cli::verify_cmd::run(&doc), 0);

    // checkout 到更早状态不会破坏标注（快照内容一致 → unchanged）
    let mut c = read_container(&doc);
    let rev_a = c.history().unwrap()[0].id.clone();
    drop(c);
    assert_eq!(cmd_checkout(&doc, &rev_a, None), 0);

    let mut c = read_container(&doc);
    let anns: serde_json::Value =
        serde_json::from_slice(&c.read_entry("semantics/annotations.json").unwrap()).unwrap();
    assert_eq!(anns["annotations"].as_array().unwrap().len(), 1);
    assert!(
        anns["annotations"][0].get("detached").is_none(),
        "标注应存活"
    );
    std::fs::remove_dir_all(&dir).ok();
}

fn find_heading_with(content: &serde_json::Value, needle: &str) -> Option<String> {
    fn walk(v: &serde_json::Value, needle: &str) -> Option<String> {
        if let Some(obj) = v.as_object() {
            if obj.get("type").and_then(|t| t.as_str()) == Some("heading") {
                let text: String = obj
                    .get("content")
                    .and_then(|c| c.as_array())
                    .map(|spans| {
                        spans
                            .iter()
                            .filter_map(|s| s.get("text").and_then(|t| t.as_str()))
                            .collect()
                    })
                    .unwrap_or_default();
                if text.contains(needle) {
                    return obj.get("id").and_then(|i| i.as_str()).map(String::from);
                }
            }
            for val in obj.values() {
                if let Some(found) = walk(val, needle) {
                    return Some(found);
                }
            }
        } else if let Some(arr) = v.as_array() {
            for item in arr {
                if let Some(found) = walk(item, needle) {
                    return Some(found);
                }
            }
        }
        None
    }
    walk(content, needle)
}

#[test]
fn history_and_checkout_missing_revision_errors() {
    let dir = tmpdir("missing");
    let doc = dir.join("d.azodoc");
    assert_eq!(
        athanor_cli::cmd_import(
            &corpus("markdown/basic.md"),
            &doc,
            None,
            "zh-CN",
            None,
            false
        ),
        0
    );
    assert_eq!(
        cmd_checkout(&doc, "rev_10000000000000000000000000", None),
        1
    );
    assert_eq!(
        cmd_commit(&doc, "robot:bad-type", "x"),
        1,
        "非法作者类型应被拒绝"
    );
    std::fs::remove_dir_all(&dir).ok();
}
