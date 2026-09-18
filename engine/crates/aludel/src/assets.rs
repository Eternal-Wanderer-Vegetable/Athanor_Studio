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

//! 会话级资产暂存与保存时落库（spec/azodoc-package.md §4.2）。
//!
//! - 暂存：图片经 `stage` 进入 App 内存表（mime 白名单 + 字节上限），
//!   PM 正文写 `asset://<as_id>/<filename>`；未提交的暂存随会话释放。
//! - 保存：`persist_assets` 在 content.json 落盘前把可达引用写入
//!   `assets/registry.json` 与 `assets/<id>/<filename>`；`data:` URI 解码
//!   登记为 embedded（迁移），裸 http(s) 登记为 external；失败保留原引用
//!   并报告，绝不产生悬空 `asset://`（§4.2.3）。
//! - 孤儿：已登记但未被引用的条目原样保留（§4.4，不自动回收）。

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use azodoc_container::Container;
use azodoc_model::id::{AzodocId, IdKind};
use serde_json::{json, Value};

use crate::api::ApiError;
use crate::doc_store;

pub const REGISTRY_ENTRY: &str = "assets/registry.json";
/// 单个资产的字节上限（暂存与 data: 迁移共用）。
pub const MAX_ASSET_BYTES: usize = 32 * 1024 * 1024;
/// 暂存接受的 MIME 白名单：仅位图；SVG 因脚本面风险不在内。
const ALLOWED_MIME: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
    "image/avif",
    "image/x-icon",
    "image/bmp",
];

fn is_allowed_mime(mime: &str) -> bool {
    ALLOWED_MIME.contains(&mime)
}

/// 一条暂存资产（未提交进容器）。
pub struct StagedAsset {
    pub filename: String,
    pub mime: String,
    pub bytes: Vec<u8>,
}

/// App 持有的暂存表：as_ id → 资产。
pub type StagedAssets = Mutex<HashMap<String, StagedAsset>>;

/// 文件名清洗：去掉路径分隔/控制字符/`..` 片段，限长，空名回退 "asset"。
/// 容器内路径一律由 `assets/<id>/<filename>` 组成，id 为引擎生成的 ULID，
/// 故此处只需保证 filename 本身不含分隔符。
pub fn sanitize_filename(name: &str) -> String {
    let last = name.rsplit(['/', '\\']).next().unwrap_or("asset");
    let cleaned: String = last.chars().filter(|c| !c.is_control()).take(100).collect();
    let trimmed = cleaned.trim_matches(|c| c == '.' || c == ' ');
    if trimmed.is_empty() || trimmed == ".." {
        "asset".to_string()
    } else {
        trimmed.to_string()
    }
}

fn mime_ext(mime: &str) -> &'static str {
    match mime {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/avif" => "avif",
        "image/x-icon" => "ico",
        "image/bmp" => "bmp",
        _ => "bin",
    }
}

/// 登记一条暂存资产，返回 `asset://<id>/<filename>` 引用。
/// mime 不在白名单或字节超限时拒绝（BadRequest 级，由调用方映射）。
pub fn stage(
    staged: &StagedAssets,
    filename: &str,
    mime: &str,
    bytes: Vec<u8>,
) -> Result<(String, String), ApiError> {
    let mime = mime.trim().to_ascii_lowercase();
    if !is_allowed_mime(&mime) {
        return Err(ApiError::BadRequest(format!(
            "不支持的资产类型 `{mime}`（仅接受位图：{}）",
            ALLOWED_MIME.join(", ")
        )));
    }
    if bytes.is_empty() {
        return Err(ApiError::BadRequest("资产内容为空".into()));
    }
    if bytes.len() > MAX_ASSET_BYTES {
        return Err(ApiError::BadRequest(format!(
            "资产过大（{} B，上限 {} B）",
            bytes.len(),
            MAX_ASSET_BYTES
        )));
    }
    let id = AzodocId::generate(IdKind::As).as_str().to_string();
    let filename = sanitize_filename(filename);
    staged.lock().unwrap_or_else(|e| e.into_inner()).insert(
        id.clone(),
        StagedAsset {
            filename: filename.clone(),
            mime,
            bytes,
        },
    );
    Ok((id.clone(), format!("asset://{id}/{filename}")))
}

/// `data:[<mime>][;base64],<payload>` → (mime, bytes)；非 base64 或解码失败返回 None。
pub fn decode_data_uri(s: &str) -> Option<(String, Vec<u8>)> {
    use base64::Engine as _;
    let rest = s.strip_prefix("data:")?;
    let (meta, payload) = rest.split_once(',')?;
    let mut mime = "application/octet-stream".to_string();
    let mut is_b64 = false;
    for part in meta.split(';') {
        if part.eq_ignore_ascii_case("base64") {
            is_b64 = true;
        } else if !part.is_empty() {
            mime = part.to_ascii_lowercase();
        }
    }
    if !is_b64 {
        return None;
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload.trim())
        .ok()?;
    Some((mime, bytes))
}

