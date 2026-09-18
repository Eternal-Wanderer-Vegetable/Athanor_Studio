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

//! 修订层（spec/azodoc-package.md：revisions/chain.json + 快照；azodoc-model.md §3.2 ID 稳定）。
//!
//! v1 只做快照模式：`commit` 把当前 content 落为 `revisions/<rev_id>/content.json`
//! 并推进 `chain.head`；`checkout` 把 content 还原为某快照（ID 原样保留）；
//! 资产不可变（R7），快照无需复制资产。
//!
//! §5.3 表现层修订快照（可选）：commit 时若 `presentation/theme.json` 存在，
//! 同步落 `revisions/<rev_id>/theme.json` 完整副本并在链条目携带
//! `theme_path`/`theme_sha256`（成员名冻结）；旧修订无此文件属正常历史形态，
//! checkout 时未携带即视为仅正文历史（表现层不落，提示而非伪造）。

use crate::builder::{rfc3339_now, sha256_hex};
use crate::{Container, ContainerError};
use azodoc_model::id::{AzodocId, IdKind};
use serde_json::{json, Map, Value};

pub const CHAIN_PATH: &str = "revisions/chain.json";
pub const THEME_ENTRY: &str = "presentation/theme.json";
pub const AUTHOR_TYPES: &[&str] = &["human", "ai", "importer", "converter", "system"];

pub struct CommitInfo<'a> {
    pub author_type: &'a str,
    pub author_id: &'a str,
    pub message: &'a str,
}

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub id: String,
    pub parent: Option<String>,
    pub author_type: String,
    pub author_id: Option<String>,
    pub timestamp: Option<String>,
    pub message: String,
    pub is_head: bool,
    pub is_current: bool,
    /// §5.3 表现层快照 sha256（None = 仅正文历史）。
    pub theme_sha256: Option<String>,
}

/// `checkout` 的结果描述（§5.3：表现层快照存在与否须如实报告）。
#[derive(Debug, Clone)]
pub struct CheckoutInfo {
    /// 该修订是否携带表现层快照并已还原到 `presentation/theme.json`。
    pub theme_restored: bool,
}

fn parse_json(bytes: &[u8]) -> Result<Value, ContainerError> {
    serde_json::from_slice(bytes)
        .map_err(|e| ContainerError::Manifest(format!("JSON 解析失败: {e}")))
}

/// 块 ID 集合差异（changes 统计用；忽略行内与属性）。
fn diff_block_ids(parent: &[u8], current: &[u8]) -> (u64, u64) {
    fn ids_of(bytes: &[u8]) -> std::collections::HashSet<String> {
        fn walk(v: &Value, out: &mut std::collections::HashSet<String>) {
            if let Some(obj) = v.as_object() {
                if let (Some(t), Some(id)) = (
                    obj.get("type").and_then(Value::as_str),
                    obj.get("id").and_then(Value::as_str),
                ) {
                    if t != "text" && t != "hard_break" && t != "code" {
                        // 块级 id（行内无 id 或不统计）
                        out.insert(id.to_string());
                    }
                }
                for val in obj.values() {
                    walk(val, out);
                }
            } else if let Some(arr) = v.as_array() {
                for item in arr {
                    walk(item, out);
                }
            }
        }
        let mut out = std::collections::HashSet::new();
        if let Ok(v) = serde_json::from_slice::<Value>(bytes) {
            walk(&v, &mut out);
        }
        out
    }
    let p = ids_of(parent);
    let c = ids_of(current);
    let added = c.difference(&p).count() as u64;
    let removed = p.difference(&c).count() as u64;
    (added, removed)
}

impl Container {
    /// 读取修订链（不存在 → None）。
    pub fn chain(&mut self) -> Result<Option<Value>, ContainerError> {
        if !self.has_entry(CHAIN_PATH) {
            return Ok(None);
        }
        let bytes = self.read_entry(CHAIN_PATH)?;
        Ok(Some(parse_json(&bytes)?))
    }

