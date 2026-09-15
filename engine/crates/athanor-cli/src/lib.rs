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

//! athanor 命令实现（`athanor-cli` 的库形态，供集成测试直接调用）。

pub use commands_m3::{
    cmd_annotate, cmd_annotations, cmd_checkout, cmd_commit, cmd_history, AnnotateArgs,
};
pub mod commands_m3;
pub mod commands_m5;
pub use commands_m5::{cmd_publish, PublishArgs};
pub mod verify_cmd;

use azodoc_container::{builder, Container, ContainerError};
use azodoc_convert::{
    build_report, ExportAsset, ExportDoc, ImportJob, LossClass, LossLog, ReportMeta,
};
use azodoc_model::id::{AzodocId, IdKind};
use serde_json::{json, Value};
use std::path::Path;

pub fn sha256_hex(data: &[u8]) -> String {
    use sha2::Digest;
    let d = sha2::Sha256::digest(data);
    let mut s = String::with_capacity(d.len() * 2);
    for b in d {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

pub(crate) fn die(e: ContainerError) -> i32 {
    eprintln!("{}", e.friendly());
    1
}

fn print_loss_summary(log: &LossLog) {
    let total_issues = log.events().len();
    if !log.has_loss() {
        println!("完成，无损失。");
        return;
    }
    let parts: Vec<String> = LossClass::all()
        .iter()
        .filter(|c| **c != LossClass::None)
        .map(|c| format!("{} {}", c.as_str(), log.count(*c)))
        .collect();
    println!("完成，含 {} 处降级（{}）", total_issues, parts.join(", "));
    for e in log.events().iter().take(10) {
        println!("  · [{}] {}: {}", e.class.as_str(), e.feature, e.message);
    }
    if total_issues > 10 {
        println!("  … 其余 {} 条见转换报告", total_issues - 10);
    }
}

fn read_file(path: &Path) -> Result<Vec<u8>, i32> {
    std::fs::read(path).map_err(|e| {
        eprintln!("错误：无法读取 {}: {e}", path.display());
        1
    })
}

pub(crate) fn read_container(path: &Path) -> Result<(Container, Vec<String>), i32> {
    let data = read_file(path)?;
    azodoc_container::open(data).map_err(|e| {
        eprintln!("{}", e.friendly());
        1
    })
}

fn now_rfc3339() -> String {
    builder::rfc3339_now()
}

fn generator() -> Value {
    json!({ "name": "athanor", "version": env!("CARGO_PKG_VERSION") })
}

// ---------------------------------------------------------------- new / info / recover

pub fn cmd_new(path: &Path, title: Option<&str>, lang: &str) -> i32 {
    if path.exists() {
        eprintln!(
            "错误：目标文件已存在，athanor new 不覆盖已有文件: {}",
            path.display()
        );
        return 1;
    }
    let title = title.unwrap_or("未命名文档");
    match builder::build_minimal_document(title, lang) {
        Ok(bytes) => match std::fs::write(path, &bytes) {
            Ok(_) => {
                println!("已创建 {}（{title}）", path.display());
                println!("  查看: athanor info {}", path.display());
                println!("  校验: athanor verify {}", path.display());
                0
            }
            Err(e) => {
                eprintln!("错误：写入失败: {e}");
                1
            }
        },
        Err(e) => die(e),
    }
}

pub fn cmd_info(path: &Path, as_json: bool) -> i32 {
    let (mut c, warnings) = match read_container(path) {
        Ok(x) => x,
        Err(code) => return code,
    };
    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(c.manifest_value()).unwrap_or_else(|_| "{}".into())
        );
        print_warnings(&warnings);
        return 0;
    }

    let m = c.manifest_typed().clone();
    let profile_str = match c.profile {
        azodoc_container::Profile::Prefixed => "prefixed",
        azodoc_container::Profile::PlainZip => "plain_zip",
    };
    println!(
        "文档:   {} ({})",
        m.document.title.as_deref().unwrap_or("<未命名>"),
        m.document.id
    );
    println!("格式:   {} · {}", m.azodoc.format_version, profile_str);
    println!(
        "修订:   {}",
        m.current_revision.as_deref().unwrap_or("（无）")
    );
    println!("条目:   {}", c.entry_names().len());
    println!("层:");
    for (key, v) in m.layers.iter() {
        match serde_json::from_value::<azodoc_model::manifest::LayerRef>(v.clone()) {
            Ok(lr) => println!(
                "  {key:<14} {}  {}…",
                lr.path,
                &lr.sha256[..lr.sha256.len().min(8)]
            ),
            Err(_) => println!("  {key:<14} <结构未识别，按 R2 保留>"),
        }
    }
    println!("兼容缓存:");
    let content_sha = c
        .read_entry("document/content.json")
        .ok()
        .map(|b| sha256_hex(&b));
    for e in &m.compatibility {
        let mut fresh = m.compat_is_fresh(e);
        if let (Some(sha), Some(cur)) = (
            e.extra.0.get("content_sha256").and_then(Value::as_str),
            &content_sha,
        ) {
            fresh = fresh && sha == cur;
        }
        println!(
            "  {:<10} {:<38} {}",
            e.format,
            e.path,
            if fresh {
                "fresh"
            } else {
                "stale（athanor upgrade 重建）"
            }
        );
    }
    let unknown = azodoc_model::manifest::unknown_manifest_key_paths(c.manifest_value());
    if !unknown.is_empty() {
        println!("未知 manifest 字段（R2 保留）: {}", unknown.join(", "));
    }
    print_warnings(&warnings);
    0
}