/// 保存管线返回的资产统计。
pub struct AssetOutcome {
    /// 本次写入容器的暂存 id（调用方据此清理暂存表）。
    pub persisted_ids: Vec<String>,
    pub staged: u64,
    pub migrated: u64,
    pub external: u64,
    /// 保留原引用并报告的问题（§4.2.3：不产生悬空引用）。
    pub warnings: Vec<String>,
}

impl AssetOutcome {
    pub fn to_json(&self) -> Value {
        json!({
            "staged": self.staged,
            "migrated": self.migrated,
            "external": self.external,
            "warnings": self.warnings,
        })
    }
}

/// 遍历 content_file，把所有 `asset` 字符串字段的引用改写/登记：
/// `asset://staged` → embedded；`data:` → 解码迁移 embedded；`http(s)` → external。
/// 返回 (report)；content_file 就地修改。
pub fn persist_assets(
    c: &mut Container,
    content_file: &mut Value,
    staged: &StagedAssets,
) -> Result<AssetOutcome, ApiError> {
    let mut out = AssetOutcome {
        persisted_ids: Vec::new(),
        staged: 0,
        migrated: 0,
        external: 0,
        warnings: Vec::new(),
    };

    // 既有 registry（可缺省——旧包无资产层）。
    let mut registry: Value = c
        .read_entry(REGISTRY_ENTRY)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_else(|| json!({"schema_version": "1.0", "assets": []}));
    if registry.get("assets").and_then(Value::as_array).is_none() {
        registry["assets"] = json!([]);
    }
    let registered: Vec<String> = registry["assets"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|e| e.get("id").and_then(Value::as_str).map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let staged_map = staged.lock().unwrap_or_else(|e| e.into_inner());
    // 同一引用串在一次保存内映射到同一资产（重复粘贴只登记一次）。
    let mut ref_to_id: HashMap<String, String> = HashMap::new();
    // 本次保存中新建的资产 ID。正文可能多次引用同一图片，registry 仍只能有一条。
    let mut pending_ids: HashSet<String> = HashSet::new();
    let mut new_entries: Vec<Value> = Vec::new();
    let mut new_files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut touched = false;

    for loc in collect_asset_slots(content_file) {
        let current = loc.as_str().unwrap_or_default().to_string();
        if let Some(rest) = current.strip_prefix("asset://") {
            let id = rest.split('/').next().unwrap_or("").to_string();
            if registered.iter().any(|r| r == &id) || pending_ids.contains(&id) {
                continue; // 已登记，原样
            }
            match staged_map.get(&id) {
                Some(a) => {
                    let path = format!("assets/{id}/{}", a.filename);
                    new_entries.push(json!({
                        "id": id,
                        "filename": a.filename,
                        "mime": a.mime,
                        "relationship": "inline",
                        "storage": "embedded",
                        "path": path,
                        "size": a.bytes.len(),
                        "sha256": doc_store::sha256_hex(&a.bytes),
                    }));
                    new_files.push((path, a.bytes.clone()));
                    out.persisted_ids.push(id.clone());
                    out.staged += 1;
                    pending_ids.insert(id);
                    touched = true;
                }
                None => {
                    out.warnings
                        .push(format!("引用未注册的资产 {id}，原样保留（检查将报告）"));
                }
            }
        } else if current.starts_with("data:") {
            match decode_data_uri(&current) {
                Some((mime, bytes)) if is_allowed_mime(&mime) && bytes.len() <= MAX_ASSET_BYTES => {
                    let id = ref_to_id
                        .entry(current.clone())
                        .or_insert_with(|| AzodocId::generate(IdKind::As).as_str().to_string())
                        .clone();
                    let filename = format!("pasted.{}", mime_ext(&mime));
                    if pending_ids.insert(id.clone()) {
                        new_entries.push(json!({
                            "id": id,
                            "filename": filename,
                            "mime": mime,
                            "relationship": "inline",
                            "storage": "embedded",
                            "path": format!("assets/{id}/{filename}"),
                            "size": bytes.len(),
                            "sha256": doc_store::sha256_hex(&bytes),
                        }));
                        new_files.push((format!("assets/{id}/{filename}"), bytes));
                        out.migrated += 1;
                        touched = true;
                    }
                    *loc = json!(format!("asset://{id}/{filename}"));
                }
                Some((_, bytes)) => out.warnings.push(format!(
                    "data: 资产超限或类型不允许（{} B），保留原引用",
                    bytes.len()
                )),
                None => out
                    .warnings
                    .push("data: URI 无法解码，保留原引用".to_string()),
            }
        } else if current.starts_with("http://") || current.starts_with("https://") {
            let id = ref_to_id
                .entry(current.clone())
                .or_insert_with(|| AzodocId::generate(IdKind::As).as_str().to_string())
                .clone();
            let filename = sanitize_filename(
                current
                    .rsplit('/')
                    .next()
                    .unwrap_or("asset")
                    .split('?')
                    .next()
                    .unwrap_or("asset"),
            );
            if pending_ids.insert(id.clone()) {
                new_entries.push(json!({
                    "id": id,
                    "filename": filename,
                    "mime": azodoc_convert::guess_mime(&filename),
                    "relationship": "inline",
                    "storage": "external",
                    "url": current,
                    "size": 0,
                    "sha256": doc_store::sha256_hex(current.as_bytes()),
                }));
                out.external += 1;
                touched = true;
            }
            *loc = json!(format!("asset://{id}/{filename}"));
        }
        // 其他 scheme（blob: 等）不属 §4.2 迁移范围，原样保留
    }
    drop(staged_map);

    if touched {
        if let Some(arr) = registry["assets"].as_array_mut() {
            arr.extend(new_entries);
        }
        let mut bytes = serde_json::to_vec_pretty(&registry)
            .map_err(|e| ApiError::Doc(format!("registry 序列化失败: {e}")))?;
        bytes.push(b'\n');
        c.set_entry(REGISTRY_ENTRY, bytes)
            .map_err(|e| ApiError::Doc(e.friendly()))?;
        for (path, data) in new_files {
            c.set_entry(&path, data)
                .map_err(|e| ApiError::Doc(e.friendly()))?;
        }
        // manifest.layers.assets 在旧包中可能缺席——set_entry 只同步已登记层，
        // 这里显式登记（与 theme 层处理同构）。
        let mut manifest = c.manifest_value().clone();
        let layers = manifest.as_object_mut().and_then(|m| {
            m.entry("layers")
                .or_insert_with(|| json!({}))
                .as_object_mut()
        });
        if let Some(layers) = layers {
            let needs = layers
                .get("assets")
                .and_then(|v| v.get("path"))
                .and_then(Value::as_str)
                != Some(REGISTRY_ENTRY);
            if needs {
                layers.insert(
                    "assets".to_string(),
                    json!({
                        "path": REGISTRY_ENTRY,
                        "sha256": doc_store::sha256_hex(&c.read_entry(REGISTRY_ENTRY).unwrap_or_default()),
                    }),
                );
                c.set_manifest(manifest)
                    .map_err(|e| ApiError::Doc(e.friendly()))?;
            }
        }
    }

    Ok(out)
}

/// 收集 content_file 中全部 `asset` 字符串字段的可变引用（块级与行内一致处理）。
fn collect_asset_slots(v: &mut Value) -> Vec<&mut Value> {
    let mut out = Vec::new();
    fn walk<'a>(v: &'a mut Value, out: &mut Vec<&'a mut Value>) {
        match v {
            Value::Object(obj) => {
                for (key, val) in obj.iter_mut() {
                    if key == "asset" && val.is_string() {
                        out.push(val);
                    } else {
                        walk(val, out);
                    }
                }
            }
            Value::Array(arr) => {
                for item in arr.iter_mut() {
                    walk(item, out);
                }
            }
            _ => {}
        }
    }
    walk(v, &mut out);
    out
}