    /// 把当前 content.json 落为一条新修订（快照模式）。
    /// 返回新修订 ID。兼容缓存的修订绑定同步刷新（内容未变，缓存仍 fresh）。
    pub fn commit(&mut self, info: &CommitInfo) -> Result<String, ContainerError> {
        if !AUTHOR_TYPES.contains(&info.author_type) {
            return Err(ContainerError::Manifest(format!(
                "无效的作者类型 `{}`（允许: {}）",
                info.author_type,
                AUTHOR_TYPES.join(", ")
            )));
        }
        let content_bytes = self.read_entry("document/content.json")?;
        let chain = self.chain()?;
        let head: Option<String> = chain
            .as_ref()
            .and_then(|c| c.get("head"))
            .and_then(Value::as_str)
            .map(String::from);

        let rev_id = AzodocId::generate(IdKind::Rev).as_str().to_string();
        let snapshot_path = format!("revisions/{rev_id}/content.json");

        // changes：与父快照的块 ID 集合差异
        let mut changes = Map::new();
        if let Some(parent) = &head {
            let ppath = format!("revisions/{parent}/content.json");
            if self.has_entry(&ppath) {
                let pb = self.read_entry(&ppath)?;
                let (added, removed) = diff_block_ids(&pb, &content_bytes);
                changes.insert("blocks_added".to_string(), json!(added));
                changes.insert("blocks_removed".to_string(), json!(removed));
            }
        }

        let mut entry = json!({
            "id": rev_id,
            "parent": head,
            "author": {"type": info.author_type, "id": info.author_id},
            "timestamp": rfc3339_now(),
            "message": info.message,
            "kind": "snapshot",
            "path": snapshot_path,
            "sha256": sha256_hex(&content_bytes),
        });
        if !changes.is_empty() {
            entry["changes"] = Value::Object(changes);
        }

        // §5.3 表现层修订快照：当前容器有 theme 层则落完整副本
        // （完整副本非 diff；与父修订主题一致仍各存一份，保证 theme_path
        // 恒指向本修订的独立快照，撤销顺序无关）。
        let theme_snapshot =
            if self.has_entry(THEME_ENTRY) || self.modified.contains_key(THEME_ENTRY) {
                let theme_bytes = self.read_entry(THEME_ENTRY)?;
                let theme_path = format!("revisions/{rev_id}/theme.json");
                entry["theme_path"] = json!(theme_path);
                entry["theme_sha256"] = json!(sha256_hex(&theme_bytes));
                Some((theme_path, theme_bytes))
            } else {
                None
            };

        let mut new_chain = match chain {
            Some(mut c) => {
                c["head"] = json!(rev_id);
                if let Some(arr) = c.get_mut("revisions").and_then(Value::as_array_mut) {
                    arr.push(entry);
                } else {
                    c["revisions"] = json!([entry]);
                }
                c
            }
            None => json!({
                "schema_version": "1.0",
                "policy": {
                    "mode": "snapshot",
                    "trigger": ["explicit_save", "import", "before_convert"],
                    "max_snapshots": 200
                },
                "head": rev_id,
                "revisions": [entry],
            }),
        };
        // 首次落链时补 policy 默认值
        if new_chain.get("policy").is_none() {
            new_chain["policy"] = json!({
                "mode": "snapshot",
                "trigger": ["explicit_save", "import", "before_convert"],
                "max_snapshots": 200
            });
        }
        let chain_bytes = {
            let mut b = serde_json::to_vec_pretty(&new_chain)
                .map_err(|e| ContainerError::Manifest(e.to_string()))?;
            b.push(b'\n');
            b
        };

        self.set_entry(&snapshot_path, content_bytes)?;
        if let Some((theme_path, theme_bytes)) = theme_snapshot {
            self.set_entry(&theme_path, theme_bytes)?;
        }
        self.set_entry(CHAIN_PATH, chain_bytes)?;

        // manifest：推进 current_revision、注册 revisions 层、刷新缓存绑定
        let chain_sha = sha256_hex(&self.read_entry(CHAIN_PATH)?);
        let mut manifest = self.manifest_value().clone();
        manifest["current_revision"] = json!(rev_id);
        if let Some(layers) = manifest.get_mut("layers").and_then(Value::as_object_mut) {
            let needs_register = layers
                .get("revisions")
                .and_then(Value::as_object)
                .map(|o| o.get("path").and_then(Value::as_str) != Some(CHAIN_PATH))
                .unwrap_or(true);
            if needs_register {
                layers.insert(
                    "revisions".to_string(),
                    json!({"path": CHAIN_PATH, "sha256": chain_sha}),
                );
            }
        }
        if let Some(arr) = manifest
            .get_mut("compatibility")
            .and_then(Value::as_array_mut)
        {
            for e in arr {
                e["revision"] = json!(rev_id);
            }
        }
        self.set_manifest(manifest)?;
        Ok(rev_id)
    }

