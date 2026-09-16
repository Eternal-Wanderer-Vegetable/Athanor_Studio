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

//! azodoc-convert — Athanor 转换基建（spec/azodoc-loss.md 的机制化）。
//!
//! - [`LossLog`]：按节点计数的损失日志（R6：宁过度报告）
//! - [`build_report`]：生成符合 spec/json-schema/conversion-report.schema.json 的报告
//! - [`ImportJob`] / [`ImportOutput`] / [`ExportDoc`]：读写器的公共数据契约
//! - [`wrap_sections`]：按标题层级包装 section（azodoc-model.md §5 的导入约定）
//! - [`txt`]：TXT 兜底导出（最后一道恢复通道）

use serde_json::{json, Map, Value};

pub mod semantics;
pub mod txt;

/// 损失分级（JSON 小写；spec/azodoc-loss.md §1）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LossClass {
    None,
    Partial,
    Degraded,
    Unsupported,
    PreservedRaw,
}

impl LossClass {
    pub fn as_str(self) -> &'static str {
        match self {
            LossClass::None => "none",
            LossClass::Partial => "partial",
            LossClass::Degraded => "degraded",
            LossClass::Unsupported => "unsupported",
            LossClass::PreservedRaw => "preserved_raw",
        }
    }
    pub fn all() -> [LossClass; 5] {
        [
            LossClass::None,
            LossClass::Partial,
            LossClass::Degraded,
            LossClass::Unsupported,
            LossClass::PreservedRaw,
        ]
    }
}

/// 一处非 NONE 损失的记录（对应报告的 issues[] 条目）。
#[derive(Debug, Clone)]
pub struct LossEvent {
    pub feature: String,
    pub class: LossClass,
    /// 导入：源文件行号等；导出：目标块 id 等
    pub location: Option<String>,
    /// preserved / preserved_quarantined / removed / degraded / dropped
    pub action: String,
    pub detail: String,
    pub message: String,
}

/// 按节点计数的损失日志。每个被评估的块/行内节点都必须计数一次。
#[derive(Debug, Default)]
pub struct LossLog {
    events: Vec<LossEvent>,
    counts: [u64; 5],
    total: u64,
}

impl LossLog {
    pub fn new() -> Self {
        Self::default()
    }

    /// 一个无损表达的节点。
    pub fn node_none(&mut self) {
        self.counts[0] += 1;
        self.total += 1;
    }

    /// 记录一处非 NONE 损失（同时计入计数）。
    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &mut self,
        class: LossClass,
        feature: &str,
        location: Option<String>,
        action: &str,
        detail: &str,
        message: String,
    ) {
        self.counts[class as usize] += 1;
        self.total += 1;
        self.events.push(LossEvent {
            feature: feature.to_string(),
            class,
            location,
            action: action.to_string(),
            detail: detail.to_string(),
            message,
        });
    }

    pub fn events(&self) -> &[LossEvent] {
        &self.events
    }

    pub fn count(&self, class: LossClass) -> u64 {
        self.counts[class as usize]
    }

    /// 已评估的节点总数（块 + 行内）。
    pub fn total_nodes(&self) -> u64 {
        self.total
    }

    pub fn has_loss(&self) -> bool {
        LossClass::all()
            .iter()
            .filter(|c| **c != LossClass::None)
            .any(|c| self.count(*c) > 0)
    }
}

/// 确定性 ID 生成器（测试与黄金产物需要可重现的 ID；运行时导入请用
/// `azodoc_model::id::AzodocId::generate`）。
pub struct IdGen {
    n: u64,
}

impl IdGen {
    pub fn new() -> Self {
        IdGen { n: 0 }
    }

    /// `<prefix>_<26 位 Crockford Base32 计数>`，确定且合法。
    pub fn uid(&mut self, prefix: &str) -> String {
        self.n += 1;
        let mut n = self.n;
        let mut chars = Vec::with_capacity(26);
        for _ in 0..26 {
            chars.push(ALPHABET[(n % 32) as usize]);
            n /= 32;
        }
        format!(
            "{prefix}_{}",
            chars.iter().map(|b| *b as char).collect::<String>()
        )
    }
}

impl Default for IdGen {
    fn default() -> Self {
        Self::new()
    }
}

const ALPHABET: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

// ---------------------------------------------------------------- 导入契约

/// 导入期间的资产。
pub struct NewAsset {
    pub filename: String,
    pub mime: String,
    pub relationship: String,
    pub source: AssetSource,
}

pub enum AssetSource {
    Embedded(Vec<u8>),
    External(String),
}