pub(crate) fn print_warnings(warnings: &[String]) {
    for w in warnings {
        println!("警告: {w}");
    }
}

pub fn cmd_recover(path: &Path, out: &Path) -> i32 {
    let data = match read_file(path) {
        Ok(d) => d,
        Err(code) => return code,
    };
    match azodoc_container::recover::recover(&data, out) {
        Ok(outcome) => {
            println!("模式: {}", outcome.mode);
            for line in &outcome.lines {
                match line {
                    Ok(msg) => println!("  OK    {msg}"),
                    Err(msg) => println!("  失败  {msg}"),
                }
            }
            println!(
                "汇总: 成功 {} · 失败 {} · 输出目录 {}",
                outcome.salvaged,
                outcome.failed,
                outcome.out_dir.display()
            );
            println!("报告: {}", out.join("recovery-report.txt").display());
            if outcome.salvaged == 0 {
                1
            } else {
                0
            }
        }
        Err(e) => die(e),
    }
}

// ---------------------------------------------------------------- import

fn detect_format(path: &Path, explicit: Option<&str>) -> Option<String> {
    if let Some(f) = explicit {
        return normalize_format(f);
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "md" | "markdown" => Some("markdown".into()),
        "html" | "htm" => Some("html".into()),
        "docx" => Some("docx".into()),
        _ => None,
    }
}

fn normalize_format_or_text(f: &str) -> Option<String> {
    match f.to_ascii_lowercase().as_str() {
        "md" | "markdown" => Some("markdown".into()),
        "html" | "htm" => Some("html".into()),
        "txt" | "text" => Some("text".into()),
        "docx" => Some("docx".into()),
        _ => None,
    }
}

fn normalize_format(f: &str) -> Option<String> {
    match f.to_ascii_lowercase().as_str() {
        "md" | "markdown" => Some("markdown".into()),
        "html" | "htm" => Some("html".into()),
        "docx" => Some("docx".into()),
        _ => None,
    }
}

/// compat 缓存路径映射。
pub fn compat_path(fmt: &str) -> Option<&'static str> {
    match fmt {
        "markdown" => Some("compatibility/markdown/document.md"),
        "text" => Some("compatibility/text/document.txt"),
        "html" => Some("compatibility/html/index.html"),
        "docx" => Some("compatibility/docx/document.docx"),
        _ => None,
    }
}

