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

//! A2 验收（Future Work §A2）：FakeProvider 跑通"标注 + AI 落链 + 可审计 +
//! 人工回滚"全流程。离线确定性，CI 可跑。

use std::path::PathBuf;

fn corpus(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../corpus")
        .join(name)
}

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("athanor-a2-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn import_doc(dir: &std::path::Path, tag: &str) -> PathBuf {
    let doc = dir.join(format!("{tag}.azodoc"));
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
    doc
}

#[test]
fn ai_pipeline_annotations_and_commit_are_auditable() {
    let dir = tmpdir("audit");
    let doc = import_doc(&dir, "d");

    let result = athanor_cli::ai_demo::run(&doc, &athanor_cli::ai_demo::FakeProvider::default())
        .expect("AI 管线应成功");
    assert!(result.annotations_added >= 1, "至少落 1 条 AI 标注");
    assert_eq!(result.rewrites_applied, 1, "FakeProvider 恰好 1 条改写");
    assert!(result.revision.is_some(), "改写应产生 AI 修订");
    assert_eq!(result.model, "fake-demo");

    // 容器内断言：AI 标注带 confidence / source / author
    let data = std::fs::read(&doc).unwrap();
    let (mut c, _) = azodoc_container::open(data).unwrap();
    let anns: serde_json::Value = serde_json::from_slice(
        &c.read_entry("semantics/annotations.json")
            .expect("标注层应存在"),
    )
    .unwrap();
    let list = anns["annotations"].as_array().unwrap();
    assert_eq!(list.len(), result.annotations_added as usize);
    for a in list {
        assert_eq!(a["author"]["type"], "ai");
        assert_eq!(a["author"]["id"], "fake-demo");
        assert!(a["confidence"].is_number(), "AI 标注必须带 confidence");
        assert_eq!(a["source"], "ai_pipeline");
    }

    // 修订链：head 由 AI 落链，可 history 审计
    let chain = c.chain().unwrap().expect("修订链应存在");
    let head = chain["head"].as_str().unwrap().to_string();
    let head_entry = chain["revisions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"].as_str() == Some(&head))
        .unwrap()
        .clone();
    assert_eq!(head_entry["author"]["type"], "ai");
    assert_eq!(result.revision.as_deref(), Some(&*head));
    drop(c);

    // 落链后容器仍通过校验
    assert_eq!(athanor_cli::verify_cmd::run(&doc), 0);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn ai_rewrite_can_be_rolled_back_via_checkout() {
    let dir = tmpdir("rollback");
    let doc = import_doc(&dir, "d");

    // 人工基线修订（AI 改写前的还原点）
    assert_eq!(athanor_cli::cmd_commit(&doc, "human:tester", "人工基线"), 0);
    let human_rev = {
        let data = std::fs::read(&doc).unwrap();
        let (c, _) = azodoc_container::open(data).unwrap();
        c.manifest_typed().current_revision.clone().unwrap()
    };

    let result = athanor_cli::ai_demo::run(&doc, &athanor_cli::ai_demo::FakeProvider::default())
        .expect("AI 管线应成功");
    assert!(result.revision.is_some(), "AI 修订应存在");

    // AI 改写已生效：内容含演示标记
    let marked = |doc: &PathBuf| {
        let data = std::fs::read(doc).unwrap();
        let (mut c, _) = azodoc_container::open(data).unwrap();
        let content: serde_json::Value =
            serde_json::from_slice(&c.read_entry("document/content.json").unwrap()).unwrap();
        let s = serde_json::to_string(&content).unwrap();
        s.contains("AI 润色演示")
    };
    assert!(marked(&doc), "AI 改写应已写入当前内容");

    // 人工否决：checkout 回人工基线（标注自动重定位）
    assert_eq!(athanor_cli::cmd_checkout(&doc, &human_rev, None), 0);
    assert!(!marked(&doc), "回滚后 AI 改写标记应消失");

    // 回滚后容器仍通过校验
    assert_eq!(athanor_cli::verify_cmd::run(&doc), 0);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn hallucinated_block_ids_are_dropped() {
    let dir = tmpdir("hallucination");
    let doc = import_doc(&dir, "d");

    // 构造引用不存在块 ID 的 LLM 应答：必须被整体丢弃，不落链
    use athanor_cli::ai_demo::{AiError, AiProvider};
    struct HallucinatingProvider;
    impl AiProvider for HallucinatingProvider {
        fn model(&self) -> &str {
            "hallucinator"
        }
        fn complete_json(&self, _system: &str, _user: &str) -> Result<serde_json::Value, AiError> {
            Ok(serde_json::json!({
                "annotations": [
                    {"block": "blk_DOES_NOT_EXIST", "type": "Concept",
                     "value": {"label": "幻觉"}, "confidence": 0.99}
                ],
                "rewrites": [
                    {"block": "blk_ALSO_MISSING", "new_text": "幻觉改写", "reason": "幻觉"}
                ]
            }))
        }
    }

    let result = athanor_cli::ai_demo::run(&doc, &HallucinatingProvider)
        .expect("幻觉应答应被安全处理而非报错");
    assert_eq!(result.annotations_added, 0, "幻觉块 ID 必须被丢弃");
    assert_eq!(result.rewrites_applied, 0);
    assert!(result.revision.is_none(), "无有效改写则不落链");
    assert_eq!(athanor_cli::verify_cmd::run(&doc), 0);
    std::fs::remove_dir_all(&dir).ok();
}
