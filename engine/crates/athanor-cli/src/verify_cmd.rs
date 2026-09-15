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

//! `athanor verify` —— spec/azodoc-package.md §9 的算法实现（M1 覆盖 1–11 步）。

use azodoc_container::Container;
use azodoc_model::manifest::{unknown_manifest_key_paths, LayerRef};
use azodoc_model::validate::{check_content, Ctx, Level};
use serde_json::Value;
use std::path::Path;

const KNOWN_DIRS: [&str; 8] = [
    "document/",
    "semantics/",
    "presentation/",
    "publication/",
    "revisions/",
    "assets/",
    "compatibility/",
    "reports/",
];

pub fn run(path: &Path) -> i32 {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("错误：无法读取 {}: {e}", path.display());
            return 1;
        }
    };
    let (mut c, mut warnings) = match azodoc_container::open(data) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("{}", e.friendly());
            return 1;
        }
    };

    let mut errors: Vec<String> = Vec::new();
    let mut infos: Vec<String> = Vec::new();
    let mut referenced: Vec<String> = vec!["manifest.json".to_string()];

    // (4) format_version 与 Header 一致
    if let Some(h) = c.header {
        let expect = format!("{}.{}", h.version_major, h.version_minor);
        let actual = c
            .manifest_value()
            .pointer("/azodoc/format_version")
            .and_then(Value::as_str)
            .unwrap_or("<缺失>");
        if actual != expect {
            errors.push(format!(
                "azodoc.format_version（{actual}）与 Header 版本（{expect}）不一致"
            ));
        }
    }

    // (5) layers：存在性、sha256、重复路径（先拷贝条目，避免借用冲突）
    let layers: Vec<(String, Value)> = c
        .manifest_typed()
        .layers
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let mut layer_paths: Vec<String> = Vec::new();
    for (key, v) in &layers {
        let lr: LayerRef = match serde_json::from_value(v.clone()) {
            Ok(lr) => lr,
            Err(e) => {
                errors.push(format!("layers.{key}: 结构无效（{e}）"));
                continue;
            }
        };
        if layer_paths.contains(&lr.path) {
            errors.push(format!("layers.{key}: 路径被重复登记（{}）", lr.path));
        }
        layer_paths.push(lr.path.clone());
        referenced.push(lr.path.clone());
        match c.read_entry(&lr.path) {
            Err(_) => errors.push(format!("layers.{key}: 条目缺失（{}）", lr.path)),
            Ok(bytes) => {
                let h = crate::sha256_hex(&bytes);
                if h != lr.sha256 {
                    errors.push(format!("layers.{key}: sha256 不匹配（{}）", lr.path));
                }
            }
        }
    }

    // (6) content 层必须存在
    let content_missing = !c.has_entry("document/content.json");
    if content_missing {
        errors.push("缺少内容层 document/content.json".to_string());
    }

    // (7) content 结构校验 + (8) 资产一致性
    let names = c.entry_names();
    let entry_exists = |n: &str| names.iter().any(|x| x == n);
    let ctx = Ctx {
        entry_exists: &entry_exists,
    };
    if !content_missing {
        match c.read_entry("document/content.json") {
            Err(e) => errors.push(format!("document/content.json 读取失败: {e}")),
            Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
                Err(e) => errors.push(format!("document/content.json: JSON 语法错误（{e}）")),
                Ok(v) => {
                    let (issues, asset_refs) = check_content(&v, &ctx);
                    for issue in issues {
                        let line = format!("{} {}（{}）", issue.path, issue.message, issue.code);
                        match issue.level {
                            Level::Error => errors.push(line),
                            Level::Warning => warnings.push(line),
                            Level::Info => infos.push(line),
                        }
                    }
                    verify_assets(
                        &mut c,
                        &asset_refs,
                        &mut errors,
                        &mut infos,
                        &mut referenced,
                    );
                }
            },
        }
    }

    // (9) 修订链
    verify_revisions(
        &mut c,
        &mut errors,
        &mut warnings,
        &mut infos,
        &mut referenced,
    );

    // (9b) 语义标注新鲜度（azodoc-model.md §10）
    verify_annotations(&mut c, &mut warnings, &mut infos, &mut referenced);

    // (9c) 出版记录完整性（artifact 存在 + sha 一致）
    verify_publications(&mut c, &mut errors, &mut referenced);

    // (10) 兼容缓存 + 报告登记
    let m = c.manifest_typed().clone();
    let content_sha = c
        .read_entry("document/content.json")
        .ok()
        .map(|b| crate::sha256_hex(&b));
    for e in &m.compatibility {
        referenced.push(e.path.clone());
        if !c.has_entry(&e.path) {
            errors.push(format!("兼容缓存条目缺失（{}）", e.path));
        }
        // 新鲜度：修订绑定 + 可选的 content_sha256 岔度检测（spec v1.0.1 增补）
        let mut fresh = m.compat_is_fresh(e);
        if let (Some(sha), Some(cur)) = (
            e.extra.0.get("content_sha256").and_then(Value::as_str),
            &content_sha,
        ) {
            fresh = fresh && sha == cur;
        }
        match (e.status.as_deref(), fresh) {
            (Some("fresh"), false) => warnings.push(format!(
                "compatibility[{}] 标记 fresh 但实际 stale（绑定 {:?} vs 当前 {:?}）",
                e.format, e.revision, m.current_revision
            )),
            (Some("stale"), true) => warnings.push(format!(
                "compatibility[{}] 标记 stale 但实际 fresh",
                e.format
            )),
            (None, _) => warnings.push(format!("compatibility[{}] 缺少 status 字段", e.format)),
            _ => {}
        }
    }
    for r in &m.reports {
        referenced.push(r.path.clone());
        if !c.has_entry(&r.path) {
            errors.push(format!("报告条目缺失（{}）", r.path));
        }
    }

    // (6b)+(11) 未登记的已知文件 / 未知内容盘点
    for name in c.entry_names() {
        if name == "manifest.json" || name.starts_with("preserved/") {
            continue;
        }
        let known = KNOWN_DIRS.iter().any(|d| name.starts_with(d));
        if !known {
            infos.push(format!("未知条目（R1 原样保留）: {name}"));
        } else if !referenced.contains(&name) {
            warnings.push(format!("已知目录中的文件未在任何层登记: {name}"));
        }
    }
    for k in unknown_manifest_key_paths(c.manifest_value()) {
        infos.push(format!("未知 manifest 字段（R2 保留）: {k}"));
    }

    // 汇总
    for e in &errors {
        println!("✗ {e}");
    }
    for w in &warnings {
        println!("△ {w}");
    }
    for i in &infos {
        println!("· {i}");
    }
    if errors.is_empty() {
        println!("✓ 校验通过（{} 个条目）", c.entry_names().len());
        println!(
            "错误 {} · 警告 {} · 提示 {}",
            errors.len(),
            warnings.len(),
            infos.len()
        );
        0
    } else {
        println!("✗ 校验失败");
        println!(
            "错误 {} · 警告 {} · 提示 {}",
            errors.len(),
            warnings.len(),
            infos.len()
        );
        1
    }
}