/// 读取资产字节供渲染：先查暂存表，再查 registry（embedded → 字节，
/// external → 返回 url 供调用方重定向）。
pub enum AssetRead {
    Embedded { mime: String, bytes: Vec<u8> },
    External { url: String },
    Missing,
}

pub fn read_asset(
    store_read: &mut dyn FnMut(&str) -> Result<Vec<u8>, ApiError>,
    staged: &StagedAssets,
    id: &str,
) -> Result<AssetRead, ApiError> {
    {
        let map = staged.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(a) = map.get(id) {
            return Ok(AssetRead::Embedded {
                mime: a.mime.clone(),
                bytes: a.bytes.clone(),
            });
        }
    }
    let reg_bytes = match store_read(REGISTRY_ENTRY) {
        Ok(b) => b,
        Err(_) => return Ok(AssetRead::Missing),
    };
    let registry: Value = serde_json::from_slice(&reg_bytes)
        .map_err(|e| ApiError::Doc(format!("registry 解析失败: {e}")))?;
    let Some(entry) = registry
        .get("assets")
        .and_then(Value::as_array)
        .and_then(|a| {
            a.iter()
                .find(|e| e.get("id").and_then(Value::as_str) == Some(id))
        })
    else {
        return Ok(AssetRead::Missing);
    };
    match entry.get("storage").and_then(Value::as_str) {
        Some("embedded") => {
            let path = entry.get("path").and_then(Value::as_str).unwrap_or("");
            let bytes = store_read(path)?;
            Ok(AssetRead::Embedded {
                mime: entry
                    .get("mime")
                    .and_then(Value::as_str)
                    .unwrap_or("application/octet-stream")
                    .to_string(),
                bytes,
            })
        }
        Some("external") => Ok(AssetRead::External {
            url: entry
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }),
        _ => Ok(AssetRead::Missing),
    }
}

/// 去掉一个已持久化的暂存条目；容器已持有字节。
pub fn drop_persisted(staged: &StagedAssets, ids: &[String]) {
    let mut map = staged.lock().unwrap_or_else(|e| e.into_inner());
    for id in ids {
        map.remove(id);
    }
}