pub fn cmd_import(
    input: &Path,
    out: &Path,
    format: Option<&str>,
    lang: &str,
    report_out: Option<&Path>,
    strict_loss: bool,
) -> i32 {
    if out.exists() {
        eprintln!(
            "错误：目标文件已存在，athanor import 不覆盖已有文件: {}",
            out.display()
        );
        return 1;
    }
    let Some(fmt) = detect_format(input, format) else {
        eprintln!(
            "错误：无法识别输入格式（按扩展名 {} 或 --format）。支持: markdown, html, docx",
            input.extension().and_then(|e| e.to_str()).unwrap_or("<无>")
        );
        return 2;
    };
    let data = match read_file(input) {
        Ok(d) => d,
        Err(code) => return code,
    };
    let text = String::from_utf8_lossy(&data).to_string();

    let mut job = ImportJob::new(input.parent().map(Path::to_path_buf));
    let output = match fmt.as_str() {
        "markdown" => azodoc_md::import(&text, &mut job),
        "html" => azodoc_html::import(&text, &mut job),
        "docx" => match azodoc_docx::import(&data, &mut job) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("{}", e.friendly());
                return 1;
            }
        },
        _ => unreachable!(),
    };
    let source_sha = sha256_hex(&data);

    // 装配容器
    let doc_id = AzodocId::generate(IdKind::Doc);
    let now = now_rfc3339();
    let content_bytes = {
        let mut b = serde_json::to_vec_pretty(&output.content).expect("content 序列化失败");
        b.push(b'\n');
        b
    };
    let content_sha = sha256_hex(&content_bytes);

    // 兼容缓存种子：TXT（最后一道恢复通道）+ 同格式
    let doc_for_export = export_doc_from_parts(
        &output.content,
        &job,
        output.title.clone(),
        output.language.clone(),
        Some(doc_id.as_str().to_string()),
    );
    let mut compat_entries: Vec<Value> = Vec::new();
    let mut compat_files: Vec<(String, Vec<u8>)> = Vec::new();
    {
        let mut seed_log = LossLog::new();
        let txt = azodoc_convert::txt::export_txt(&doc_for_export, &mut seed_log);
        compat_files.push((compat_path("text").unwrap().to_string(), txt.into_bytes()));
        compat_entries.push(compat_entry_json(
            "text",
            compat_path("text").unwrap(),
            &now,
            &content_sha,
        ));
        let same_fmt = match fmt.as_str() {
            "markdown" => {
                Some(azodoc_md::export_markdown(&doc_for_export, &mut seed_log).into_bytes())
            }
            "html" => Some(azodoc_html::export_html(&doc_for_export, &mut seed_log).into_bytes()),
            _ => None,
        };
        if let Some(bytes) = same_fmt {
            let p = compat_path(&fmt).unwrap();
            compat_files.push((p.to_string(), bytes));
            compat_entries.push(compat_entry_json(&fmt, p, &now, &content_sha));
        }
    }

    // 资产
    let mut registry_assets: Vec<Value> = Vec::new();
    let mut asset_files: Vec<(String, Vec<u8>)> = Vec::new();
    for (i, a) in job.assets.iter().enumerate() {
        let id = extract_asset_id(&output.content, i).unwrap_or_default();
        let mut entry = json!({
            "id": id,
            "filename": a.filename,
            "mime": a.mime,
            "relationship": a.relationship,
        });
        match &a.source {
            azodoc_convert::AssetSource::Embedded(bytes) => {
                let path = format!("assets/{id}/{}", a.filename);
                entry["storage"] = json!("embedded");
                entry["path"] = json!(path);
                entry["size"] = json!(bytes.len());
                entry["sha256"] = json!(sha256_hex(bytes));
                asset_files.push((path, bytes.clone()));
            }
            azodoc_convert::AssetSource::External(url) => {
                entry["storage"] = json!("external");
                entry["url"] = json!(url);
                entry["size"] = json!(0);
                entry["sha256"] = json!(sha256_hex(url.as_bytes()));
            }
        }
        registry_assets.push(entry);
    }
    let registry = json!({ "schema_version": "1.0", "assets": registry_assets });
    let registry_bytes = pretty(&registry);

    // 转换报告
    let report = build_report(
        &ReportMeta {
            direction: "import",
            source_format: &fmt,
            target_format: "azodoc",
            source_path: input.to_str(),
            source_sha256: Some(&source_sha),
            document_id: Some(doc_id.as_str()),
            revision: None,
            converter: match fmt.as_str() {
                "markdown" => azodoc_md::converter_name(),
                _ => azodoc_html::converter_name(),
            },
        },
        &output.log,
    );
    let report_bytes = pretty(&report);
    let report_path = format!("reports/import-{fmt}-0001.json");

    // manifest
    let mut document = json!({
        "id": doc_id.as_str(),
        "schema_version": "1.0",
        "title": output.title.clone().unwrap_or_else(|| input.file_stem().unwrap_or_default().to_string_lossy().to_string()),
        "language": output.language.clone().unwrap_or_else(|| lang.to_string()),
        "created_at": now,
        "modified_at": now,
    });
    if !output.doc_extra.is_empty() {
        if let Some(dobj) = document.as_object_mut() {
            for (k, v) in &output.doc_extra {
                dobj.insert(k.clone(), v.clone());
            }
        }
    }
    let manifest = json!({
        "azodoc": {
            "format_version": "1.0",
            "container_profile": "prefixed",
            "generator": generator(),
        },
        "document": document,
        "current_revision": null,
        "layers": {
            "content": { "path": "document/content.json", "sha256": content_sha },
            "assets": { "path": "assets/registry.json", "sha256": sha256_hex(&registry_bytes) },
        },
        "compatibility": compat_entries,
        "reports": [ { "path": report_path, "kind": "conversion", "sha256": sha256_hex(&report_bytes) } ],
    });
    let manifest_bytes = pretty(&manifest);

    // 打包
    let mut entries: Vec<(String, Vec<u8>, bool)> =
        vec![("manifest.json".to_string(), manifest_bytes, true)];
    entries.push(("document/content.json".to_string(), content_bytes, false));
    entries.push(("assets/registry.json".to_string(), registry_bytes, false));
    for (p, bytes) in asset_files {
        entries.push((p, bytes, true));
    }
    for (path, bytes) in &job.preserved {
        entries.push((path.clone(), bytes.clone(), true));
    }
    for (p, bytes) in compat_files {
        entries.push((p, bytes, false));
    }
    entries.push((report_path.clone(), report_bytes, false));

    let bytes = match builder::pack(entries) {
        Ok(b) => b,
        Err(e) => return die(e),
    };

    // 初始修订：importer 落链（author type 一等公民）
    let bytes = {
        let (mut c, _) = match azodoc_container::open(bytes) {
            Ok(x) => x,
            Err(e) => return die(e),
        };
        let fname = input
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();
        let converter = match fmt.as_str() {
            "markdown" => azodoc_md::converter_name(),
            _ => azodoc_html::converter_name(),
        };
        if let Err(e) = c.commit(&azodoc_container::revisions::CommitInfo {
            author_type: "importer",
            author_id: converter,
            message: &format!("import from {fname}"),
        }) {
            return die(e);
        }
        match c.write() {
            Ok(b) => b,
            Err(e) => return die(e),
        }
    };

    if let Err(e) = std::fs::write(out, &bytes) {
        eprintln!("错误：写入失败: {e}");
        return 1;
    }

    println!("已导入 {} → {}（{fmt}）", input.display(), out.display());
    print_loss_summary(&output.log);
    println!("报告: 容器内 {report_path}");
    if let Some(rp) = report_out {
        let _ = std::fs::write(rp, pretty(&report));
        println!("报告副本: {}", rp.display());
    }
    if strict_loss && output.log.has_loss() {
        return 3;
    }
    0
}

