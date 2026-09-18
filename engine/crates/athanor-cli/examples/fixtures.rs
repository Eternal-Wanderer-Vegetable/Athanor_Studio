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

//! E0 示例：生成 editor 增强路线图的契约夹具（corpus/editor/）。
//!
//! 用法：
//!
//! ```text
//! cargo run -p athanor-cli --example fixtures
//! ```
//!
//! 输出（全部确定性字节，ID/时间戳固定，便于 golden 断言）：
//! - asset-mixed.azodoc      内嵌 PNG + 外链图 + 孤儿资产（registry/verify 用）
//! - asset-external.azodoc   仅 external 图（离线预览降级用）
//! - legacy-data-uri.json    旧式 data: URI 的 content.json（非容器；读入/迁移契约用）
//! - table-spans.azodoc      colSpan/rowSpan/width/role/header_row（PM↔Prima/DOCX 往返用）
//! - table-irregular.azodoc  重叠 + 越界（不规则表：仅诊断，不自动重写）
//! - theme-history.azodoc    rev2 携带 theme_path/theme_sha256，rev1 仅正文（历史兼容用）
//! - layout-hints.azodoc     脚注 + presentation/layout_hints（分页/脚注用）
//!
//! 再生成：`cargo run -p athanor-cli --example fixtures`（覆盖写入，字节稳定）。

use athanor_cli::sha256_hex;
use azodoc_container::builder::pack;
use serde_json::{json, Value};
use std::path::PathBuf;

/// 与 corpus/editor/pixel.png 完全相同的字节（生成后勿改——registry.sha256 引用它）。
const PIXEL_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAAXNSR0IArs4c6QAAAARnQU1BAACxjwv8YQUAAAAJcEhZcwAADsMAAA7DAcdvqGQAAAANSURBVBhXY7Cp0DgBAAN1AaVCMuqGAAAAAElFTkSuQmCC";

const NOW: &str = "2026-09-18T00:00:00Z";
const GEN: &str = "corpus-fixtures";

/// 确定性 ULID 形态：`01JGF` + 右对齐标签（零填充），总长 26 位。
/// 标签内只用 Crockford Base32 字符（不含 I L O U）。
fn u(tag: &str) -> String {
    assert!(tag.len() <= 21, "ULID 标签过长: {tag}");
    let s = format!("01JGF{:0>21}", tag);
    debug_assert_eq!(s.len(), 26);
    s
}

fn id(prefix: &str, tag: &str) -> String {
    let s = format!("{prefix}_{}", u(tag));
    assert!(azodoc_model::id::is_valid_id(&s), "生成的 id 非法: {s}");
    s
}

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join("corpus")
        .join("editor")
}

fn pretty(v: &Value) -> Vec<u8> {
    let mut b = serde_json::to_vec_pretty(v).expect("序列化失败");
    b.push(b'\n');
    b
}

fn b64_decode(s: &str) -> Vec<u8> {
    fn val(b: u8) -> u8 {
        match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => 0,
        }
    }
    let bytes: Vec<u8> = s.bytes().filter(|b| *b != b'=').collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let v: u32 = chunk
            .iter()
            .enumerate()
            .map(|(i, c)| (val(*c) as u32) << (18 - 6 * i))
            .sum();
        out.push((v >> 16) as u8);
        if chunk.len() > 2 {
            out.push((v >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(v as u8);
        }
    }
    out
}

/// 文档级 manifest 骨架；layers 由调用方补齐。
fn manifest(doc_id: &str, title: &str, layers: Value) -> Value {
    json!({
        "azodoc": {
            "format_version": "1.0",
            "container_profile": "prefixed",
            "generator": { "name": "athanor", "version": GEN }
        },
        "document": {
            "id": doc_id,
            "schema_version": "1.0",
            "title": title,
            "language": "zh-CN",
            "created_at": NOW,
            "modified_at": NOW
        },
        "current_revision": null,
        "layers": layers,
        "compatibility": [],
        "reports": []
    })
}

fn paragraph(id: &str, text: &str) -> Value {
    json!({
        "id": id,
        "type": "paragraph",
        "content": [ { "type": "text", "text": text } ]
    })
}

fn write_fixture(dir: &std::path::Path, name: &str, bytes: &[u8]) {
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap_or_else(|e| panic!("写入 {name} 失败: {e}"));
    println!("  {name} ({} bytes)", bytes.len());
}

