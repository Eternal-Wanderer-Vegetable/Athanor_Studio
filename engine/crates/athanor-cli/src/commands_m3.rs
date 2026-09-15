//! M3 命令：修订层（history / checkout / commit）与语义层（annotate / annotations）。

use azodoc_container::revisions::CommitInfo;
use azodoc_container::{Container, ContainerError};
use azodoc_convert::semantics::relocate_annotations;
use serde_json::{json, Value};
use std::path::Path;

use crate::{die, sha256_hex};

fn now() -> String {
    azodoc_container::builder::rfc3339_now()
}

fn generator() -> Value {
    json!({ "name": "athanor", "version": env!("CARGO_PKG_VERSION") })
}

fn pretty(v: &Value) -> Vec<u8> {
    let mut b = serde_json::to_vec_pretty(v).expect("序列化失败");
    b.push(b'\n');
    b
}

/// 解析 `type:id`（如 `human:veg`、`ai:gpt-x`）；缺 id 记空串。
fn split_author(author: &str) -> (String, String) {
    match author.split_once(':') {
        Some((t, i)) => (t.trim().to_string(), i.trim().to_string()),
        None => (author.trim().to_string(), String::new()),
    }
}

// ---------------------------------------------------------------- history

pub fn cmd_history(path: &Path, as_json: bool) -> i32 {
    let (mut c, warnings) = match crate::read_container(path) {
        Ok(x) => x,
        Err(code) => return code,
    };
    let entries = match c.history() {
        Ok(e) => e,
        Err(e) => return die(e),
    };
    if entries.is_empty() {
        println!("该文档没有修订链。");
        return 0;
    }
    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&c.chain().ok().flatten().unwrap_or(Value::Null))
                .unwrap_or_else(|_| "{}".into())
        );
        crate::print_warnings(&warnings);
        return 0;
    }
    let head = entries.iter().find(|e| e.is_head).map(|e| e.id.clone());
    println!("修订链（head: {}）：", head.as_deref().unwrap_or("<无>"));
    for e in entries.iter().rev() {
        let flags = match (e.is_current, e.is_head) {
            (true, true) => " [当前·head]",
            (true, false) => " [当前]",
            (false, true) => " [head]",
            _ => "",
        };
        println!(
            "* {}  {}  {}:{}  {}{}",
            e.id,
            e.timestamp.as_deref().unwrap_or("?"),
            e.author_type,
            e.author_id.as_deref().unwrap_or("-"),
            e.message,
            flags
        );
        if let Some(p) = &e.parent {
            println!("    ↳ parent: {p}");
        }
    }
    if let Some(cur) = c.manifest_typed().current_revision.as_deref() {
        if !entries.iter().any(|e| e.id == cur) {
            println!("警告: manifest.current_revision（{cur}）不在修订链中");
        }
    }
    crate::print_warnings(&warnings);
    0
}

// ---------------------------------------------------------------- commit

pub fn cmd_commit(path: &Path, author: &str, message: &str) -> i32 {
    let (mut c, _) = match crate::read_container(path) {
        Ok(x) => x,
        Err(code) => return code,
    };
    let (atype, aid) = split_author(author);
    let rev = match c.commit(&CommitInfo {
        author_type: &atype,
        author_id: &aid,
        message,
    }) {
        Ok(r) => r,
        Err(e) => return die(e),
    };
    match c.write() {
        Ok(bytes) => {
            if let Err(e) = std::fs::write(path, bytes) {
                eprintln!("错误：容器回写失败: {e}");
                return 1;
            }
            println!("已落链 {rev}（{atype}:{aid}）");
            0
        }
        Err(e) => die(e),
    }
}

// ---------------------------------------------------------------- checkout