fn compat_entry_json(fmt: &str, path: &str, now: &str, content_sha: &str) -> Value {
    json!({
        "format": fmt,
        "path": path,
        "revision": null,
        "generated_at": now,
        "generator": generator(),
        "status": "fresh",
        "content_sha256": content_sha,
    })
}

fn pretty(v: &Value) -> Vec<u8> {
    let mut b = serde_json::to_vec_pretty(v).expect("序列化失败");
    b.push(b'\n');
    b
}

/// 从 content 树中提取第 i 个引用的资产 id（与 job.assets 的登记顺序一致）。
fn extract_asset_id(content: &Value, index: usize) -> Option<String> {
    let mut ids = Vec::new();
    collect_asset_ids(content, &mut ids);
    ids.get(index).cloned()
}

fn collect_asset_ids(v: &Value, ids: &mut Vec<String>) {
    if let Some(s) = v.as_str() {
        if let Some(rest) = s.strip_prefix("asset://") {
            let id = rest.split('/').next().unwrap_or(rest);
            if !ids.iter().any(|x| x == id) {
                ids.push(id.to_string());
            }
        }
        return;
    }
    if let Some(obj) = v.as_object() {
        for val in obj.values() {
            collect_asset_ids(val, ids);
        }
    } else if let Some(arr) = v.as_array() {
        for item in arr {
            collect_asset_ids(item, ids);
        }
    }
}

