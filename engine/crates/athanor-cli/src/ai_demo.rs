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

//! A2：语义层 AI 管线示例（Prima ↔ LLM，Future Work §A2）。
//!
//! 演示"AI 原生"闭环：读取 `.azodoc` → 抽取块文本喂给 LLM → 结构化输出
//! （概念标注 + 改写建议）→ `annotate`（author `ai:<model>`，带 confidence）
//! → 改写落盘后 `commit`（author `ai`）→ 全程可经 `history` 审计、可
//! `checkout` 回滚。真实推理走 OpenAI 兼容 `/chat/completions`
//! （`AZODOC_AI_BASE_URL` / `AZODOC_AI_API_KEY` / `AZODOC_AI_MODEL`）；
//! 未配置时用内置 [`FakeProvider`]（确定性、离线），供演示与 CI 测试。

use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

use crate::{cmd_annotate, cmd_commit, read_container, AnnotateArgs};

/// 标注 source 字段标识（区分演示管线产生的标注）。
pub const DEMO_SOURCE: &str = "ai_pipeline";
/// 单次运行最多落链的标注数（演示规模控制）。
const MAX_ANNOTATIONS: usize = 8;
/// 单次运行最多应用的改写数。
const MAX_REWRITES: usize = 2;
/// 提示词中单块文本截断长度（字符）。
const PROMPT_TEXT_TRUNC: usize = 240;

// ---------------------------------------------------------------- 错误

#[derive(Debug)]
pub enum AiError {
    /// 提供方未配置或调用失败
    Provider(String),
    /// LLM 返回不是预期的 JSON 结构
    BadResponse(String),
    /// 容器读写失败
    Container(String),
    Io(std::io::Error),
}

impl std::fmt::Display for AiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AiError::Provider(m) => write!(f, "AI 提供方调用失败: {m}"),
            AiError::BadResponse(m) => write!(f, "AI 返回不是预期的 JSON: {m}"),
            AiError::Container(m) => write!(f, "容器读写失败: {m}"),
            AiError::Io(e) => write!(f, "IO 错误: {e}"),
        }
    }
}

impl std::error::Error for AiError {}

impl AiError {
    /// 四段式友好错误（沿用引擎错误风格）。
    pub fn friendly(&self) -> String {
        match self {
            AiError::Provider(_) => format!(
                "错误：{self}\n建议：检查 AZODOC_AI_BASE_URL / AZODOC_AI_API_KEY / \
                 AZODOC_AI_MODEL 配置，或改用 --provider fake 离线演示。"
            ),
            AiError::BadResponse(_) => {
                format!("错误：{self}\n建议：换用支持 JSON 输出的模型，或重试一次。")
            }
            _ => format!("错误：{self}\n建议：检查文档文件后重试。"),
        }
    }
}

// ---------------------------------------------------------------- 提供方

/// LLM 提供方抽象：一次调用，返回一个 JSON 对象。
pub trait AiProvider {
    /// 模型/供应商标识（写入修订与标注的 author id）
    fn model(&self) -> &str;
    fn complete_json(&self, system: &str, user: &str) -> Result<Value, AiError>;
}

/// 离线确定性提供方：从提示词里读块清单，生成固定形状的建议。
/// 不联网、可复现，供演示与 CI 测试使用。
pub struct FakeProvider {
    model: String,
}

impl Default for FakeProvider {
    fn default() -> Self {
        Self {
            model: "fake-demo".to_string(),
        }
    }
}

impl AiProvider for FakeProvider {
    fn model(&self) -> &str {
        &self.model
    }

    fn complete_json(&self, _system: &str, user: &str) -> Result<Value, AiError> {
        let listing = extract_prompt_blocks(user)
            .ok_or_else(|| AiError::BadResponse("提示词中没有块清单".to_string()))?;
        let arr = listing
            .as_array()
            .ok_or_else(|| AiError::BadResponse("块清单不是 JSON 数组".to_string()))?;
        let blocks: Vec<(String, String, String)> = arr
            .iter()
            .filter_map(|b| {
                Some((
                    b.get("id")?.as_str()?.to_string(),
                    b.get("type")?.as_str()?.to_string(),
                    b.get("text")?.as_str()?.to_string(),
                ))
            })
            .collect();

        let heading = blocks.iter().find(|(_, kind, _)| kind == "heading");
        let long_para = blocks
            .iter()
            .filter(|(_, kind, text)| kind == "paragraph" && text.chars().count() >= 20)
            .max_by_key(|(_, _, text)| text.chars().count());

        let mut annotations = Vec::new();
        if let Some((id, _, text)) = heading {
            annotations.push(json!({
                "block": id,
                "type": "Concept",
                "value": {"label": truncate(text, 24)},
                "confidence": 0.9,
                "quote": truncate(text, 20),
            }));
        }
        if let Some((id, _, text)) = long_para {
            annotations.push(json!({
                "block": id,
                "type": "Concept",
                "value": {"label": truncate(text, 24)},
                "confidence": 0.75,
                "quote": truncate(text, 20),
            }));
        }

        let rewrites = long_para
            .iter()
            .map(|(id, _, text)| {
                json!({
                    "block": id,
                    "new_text": format!("{}（AI 润色演示）", text.trim_end()),
                    "reason": "FakeProvider：演示 AI 改写落链，请人工审阅".to_string(),
                })
            })
            .collect::<Vec<_>>();

        Ok(json!({"annotations": annotations, "rewrites": rewrites}))
    }
}