pub fn cmd_checkout(path: &Path, rev: &str, out: Option<&Path>) -> i32 {
    let (mut c, _) = match crate::read_container(path) {
        Ok(x) => x,
        Err(code) => return code,
    };

    // 仅导出快照：不改容器
    if let Some(dir) = out {
        let ppath = format!("revisions/{rev}/content.json");
        let bytes = match c.read_entry(&ppath) {
            Ok(b) => b,
            Err(e) => return die(e),
        };
        if let Err(e) = std::fs::create_dir_all(dir) {
            eprintln!("错误：创建目录失败: {e}");
            return 1;
        }
        let target = dir.join("content.json");
        if let Err(e) = std::fs::write(&target, &bytes) {
            eprintln!("错误：写入失败: {e}");
            return 1;
        }
        println!("已导出修订 {rev} → {}", target.display());
        return 0;
    }

    if let Err(e) = c.checkout(rev) {
        return die(e);
    }

    // 标注自动重定位
    let stats = match relocate_annotations_layer(&mut c) {
        Ok(s) => s,
        Err(e) => return die(e),
    };

    let bytes = match c.write() {
        Ok(b) => b,
        Err(e) => return die(e),
    };
    if let Err(e) = std::fs::write(path, bytes) {
        eprintln!("错误：容器回写失败: {e}");
        return 1;
    }
    println!("已切换到修订 {rev}（content 与快照一致；ID 原样保留）");
    if let Some(s) = stats {
        println!(
            "标注重定位: 未变 {} · 重锚 {} · 迁移 {} · 失配 {}",
            s.unchanged, s.reanchored, s.moved, s.detached
        );
    }
    println!("提示: 兼容缓存已标记 stale，运行 athanor upgrade 重建");
    0
}

/// 对容器的语义层执行重定位（层不存在 → None）。
pub fn relocate_annotations_layer(
    c: &mut Container,
) -> Result<Option<azodoc_convert::semantics::RelocateStats>, ContainerError> {
    const LAYER: &str = "semantics/annotations.json";
    if !c.has_entry(LAYER) {
        return Ok(None);
    }
    let ann_bytes = c.read_entry(LAYER)?;
    let mut ann: Value = serde_json::from_slice(&ann_bytes)
        .map_err(|e| ContainerError::Manifest(format!("annotations.json 解析失败: {e}")))?;
    let content_bytes = c.read_entry("document/content.json")?;
    let content: Value = serde_json::from_slice(&content_bytes)
        .map_err(|e| ContainerError::Manifest(format!("content 解析失败: {e}")))?;
    let stats = relocate_annotations(&content, &mut ann);
    c.set_entry(LAYER, pretty(&ann))?;
    // 确保 semantics 层已注册（注册后 set_entry 会自动同步 sha256）
    let sha = sha256_hex(&c.read_entry(LAYER)?);
    let mut manifest = c.manifest_value().clone();
    if let Some(layers) = manifest.get_mut("layers").and_then(Value::as_object_mut) {
        let needs = layers
            .get("semantics")
            .and_then(Value::as_object)
            .map(|o| o.get("path").and_then(Value::as_str) != Some(LAYER))
            .unwrap_or(true);
        if needs {
            layers.insert(
                "semantics".to_string(),
                json!({"path": LAYER, "sha256": sha}),
            );
        }
    }
    c.set_manifest(manifest)?;
    Ok(Some(stats))
}

// ---------------------------------------------------------------- annotate / annotations

pub struct AnnotateArgs<'a> {
    pub block: &'a str,
    pub ann_type: &'a str,
    pub value_json: &'a str,
    pub exact: Option<&'a str>,
    pub prefix: Option<&'a str>,
    pub suffix: Option<&'a str>,
    pub confidence: Option<f64>,
    pub author: Option<&'a str>,
    pub source: Option<&'a str>,
}

