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

//! 原生 OOXML 读取（B2：摆脱 Pandoc 的进程内读取器）。
//!
//! 管线：zip 打开 → 侧部件 preserved（归档序，确定性）→ styles/numbering/
//! comments/footnotes 解析 → document.xml 正文映射（`document::map_block_seq`）
//! → wrap_sections + merge_text_spans → theme/annotations 组装。
//! 损失政策见 RFC B2 §3/§4；纯进程内、无外部二进制（A3 GUI 的前置能力）。

pub mod comments;
pub mod document;
pub mod error;
pub use error::OoxmlError;
pub mod numbering;
pub mod package;
pub mod styles;
pub mod xml;

use crate::bridge;
use crate::ooxml::document::{TrackedStats, TRACKED};
use crate::ooxml::package::{sanitize_part_name, OoxmlPackage};
use crate::ooxml::styles::{build_theme, Styles};
use crate::ooxml::xml::El;
use azodoc_convert::{
    merge_text_spans, wrap_sections, ImportJob, ImportOutput, LossClass, LossLog,
};
use serde_json::{json, Map};
use std::collections::BTreeMap;

/// 读取入口（CLI 与库调用共用）。
pub enum DocxReadPath {
    /// 原生 OOXML 读取（进程内）
    Native,
    /// Pandoc 子进程桥（fallback / 显式指定）
    Pandoc,
}

impl DocxReadPath {
    pub fn converter_name(&self) -> &'static str {
        match self {
            DocxReadPath::Native => "athanor-docx",
            DocxReadPath::Pandoc => "athanor-docx-pandoc",
        }
    }
}

/// 统一读取入口：`reader` ∈ auto | native | pandoc。
///
/// auto = 原生优先，硬失败且 Pandoc 可用时回落（注入 `docx_reader_fallback`
/// 事件）；Pandoc 不可用时返回原生错误的四段式信息。
pub fn import_docx(
    docx: &[u8],
    job: &mut ImportJob,
    reader: &str,
) -> Result<(ImportOutput, DocxReadPath), String> {
    match reader {
        "native" => crate::ooxml::import_native(docx, job)
            .map(|o| (o, DocxReadPath::Native))
            .map_err(|e| e.friendly()),
        "pandoc" => crate::import(docx, job)
            .map(|o| (o, DocxReadPath::Pandoc))
            .map_err(|e| e.friendly()),
        "auto" => match crate::ooxml::import_native(docx, job) {
            Ok(o) => Ok((o, DocxReadPath::Native)),
            Err(native_err) => {
                if bridge::find_pandoc().is_ok() {
                    match crate::import(docx, job) {
                        Ok(mut o) => {
                            o.log.record(
                                LossClass::Partial,
                                "docx_reader_fallback",
                                None,
                                "degraded",
                                &native_err.to_string(),
                                "原生读取失败，已回落 Pandoc 桥完成导入".to_string(),
                            );
                            Ok((o, DocxReadPath::Pandoc))
                        }
                        Err(e) => Err(e.friendly()),
                    }
                } else {
                    Err(native_err.friendly())
                }
            }
        },
        other => Err(format!(
            "错误：未知 --reader 取值 {other}。\n\
             文件不受影响。取值：auto（默认，原生优先回落 Pandoc）/ native / pandoc。"
        )),
    }
}