/// OpenAI 兼容 HTTP 提供方（`POST {base_url}/chat/completions`）。
pub struct HttpProvider {
    base_url: String,
    api_key: Option<String>,
    model: String,
    agent: ureq::Agent,
}

impl HttpProvider {
    /// 从环境变量构造：`AZODOC_AI_BASE_URL`（必需）、
    /// `AZODOC_AI_API_KEY`（可选，本地服务可不带）、`AZODOC_AI_MODEL`（可选）。
    pub fn from_env() -> Result<Self, AiError> {
        let base_url = std::env::var("AZODOC_AI_BASE_URL").map_err(|_| {
            AiError::Provider(
                "未设置 AZODOC_AI_BASE_URL（如 https://api.openai.com/v1 或本地 Ollama 地址）"
                    .to_string(),
            )
        })?;
        let base_url = base_url.trim_end_matches('/').to_string();
        Ok(Self {
            base_url,
            api_key: std::env::var("AZODOC_AI_API_KEY")
                .ok()
                .filter(|k| !k.is_empty()),
            model: std::env::var("AZODOC_AI_MODEL")
                .ok()
                .filter(|m| !m.is_empty())
                .unwrap_or_else(|| "gpt-4o-mini".to_string()),
            agent: ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(120))
                .build(),
        })
    }

    /// 环境变量是否已配置（供示例自动选择模式）。
    pub fn is_configured() -> bool {
        std::env::var_os("AZODOC_AI_BASE_URL").is_some()
    }
}

impl AiProvider for HttpProvider {
    fn model(&self) -> &str {
        &self.model
    }

    fn complete_json(&self, system: &str, user: &str) -> Result<Value, AiError> {
        let url = format!("{}/chat/completions", self.base_url);
        let mut req = self
            .agent
            .post(&url)
            .set("Content-Type", "application/json");
        if let Some(key) = &self.api_key {
            req = req.set("Authorization", &format!("Bearer {key}"));
        }
        let body = json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user},
            ],
            "temperature": 0.2,
            "response_format": {"type": "json_object"},
        });
        let resp = req
            .send_json(body)
            .map_err(|e| AiError::Provider(e.to_string()))?;
        let root: Value = resp
            .into_json()
            .map_err(|e| AiError::Provider(format!("响应不是 JSON: {e}")))?;
        let content = root
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                AiError::BadResponse(format!(
                    "choices[0].message.content 缺失：{}",
                    truncate(&root.to_string(), 200)
                ))
            })?;
        parse_model_json(content)
    }
}

/// 解析模型输出：容忍 ```json 围栏。
fn parse_model_json(content: &str) -> Result<Value, AiError> {
    let trimmed = content.trim();
    let stripped = if let Some(rest) = trimmed.strip_prefix("```") {
        let rest = rest.trim_start_matches("json").trim_start();
        rest.strip_suffix("```").unwrap_or(rest).trim()
    } else {
        trimmed
    };
    let v: Value = serde_json::from_str(stripped)
        .map_err(|e| AiError::BadResponse(format!("{e}；原文：{}", truncate(stripped, 200))))?;
    if !v.is_object() {
        return Err(AiError::BadResponse("顶层必须是 JSON 对象".to_string()));
    }
    Ok(v)
}

/// 从提示词中提取 ```json 围栏里的块清单（FakeProvider 的"输入端"）。
fn extract_prompt_blocks(user: &str) -> Option<Value> {
    let start = user.rfind("```json")? + "```json".len();
    let end = user[start..].find("```")? + start;
    serde_json::from_str(user[start..end].trim()).ok()
}