fn export_doc_from_parts(
    content: &Value,
    job: &ImportJob,
    title: Option<String>,
    language: Option<String>,
    document_id: Option<String>,
) -> ExportDoc {
    // 注册表顺序 = job.assets 顺序；id 通过与 content 相同的收集规则对应
    let mut ids_in_content = Vec::new();
    collect_asset_ids(content, &mut ids_in_content);
    let assets = job
        .assets
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let id = ids_in_content.get(i).cloned().unwrap_or_default();
            let (storage, url, bytes) = match &a.source {
                azodoc_convert::AssetSource::Embedded(b) => ("embedded", None, Some(b.clone())),
                azodoc_convert::AssetSource::External(u) => ("external", Some(u.clone()), None),
            };
            ExportAsset {
                id,
                filename: a.filename.clone(),
                mime: a.mime.clone(),
                storage: storage.to_string(),
                url,
                bytes,
            }
        })
        .collect();
    ExportDoc {
        content: content.clone(),
        assets,
        preserved: job
            .preserved
            .iter()
            .map(|(p, b)| (p.clone(), b.clone()))
            .collect(),
        title,
        language,
        document_id,
    }
}

// ---------------------------------------------------------------- transmute

pub fn cmd_transmute(
    path: &Path,
    to: &str,
    out: Option<&Path>,
    no_cache: bool,
    strict_loss: bool,
) -> i32 {
    let Some(fmt) = normalize_format_or_text(to) else {
        eprintln!("错误：不支持的输出格式 `{to}`。支持: markdown/md, text/txt, html");
        return 2;
    };
    let Some(compat) = compat_path(&fmt) else {
        eprintln!("错误：格式 {fmt} 的兼容缓存路径未定义");
        return 2;
    };
    let (mut c, _warnings) = match read_container(path) {
        Ok(x) => x,
        Err(code) => return code,
    };

    let content_bytes = match c.read_entry("document/content.json") {
        Ok(b) => b,
        Err(e) => return die(e),
    };
    let content: Value = serde_json::from_slice(&content_bytes)
        .unwrap_or(json!({"schema_version":"1.0","content":[]}));
    let m = c.manifest_typed().clone();

    // 资产装载
    let mut assets = Vec::new();
    if let Ok(reg_bytes) = c.read_entry("assets/registry.json") {
        if let Ok(reg) = serde_json::from_slice::<Value>(&reg_bytes) {
            if let Some(list) = reg.get("assets").and_then(Value::as_array) {
                for a in list {
                    let id = a
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let storage = a
                        .get("storage")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let asset_path = a.get("path").and_then(Value::as_str).map(String::from);
                    let bytes = match (&asset_path, storage.as_str()) {
                        (Some(p), "embedded") => c.read_entry(p).ok(),
                        _ => None,
                    };
                    assets.push(ExportAsset {
                        id,
                        filename: a
                            .get("filename")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        mime: a
                            .get("mime")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        storage,
                        url: a.get("url").and_then(Value::as_str).map(String::from),
                        bytes,
                    });
                }
            }
        }
    }

    // preserved 载荷装载
    let mut payload_refs = Vec::new();
    collect_payload_refs(&content, &mut payload_refs);
    let mut preserved = Vec::new();
    for p in payload_refs {
        if let Ok(bytes) = c.read_entry(&p) {
            preserved.push((p, bytes));
        }
    }

    let doc = ExportDoc {
        content,
        assets,
        preserved,
        title: m.document.title.clone(),
        language: m.document.language.clone(),
        document_id: Some(m.document.id.clone()),
    };

    let mut log = LossLog::new();
    let (bytes, ext) = match fmt.as_str() {
        "markdown" => (
            azodoc_md::export_markdown(&doc, &mut log).into_bytes(),
            "md",
        ),
        "text" => (
            azodoc_convert::txt::export_txt(&doc, &mut log).into_bytes(),
            "txt",
        ),
        "html" => (
            azodoc_html::export_html(&doc, &mut log).into_bytes(),
            "html",
        ),
        "docx" => match azodoc_docx::export(&doc, &mut log) {
            Ok(b) => (b, "docx"),
            Err(e) => {
                eprintln!("{}", e.friendly());
                return 1;
            }
        },
        _ => unreachable!(),
    };

    // 输出文件
    let out_path = match out {
        Some(p) => p.to_path_buf(),
        None => path.with_extension(ext),
    };
    if let Err(e) = std::fs::write(&out_path, &bytes) {
        eprintln!("错误：写入 {} 失败: {e}", out_path.display());
        return 1;
    }

    println!(
        "已导出 {} → {}（{fmt}，{} 字节）",
        path.display(),
        out_path.display(),
        bytes.len()
    );
    print_loss_summary(&log);

    // 刷新兼容缓存 + 登记 report
    let mut cache_note = String::new();
    if !no_cache {
        match refresh_cache(&mut c, &fmt, compat, &bytes, &content_bytes, &log) {
            Ok(()) => match c.write() {
                Ok(new_bytes) => {
                    if let Err(e) = std::fs::write(path, new_bytes) {
                        eprintln!("错误：容器回写失败: {e}");
                        return 1;
                    }
                    cache_note = format!("；兼容缓存已刷新（{compat}）");
                }
                Err(e) => {
                    eprintln!("错误：容器重写失败: {e}");
                    return 1;
                }
            },
            Err(e) => {
                eprintln!("错误：缓存刷新失败: {e}");
                return 1;
            }
        }
    }
    println!("缓存: {cache_note}");

    if strict_loss && log.has_loss() {
        return 3;
    }
    0
}