/// 导入过程中累积的上下文（资产、preserved 载荷、ID）。
pub struct ImportJob {
    pub idgen: IdGen,
    pub assets: Vec<NewAsset>,
    /// (容器内路径, 字节)——路径由 [`ImportJob::add_preserved`] 生成并保存
    pub preserved: Vec<(String, Vec<u8>)>,
    /// 用于解析相对路径的本地资源（输入文件所在目录）
    pub base_dir: Option<std::path::PathBuf>,
}

impl ImportJob {
    pub fn new(base_dir: Option<std::path::PathBuf>) -> Self {
        ImportJob {
            idgen: IdGen::new(),
            assets: Vec::new(),
            preserved: Vec::new(),
            base_dir,
        }
    }

    /// 登记一个资产，返回 `asset://<id>` 引用。
    pub fn add_asset(&mut self, filename: &str, mime: &str, source: AssetSource) -> String {
        let id = azodoc_model::id::AzodocId::generate(azodoc_model::id::IdKind::As);
        self.assets.push(NewAsset {
            filename: filename.to_string(),
            mime: mime.to_string(),
            relationship: "inline".to_string(),
            source,
        });
        format!("asset://{id}")
    }

    /// 登记一段无法建模的原始内容，返回 `preserved/<origin>/part-<seq>.<ext>`。
    pub fn add_preserved(&mut self, origin: &str, ext: &str, bytes: Vec<u8>) -> String {
        let seq = self.preserved.len() + 1;
        let path = format!("preserved/{origin}/part-{seq:04}.{ext}");
        self.preserved.push((path.clone(), bytes));
        path
    }
}

/// 导入结果。
pub struct ImportOutput {
    /// content.json 的完整 JSON（schema_version + content）
    pub content: Value,
    pub title: Option<String>,
    pub language: Option<String>,
    /// frontmatter 等来源的未知文档级元数据（写入 manifest.document.extra）
    pub doc_extra: Map<String, Value>,
    /// 语义标注（B2：DOCX 批注 → semantics/annotations.json；空 = 不写该层）
    pub annotations: Vec<Value>,
    /// 表现层主题（B2：DOCX 样式 → presentation/theme.json；None = 不写该层）
    pub theme: Option<Value>,
    pub log: LossLog,
}

// ---------------------------------------------------------------- 导出契约

pub struct ExportAsset {
    pub id: String,
    pub filename: String,
    pub mime: String,
    pub storage: String,
    pub url: Option<String>,
    pub bytes: Option<Vec<u8>>,
}

pub struct ExportDoc {
    pub content: Value,
    pub assets: Vec<ExportAsset>,
    pub preserved: Vec<(String, Vec<u8>)>,
    pub title: Option<String>,
    pub language: Option<String>,
    pub document_id: Option<String>,
}

impl ExportDoc {
    /// `asset://<id>[/<name>]` → 供目标格式使用的 URL（内嵌 = data URI，外链 = 原始 URL）。
    pub fn asset_url(&self, asset_field: &str) -> String {
        let Some(id) = asset_field.strip_prefix("asset://") else {
            return asset_field.to_string();
        };
        let id = id.split('/').next().unwrap_or(id);
        for a in &self.assets {
            if a.id == id {
                if a.storage == "external" {
                    return a.url.clone().unwrap_or_default();
                }
                if let Some(bytes) = &a.bytes {
                    return data_uri(&a.mime, bytes);
                }
                return format!("asset://{id}");
            }
        }
        format!("asset://{id}")
    }

    pub fn asset_filename(&self, asset_field: &str) -> Option<&str> {
        let id = asset_field.strip_prefix("asset://")?;
        let id = id.split('/').next().unwrap_or(id);
        self.assets
            .iter()
            .find(|a| a.id == id)
            .map(|a| a.filename.as_str())
    }

    pub fn preserved_bytes(&self, payload_ref: &str) -> Option<&[u8]> {
        self.preserved
            .iter()
            .find(|(p, _)| p == payload_ref)
            .map(|(_, b)| b.as_slice())
    }
}

pub fn data_uri(mime: &str, bytes: &[u8]) -> String {
    use base64::Engine as _;
    format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

/// 按扩展名猜测 MIME（M2 够用的小表）。
pub fn guess_mime(name: &str) -> &'static str {
    let lower = name.split('?').next().unwrap_or(name).to_ascii_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");
    match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "pdf" => "application/pdf",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "txt" => "text/plain",
        _ => "application/octet-stream",
    }
}

// ---------------------------------------------------------------- 报告

pub struct ReportMeta<'a> {
    pub direction: &'a str,
    pub source_format: &'a str,
    pub target_format: &'a str,
    pub source_path: Option<&'a str>,
    pub source_sha256: Option<&'a str>,
    pub document_id: Option<&'a str>,
    pub revision: Option<&'a str>,
    pub converter: &'a str,
}