fn truncate(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

// ---------------------------------------------------------------- 管线

#[derive(Debug)]
pub struct AiDemoResult {
    pub model: String,
    pub annotations_added: usize,
    pub rewrites_applied: usize,
    /// 最后一次 AI 落链产生的修订 ID（有改写时才有）
    pub revision: Option<String>,
}

struct BlockInfo {
    id: String,
    kind: String,
    text: String,
}

/// 运行 A2 演示管线：标注（author ai）+ 改写落链（author ai）。
///
/// LLM 返回的块 ID 会先对照真实文档校验，幻觉 ID 直接丢弃；
/// 标注 quote 不是原文子串时退化为块级标注。
pub fn run(path: &Path, provider: &dyn AiProvider) -> Result<AiDemoResult, AiError> {
    let blocks = collect_blocks(path)?;
    if blocks.is_empty() {
        return Err(AiError::BadResponse(
            "文档没有可分析的文本块（heading/paragraph 等）".to_string(),
        ));
    }

    // 1. 组提示词并调用 LLM
    let system = "你是 Azodoc 文档的语义分析助手。只输出一个 JSON 对象，不要输出任何其他文字：\n\
                  {\"annotations\":[{\"block\":\"<块ID>\",\"type\":\"Concept\",\"value\":{\"label\":\"<概念名>\"},\"confidence\":<0到1的小数>,\"quote\":\"<原文片段(可选，必须是块文本的子串)>\"}],\"rewrites\":[{\"block\":\"<块ID>\",\"new_text\":\"<改写后的整段文本>\",\"reason\":\"<一句话理由>\"}]}\n\
                  规则：block 必须来自我给你的块清单；标注宁缺毋滥（最多 5 条）；rewrites 最多 2 条且只改 paragraph。";
    let listing: Vec<Value> = blocks
        .iter()
        .map(|b| {
            json!({
                "id": b.id,
                "type": b.kind,
                "text": truncate(&b.text, PROMPT_TEXT_TRUNC),
            })
        })
        .collect();
    let user = format!(
        "请分析以下文档块：\n\n```json\n{}\n```\n",
        serde_json::to_string_pretty(&listing).map_err(|e| AiError::BadResponse(e.to_string()))?
    );
    let answer = provider.complete_json(system, &user)?;

    // 2. 校验并应用标注（幻觉 ID 丢弃；quote 非子串退化为块级标注）
    let valid_ids: HashSet<&str> = blocks.iter().map(|b| b.id.as_str()).collect();
    let block_text = |id: &str| blocks.iter().find(|b| b.id == id).map(|b| b.text.clone());
    let author = format!("ai:{}", provider.model());
    let mut annotations_added = 0;
    let anns = answer.get("annotations").and_then(Value::as_array);
    for a in anns.into_iter().flatten().take(MAX_ANNOTATIONS) {
        let Some(block) = a.get("block").and_then(Value::as_str) else {
            continue;
        };
        if !valid_ids.contains(block) {
            continue;
        }
        let value = a.get("value").cloned().unwrap_or(json!({}));
        if !value.is_object() {
            continue;
        }
        let value_json = value.to_string();
        let exact = a
            .get("quote")
            .and_then(Value::as_str)
            .filter(|q| !q.is_empty())
            .filter(|q| block_text(block).is_some_and(|t| t.contains(q)));
        let confidence = a
            .get("confidence")
            .and_then(Value::as_f64)
            .filter(|c| (0.0..=1.0).contains(c));
        let args = AnnotateArgs {
            block,
            ann_type: a.get("type").and_then(Value::as_str).unwrap_or("Concept"),
            value_json: &value_json,
            exact,
            prefix: None,
            suffix: None,
            confidence,
            author: Some(&author),
            source: Some(DEMO_SOURCE),
        };
        if cmd_annotate(path, &args) == 0 {
            annotations_added += 1;
        }
    }

    // 3. 应用改写并逐条落链（author ai，全程 history 可审计）
    let mut rewrites_applied = 0;
    let mut revision = None;
    let rewrites = answer.get("rewrites").and_then(Value::as_array);
    for r in rewrites.into_iter().flatten().take(MAX_REWRITES) {
        let Some(block) = r.get("block").and_then(Value::as_str) else {
            continue;
        };
        let Some(new_text) = r.get("new_text").and_then(Value::as_str) else {
            continue;
        };
        if !valid_ids.contains(block) || new_text.trim().is_empty() {
            continue;
        }
        let reason = r
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("AI 改写建议")
            .to_string();
        if apply_rewrite(path, block, new_text)? {
            rewrites_applied += 1;
            let code = cmd_commit(path, &author, &reason);
            if code != 0 {
                return Err(AiError::Container(format!("commit 失败（退出码 {code}）")));
            }
            revision = current_revision(path)?;
        }
    }

    Ok(AiDemoResult {
        model: provider.model().to_string(),
        annotations_added,
        rewrites_applied,
        revision,
    })
}

// ---------------------------------------------------------------- 内部工具

/// 抽取可分析块（跳过 section/quote/list 等容器与行内 span，避免提示词重复计数）。
fn collect_blocks(path: &Path) -> Result<Vec<BlockInfo>, AiError> {
    let (mut c, _) = read_container(path)
        .map_err(|_| AiError::Container(format!("无法读取容器 {}", path.display())))?;
    let bytes = c
        .read_entry("document/content.json")
        .map_err(|e| AiError::Container(e.friendly()))?;
    let content: Value = serde_json::from_slice(&bytes)
        .map_err(|e| AiError::Container(format!("content 解析失败: {e}")))?;
    drop(c);

    const SKIP_KINDS: [&str; 6] = ["text", "hard_break", "code", "section", "list", "quote"];
    let mut out = Vec::new();
    walk_blocks(&content, &mut out);
    fn walk_blocks(v: &Value, out: &mut Vec<BlockInfo>) {
        if let Some(obj) = v.as_object() {
            let t = obj.get("type").and_then(Value::as_str).unwrap_or("");
            if let Some(id) = obj.get("id").and_then(Value::as_str) {
                if !id.is_empty() && !SKIP_KINDS.contains(&t) {
                    let text = collect_text(v);
                    if !text.trim().is_empty() {
                        out.push(BlockInfo {
                            id: id.to_string(),
                            kind: t.to_string(),
                            text,
                        });
                    }
                }
            }
            for val in obj.values() {
                walk_blocks(val, out);
            }
        } else if let Some(arr) = v.as_array() {
            for item in arr {
                walk_blocks(item, out);
            }
        }
    }
    // 只收集 text/caption 字段里的 span 文本（跳过 id/type 等元数据字符串）
    fn collect_text(v: &Value) -> String {
        let mut parts = Vec::new();
        fn go(v: &Value, out: &mut Vec<String>) {
            match v {
                Value::Array(arr) => arr.iter().for_each(|x| go(x, out)),
                Value::Object(obj) => {
                    for (k, val) in obj {
                        match k.as_str() {
                            "text" => {
                                if let Some(s) = val.as_str() {
                                    out.push(s.to_string());
                                }
                            }
                            "content" | "caption" => go(val, out),
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        go(v, &mut parts);
        parts.join(" ")
    }
    Ok(out)
}

/// 把目标块文本替换为 `new_text`（只允许 paragraph；保留块 ID）。
fn apply_rewrite(path: &Path, block_id: &str, new_text: &str) -> Result<bool, AiError> {
    let data = std::fs::read(path).map_err(AiError::Io)?;
    let (mut c, _) = azodoc_container::open(data).map_err(|e| AiError::Container(e.friendly()))?;
    let bytes = c
        .read_entry("document/content.json")
        .map_err(|e| AiError::Container(e.friendly()))?;
    let mut content: Value = serde_json::from_slice(&bytes)
        .map_err(|e| AiError::Container(format!("content 解析失败: {e}")))?;
    if !rewrite_block_text(&mut content, block_id, new_text) {
        return Ok(false);
    }
    let pretty =
        serde_json::to_vec_pretty(&content).map_err(|e| AiError::BadResponse(e.to_string()))?;
    c.set_entry("document/content.json", pretty)
        .map_err(|e| AiError::Container(e.friendly()))?;
    let out = c.write().map_err(|e| AiError::Container(e.friendly()))?;
    std::fs::write(path, out).map_err(AiError::Io)?;
    Ok(true)
}

fn rewrite_block_text(v: &mut Value, block_id: &str, new_text: &str) -> bool {
    match v {
        Value::Object(obj) => {
            let id_match = obj.get("id").and_then(Value::as_str) == Some(block_id);
            let is_paragraph = obj.get("type").and_then(Value::as_str) == Some("paragraph");
            if id_match && is_paragraph {
                obj.insert(
                    "content".to_string(),
                    json!([{"type": "text", "text": new_text}]),
                );
                return true;
            }
            obj.values_mut()
                .any(|val| rewrite_block_text(val, block_id, new_text))
        }
        Value::Array(arr) => arr
            .iter_mut()
            .any(|val| rewrite_block_text(val, block_id, new_text)),
        _ => false,
    }
}

fn current_revision(path: &Path) -> Result<Option<String>, AiError> {
    let data = std::fs::read(path).map_err(AiError::Io)?;
    let (c, _) = azodoc_container::open(data).map_err(|e| AiError::Container(e.friendly()))?;
    Ok(c.manifest_typed().current_revision.clone())
}