fn main() {
    let dir = corpus_dir();
    std::fs::create_dir_all(&dir).expect("创建 corpus/editor 失败");
    let pixel = b64_decode(PIXEL_B64);
    let pixel_sha = sha256_hex(&pixel);

    // ---- asset-mixed.azodoc -------------------------------------------------
    {
        let as_img = id("as", "A1");
        let as_ext = id("as", "A2");
        let as_orphan = id("as", "A3");
        let ext_url = "https://example.com/remote.png";
        let content = json!({
            "schema_version": "1.0",
            "content": [
                paragraph(&id("blk", "P1"), "内嵌图与外链图夹具"),
                {
                    "id": id("blk", "P2"),
                    "type": "image",
                    "asset": format!("asset://{as_img}/pixel.png"),
                    "alt": "内嵌像素图"
                },
                {
                    "id": id("blk", "P3"),
                    "type": "image",
                    "asset": format!("asset://{as_ext}/remote.png"),
                    "alt": "外链图（external，离线不可见）"
                }
            ]
        });
        let content_bytes = pretty(&content);
        let registry = json!({
            "schema_version": "1.0",
            "assets": [
                {
                    "id": as_img,
                    "filename": "pixel.png",
                    "mime": "image/png",
                    "relationship": "inline",
                    "storage": "embedded",
                    "path": format!("assets/{as_img}/pixel.png"),
                    "size": pixel.len(),
                    "sha256": pixel_sha
                },
                {
                    "id": as_ext,
                    "filename": "remote.png",
                    "mime": "image/png",
                    "relationship": "inline",
                    "storage": "external",
                    "url": ext_url,
                    "size": 0,
                    "sha256": sha256_hex(ext_url.as_bytes())
                },
                {
                    "id": as_orphan,
                    "filename": "orphan.png",
                    "mime": "image/png",
                    "relationship": "inline",
                    "storage": "embedded",
                    "path": format!("assets/{as_orphan}/orphan.png"),
                    "size": pixel.len(),
                    "sha256": pixel_sha
                }
            ]
        });
        let registry_bytes = pretty(&registry);
        let layers = json!({
            "content": { "path": "document/content.json", "sha256": sha256_hex(&content_bytes) },
            "assets": { "path": "assets/registry.json", "sha256": sha256_hex(&registry_bytes) }
        });
        let bytes = pack(vec![
            (
                "manifest.json".into(),
                pretty(&manifest(&id("doc", "D1"), "资产混合夹具", layers)),
                true,
            ),
            ("document/content.json".into(), content_bytes, false),
            ("assets/registry.json".into(), registry_bytes, false),
            (format!("assets/{as_img}/pixel.png"), pixel.clone(), true),
            (
                format!("assets/{as_orphan}/orphan.png"),
                pixel.clone(),
                true,
            ),
        ])
        .expect("pack asset-mixed");
        write_fixture(&dir, "asset-mixed.azodoc", &bytes);
    }

    // ---- asset-external.azodoc ----------------------------------------------
    {
        let as_ext = id("as", "B1");
        let ext_url = "https://example.com/offline-only.png";
        let content = json!({
            "schema_version": "1.0",
            "content": [
                paragraph(&id("blk", "Q1"), "仅外链资产夹具"),
                {
                    "id": id("blk", "Q2"),
                    "type": "image",
                    "asset": format!("asset://{as_ext}/offline-only.png"),
                    "alt": "external 图"
                }
            ]
        });
        let content_bytes = pretty(&content);
        let registry = json!({
            "schema_version": "1.0",
            "assets": [ {
                "id": as_ext,
                "filename": "offline-only.png",
                "mime": "image/png",
                "relationship": "inline",
                "storage": "external",
                "url": ext_url,
                "size": 0,
                "sha256": sha256_hex(ext_url.as_bytes())
            } ]
        });
        let registry_bytes = pretty(&registry);
        let layers = json!({
            "content": { "path": "document/content.json", "sha256": sha256_hex(&content_bytes) },
            "assets": { "path": "assets/registry.json", "sha256": sha256_hex(&registry_bytes) }
        });
        let bytes = pack(vec![
            (
                "manifest.json".into(),
                pretty(&manifest(&id("doc", "D2"), "外链资产夹具", layers)),
                true,
            ),
            ("document/content.json".into(), content_bytes, false),
            ("assets/registry.json".into(), registry_bytes, false),
        ])
        .expect("pack asset-external");
        write_fixture(&dir, "asset-external.azodoc", &bytes);
    }

    // ---- legacy-data-uri.json（非容器；旧引用形态读入/迁移契约） ----------------
    {
        let content = json!({
            "schema_version": "1.0",
            "content": [
                paragraph(&id("blk", "M1"), "旧式 data URI 夹具（打开可显示，保存时迁移）"),
                {
                    "id": id("blk", "M2"),
                    "type": "image",
                    "asset": format!("data:image/png;base64,{PIXEL_B64}"),
                    "alt": "旧 data URI 图"
                }
            ]
        });
        write_fixture(&dir, "legacy-data-uri.json", &pretty(&content));
    }

    // ---- table-spans.azodoc --------------------------------------------------
    {
        // cell 子块 id 由调用方给（全文档唯一性）
        #[allow(clippy::too_many_arguments)]
        fn cell(
            cel: &str,
            child: &str,
            col: u32,
            cs: u32,
            rs: u32,
            role: Option<&str>,
            text: &str,
        ) -> Value {
            let mut c = json!({
                "id": cel,
                "column": col,
                "children": [ paragraph(child, text) ]
            });
            if cs != 1 {
                c["colSpan"] = json!(cs);
            }
            if rs != 1 {
                c["rowSpan"] = json!(rs);
            }
            if let Some(r) = role {
                c["role"] = json!(r);
            }
            c
        }
        let content = json!({
            "schema_version": "1.0",
            "content": [
                paragraph(&id("blk", "T1"), "跨列/跨行/列宽/表头角色夹具"),
                {
                    "id": id("blk", "T2"),
                    "type": "table",
                    "header_row": true,
                    "columns": [
                        { "id": id("col", "C1"), "name": "名称", "width": 2 },
                        { "id": id("col", "C2"), "name": "数量", "width": 1 },
                        { "id": id("col", "C3"), "name": "备注", "width": 3 }
                    ],
                    "rows": [
                        { "id": id("row", "R1"), "cells": [
                            cell(&id("cel", "E1"), &id("blk", "TA"), 0, 2, 1, Some("header"), "跨两列表头"),
                            cell(&id("cel", "E2"), &id("blk", "TB"), 2, 1, 2, Some("header"), "跨两行表头")
                        ] },
                        { "id": id("row", "R2"), "cells": [
                            cell(&id("cel", "E3"), &id("blk", "TC"), 0, 1, 1, None, "甲"),
                            cell(&id("cel", "E4"), &id("blk", "TD"), 1, 1, 1, None, "10")
                        ] }
                    ]
                }
            ]
        });
        let content_bytes = pretty(&content);
        let layers = json!({
            "content": { "path": "document/content.json", "sha256": sha256_hex(&content_bytes) }
        });
        let bytes = pack(vec![
            (
                "manifest.json".into(),
                pretty(&manifest(&id("doc", "D3"), "表格跨度夹具", layers)),
                true,
            ),
            ("document/content.json".into(), content_bytes, false),
        ])
        .expect("pack table-spans");
        write_fixture(&dir, "table-spans.azodoc", &bytes);
    }

    // ---- table-irregular.azodoc ----------------------------------------------
    {
        fn cell(cel: &str, child: &str, col: u32, cs: u32, text: &str) -> Value {
            json!({
                "id": cel,
                "column": col,
                "colSpan": cs,
                "children": [ paragraph(child, text) ]
            })
        }
        let content = json!({
            "schema_version": "1.0",
            "content": [
                paragraph(&id("blk", "W1"), "不规则表夹具：重叠 + 越界（仅诊断）"),
                {
                    "id": id("blk", "W2"),
                    "type": "table",
                    "columns": [
                        { "id": id("col", "D1") },
                        { "id": id("col", "D2") }
                    ],
                    "rows": [
                        { "id": id("row", "S1"), "cells": [
                            cell(&id("cel", "F1"), &id("blk", "WA"), 0, 2, "占两列"),
                            cell(&id("cel", "F2"), &id("blk", "WB"), 1, 1, "重叠在已占格")
                        ] },
                        { "id": id("row", "S2"), "cells": [
                            cell(&id("cel", "F3"), &id("blk", "WC"), 1, 3, "越界 colSpan")
                        ] }
                    ]
                }
            ]
        });
        let content_bytes = pretty(&content);
        let layers = json!({
            "content": { "path": "document/content.json", "sha256": sha256_hex(&content_bytes) }
        });
        let bytes = pack(vec![
            (
                "manifest.json".into(),
                pretty(&manifest(&id("doc", "D4"), "不规则表夹具", layers)),
                true,
            ),
            ("document/content.json".into(), content_bytes, false),
        ])
        .expect("pack table-irregular");
        write_fixture(&dir, "table-irregular.azodoc", &bytes);
    }

    // ---- theme-history.azodoc（rev1 仅正文、rev2 带表现层快照） ------------------
    {
        let rev1 = id("rev", "V1");
        let rev2 = id("rev", "V2");
        let theme = json!({
            "schema_version": "1.0",
            "theme": "sepia",
            "defaults": {
                "page": { "size": "A4", "margin": "25mm" },
                "base_font": "Noto Serif",
                "base_size": "11pt"
            },
            "styles": [
                {
                    "name": "quote-soft",
                    "applies_to": ["quote"],
                    "css_like": { "border-left": "3px solid #c8b89a" }
                }
            ],
            "layout_hints": [
                { "block": id("blk", "H1"), "hint": "page-before" }
            ]
        });
        let theme_bytes = pretty(&theme);
        let rev1_content = json!({
            "schema_version": "1.0",
            "content": [ paragraph(&id("blk", "H1"), "修订一的正文（无主题快照）") ]
        });
        let rev2_content = json!({
            "schema_version": "1.0",
            "content": [
                paragraph(&id("blk", "H1"), "修订二的正文"),
                paragraph(&id("blk", "H2"), "主题变化后的段落")
            ]
        });
        let rev1_bytes = pretty(&rev1_content);
        let rev2_bytes = pretty(&rev2_content);
        let chain = json!({
            "schema_version": "1.0",
            "policy": { "mode": "snapshot", "trigger": ["explicit_save"] },
            "head": rev2,
            "revisions": [
                {
                    "id": rev1,
                    "parent": null,
                    "author": { "type": "human", "id": "local" },
                    "timestamp": NOW,
                    "message": "initial",
                    "kind": "snapshot",
                    "path": format!("revisions/{rev1}/content.json"),
                    "sha256": sha256_hex(&rev1_bytes)
                },
                {
                    "id": rev2,
                    "parent": rev1,
                    "author": { "type": "human", "id": "local" },
                    "timestamp": NOW,
                    "message": "theme change + paragraph",
                    "kind": "snapshot",
                    "path": format!("revisions/{rev2}/content.json"),
                    "sha256": sha256_hex(&rev2_bytes),
                    "theme_path": format!("revisions/{rev2}/theme.json"),
                    "theme_sha256": sha256_hex(&theme_bytes)
                }
            ]
        });
        let chain_bytes = pretty(&chain);
        let layers = json!({
            "content": { "path": "document/content.json", "sha256": sha256_hex(&rev2_bytes) },
            "presentation": { "path": "presentation/theme.json", "sha256": sha256_hex(&theme_bytes) },
            "revisions": { "path": "revisions/chain.json", "sha256": sha256_hex(&chain_bytes) }
        });
        let mut m = manifest(&id("doc", "D5"), "主题历史夹具", layers);
        m["current_revision"] = json!(rev2);
        let bytes = pack(vec![
            ("manifest.json".into(), pretty(&m), true),
            ("document/content.json".into(), rev2_bytes.clone(), false),
            ("presentation/theme.json".into(), theme_bytes.clone(), false),
            ("revisions/chain.json".into(), chain_bytes, false),
            (format!("revisions/{rev1}/content.json"), rev1_bytes, false),
            (format!("revisions/{rev2}/content.json"), rev2_bytes, false),
            (format!("revisions/{rev2}/theme.json"), theme_bytes, false),
        ])
        .expect("pack theme-history");
        write_fixture(&dir, "theme-history.azodoc", &bytes);
    }

    // ---- layout-hints.azodoc（脚注 + layout_hints 分页提示） --------------------
    {
        let theme = json!({
            "schema_version": "1.0",
            "layout_hints": [
                { "block": id("blk", "G2"), "hint": "page-before" },
                { "block": id("blk", "G4"), "hint": "avoid-break" }
            ]
        });
        let theme_bytes = pretty(&theme);
        let content = json!({
            "schema_version": "1.0",
            "content": [
                paragraph(&id("blk", "G1"), "分页提示与脚注夹具"),
                {
                    "id": id("blk", "G2"),
                    "type": "paragraph",
                    "content": [
                        { "type": "text", "text": "本段带 page-before 提示，并引用脚注" },
                        { "type": "footnote_ref", "id": id("blk", "G4") }
                    ]
                },
                {
                    "id": id("blk", "G4"),
                    "type": "footnote",
                    "children": [
                        paragraph(&id("blk", "G3"), "脚注正文（avoid-break）")
                    ]
                }
            ]
        });
        let content_bytes = pretty(&content);
        let layers = json!({
            "content": { "path": "document/content.json", "sha256": sha256_hex(&content_bytes) },
            "presentation": { "path": "presentation/theme.json", "sha256": sha256_hex(&theme_bytes) }
        });
        let bytes = pack(vec![
            (
                "manifest.json".into(),
                pretty(&manifest(&id("doc", "D6"), "分页与脚注夹具", layers)),
                true,
            ),
            ("document/content.json".into(), content_bytes, false),
            ("presentation/theme.json".into(), theme_bytes, false),
        ])
        .expect("pack layout-hints");
        write_fixture(&dir, "layout-hints.azodoc", &bytes);
    }

    println!("corpus/editor 夹具已生成（确定性字节）");
}
