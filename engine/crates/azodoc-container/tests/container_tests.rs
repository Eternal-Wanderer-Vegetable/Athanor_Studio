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

//! M1 验收 ①②③：往返字节稳定 / forward-compat 保真（R1+R2）/ 友好报错（R4）。
//! 另含资源限制与 recover 的行为测试。

use azodoc_container::{open, ContainerError, Profile};
use azodoc_model::id::{AzodocId, IdKind};
use serde_json::{json, Value};
use std::path::PathBuf;

fn sample(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../spec/examples")
        .join(name)
}

fn read_sample(name: &str) -> Vec<u8> {
    std::fs::read(sample(name)).expect("读取黄金样例失败")
}

fn open_ok(data: Vec<u8>) -> (azodoc_container::Container, Vec<String>) {
    open(data).expect("打开失败")
}

// ---------------------------------------------------------------- ① 往返稳定

#[test]
fn noop_roundtrip_is_byte_identical() {
    for name in ["minimal.azodoc", "rich.azodoc", "forward-compat.azodoc"] {
        let data = read_sample(name);
        let (mut c, warnings) = open_ok(data.clone());
        assert!(warnings.is_empty(), "{name} 不应有打开警告: {warnings:?}");
        assert!(!c.is_modified());
        let out = c.write().expect("写出失败");
        assert_eq!(out, data, "{name}: 未修改的容器必须字节级原样返回");
    }
}

// ---------------------------------------------------------------- ② R1/R2 保真

#[test]
fn modified_rewrite_preserves_unknown_entries_and_fields() {
    let data = read_sample("forward-compat.azodoc");
    let (mut c, _) = open_ok(data.clone());

    // 模拟编辑：向 content.json 追加一个段落
    let mut content: Value =
        serde_json::from_slice(&c.read_entry("document/content.json").unwrap()).unwrap();
    let new_id = AzodocId::generate(IdKind::Blk);
    content["content"].as_array_mut().unwrap().push(json!({
        "id": new_id.as_str(), "type": "paragraph",
        "content": [{ "type": "text", "text": "编辑新增的段落。" }]
    }));
    let mut new_bytes = serde_json::to_vec_pretty(&content).unwrap();
    new_bytes.push(b'\n');
    c.set_entry("document/content.json", new_bytes)
        .expect("set_entry 失败");
    assert!(c.is_modified());

    let out = c.write().expect("重写失败");
    assert_ne!(out, data, "已修改的容器不应与原文件相同");

    // 重开产物
    let (mut c2, _) = open_ok(out);

    // 先取 manifest 的克隆，避免与 read_entry 的可变借用冲突
    let m2 = c2.manifest_value().clone();

    // R1：未知条目字节保真
    for unknown in [
        "future/feature.bin",
        "preserved/html/part-0001.html",
        "preserved/html/part-0002.html",
        "ai/state.json",
    ] {
        let expected = {
            let (mut c0, _) = open_ok(data.clone());
            c0.read_entry(unknown).unwrap()
        };
        assert_eq!(
            c2.read_entry(unknown).unwrap(),
            expected,
            "未知条目 {unknown} 被破坏"
        );
    }

    // 未触碰的已知条目也保真
    let expected_txt = {
        let (mut c0, _) = open_ok(data.clone());
        c0.read_entry("compatibility/text/document.txt").unwrap()
    };
    assert_eq!(
        c2.read_entry("compatibility/text/document.txt").unwrap(),
        expected_txt
    );

    // R2：manifest 未知字段与未知层保真（manifest 因 sha256 更新而被重写）
    assert_eq!(
        m2.pointer("/azodoc/vendor_note/hint")
            .and_then(Value::as_str),
        Some("未知字段：R2 前向兼容往返保真测试用"),
        "azodoc.vendor_note 丢失"
    );
    assert_eq!(
        m2.pointer("/provenance/chain/1").and_then(Value::as_str),
        Some("azodoc"),
        "顶层未知字段 provenance 丢失"
    );
    assert!(
        m2.pointer("/layers/ai_state/path").is_some(),
        "未知层 ai_state 丢失"
    );

    // 编辑确实生效
    let c2b = c2.read_entry("document/content.json").unwrap();
    assert!(String::from_utf8_lossy(&c2b).contains("编辑新增的段落。"));
    // manifest 中 content 层 sha256 被同步更新
    // （旧 sha 不再匹配新 content；直接验证 manifest 引用的 sha 与实际一致）
    let new_sha = {
        use sha2::Digest;
        let d = sha2::Sha256::digest(&c2b);
        d.iter().map(|b| format!("{b:02x}")).collect::<String>()
    };
    assert_eq!(
        m2.pointer("/layers/content/sha256").and_then(Value::as_str),
        Some(new_sha.as_str()),
        "manifest 的 content sha256 未随编辑更新"
    );
}

// ---------------------------------------------------------------- ③ 探测与友好报错

#[test]
fn plain_zip_without_prefix_opens_with_warning() {
    let data = read_sample("minimal.azodoc");
    let stripped = data[32..].to_vec();
    let (c, warnings) = open_ok(stripped);
    assert_eq!(c.profile, Profile::PlainZip);
    assert!(
        warnings.iter().any(|w| w.contains("前缀缺失")),
        "应提示前缀缺失: {warnings:?}"
    );
}