/// 原生读取：docx 字节 → Prima + 标注 + 主题 + 损失报告。
pub fn import_native(docx: &[u8], job: &mut ImportJob) -> Result<ImportOutput, OoxmlError> {
    let mut pkg = OoxmlPackage::open(docx)?;
    let mut log = LossLog::new();

    // 侧部件 preserved（先于正文映射 → preserved 序号按归档序确定）
    preserve_side_parts(&mut pkg, job, &mut log)?;

    // settings.xml：是否启用修订标记
    let mut tracked = TrackedStats::default();
    if pkg.has_part("word/settings.xml") {
        if let Ok(settings) = pkg.part_tree("word/settings.xml") {
            tracked.enabled = settings.find_descendant("w", "trackChanges").is_some();
        }
    }

    let core = read_core_props(&mut pkg);
    let styles = Styles::parse(&mut pkg)?;
    let numbering = numbering::Numbering::parse(&mut pkg)?;
    let comments = comments::Comments::parse(&mut pkg)?;
    let footnotes = parse_notes(&mut pkg, "word/footnotes.xml", "footnote")?;
    let endnotes = parse_notes(&mut pkg, "word/endnotes.xml", "endnote")?;
    let rels = pkg.rels("word/document.xml")?;

    let doc = pkg.part_tree("word/document.xml")?;
    let body = doc
        .child("w", "body")
        .ok_or_else(|| OoxmlError::MissingPart("word/document.xml（w:body）".to_string()))?;
    // 原件留档用（有修订时整件 preserved，RFC 决策 6）
    let document_xml_bytes = pkg.read_part("word/document.xml")?;

    let annotations;
    let styles_used;
    let page;
    let default_styles;
    let content = {
        let mut ctx = document::BodyCtx {
            job,
            log: &mut log,
            pkg: &mut pkg,
            agg: document::Agg::new(),
            rels,
            styles,
            numbering,
            comments,
            footnotes,
            endnotes,
            footnote_defs: BTreeMap::new(),
            pending_footnotes: Vec::new(),
            pending_blocks: Vec::new(),
            annotations: Vec::new(),
            styles_used: Vec::new(),
            tracked,
            page: None,
            now: azodoc_container::builder::rfc3339_now(),
        };
        let mut blocks = document::map_block_seq(body.children.iter(), &mut ctx);
        blocks.append(&mut ctx.pending_footnotes);
        let wrapped = wrap_sections(&mut ctx.job.idgen, blocks);
        let mut content = json!({"schema_version": "1.0", "content": wrapped});
        merge_text_spans(&mut content);

        // 修订：终稿视图 + 原件 preserved + 聚合事件（决策 6）
        flush_tracked(&mut ctx, &document_xml_bytes);
        ctx.agg.flush(&mut *ctx.log);

        annotations = std::mem::take(&mut ctx.annotations);
        styles_used = std::mem::take(&mut ctx.styles_used);
        page = ctx.page.take();
        default_styles = std::mem::take(&mut ctx.styles);
        content
    };

    // theme（首个表现层写入器，决策 7）
    let theme = if !styles_used.is_empty()
        || page.is_some()
        || default_styles.defaults.font.is_some()
        || default_styles.defaults.size_half_points.is_some()
    {
        Some(build_theme(&default_styles, &styles_used, page.as_ref()))
    } else {
        None
    };

    // 文档元数据（creator/created/modified 走 doc_extra，R2 合并进 document）
    let mut doc_extra = Map::new();
    if let Some(c) = core.creator {
        doc_extra.insert("creator".into(), json!(c));
    }
    if let Some(c) = core.created {
        doc_extra.insert("created_at".into(), json!(c));
    }
    if let Some(m) = core.modified {
        doc_extra.insert("modified_at".into(), json!(m));
    }

    Ok(ImportOutput {
        content,
        title: core.title,
        language: core.language,
        doc_extra,
        log,
        annotations,
        theme,
    })
}

// ---------------------------------------------------------------- 侧部件