fn collect_payload_refs(v: &Value, refs: &mut Vec<String>) {
    if let Some(s) = v.as_str() {
        if s.starts_with("preserved/") && !refs.iter().any(|x| x == s) {
            refs.push(s.to_string());
        }
        return;
    }
    if let Some(obj) = v.as_object() {
        for val in obj.values() {
            collect_payload_refs(val, refs);
        }
    } else if let Some(arr) = v.as_array() {
        for item in arr {
            collect_payload_refs(item, refs);
        }
    }
}

/// upsert 兼容缓存条目 + 追加导出报告。
fn refresh_cache(
    c: &mut Container,
    fmt: &str,
    compat: &str,
    bytes: &[u8],
    content_bytes: &[u8],
    log: &LossLog,
) -> Result<(), ContainerError> {
    let mut manifest = c.manifest_value().clone();
    let now = now_rfc3339();
    let entry = json!({
        "format": fmt,
        "path": compat,
        "revision": manifest.get("current_revision").cloned().unwrap_or(Value::Null),
        "generated_at": now,
        "generator": generator(),
        "status": "fresh",
        "content_sha256": sha256_hex(content_bytes),
    });
    if let Some(arr) = manifest
        .get_mut("compatibility")
        .and_then(Value::as_array_mut)
    {
        arr.retain(|e| e.get("format").and_then(Value::as_str) != Some(fmt));
        arr.push(entry);
    } else {
        manifest["compatibility"] = json!([entry]);
    }

    let prefix = format!("export-{fmt}");
    let seq = next_report_seq(&manifest, &prefix);
    let report = build_report(
        &ReportMeta {
            direction: "export",
            source_format: "azodoc",
            target_format: fmt,
            source_path: None,
            source_sha256: Some(&sha256_hex(content_bytes)),
            document_id: manifest.pointer("/document/id").and_then(Value::as_str),
            revision: manifest.get("current_revision").and_then(Value::as_str),
            converter: match fmt {
                "markdown" => azodoc_md::converter_name(),
                "html" => azodoc_html::converter_name(),
                _ => "athanor-txt",
            },
        },
        log,
    );
    let report_bytes = pretty(&report);
    let report_path = format!("reports/{prefix}-{seq:04}.json");
    if let Some(arr) = manifest.get_mut("reports").and_then(Value::as_array_mut) {
        arr.push(json!({
            "path": report_path,
            "kind": "conversion",
            "sha256": sha256_hex(&report_bytes),
        }));
    } else {
        manifest["reports"] = json!([{
            "path": report_path,
            "kind": "conversion",
            "sha256": sha256_hex(&report_bytes),
        }]);
    }

    c.set_manifest(manifest)?;
    c.set_entry(compat, bytes.to_vec())?;
    c.set_entry(&report_path, report_bytes)?;
    Ok(())
}