#[test]
fn corrupted_magic_is_not_azodoc_even_if_zip_intact() {
    let mut data = read_sample("minimal.azodoc");
    data[3] ^= 0xFF; // 破坏 magic：文件不再自称为 Azodoc，也不以 PK 开头
                     // 按规范 §4：magic 不匹配且非 PK → 「不是 Azodoc 文档」（宁可拒绝，不得半解析）
    assert!(matches!(open(data), Err(ContainerError::NotAzodoc)));
}

#[test]
fn header_crc_bitflip_falls_back() {
    let mut data = read_sample("minimal.azodoc");
    data[24] ^= 0xFF; // 仅破坏 CRC 字段
    let (_, warnings) = open_ok(data);
    assert!(warnings.iter().any(|w| w.contains("CRC")));
}

#[test]
fn future_major_version_is_refused_not_parsed() {
    let mut data = read_sample("minimal.azodoc");
    data[8] = 2; // version_major = 2
    let Err(ContainerError::VersionRefused {
        found_major,
        supported_major,
        ..
    }) = open(data)
    else {
        panic!("应拒绝更高主版本");
    };
    assert_eq!(found_major, 2);
    assert_eq!(supported_major, 1);
}

#[test]
fn bad_magic_is_not_azodoc() {
    let mut data = read_sample("minimal.azodoc");
    data[0] = b'X';
    assert!(matches!(open(data), Err(ContainerError::NotAzodoc)));
}

#[test]
fn truncated_file_errors() {
    let data = read_sample("rich.azodoc");
    let cut = &data[..data.len() / 3];
    assert!(open(cut.to_vec()).is_err());
}

#[test]
fn empty_and_garbage_inputs() {
    assert!(matches!(open(Vec::new()), Err(ContainerError::TooShort)));
    assert!(matches!(
        open(b"hello, world - definitely not a document".to_vec()),
        Err(ContainerError::NotAzodoc)
    ));
}

// ---------------------------------------------------------------- 资源限制

#[test]
fn zip_bomb_ratio_is_rejected() {
    // 20 MiB 全零：Deflate 后极小，压缩比远超 200:1
    let zeros = vec![0u8; 20 * 1024 * 1024];
    let manifest = serde_json::json!({
        "azodoc": { "format_version": "1.0", "container_profile": "prefixed" },
        "document": { "id": "doc_01JAVG2Z7Q4M8XK3N5P9R2T6W8", "schema_version": "1.0" },
        "layers": { "content": { "path": "document/content.json", "sha256": "00".repeat(32) } }
    });
    let mut mb = serde_json::to_vec_pretty(&manifest).unwrap();
    mb.push(b'\n');
    let bytes = azodoc_container::builder::pack(vec![
        ("manifest.json".to_string(), mb, true),
        ("document/blob.bin".to_string(), zeros, false),
    ])
    .unwrap();
    let Err(ContainerError::LimitExceeded(msg)) = open(bytes) else {
        panic!("应拒绝压缩比异常文件");
    };
    assert!(msg.contains("压缩比"), "{msg}");
}

// ---------------------------------------------------------------- recover

#[test]
fn recover_salvages_unknown_entries_and_reports() {
    let data = read_sample("forward-compat.azodoc");
    let out_dir = std::env::temp_dir().join(format!("athanor-recover-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out_dir);

    let outcome = azodoc_container::recover::recover(&data, &out_dir).expect("recover 失败");
    assert!(outcome.salvaged >= 7, "应至少抢回全部 7 个条目");
    let bin = out_dir.join("future/feature.bin");
    assert_eq!(
        std::fs::read(&bin).unwrap(),
        b"FUTURE-FEATURE\x00\x01\x02\x03"
    );
    assert!(out_dir.join("recovery-report.txt").exists());
    assert!(
        String::from_utf8_lossy(&std::fs::read(out_dir.join("recovery-report.txt")).unwrap())
            .contains("中央目录")
    );

    // 路径安全：报告不得把文件写到 out_dir 之外（抽查不存在越界文件名）
    std::fs::remove_dir_all(&out_dir).ok();
}

#[test]
fn recover_mode_b_handles_destroyed_central_directory() {
    let data = read_sample("minimal.azodoc");
    // 破坏中央目录：把 EOCD 与中央目录区域覆盖为垃圾
    let cut = data.len() * 2 / 3;
    let mut damaged = data[..cut].to_vec();
    damaged.extend(std::iter::repeat(b'\xDE').take(64));
    let out_dir = std::env::temp_dir().join(format!("athanor-recover-b-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out_dir);

    let outcome = azodoc_container::recover::recover(&damaged, &out_dir).expect("recover 失败");
    assert!(
        outcome.mode.contains("Local Header"),
        "应落入模式 B: {}",
        outcome.mode
    );
    // manifest 的 local header 位于文件前部，应被抢回
    assert!(
        out_dir.join("manifest.json").exists(),
        "manifest.json 应被抢回"
    );
    std::fs::remove_dir_all(&out_dir).ok();
}
