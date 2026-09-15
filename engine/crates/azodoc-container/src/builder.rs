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

//! 从零构建容器（`athanor new` 用）。确定性时间戳 = 1980-01-01（zip 默认）。

use crate::header::Header;
use crate::ContainerError;
use azodoc_model::id::{AzodocId, IdKind};
use serde_json::{json, Value};
use std::io::{Cursor, Write};
use zip::ZipWriter;

/// 构建一个最小合法文档（manifest + content + TXT 兼容缓存）。
pub fn build_minimal_document(title: &str, language: &str) -> Result<Vec<u8>, ContainerError> {
    let doc_id = AzodocId::generate(IdKind::Doc);
    let h1 = AzodocId::generate(IdKind::Blk);
    let p1 = AzodocId::generate(IdKind::Blk);
    let now = rfc3339_now();
    let generator = json!({ "name": "athanor", "version": env!("CARGO_PKG_VERSION") });

    let content = json!({
        "schema_version": "1.0",
        "content": [
            { "id": h1.as_str(), "type": "heading", "level": 1,
              "content": [ { "type": "text", "text": title } ] },
            { "id": p1.as_str(), "type": "paragraph",
              "content": [ { "type": "text", "text": "此文档由 athanor new 创建。" } ] }
        ]
    });
    let content_bytes = pretty_json(&content)?;

    let underline: String = "=".repeat(title.chars().count().max(1));
    let txt = format!("{title}\n{underline}\n\n此文档由 athanor new 创建。\n");

    let manifest = json!({
        "azodoc": {
            "format_version": "1.0",
            "container_profile": "prefixed",
            "generator": generator
        },
        "document": {
            "id": doc_id.as_str(),
            "schema_version": "1.0",
            "title": title,
            "language": language,
            "created_at": now,
            "modified_at": now
        },
        "current_revision": null,
        "layers": {
            "content": { "path": "document/content.json", "sha256": sha256_hex(&content_bytes) }
        },
        "compatibility": [ {
            "format": "text",
            "path": "compatibility/text/document.txt",
            "revision": null,
            "generated_at": now,
            "generator": generator,
            "status": "fresh"
        } ],
        "reports": []
    });
    let manifest_bytes = pretty_json(&manifest)?;
    let txt_bytes = txt.into_bytes();

    pack(vec![
        ("manifest.json".to_string(), manifest_bytes, true),
        ("document/content.json".to_string(), content_bytes, false),
        (
            "compatibility/text/document.txt".to_string(),
            txt_bytes,
            false,
        ),
    ])
}

/// Header + ZIP 组装（新容器；manifest 必须是第一个条目且 STORED）。
pub fn pack(entries: Vec<(String, Vec<u8>, bool)>) -> Result<Vec<u8>, ContainerError> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data, stored) in entries {
        let method = if stored {
            zip::CompressionMethod::Stored
        } else {
            zip::CompressionMethod::Deflated
        };
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(method)
            .unix_permissions(0o644);
        writer
            .start_file(&name, opts)
            .map_err(|e| ContainerError::Zip(e.to_string()))?;
        writer.write_all(&data).map_err(ContainerError::Io)?;
    }
    let zip_bytes = writer
        .finish()
        .map_err(|e| ContainerError::Zip(e.to_string()))?
        .into_inner();
    let mut out = Vec::with_capacity(crate::header::HEADER_SIZE + zip_bytes.len());
    out.extend_from_slice(&Header::v1().to_bytes());
    out.extend_from_slice(&zip_bytes);
    Ok(out)
}

fn pretty_json(v: &Value) -> Result<Vec<u8>, ContainerError> {
    let mut b =
        serde_json::to_vec_pretty(v).map_err(|e| ContainerError::Manifest(e.to_string()))?;
    b.push(b'\n');
    Ok(b)
}

pub(crate) fn sha256_hex(data: &[u8]) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(data);
    let mut s = String::with_capacity(digest.len() * 2);
    for byte in digest {
        s.push_str(&format!("{byte:02x}"));
    }
    s
}

/// RFC 3339 UTC 当前时间（无外部时间依赖； civil 算法为 Howard Hinnant 的 days_from_civil 逆变换）。
pub fn rfc3339_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (hh, mm, ss) = (rem / 3_600, (rem % 3_600) / 60, rem % 60);

    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}