/// (8) 资产一致性（spec/azodoc-package.md §4）。
fn verify_assets(
    c: &mut Container,
    asset_refs: &[(String, String)],
    errors: &mut Vec<String>,
    infos: &mut Vec<String>,
    referenced: &mut Vec<String>,
) {
    let registry: Option<Value> = c
        .read_entry("assets/registry.json")
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());

    if registry.is_none() {
        if !asset_refs.is_empty() {
            errors.push("内容引用了资产，但缺少 assets/registry.json 层".to_string());
        }
        return;
    }
    let reg = registry.unwrap();

    let mut reg_ids: Vec<String> = Vec::new();
    if let Some(assets) = reg.get("assets").and_then(Value::as_array) {
        for a in assets {
            let id = a.get("id").and_then(Value::as_str).unwrap_or("");
            if !azodoc_model::id::is_valid_id(id) {
                errors.push(format!("资产 id 非法: `{id}`"));
                continue;
            }
            reg_ids.push(id.to_string());
            let storage = a.get("storage").and_then(Value::as_str).unwrap_or("");
            match storage {
                "embedded" => {
                    let (Some(p), Some(s)) = (
                        a.get("path").and_then(Value::as_str),
                        a.get("sha256").and_then(Value::as_str),
                    ) else {
                        errors.push(format!("embedded 资产缺少 path/sha256: {id}"));
                        continue;
                    };
                    referenced.push(p.to_string());
                    match c.read_entry(p) {
                        Err(_) => errors.push(format!("资产条目缺失: {p}")),
                        Ok(bytes) => {
                            if crate::sha256_hex(&bytes) != s {
                                errors.push(format!("资产 sha256 不匹配: {p}"));
                            }
                        }
                    }
                }
                "external" => {
                    if a.get("url").and_then(Value::as_str).is_none() {
                        errors.push(format!("external 资产缺少 url: {id}"));
                    }
                }
                other => errors.push(format!("资产 storage 无效（{other}）: {id}")),
            }
        }
    }

    for (loc, url) in asset_refs {
        let Some(id) = url
            .strip_prefix("asset://")
            .map(|s| s.split('/').next().unwrap_or(s).to_string())
        else {
            errors.push(format!("{loc}: 非 asset:// 引用（{url}）"));
            continue;
        };
        if !azodoc_model::id::is_valid_id(&id) {
            errors.push(format!("{loc}: 资产 id 非法（{id}）"));
        } else if !reg_ids.contains(&id) {
            errors.push(format!("{loc}: 引用了未注册资产（{id}）"));
        }
    }
    // 孤儿资产允许存在（宁多勿删）
    for id in &reg_ids {
        if !asset_refs.iter().any(|(_, u)| u.contains(id)) {
            infos.push(format!("孤儿资产（允许存在，不自动清理）: {id}"));
        }
    }
}

