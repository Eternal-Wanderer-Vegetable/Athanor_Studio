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

//! manifest.json 类型（spec/azodoc-package.md §3）。
//!
//! 未知字段一律落入 `ExtraMap`（R2）：反序列化不丢弃、序列化原样回写；
//! serde_json 开启 `preserve_order`，保证重写后的键序与原文件一致。

use crate::JsonMap;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

/// 未知字段捕获袋（R2）。空序列化时不产生任何键。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExtraMap(pub JsonMap);

impl Serialize for ExtraMap {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ExtraMap {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        JsonMap::deserialize(deserializer).map(ExtraMap)
    }
}

impl JsonSchema for ExtraMap {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ExtraMap".into()
    }
    fn json_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::Schema::try_from(serde_json::json!({
            "type": "object",
            "additionalProperties": true
        }))
        .expect("静态 schema 必然合法")
    }
}

/// 已知层成员名（spec/azodoc-package.md §3.4）。
pub const KNOWN_LAYERS: &[&str] = &[
    "content",
    "assets",
    "semantics",
    "presentation",
    "revisions",
    "publication",
];

/// `layers` 对象：已知层 + 未来版本的未知层（原样保留）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LayersMap(pub JsonMap);

impl Serialize for LayersMap {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for LayersMap {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        JsonMap::deserialize(deserializer).map(LayersMap)
    }
}

impl JsonSchema for LayersMap {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "LayersMap".into()
    }
    fn json_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
        // 宽容形态：层值不限定为 LayerRef（未来层可能是任意对象）
        schemars::Schema::try_from(serde_json::json!({
            "type": "object",
            "additionalProperties": true
        }))
        .expect("静态 schema 必然合法")
    }
}

impl std::ops::Deref for LayersMap {
    type Target = JsonMap;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GeneratorInfo {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(flatten)]
    pub extra: ExtraMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AzodocMeta {
    pub format_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container_profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generator: Option<GeneratorInfo>,
    #[serde(flatten)]
    pub extra: ExtraMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DocumentMeta {
    pub id: String,
    pub schema_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
    #[serde(flatten)]
    pub extra: ExtraMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LayerRef {
    pub path: String,
    pub sha256: String,
    #[serde(flatten)]
    pub extra: ExtraMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CompatEntry {
    pub format: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generator: Option<GeneratorInfo>,
    /// 写入器声称的状态；判定一律以 revision 绑定重算为准（spec/azodoc-package.md §6）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(flatten)]
    pub extra: ExtraMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ReportEntry {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(flatten)]
    pub extra: ExtraMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Manifest {
    pub azodoc: AzodocMeta,
    pub document: DocumentMeta,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_revision: Option<String>,
    pub layers: LayersMap,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compatibility: Vec<CompatEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reports: Vec<ReportEntry>,
    #[serde(flatten)]
    pub extra: ExtraMap,
}

impl Manifest {
    /// 从原始 JSON Value 解析（宽容：未知字段进 extra）。
    pub fn from_value(v: &Value) -> Result<Self, serde_json::Error> {
        serde_json::from_value(v.clone())
    }

    pub fn to_value(&self) -> Result<Value, serde_json::Error> {
        serde_json::to_value(self)
    }

    /// 当前修订号的统一取值（缺省为 None）。
    pub fn current_revision(&self) -> Option<&str> {
        self.current_revision.as_deref()
    }

    /// 兼容缓存新鲜度判定（spec/azodoc-package.md §6）：只看修订绑定。
    pub fn compat_is_fresh(&self, entry: &CompatEntry) -> bool {
        match (&entry.revision, &self.current_revision) {
            (None, None) => true,
            (Some(a), Some(b)) => a == b,
            _ => false,
        }
    }
}

/// 已知 manifest 顶层键。
pub const KNOWN_MANIFEST_KEYS: &[&str] = &[
    "azodoc",
    "document",
    "current_revision",
    "layers",
    "compatibility",
    "reports",
    "extra",
];
/// 已知 `azodoc` 成员键。
pub const KNOWN_AZODOC_KEYS: &[&str] = &["format_version", "container_profile", "generator"];
/// 已知 `document` 成员键。
pub const KNOWN_DOCUMENT_KEYS: &[&str] = &[
    "id",
    "schema_version",
    "title",
    "language",
    "created_at",
    "modified_at",
];
/// 已知 generator 成员键。
pub const KNOWN_GENERATOR_KEYS: &[&str] = &["name", "version"];

/// 数未知键（供 verify 的未知内容盘点）。
pub fn unknown_manifest_key_paths(m: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let Some(obj) = m.as_object() else {
        return out;
    };
    for (k, _) in obj {
        if !KNOWN_MANIFEST_KEYS.contains(&k.as_str()) && k != "extra" {
            out.push(k.clone());
        }
    }
    if let Some(az) = m.get("azodoc").and_then(Value::as_object) {
        for k in az.keys() {
            if !KNOWN_AZODOC_KEYS.contains(&k.as_str()) {
                out.push(format!("azodoc.{k}"));
            }
        }
    }
    if let Some(doc) = m.get("document").and_then(Value::as_object) {
        for k in doc.keys() {
            if !KNOWN_DOCUMENT_KEYS.contains(&k.as_str()) {
                out.push(format!("document.{k}"));
            }
        }
    }
    if let Some(layers) = m.get("layers").and_then(Value::as_object) {
        for k in layers.keys() {
            if !KNOWN_LAYERS.contains(&k.as_str()) {
                out.push(format!("layers.{k}"));
            }
        }
        for key in KNOWN_LAYERS {
            if let Some(layer) = layers.get(*key).and_then(Value::as_object) {
                for k in layer.keys() {
                    if k != "path" && k != "sha256" {
                        out.push(format!("layers.{key}.{k}"));
                    }
                }
            }
        }
    }
    out
}
