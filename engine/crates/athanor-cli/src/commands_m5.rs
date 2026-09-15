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

//! M5 命令：PDF 出版（publish）。
//!
//! 管线：Prima →（azodoc-html）→ 语义 HTML →（印刷 CSS 注入）→ 无头 Chromium 打印
//! → PDF。publication.json 记录冻结出版状态（source_revision / 渲染器指纹 /
//! content_hash / layout_hash / artifact sha256），落地方案 §14 的 Publication Layer。

use azodoc_container::{Container, ContainerError};
use azodoc_convert::{ExportAsset, ExportDoc};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

use crate::{die, read_container, sha256_hex};

pub const PRINT_CSS_VERSION: &str = "print-css-v1";
pub const PRINT_CSS: &str = "@page { size: A4; margin: 22mm 18mm; } \
h1,h2,h3,h4,h5,h6 { break-after: avoid; } \
table, figure, pre { break-inside: avoid; } \
.footnotes { break-before: page; }";

fn now() -> String {
    azodoc_container::builder::rfc3339_now()
}

fn pretty(v: &Value) -> Vec<u8> {
    let mut b = serde_json::to_vec_pretty(v).expect("序列化失败");
    b.push(b'\n');
    b
}

fn unique_tmp(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("athanor-pdf-{tag}-{}-{nanos}", std::process::id()))
}

/// 装载导出所需的资产与 preserved 载荷。
fn load_export_doc(
    c: &mut Container,
    title: Option<String>,
    language: Option<String>,
) -> ExportDoc {
    let content: Value = c
        .read_entry("document/content.json")
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(json!({"schema_version": "1.0", "content": []}));

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
    let preserved = payload_refs
        .into_iter()
        .filter_map(|p| c.read_entry(&p).ok().map(|b| (p, b)))
        .collect();

    let m = c.manifest_typed();
    ExportDoc {
        content,
        assets,
        preserved,
        title: title.or_else(|| m.document.title.clone()),
        language: language.or_else(|| m.document.language.clone()),
        document_id: Some(m.document.id.clone()),
    }
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

pub struct PublishArgs<'a> {
    /// 额外把 PDF 复制到容器外路径
    pub out: Option<&'a Path>,
    /// 浏览器路径（覆盖 AZODOC_BROWSER_PATH / 自动定位）
    pub browser: Option<&'a str>,
}