/// 归档序遍历：无法建模的部件整件 preserved + 事件（R3/R6）。
fn preserve_side_parts(
    pkg: &mut OoxmlPackage,
    job: &mut ImportJob,
    log: &mut LossLog,
) -> Result<(), OoxmlError> {
    for name in pkg.part_names() {
        let norm = match sanitize_part_name(&name) {
            Ok(n) => n,
            Err(_) => continue, // 消毒失败的条目不读取
        };
        let lower = norm.to_ascii_lowercase();
        if lower.ends_with(".rels") || lower == "[content_types].xml" || lower.starts_with("_rels/")
        {
            continue;
        }
        enum Kind {
            Skip,
            Headers,
            Footers,
            Ole,
            CustomXml,
            UnknownPart,
        }
        let kind = if lower.starts_with("word/header") && lower.ends_with(".xml") {
            Kind::Headers
        } else if lower.starts_with("word/footer") && lower.ends_with(".xml") {
            Kind::Footers
        } else if lower.starts_with("word/embeddings/")
            || lower == "word/vbaproject.bin"
            || lower.starts_with("word/activex/")
        {
            Kind::Ole
        } else if lower.starts_with("customxml/") || lower == "docprops/custom.xml" {
            Kind::CustomXml
        } else if lower == "word/document.xml"
            || lower == "word/styles.xml"
            || lower == "word/numbering.xml"
            || lower == "word/comments.xml"
            || lower == "word/footnotes.xml"
            || lower == "word/endnotes.xml"
            || lower == "word/settings.xml"
            || lower == "word/fonttable.xml"
            || lower == "word/websettings.xml"
            || lower == "word/styleswitheffects.xml"
            || lower == "word/people.xml"
            || lower.starts_with("word/media/")
            || lower.starts_with("word/theme/")
            || lower.starts_with("word/tags/")
            || lower == "word/commentsexternal.xml"
            || lower == "word/commentsextended.xml"
            || lower == "word/commentsids.xml"
            || lower == "word/commentsextensible.xml"
            || lower.starts_with("docprops/")
        {
            Kind::Skip
        } else if lower.starts_with("word/") && lower.ends_with(".xml") {
            Kind::UnknownPart
        } else {
            Kind::Skip
        };

        let (feature, action, what) = match kind {
            Kind::Skip => continue,
            Kind::Headers => ("docx_headers", "preserved", "页眉整件保留（不参与渲染）"),
            Kind::Footers => ("docx_footers", "preserved", "页脚整件保留（不参与渲染）"),
            Kind::Ole => (
                "docx_ole_object",
                "preserved_quarantined",
                "OLE/宏部件隔离保留（security，不参与渲染）",
            ),
            Kind::CustomXml => ("docx_customxml", "preserved", "自定义 XML 部件整件保留"),
            Kind::UnknownPart => (
                "docx_unknown_part",
                "preserved",
                "未识别的 OOXML 部件整件保留",
            ),
        };
        let ext = norm.rsplit('.').next().unwrap_or("bin").to_string();
        let bytes = match pkg.read_part(&norm) {
            Ok(b) => b,
            Err(OoxmlError::MissingPart(_)) => continue,
            Err(_) => continue, // 加密/超限的侧部件跳过读取（正文映射中的部件会硬报错）
        };
        let payload = job.add_preserved("docx", &ext, bytes);
        log.record(
            LossClass::PreservedRaw,
            feature,
            None,
            action,
            &payload,
            format!("{what}: {norm}"),
        );
    }
    Ok(())
}

/// footnote/endnote 定义表（跳过分隔符条目 id=0/-1）。
fn parse_notes(
    pkg: &mut OoxmlPackage,
    part: &str,
    note_tag: &str,
) -> Result<BTreeMap<String, El>, OoxmlError> {
    let mut out = BTreeMap::new();
    if !pkg.has_part(part) {
        return Ok(out);
    }
    let doc = pkg.part_tree(part)?;
    for c in &doc.children {
        if !c.is("w", note_tag) {
            continue;
        }
        let Some(id) = c.attr("w:id") else { continue };
        if id == "0" || id == "-1" {
            continue;
        }
        out.insert(id.to_string(), c.clone());
    }
    Ok(out)
}

/// docProps/core.xml → 文档元数据。
#[derive(Debug, Default)]
struct CoreProps {
    title: Option<String>,
    creator: Option<String>,
    language: Option<String>,
    created: Option<String>,
    modified: Option<String>,
}

fn read_core_props(pkg: &mut OoxmlPackage) -> CoreProps {
    let mut out = CoreProps::default();
    if !pkg.has_part("docProps/core.xml") {
        return out;
    }
    let Ok(doc) = pkg.part_tree("docProps/core.xml") else {
        return out;
    };
    let text_of = |prefix: &str, local: &str| {
        doc.children
            .iter()
            .find(|c| c.is(prefix, local))
            .map(|c| c.text.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    out.title = text_of("dc", "title");
    out.creator = text_of("dc", "creator");
    out.language = text_of("dc", "language");
    out.created = text_of("dcterms", "created");
    out.modified = text_of("dcterms", "modified");
    out
}

/// 修订收口：有 tracked changes 时原件 preserved + 聚合事件。
fn flush_tracked(ctx: &mut document::BodyCtx, document_xml_bytes: &[u8]) {
    if ctx.tracked.total() == 0 {
        return;
    }
    let stats = std::mem::take(&mut ctx.tracked);
    let payload = ctx
        .job
        .add_preserved("docx", "xml", document_xml_bytes.to_vec());
    let authors = stats
        .authors
        .iter()
        .map(|(a, (i, d))| format!("{a}: 插入 {i} 删除 {d}"))
        .collect::<Vec<_>>()
        .join("；");
    let detail = format!(
        "regions={}; authors: {authors}; original preserved at {payload}; track_changes_enabled={}",
        stats.total(),
        stats.enabled
    );
    ctx.log.record(
        LossClass::Partial,
        TRACKED,
        None,
        "degraded",
        &detail,
        "修订按终稿视图导入；原始 document.xml 已整件保留".to_string(),
    );
}