/// 生成 conversion-report（符合 spec/json-schema/conversion-report.schema.json）。
pub fn build_report(meta: &ReportMeta, log: &LossLog) -> Value {
    let status = if log.has_loss() {
        "partial"
    } else {
        "complete"
    };
    let mut counts = Map::new();
    let mut loss = Map::new();
    for c in LossClass::all() {
        loss.insert(c.as_str().to_string(), json!(log.count(c)));
    }
    counts.insert("blocks".to_string(), json!(log.total_nodes()));
    counts.insert("issues".to_string(), json!(log.events().len()));
    let mut issues = Vec::new();
    let mut idgen = IdGen::new();
    for e in log.events() {
        issues.push(json!({
            "id": idgen.uid("iss"),
            "feature": e.feature,
            "loss_class": e.class.as_str(),
            "location": e.location.clone().map(|l| json!({"context": l})).unwrap_or(Value::Null),
            "action": e.action,
            "detail": e.detail,
            "message": e.message,
        }));
    }
    // Option 字段为 None 时省略（Schema 要求是 string，null 不合法）
    let mut source = Map::new();
    source.insert("format".to_string(), json!(meta.source_format));
    if let Some(p) = meta.source_path {
        source.insert("path".to_string(), json!(p));
    }
    if let Some(h) = meta.source_sha256 {
        source.insert("sha256".to_string(), json!(h));
    }
    let mut target = Map::new();
    target.insert("format".to_string(), json!(meta.target_format));
    if let Some(d) = meta.document_id {
        target.insert("document_id".to_string(), json!(d));
    }
    if let Some(r) = meta.revision {
        target.insert("revision".to_string(), json!(r));
    }
    json!({
        "report_version": "1.0",
        "direction": meta.direction,
        "source": source,
        "target": target,
        "engine": {
            "name": "athanor",
            "version": env!("CARGO_PKG_VERSION"),
            "converter": meta.converter,
        },
        "status": status,
        "summary": { "counts": counts, "loss": loss },
        "issues": issues,
    })
}

// ---------------------------------------------------------------- 内容树工具

/// 统计块与行内节点数（报告 summary.counts 用）。
pub fn count_nodes(content: &Value) -> (u64, u64) {
    fn walk_node(v: &Value, blocks: &mut u64, spans: &mut u64) {
        if v.get("type").is_some() {
            *blocks += 1;
        }
        for key in ["children", "items"] {
            if let Some(arr) = v.get(key).and_then(Value::as_array) {
                for c in arr {
                    walk_node(c, blocks, spans);
                }
            }
        }
        if let Some(cells) = v.get("cells").and_then(Value::as_array) {
            for c in cells {
                walk_node(c, blocks, spans);
            }
        }
        if let Some(rows) = v.get("rows").and_then(Value::as_array) {
            for r in rows {
                walk_node(r, blocks, spans);
            }
        }
        for key in ["content", "caption"] {
            if let Some(arr) = v.get(key).and_then(Value::as_array) {
                for s in arr {
                    if s.get("type").is_some() {
                        *spans += 1;
                    }
                    if let Some(inner) = s.get("content").and_then(Value::as_array) {
                        walk_span(inner, spans);
                    }
                }
            }
        }
    }
    fn walk_span(arr: &[Value], spans: &mut u64) {
        for s in arr {
            *spans += 1;
            if let Some(inner) = s.get("content").and_then(Value::as_array) {
                walk_span(inner, spans);
            }
        }
    }
    let (mut blocks, mut spans) = (0, 0);
    if let Some(arr) = content.get("content").and_then(Value::as_array) {
        for n in arr {
            walk_node(n, &mut blocks, &mut spans);
        }
    }
    (blocks, spans)
}

/// 按标题层级包装 section（azodoc-model.md §5 的导入约定）。
pub fn wrap_sections(idgen: &mut IdGen, nodes: Vec<Value>) -> Vec<Value> {
    struct Open {
        level: i64,
        children: Vec<Value>,
    }
    let mut root: Vec<Value> = Vec::new();
    let mut stack: Vec<Open> = Vec::new();

    fn close(stack: &mut [Open], root: &mut Vec<Value>, top: Open, idgen: &mut IdGen) {
        let node = json!({
            "id": idgen.uid("sec"),
            "type": "section",
            "children": top.children,
        });
        match stack.last_mut() {
            Some(parent) => parent.children.push(node),
            None => root.push(node),
        }
    }

    for node in nodes {
        let level = node
            .get("type")
            .and_then(Value::as_str)
            .filter(|t| *t == "heading")
            .and_then(|_| node.get("level"))
            .and_then(Value::as_i64);
        match level {
            Some(l) => {
                while stack.last().map(|o| o.level >= l).unwrap_or(false) {
                    let top = stack.pop().unwrap();
                    close(&mut stack, &mut root, top, idgen);
                }
                stack.push(Open {
                    level: l,
                    children: vec![node],
                });
            }
            None => match stack.last_mut() {
                Some(top) => top.children.push(node),
                None => root.push(node),
            },
        }
    }
    while let Some(top) = stack.pop() {
        close(&mut stack, &mut root, top, idgen);
    }
    root
}