pub fn cmd_annotate(path: &Path, args: &AnnotateArgs) -> i32 {
    let (mut c, _) = match crate::read_container(path) {
        Ok(x) => x,
        Err(code) => return code,
    };
    const LAYER: &str = "semantics/annotations.json";

    // 目标块必须存在
    let content_bytes = match c.read_entry("document/content.json") {
        Ok(b) => b,
        Err(e) => return die(e),
    };
    let content: Value = match serde_json::from_slice(&content_bytes) {
        Ok(v) => v,
        Err(e) => return die(ContainerError::Manifest(format!("content 解析失败: {e}"))),
    };
    let block_ids = collect_block_ids(&content);
    if !block_ids.iter().any(|x| x == args.block) {
        eprintln!("错误：目标块不存在: {}", args.block);
        return 1;
    }

    // value 必须是 JSON 对象
    let value: Value = match serde_json::from_str::<Value>(args.value_json) {
        Ok(v) if v.is_object() => v,
        Ok(_) => {
            eprintln!("错误：--value 必须是 JSON 对象");
            return 2;
        }
        Err(e) => {
            eprintln!("错误：--value 不是合法 JSON: {e}");
            return 2;
        }
    };

    let (atype, aid) = args
        .author
        .map(split_author)
        .unwrap_or(("human".to_string(), "local".to_string()));

    let id = azodoc_model::id::AzodocId::generate(azodoc_model::id::IdKind::Ann);
    let target = if let Some(exact) = args.exact {
        let mut sel = json!({"type": "text_quote", "exact": exact});
        if let Some(p) = args.prefix {
            sel["prefix"] = json!(p);
        }
        if let Some(s) = args.suffix {
            sel["suffix"] = json!(s);
        }
        json!({"kind": "text_quote", "block": args.block, "selector": sel})
    } else {
        json!({"kind": "block", "block": args.block})
    };

    let mut ann = json!({
        "id": id.as_str(),
        "type": args.ann_type,
        "target": target,
        "value": value,
        "author": {"type": atype, "id": aid},
        "created_at": now(),
    });
    if let Some(conf) = args.confidence {
        ann["confidence"] = json!(conf);
    }
    if let Some(src) = args.source {
        ann["source"] = json!(src);
    }

    // 读旧层（或建新层）
    let mut layer: Value = if c.has_entry(LAYER) {
        match c.read_entry(LAYER) {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(v) => v,
                Err(e) => {
                    return die(ContainerError::Manifest(format!(
                        "annotations.json 解析失败: {e}"
                    )))
                }
            },
            Err(e) => return die(e),
        }
    } else {
        json!({"schema_version": "1.0", "annotations": []})
    };
    if let Some(arr) = layer.get_mut("annotations").and_then(Value::as_array_mut) {
        arr.push(ann.clone());
    }

    if let Err(e) = c.set_entry(LAYER, pretty(&layer)) {
        return die(e);
    }
    let sha = match c.read_entry(LAYER) {
        Ok(b) => sha256_hex(&b),
        Err(e) => return die(e),
    };
    let mut manifest = c.manifest_value().clone();
    if let Some(layers) = manifest.get_mut("layers").and_then(Value::as_object_mut) {
        let needs = layers
            .get("semantics")
            .and_then(Value::as_object)
            .map(|o| o.get("path").and_then(Value::as_str) != Some(LAYER))
            .unwrap_or(true);
        if needs {
            layers.insert(
                "semantics".to_string(),
                json!({"path": LAYER, "sha256": sha}),
            );
        }
    }
    if let Err(e) = c.set_manifest(manifest) {
        return die(e);
    }
    let bytes = match c.write() {
        Ok(b) => b,
        Err(e) => return die(e),
    };
    if let Err(e) = std::fs::write(path, bytes) {
        eprintln!("错误：容器回写失败: {e}");
        return 1;
    }
    println!("已添加标注 {}", id.as_str());
    0
}

pub fn cmd_annotations(path: &Path, as_json: bool) -> i32 {
    let (mut c, _) = match crate::read_container(path) {
        Ok(x) => x,
        Err(code) => return code,
    };
    const LAYER: &str = "semantics/annotations.json";
    if !c.has_entry(LAYER) {
        if as_json {
            println!("{{\"annotations\": []}}");
        } else {
            println!("该文档没有语义标注层。");
        }
        return 0;
    }
    let bytes = match c.read_entry(LAYER) {
        Ok(b) => b,
        Err(e) => return die(e),
    };
    let layer: Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => {
            return die(ContainerError::Manifest(format!(
                "annotations.json 解析失败: {e}"
            )))
        }
    };
    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&layer).unwrap_or_else(|_| "{}".into())
        );
        return 0;
    }
    let empty = Vec::new();
    let anns = layer
        .get("annotations")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    println!("语义标注（{} 条）：", anns.len());
    for a in anns {
        let id = a.get("id").and_then(Value::as_str).unwrap_or("?");
        let ty = a.get("type").and_then(Value::as_str).unwrap_or("?");
        let kind = a
            .pointer("/target/kind")
            .and_then(Value::as_str)
            .unwrap_or("?");
        let block = a
            .pointer("/target/block")
            .and_then(Value::as_str)
            .unwrap_or("-");
        let detached = a.get("detached") == Some(&Value::Bool(true));
        println!(
            "  {id}  [{ty}] {kind} @{block}{}",
            if detached {
                " （失配，已标记 detached）"
            } else {
                ""
            }
        );
    }
    0
}

fn collect_block_ids(content: &Value) -> Vec<String> {
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
