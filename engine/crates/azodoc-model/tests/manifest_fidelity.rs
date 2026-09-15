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

//! R2 前向兼容：typed Manifest 解析→再序列化不得丢失未知字段与键序语义。

use azodoc_model::manifest::{CompatEntry, Manifest};
use serde_json::{json, Value};

#[test]
fn unknown_fields_survive_typed_roundtrip() {
    let original = json!({
        "azodoc": {
            "format_version": "1.0",
            "container_profile": "prefixed",
            "vendor_note": { "hint": "未知字段" }
        },
        "document": {
            "id": "doc_01JAVG2Z7Q4M8XK3N5P9R2T6W8",
            "schema_version": "1.0",
            "title": "夹具",
            "future_flag": true
        },
        "current_revision": null,
        "layers": {
            "content": { "path": "document/content.json", "sha256": "00".repeat(32) },
            "ai_state": { "path": "ai/state.json", "sha256": "11".repeat(32) }
        },
        "compatibility": [
            {
                "format": "text",
                "path": "compatibility/text/document.txt",
                "revision": null,
                "status": "fresh",
                "custom_meta": 42
            }
        ],
        "provenance": { "chain": ["html", "azodoc"] }
    });

    let manifest: Manifest = serde_json::from_value(original.clone()).expect("typed 解析失败");
    let round: Value = serde_json::to_value(&manifest).expect("序列化失败");

    assert_eq!(
        round["azodoc"]["vendor_note"],
        original["azodoc"]["vendor_note"]
    );
    assert_eq!(round["document"]["future_flag"], json!(true));
    assert_eq!(round["layers"]["ai_state"]["path"], "ai/state.json");
    assert_eq!(round["compatibility"][0]["custom_meta"], json!(42));
    assert_eq!(round["provenance"], original["provenance"]);
    // 已知字段不丢
    assert_eq!(round["azodoc"]["format_version"], "1.0");
    assert_eq!(round["document"]["title"], "夹具");
}

#[test]
fn compat_freshness_by_revision_binding() {
    let entry = |revision: Value| CompatEntry {
        format: "text".into(),
        path: "compatibility/text/document.txt".into(),
        revision: serde_json::from_value(revision).unwrap(),
        generated_at: None,
        generator: None,
        status: None,
        extra: Default::default(),
    };
    let mut m = manifest_with_revision(Some("rev_01JAVG3A9B2C4D6E8F0H1J3K5M7".into()));
    assert!(m.compat_is_fresh(&entry(json!("rev_01JAVG3A9B2C4D6E8F0H1J3K5M7"))));
    assert!(!m.compat_is_fresh(&entry(json!("rev_01JAVG3QK2M4N6P8R0T2V4X6Z8"))));
    // 无修订文档：null 缓存视为 fresh
    m.current_revision = None;
    assert!(m.compat_is_fresh(&entry(json!(null))));
    // null 缓存 + 有修订 → 不匹配
    m.current_revision = Some("rev_01JAVG3A9B2C4D6E8F0H1J3K5M7".into());
    assert!(!m.compat_is_fresh(&entry(json!(null))));
}

fn manifest_with_revision(rev: Option<String>) -> Manifest {
    let value = json!({
        "azodoc": { "format_version": "1.0" },
        "document": {
            "id": "doc_01JAVG2Z7Q4M8XK3N5P9R2T6W8",
            "schema_version": "1.0"
        },
        "current_revision": rev,
        "layers": { "content": { "path": "document/content.json", "sha256": "00".repeat(32) } },
        "compatibility": []
    });
    serde_json::from_value(value).unwrap()
}