pub fn cmd_publish(path: &Path, args: &PublishArgs) -> i32 {
    let browser_override: Option<String> = args.browser.map(String::from);
    if let Some(b) = &browser_override {
        std::env::set_var("AZODOC_BROWSER_PATH", b);
    }

    let (mut c, _) = match read_container(path) {
        Ok(x) => x,
        Err(code) => return code,
    };

    // 1. 导出印刷用 HTML
    let doc = load_export_doc(&mut c, None, None);
    let mut seed_log = azodoc_convert::LossLog::new();
    let html = azodoc_html::export_html(&doc, &mut seed_log);
    let print_html = html.replace(
        "</head>",
        &format!("<style id=\"azodoc-print\">{}</style>\n</head>", PRINT_CSS),
    );

    // 2. 无头打印
    let browser = match azodoc_pdf::find_browser() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{}", e.friendly());
            return 1;
        }
    };
    let tmp = unique_tmp("publish");
    if let Err(e) = std::fs::create_dir_all(&tmp) {
        eprintln!("错误：临时目录创建失败: {e}");
        return 1;
    }
    let html_path = tmp.join("print.html");
    let pdf_path = tmp.join("document.pdf");
    if let Err(e) = std::fs::write(&html_path, print_html.as_bytes()) {
        eprintln!("错误：印刷 HTML 写入失败: {e}");
        return 1;
    }
    if let Err(e) =
        azodoc_pdf::print_html_to_pdf(&browser, &html_path, &pdf_path, azodoc_pdf::DEFAULT_TIMEOUT)
    {
        eprintln!("{}", e.friendly());
        let _ = std::fs::remove_dir_all(&tmp);
        return 1;
    }
    let pdf_bytes = match std::fs::read(&pdf_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("错误：PDF 读取失败: {e}");
            return 1;
        }
    };
    if !pdf_bytes.starts_with(b"%PDF") {
        eprintln!("错误：产物不是有效 PDF");
        return 1;
    }
    let pages = azodoc_pdf::pdf_page_count(&pdf_bytes);

    // 3. 出版记录
    let content_bytes = c
        .read_entry("document/content.json")
        .map_err(die)
        .unwrap_or_default();
    let content_hash = sha256_hex(&content_bytes);
    let pdf_sha = sha256_hex(&pdf_bytes);
    let pub_id = azodoc_model::id::AzodocId::generate(azodoc_model::id::IdKind::Pub);
    let layout_hash = {
        use sha2::Digest;
        let mut h = sha2::Sha256::new();
        h.update(browser.fingerprint.as_bytes());
        h.update(content_hash.as_bytes());
        h.update(PRINT_CSS_VERSION.as_bytes());
        let d = h.finalize();
        d.iter().map(|b| format!("{b:02x}")).collect::<String>()
    };
    let revision = c.manifest_typed().current_revision.clone();
    // 每次出版的产物独立存储（出版是冻结历史，互不覆盖）
    let artifact_path = format!("compatibility/pdf/{pub_id}.pdf");

    let publication = json!({
        "id": pub_id.as_str(),
        "source_revision": revision,
        "created_at": now(),
        "renderer": {
            "name": "athanor-pdf",
            "version": env!("CARGO_PKG_VERSION"),
            "engine": format!("chromium-print ({})", browser.path.display()),
            "fingerprint": browser.fingerprint,
        },
        "artifact": {"path": artifact_path, "sha256": pdf_sha},
        "page": {"count": pages.unwrap_or(0), "size": "A4"},
        "content_hash": content_hash,
        "layout_hash": layout_hash,
        "signature": null,
    });

    // 4. 容器装配：publication 层 + pdf 兼容缓存 + 报告
    const PUB_LAYER: &str = "publication/publication.json";
    let mut layer: Value = if c.has_entry(PUB_LAYER) {
        let bytes = match c.read_entry(PUB_LAYER) {
            Ok(b) => b,
            Err(e) => return die(e),
        };
        match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(e) => {
                return die(ContainerError::Manifest(format!(
                    "publication.json 解析失败: {e}"
                )))
            }
        }
    } else {
        json!({"schema_version": "1.0", "publications": []})
    };
    if let Some(arr) = layer.get_mut("publications").and_then(Value::as_array_mut) {
        arr.push(publication);
    }

    c.set_entry(PUB_LAYER, pretty(&layer)).map_err(die).unwrap();
    c.set_entry(&artifact_path, pdf_bytes.clone())
        .map_err(die)
        .unwrap();

    let mut manifest = c.manifest_value().clone();
    // layers.publication 注册
    {
        let sha = sha256_hex(&c.read_entry(PUB_LAYER).unwrap_or_default());
        if let Some(layers) = manifest.get_mut("layers").and_then(Value::as_object_mut) {
            let needs = layers
                .get("publication")
                .and_then(Value::as_object)
                .map(|o| o.get("path").and_then(Value::as_str) != Some(PUB_LAYER))
                .unwrap_or(true);
            if needs {
                layers.insert(
                    "publication".to_string(),
                    json!({"path": PUB_LAYER, "sha256": sha}),
                );
            }
        }
    }
    // compatibility[].pdf 刷新
    let current_rev = manifest
        .get("current_revision")
        .cloned()
        .unwrap_or(Value::Null);
    let pdf_entry = json!({
        "format": "pdf",
        "path": artifact_path,
        "revision": current_rev,
        "generated_at": now(),
        "generator": {"name": "athanor-pdf", "version": env!("CARGO_PKG_VERSION")},
        "status": "fresh",
        "content_sha256": content_hash,
    });
    if let Some(arr) = manifest
        .get_mut("compatibility")
        .and_then(Value::as_array_mut)
    {
        arr.retain(|e| e.get("format").and_then(Value::as_str) != Some("pdf"));
        arr.push(pdf_entry);
    } else {
        manifest["compatibility"] = json!([pdf_entry]);
    }
    // 报告登记（PDF 导出）
    let prefix = "export-pdf";
    let mut seq = 1u32;
    if let Some(reports) = manifest.get("reports").and_then(Value::as_array) {
        for r in reports {
            if let Some(p) = r.get("path").and_then(Value::as_str) {
                if let Some(stem) = p
                    .rsplit('/')
                    .next()
                    .and_then(|n| n.strip_prefix(&format!("{prefix}-")))
                    .and_then(|n| n.strip_suffix(".json"))
                {
                    if let Ok(n) = stem.parse::<u32>() {
                        seq = seq.max(n + 1);
                    }
                }
            }
        }
    }
    let report = json!({
        "report_version": "1.0",
        "direction": "export",
        "source": {"format": "azodoc", "sha256": content_hash},
        "target": {
            "format": "pdf",
            "document_id": manifest.pointer("/document/id"),
            "revision": manifest.get("current_revision"),
        },
        "engine": {"name": "athanor", "version": env!("CARGO_PKG_VERSION"), "converter": "athanor-pdf"},
        "status": "complete",
        "summary": {"counts": {"blocks": 0}, "loss": {
            "none": 0, "partial": 0, "degraded": 0, "unsupported": 0, "preserved_raw": 0
        }},
        "issues": [],
    });
    let report_bytes = pretty(&report);
    let report_path = format!("reports/{prefix}-{seq:04}.json");
    if let Some(arr) = manifest.get_mut("reports").and_then(Value::as_array_mut) {
        arr.push(json!({
            "path": report_path,
            "kind": "conversion",
            "sha256": sha256_hex(&report_bytes),
        }));
    }

    if let Err(e) = c.set_manifest(manifest) {
        return die(e);
    }
    if let Err(e) = c.set_entry(&report_path, report_bytes) {
        return die(e);
    }

    let new_bytes = match c.write() {
        Ok(b) => b,
        Err(e) => return die(e),
    };
    if let Err(e) = std::fs::write(path, new_bytes) {
        eprintln!("错误：容器回写失败: {e}");
        return 1;
    }

    // 5. 外部副本
    if let Some(out) = args.out {
        if let Err(e) = std::fs::write(out, &pdf_bytes) {
            eprintln!("错误：外部副本写入失败: {e}");
            return 1;
        }
        println!("外部副本: {}", out.display());
    }

    let _ = std::fs::remove_dir_all(&tmp);
    println!(
        "已出版 {}（{}）",
        pub_id.as_str(),
        match pages {
            Some(n) => format!("{n} 页"),
            None => "页数未知".to_string(),
        }
    );
    println!("  修订: {}", revision.as_deref().unwrap_or("<无>"));
    println!("  产物: {artifact_path}（sha256 {pdf_sha}…）");
    println!("  layout_hash: {layout_hash}");
    0
}