/// (9b) 语义标注：目标存在性 + text_quote 匹配检查。
fn verify_annotations(
    c: &mut Container,
    warnings: &mut Vec<String>,
    infos: &mut Vec<String>,
    referenced: &mut Vec<String>,
) {
    const LAYER: &str = "semantics/annotations.json";
    if !c.has_entry(LAYER) {
        return;
    }
    referenced.push(LAYER.to_string());
    let bytes = match c.read_entry(LAYER) {
        Ok(b) => b,
        Err(e) => {
            warnings.push(format!("语义层读取失败: {e}"));
            return;
        }
    };
    let layer: Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => {
            errors_push(
                warnings,
                format!("semantics/annotations.json 解析失败（{e}）"),
            );
            return;
        }
    };
    let content: Value = c
        .read_entry("document/content.json")
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(Value::Null);
    let block_ids = collect_block_ids_from(&content);
    let empty = Vec::new();
    let anns = layer
        .get("annotations")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    for a in anns {
        let id = a.get("id").and_then(Value::as_str).unwrap_or("?");
        if a.get("detached") == Some(&Value::Bool(true)) {
            infos.push(format!(
                "标注 {id} 处于 detached 状态（原文已失配，未删除）"
            ));
            continue;
        }
        let kind = a
            .pointer("/target/kind")
            .and_then(Value::as_str)
            .unwrap_or("");
        let block = a
            .pointer("/target/block")
            .and_then(Value::as_str)
            .unwrap_or("");
        if !block.is_empty() && !block_ids.iter().any(|x| x == block) {
            warnings.push(format!(
                "标注 {id} 的目标块不存在（{block}）——建议 athanor relocate 或重新标注"
            ));
            continue;
        }
        if kind == "text_quote" && !block.is_empty() {
            let exact = a
                .pointer("/target/selector/exact")
                .and_then(Value::as_str)
                .unwrap_or("");
            if !exact.is_empty() {
                let text = find_block_text(&content, block);
                if !text.contains(exact) {
                    warnings.push(format!(
                        "标注 {id} 的引用文本未在块 {block} 中找到——运行 athanor relocate 重定位"
                    ));
                }
            }
        }
    }
}

fn errors_push(warnings: &mut Vec<String>, msg: String) {
    warnings.push(msg);
}