fn next_report_seq(manifest: &Value, prefix: &str) -> u32 {
    let mut max = 0u32;
    if let Some(reports) = manifest.get("reports").and_then(Value::as_array) {
        for r in reports {
            if let Some(p) = r.get("path").and_then(Value::as_str) {
                let name = p.rsplit('/').next().unwrap_or("");
                if let Some(stem) = name.strip_prefix(&format!("{prefix}-")) {
                    if let Some(num) = stem.strip_suffix(".json") {
                        if let Ok(n) = num.parse::<u32>() {
                            max = max.max(n);
                        }
                    }
                }
            }
        }
    }
    max + 1
}

// ---------------------------------------------------------------- upgrade

pub fn cmd_upgrade(path: &Path) -> i32 {
    let (mut c, _warnings) = match read_container(path) {
        Ok(x) => x,
        Err(code) => return code,
    };
    let content_bytes = match c.read_entry("document/content.json") {
        Ok(b) => b,
        Err(e) => return die(e),
    };
    let content_sha = sha256_hex(&content_bytes);
    let content: Value = serde_json::from_slice(&content_bytes)
        .unwrap_or(json!({"schema_version":"1.0","content":[]}));
    let m = c.manifest_typed().clone();

    // 装载导出所需数据（与 transmute 相同）
    let mut assets = Vec::new();
    if let Ok(reg_bytes) = c.read_entry("assets/registry.json") {
        if let Ok(reg) = serde_json::from_slice::<Value>(&reg_bytes) {
            if let Some(list) = reg.get("assets").and_then(Value::as_array) {
                for a in list {
                    let id = a
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let storage = a
                        .get("storage")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let asset_path = a.get("path").and_then(Value::as_str).map(String::from);
                    let bytes = match (&asset_path, storage.as_str()) {
                        (Some(p), "embedded") => c.read_entry(p).ok(),
                        _ => None,
                    };
                    assets.push(ExportAsset {
                        id,
                        filename: a
                            .get("filename")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        mime: a
                            .get("mime")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        storage,
                        url: a.get("url").and_then(Value::as_str).map(String::from),
                        bytes,
                    });
                }
            }
        }
    }
    let mut payload_refs = Vec::new();
    collect_payload_refs(&content, &mut payload_refs);
    let mut preserved = Vec::new();
    for p in payload_refs {
        if let Ok(bytes) = c.read_entry(&p) {
            preserved.push((p, bytes));
        }
    }
    let doc = ExportDoc {
        content,
        assets,
        preserved,
        title: m.document.title.clone(),
        language: m.document.language.clone(),
        document_id: Some(m.document.id.clone()),
    };

    let mut manifest = c.manifest_value().clone();
    let mut rebuilt = 0usize;
    let mut skipped_unsupported = 0usize;

    let entries: Vec<Value> = m
        .compatibility
        .iter()
        .map(|e| {
            json!({
                "format": e.format,
                "path": e.path,
                "revision": e.revision,
                "content_sha256": e.extra.0.get("content_sha256").cloned().unwrap_or(Value::Null),
            })
        })
        .collect();
    for e in &entries {
        let fmt = e
            .get("format")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let Some(compat) = compat_path(&fmt) else {
            skipped_unsupported += 1;
            continue;
        };
        let bound_rev = e.get("revision").and_then(Value::as_str);
        let bound_sha = e.get("content_sha256").and_then(Value::as_str);
        let rev_ok = match (bound_rev, m.current_revision.as_deref()) {
            (None, None) => true,
            (Some(a), Some(b)) => a == b,
            _ => false,
        };
        let sha_ok = bound_sha.map(|s| s == content_sha).unwrap_or(true);
        if rev_ok && sha_ok {
            continue; // fresh，无需重建
        }
        let mut log = LossLog::new();
        let bytes = match fmt.as_str() {
            "markdown" => azodoc_md::export_markdown(&doc, &mut log).into_bytes(),
            "text" => azodoc_convert::txt::export_txt(&doc, &mut log).into_bytes(),
            "html" => azodoc_html::export_html(&doc, &mut log).into_bytes(),
            _ => continue,
        };
        // 更新条目
        let current_rev = manifest
            .get("current_revision")
            .cloned()
            .unwrap_or(Value::Null);
        let doc_id = manifest
            .pointer("/document/id")
            .cloned()
            .unwrap_or(Value::Null);
        if let Some(arr) = manifest
            .get_mut("compatibility")
            .and_then(Value::as_array_mut)
        {
            if let Some(target) = arr
                .iter_mut()
                .find(|x| x.get("format").and_then(Value::as_str) == Some(&fmt))
            {
                target["revision"] = current_rev.clone();
                target["generated_at"] = json!(now_rfc3339());
                target["generator"] = generator();
                target["status"] = json!("fresh");
                target["content_sha256"] = json!(content_sha);
                target["path"] = json!(compat);
            }
        }
        let prefix = format!("export-{fmt}");
        let seq = next_report_seq(&manifest, &prefix);
        let report = build_report(
            &ReportMeta {
                direction: "export",
                source_format: "azodoc",
                target_format: &fmt,
                source_path: None,
                source_sha256: Some(&content_sha),
                document_id: doc_id.as_str(),
                revision: manifest.get("current_revision").and_then(Value::as_str),
                converter: match fmt.as_str() {
                    "markdown" => azodoc_md::converter_name(),
                    "html" => azodoc_html::converter_name(),
                    _ => "athanor-txt",
                },
            },
            &log,
        );
        let report_bytes = pretty(&report);
        let report_path = format!("reports/{prefix}-{seq:04}.json");
        if let Some(arr) = manifest.get_mut("reports").and_then(Value::as_array_mut) {
            arr.push(json!({
                "path": report_path,
                "kind": "conversion",
                "sha256": sha256_hex(&report_bytes),
            }));
        } else {
            manifest["reports"] = json!([{
                "path": report_path,
                "kind": "conversion",
                "sha256": sha256_hex(&report_bytes),
            }]);
        }
        if let Err(e) = c.set_entry(compat, bytes) {
            return die(e);
        }
        if let Err(e) = c.set_entry(&report_path, report_bytes) {
            return die(e);
        }
        rebuilt += 1;
        println!("  重建 {fmt} → {compat}");
    }

    if let Err(e) = c.set_manifest(manifest) {
        return die(e);
    }
    if rebuilt > 0 {
        let new_bytes = match c.write() {
            Ok(b) => b,
            Err(e) => return die(e),
        };
        if let Err(e) = std::fs::write(path, new_bytes) {
            eprintln!("错误：容器回写失败: {e}");
            return 1;
        }
    }
    println!(
        "upgrade 完成：重建 {rebuilt} 个缓存{}",
        if skipped_unsupported > 0 {
            format!("，跳过 {skipped_unsupported} 个不支持的格式")
        } else {
            String::new()
        }
    );
    0
}
