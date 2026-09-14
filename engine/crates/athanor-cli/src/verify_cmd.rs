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
    verify_revisions(&mut c, &mut errors, &mut warnings, &mut referenced);

    // (10) 兼容缓存 + 报告登记
    let m = c.manifest_typed().clone();
    for e in &m.compatibility {
        referenced.push(e.path.clone());
        if !c.has_entry(&e.path) {
            errors.push(format!("兼容缓存条目缺失（{}）", e.path));
        }
        let fresh = m.compat_is_fresh(e);
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

/// (9) 修订链一致性。
fn verify_revisions(
    c: &mut Container,
    errors: &mut Vec<String>,
    warnings: &mut Vec<String>,
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

    let head = chain.get("head").and_then(Value::as_str);
    match (head, current.as_deref()) {
        (Some(h), Some(cur)) if h == cur => {}
        (Some(h), Some(cur)) => errors.push(format!(
            "chain.head（{h}）与 manifest.current_revision（{cur}）不一致"
        )),
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