fn collect_block_ids_from(content: &Value) -> Vec<String> {
    fn walk(v: &Value, out: &mut Vec<String>) {
        if let Some(obj) = v.as_object() {
            let t = obj.get("type").and_then(Value::as_str).unwrap_or("");
            if let Some(id) = obj.get("id").and_then(Value::as_str) {
                if !matches!(t, "text" | "hard_break" | "code") {
                    out.push(id.to_string());
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
    let mut out = Vec::new();
    walk(content, &mut out);
    out
}

fn find_block_text(content: &Value, block: &str) -> String {
    fn walk(v: &Value, block: &str) -> Option<String> {
        if let Some(obj) = v.as_object() {
            if obj.get("id").and_then(Value::as_str) == Some(block) {
                return Some(azodoc_convert::plain_text_of_block(v));
            }
            for val in obj.values() {
                if let Some(found) = walk(val, block) {
                    return Some(found);
                }
            }
        } else if let Some(arr) = v.as_array() {
            for item in arr {
                if let Some(found) = walk(item, block) {
                    return Some(found);
                }
            }
        }
        None
    }
    walk(content, block).unwrap_or_default()
}

/// (9c) 出版记录：publication.json 的 artifact 完整性。
/// 出版是冻结历史（不随内容演进过期），只校验产物完整性与 ID 合法性。
fn verify_publications(c: &mut Container, errors: &mut Vec<String>, referenced: &mut Vec<String>) {
    const LAYER: &str = "publication/publication.json";
    if !c.has_entry(LAYER) {
        return;
    }
    referenced.push(LAYER.to_string());
    let bytes = match c.read_entry(LAYER) {
        Ok(b) => b,
        Err(e) => {
            errors.push(format!("publication 层读取失败: {e}"));
            return;
        }
    };
    let layer: Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => {
            errors.push(format!("publication.json 解析失败（{e}）"));
            return;
        }
    };
    let empty = Vec::new();
    let pubs = layer
        .get("publications")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    for p in pubs {
        let id = p.get("id").and_then(Value::as_str).unwrap_or("?");
        if !azodoc_model::id::is_valid_id(id) {
            errors.push(format!("publication id 非法（{id}）"));
        }
        if let (Some(path), Some(sha)) = (
            p.pointer("/artifact/path").and_then(Value::as_str),
            p.pointer("/artifact/sha256").and_then(Value::as_str),
        ) {
            referenced.push(path.to_string());
            match c.read_entry(path) {
                Err(_) => errors.push(format!("publication {id}: PDF 产物缺失（{path}）")),
                Ok(bytes) => {
                    if crate::sha256_hex(&bytes) != sha {
                        errors.push(format!("publication {id}: PDF sha256 不匹配（{path}）"));
                    }
                }
            }
        } else {
            errors.push(format!("publication {id}: 缺少 artifact path/sha256"));
        }
    }
}

/// (9) 修订链一致性。
fn verify_revisions(
    c: &mut Container,
    errors: &mut Vec<String>,
    warnings: &mut Vec<String>,
    infos: &mut Vec<String>,
    referenced: &mut Vec<String>,
) {
    let chain: Option<Value> = c
        .read_entry("revisions/chain.json")
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());
    let current = c.manifest_typed().current_revision.clone();

    let Some(chain) = chain else {
        if current.is_some() {
            errors.push(format!(
                "current_revision 指向修订（{current:?}）但缺少 revisions/chain.json"
            ));
        }
        return;
    };
    referenced.push("revisions/chain.json".to_string());

    let head = chain.get("head").and_then(Value::as_str).map(String::from);
    // 收集修订 ID：current_revision 必须是链中一员（checkout 后允许 current ≠ head）
    let rev_ids: Vec<String> = chain
        .get("revisions")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|r| r.get("id").and_then(Value::as_str))
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    match (head.as_deref(), current.as_deref()) {
        (Some(h), Some(cur)) => {
            if h != cur {
                if rev_ids.iter().any(|x| x == cur) {
                    infos.push(format!(
                        "文档处于修订 {cur}（非最新 head {h}）——checkout 状态"
                    ));
                } else {
                    errors.push(format!("manifest.current_revision（{cur}）不在修订链中"));
                }
            }
        }
        (Some(_), None) => {
            warnings.push("chain.head 存在但 manifest.current_revision 为 null".to_string())
        }
        (None, _) => errors.push("chain.json 缺少 head".to_string()),
    }

    if let Some(revs) = chain.get("revisions").and_then(Value::as_array) {
        for (i, r) in revs.iter().enumerate() {
            let id = r.get("id").and_then(Value::as_str).unwrap_or("");
            if !azodoc_model::id::is_valid_id(id) {
                errors.push(format!("revisions[{i}]: id 非法（{id}）"));
            }
            if let Some(p) = r.get("path").and_then(Value::as_str) {
                referenced.push(p.to_string());
                match c.read_entry(p) {
                    Err(_) => errors.push(format!("revisions[{i}]: 快照缺失（{p}）")),
                    Ok(bytes) => {
                        if let Some(sha) = r.get("sha256").and_then(Value::as_str) {
                            if crate::sha256_hex(&bytes) != sha {
                                errors.push(format!("revisions[{i}]: 快照 sha256 不匹配（{p}）"));
                            }
                        }
                    }
                }
            } else {
                errors.push(format!("revisions[{i}]: 快照缺少 path"));
            }
        }
    }
}