/// 合并相邻的 text span 并丢弃空文本（导入归一化：让不同来源的等价内容树一致）。
pub fn merge_text_spans(content: &mut Value) {
    fn merge_array(arr: &mut Vec<Value>) {
        let mut merged: Vec<Value> = Vec::new();
        for mut item in arr.drain(..) {
            walk(&mut item);
            let is_text = item.get("type").and_then(Value::as_str) == Some("text");
            if is_text {
                let text = item
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                if text.is_empty() {
                    continue; // 丢弃空文本 span
                }
                if let Some(last) = merged.last_mut() {
                    if last.get("type").and_then(Value::as_str) == Some("text") {
                        let prev = last
                            .get("text")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        last["text"] = Value::String(format!("{prev}{text}"));
                        continue;
                    }
                }
            }
            merged.push(item);
        }
        *arr = merged;
    }
    fn walk(v: &mut Value) {
        match v {
            Value::Array(a) => {
                for x in a.iter_mut() {
                    walk(x);
                }
                merge_array(a);
            }
            Value::Object(o) => {
                for val in o.values_mut() {
                    walk(val);
                }
            }
            _ => {}
        }
    }
    walk(content);
}

/// 往返恒等比较前的规范化：把 asset 引用替换为注册表序号，抹掉随机 ID 差异。
pub fn canonicalize_assets(content: &mut Value, asset_ids: &[String]) {
    fn walk(v: &mut Value, asset_ids: &[String]) {
        if let Some(s) = v.as_str() {
            if let Some(rest) = s.strip_prefix("asset://") {
                let id = rest.split('/').next().unwrap_or(rest);
                if let Some(idx) = asset_ids.iter().position(|a| a == id) {
                    *v = Value::String(format!("asset://#{idx}"));
                }
            }
            return;
        }
        if let Some(obj) = v.as_object_mut() {
            for (_, val) in obj.iter_mut() {
                walk(val, asset_ids);
            }
        } else if let Some(arr) = v.as_array_mut() {
            for item in arr {
                walk(item, asset_ids);
            }
        }
    }
    walk(content, asset_ids);
}

/// 提取块的纯文本（azodoc-model.md §9；TXT 导出与锚定共用）。
pub fn plain_text_of_span(span: &Value) -> String {
    let t = span.get("type").and_then(Value::as_str).unwrap_or("");
    match t {
        "text" => span
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        "hard_break" => "\n".to_string(),
        "code" => span
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        "inline_math" => span
            .get("latex")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        "inline_image" => span
            .get("alt")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        "mention" => span
            .get("target")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        "cite" => span
            .get("key")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        "footnote_ref" => String::new(),
        "unknown" => String::new(),
        _ => {
            let mut s = String::new();
            if let Some(arr) = span.get("content").and_then(Value::as_array) {
                for inner in arr {
                    s.push_str(&plain_text_of_span(inner));
                }
            }
            s
        }
    }
}

/// 块级纯文本（§9.2；不叠加剧排版增强）。
pub fn plain_text_of_block(block: &Value) -> String {
    let t = block.get("type").and_then(Value::as_str).unwrap_or("");
    match t {
        "paragraph" | "heading" => spans_text(block.get("content")),
        "quote" | "callout" | "footnote" => children_text(block.get("children"), "\n"),
        "list" => children_text(block.get("items"), "\n"),
        "code_block" => block
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        "math_block" => block
            .get("latex")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        "figure" | "image" => block
            .get("alt")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        "table" => {
            let mut out = Vec::new();
            if let Some(rows) = block.get("rows").and_then(Value::as_array) {
                for row in rows {
                    let mut cells = Vec::new();
                    if let Some(cs) = row.get("cells").and_then(Value::as_array) {
                        for c in cs {
                            cells.push(children_text(c.get("children"), " "));
                        }
                    }
                    out.push(cells.join("\t"));
                }
            }
            out.join("\n")
        }
        "embed" => String::new(),
        "horizontal_rule" => String::new(),
        "unknown" => String::new(),
        _ => String::new(),
    }
}

fn spans_text(v: Option<&Value>) -> String {
    let mut s = String::new();
    if let Some(arr) = v.and_then(Value::as_array) {
        for span in arr {
            s.push_str(&plain_text_of_span(span));
        }
    }
    s
}

fn children_text(v: Option<&Value>, sep: &str) -> String {
    let mut parts = Vec::new();
    if let Some(arr) = v.and_then(Value::as_array) {
        for c in arr {
            parts.push(plain_text_of_block(c));
        }
    }
    parts.join(sep)
}