    /// 把 content.json 还原为某修订的快照（ID 原样保留）。
    /// §5.3：条目携带 `theme_path`/`theme_sha256` 时同步还原
    /// `presentation/theme.json`；未携带（旧修订）表现层原样保留，
    /// `CheckoutInfo.theme_restored = false` 由调用方提示"仅正文历史"。
    /// 兼容缓存标记为 stale（其内容描述的是还原前的文档），由 upgrade 重建。
    pub fn checkout(&mut self, rev_id: &str) -> Result<CheckoutInfo, ContainerError> {
        let chain = self
            .chain()?
            .ok_or_else(|| ContainerError::Manifest("容器没有修订链".to_string()))?;
        let rev_entry = chain
            .get("revisions")
            .and_then(Value::as_array)
            .and_then(|a| {
                a.iter()
                    .find(|r| r.get("id").and_then(Value::as_str) == Some(rev_id))
                    .cloned()
            });
        let Some(rev_entry) = rev_entry else {
            return Err(ContainerError::Manifest(format!("修订不存在: {rev_id}")));
        };
        let ppath = format!("revisions/{rev_id}/content.json");
        if !self.has_entry(&ppath) {
            return Err(ContainerError::Manifest(format!("修订快照缺失: {ppath}")));
        }
        let bytes = self.read_entry(&ppath)?;
        self.set_entry("document/content.json", bytes)?;

        // §5.3：表现层快照存在时还原（sha256 由 verify 兜底；这里信任链中
        // 声明的路径，还原失败按容器错误上抛而非伪造主题）。
        let mut theme_restored = false;
        if let Some(theme_path) = rev_entry.get("theme_path").and_then(Value::as_str) {
            let theme_bytes = self.read_entry(theme_path)?;
            self.set_entry(THEME_ENTRY, theme_bytes)?;
            // 同步 manifest 的 presentation 层登记（sha256 刷新）
            let theme_sha = sha256_hex(&self.read_entry(THEME_ENTRY)?);
            let mut manifest = self.manifest_value().clone();
            if let Some(layers) = manifest.get_mut("layers").and_then(Value::as_object_mut) {
                layers.insert(
                    "presentation".to_string(),
                    json!({"path": THEME_ENTRY, "sha256": theme_sha}),
                );
            }
            self.set_manifest(manifest)?;
            theme_restored = true;
        }

        let mut manifest = self.manifest_value().clone();
        manifest["current_revision"] = json!(rev_id);
        if let Some(arr) = manifest
            .get_mut("compatibility")
            .and_then(Value::as_array_mut)
        {
            for e in arr {
                e["status"] = json!("stale");
            }
        }
        self.set_manifest(manifest)?;
        Ok(CheckoutInfo { theme_restored })
    }

    /// 修订链条目（旧 → 新）。
    pub fn history(&mut self) -> Result<Vec<HistoryEntry>, ContainerError> {
        let chain = self.chain()?;
        let current = self.manifest_typed().current_revision.clone();
        let head = chain
            .as_ref()
            .and_then(|c| c.get("head"))
            .and_then(Value::as_str)
            .map(String::from);
        let mut out = Vec::new();
        if let Some(revs) = chain
            .as_ref()
            .and_then(|c| c.get("revisions"))
            .and_then(Value::as_array)
        {
            for r in revs {
                out.push(HistoryEntry {
                    id: r
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    parent: r.get("parent").and_then(Value::as_str).map(String::from),
                    author_type: r
                        .pointer("/author/type")
                        .and_then(Value::as_str)
                        .unwrap_or("?")
                        .to_string(),
                    author_id: r
                        .pointer("/author/id")
                        .and_then(Value::as_str)
                        .map(String::from),
                    timestamp: r.get("timestamp").and_then(Value::as_str).map(String::from),
                    message: r
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    is_head: head.as_deref() == r.get("id").and_then(Value::as_str),
                    is_current: current.as_deref() == r.get("id").and_then(Value::as_str),
                    theme_sha256: r
                        .get("theme_sha256")
                        .and_then(Value::as_str)
                        .map(String::from),
                });
            }
        }
        Ok(out)
    }
}
