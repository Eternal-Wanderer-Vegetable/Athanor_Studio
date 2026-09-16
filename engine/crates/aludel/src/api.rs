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

//! API 层：open / save / verify。App 绑定**唯一**一个文档（启动时给定），
//! 只监听回环地址——原型期的安全边界就是"本机、单文档"。
//!
//! 保存管线（RFC §决策 3 / 落地方案 M6 验收）：
//! 1. `pm_to_content_file`：PM JSON → Prima（ID 缺失/重复/非法按 IdKind 补发）
//! 2. `azodoc_model::validate::check_content`：规范级校验，Error 级拒绝保存
//! 3. `set_entry("document/content.json")`
//! 4. `relocate_annotations_layer`：标注随编辑自动重定位（验收④）
//! 5. `Container::commit(author_type: "human")`（验收③，硬编码）
//! 6. `write()` 回写文件
//! 7. 响应：修订号 + ID 统计 + 重定位统计 + 容器警告（损失报告面向真人）

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use azodoc_container::revisions::CommitInfo;
use azodoc_container::Container;
use azodoc_model::id::{AzodocId, IdKind};
use azodoc_model::validate::{self, Level};
use serde_json::{json, Value};

use crate::http::{Request, Response};

const CONTENT_ENTRY: &str = "document/content.json";
const ANNOTATIONS_ENTRY: &str = "semantics/annotations.json";

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Doc(String),
}

impl ApiError {
    pub fn status(&self) -> u16 {
        match self {
            ApiError::BadRequest(_) => 400,
            ApiError::Doc(_) => 500,
        }
    }

    fn to_response(&self) -> Response {
        Response::json_with_status(self.status(), &json!({ "error": self.to_string() }))
    }
}

pub struct App {
    doc_path: PathBuf,
    /// 串行化 open/save，避免进程内并发读写同一文件
    lock: Mutex<()>,
}

impl App {
    pub fn new(doc_path: PathBuf) -> Self {
        App {
            doc_path,
            lock: Mutex::new(()),
        }
    }

    pub fn doc_path(&self) -> &Path {
        &self.doc_path
    }

    fn open_container(&self) -> Result<(Container, Vec<String>), ApiError> {
        let data = std::fs::read(&self.doc_path)
            .map_err(|e| ApiError::Doc(format!("无法读取 {}: {e}", self.doc_path.display())))?;
        azodoc_container::open(data).map_err(|e| ApiError::Doc(e.friendly()))
    }

    /// GET /api/doc —— 文档全貌：PM 状态 + 语义层 + 修订链 + 损失盘点。
    /// 修订层/语义层/损失报告第一次面向真人的出口。
    pub fn open(&self) -> Result<Value, ApiError> {
        let _g = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let (mut c, warnings) = self.open_container()?;

        let content_bytes = c
            .read_entry(CONTENT_ENTRY)
            .map_err(|e| ApiError::Doc(e.friendly()))?;
        let content: Value = serde_json::from_slice(&content_bytes)
            .map_err(|e| ApiError::Doc(format!("content.json 解析失败: {e}")))?;
        let pm_doc = azodoc_pm::content_file_to_pm(&content)
            .map_err(|e| ApiError::Doc(format!("Prima → ProseMirror 转换失败: {e}")))?;

        let annotations = if c.has_entry(ANNOTATIONS_ENTRY) {
            let bytes = c
                .read_entry(ANNOTATIONS_ENTRY)
                .map_err(|e| ApiError::Doc(e.friendly()))?;
            serde_json::from_slice(&bytes)
                .unwrap_or_else(|_| json!({"schema_version": "1.0", "annotations": []}))
        } else {
            json!({"schema_version": "1.0", "annotations": []})
        };

        let history: Vec<Value> = c
            .history()
            .map_err(|e| ApiError::Doc(e.friendly()))?
            .iter()
            .map(|e| {
                json!({
                    "id": e.id,
                    "parent": e.parent,
                    "author_type": e.author_type,
                    "author_id": e.author_id,
                    "message": e.message,
                    "timestamp": e.timestamp,
                    "is_head": e.is_head,
                    "is_current": e.is_current,
                })
            })
            .collect();

        Ok(json!({
            "path": self.doc_path.display().to_string(),
            "revision": c.manifest_typed().current_revision,
            "pm_doc": pm_doc,
            "annotations": annotations,
            "history": history,
            "warnings": warnings,
            "loss_summary": loss_summary(&pm_doc, &annotations),
        }))
    }

    /// POST /api/save —— 七步保存管线。
    pub fn save(&self, body: &Value) -> Result<Value, ApiError> {
        let Some(pm_doc) = body.get("pm_doc") else {
            return Err(ApiError::BadRequest("缺少 pm_doc 字段".into()));
        };
        if !pm_doc.is_object() {
            return Err(ApiError::BadRequest("pm_doc 必须是 JSON 对象".into()));
        }
        let message = body
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Aludel 保存")
            .to_string();
        let author_id = body
            .get("author_id")
            .and_then(Value::as_str)
            .unwrap_or("local")
            .to_string();

        let _g = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let (mut c, warnings) = self.open_container()?;

        // 1. PM → Prima（ID 补发走引擎的 ULID 生成器）
        let mut idgen = |k: IdKind| -> String { AzodocId::generate(k).as_str().to_string() };
        let converted = azodoc_pm::pm_to_content_file(pm_doc, &mut idgen)
            .map_err(|e| ApiError::BadRequest(format!("ProseMirror → Prima 转换失败: {e}")))?;

        // 2. 规范级校验（Error 级拒绝；payload_ref/资产存在性经容器条目回调检查）
        let entry_exists = |p: &str| c.has_entry(p);
        let ctx = validate::Ctx {
            entry_exists: &entry_exists,
        };
        let (issues, _) = validate::check_content(&converted.content_file, &ctx);
        let errors: Vec<Value> = issues
            .iter()
            .filter(|i| i.level == Level::Error)
            .map(|i| {
                json!({
                    "code": i.code,
                    "path": i.path,
                    "message": i.message,
                })
            })
            .collect();
        if !errors.is_empty() {
            return Err(ApiError::BadRequest(format!(
                "content 校验未通过（{} 个错误），已放弃保存",
                errors.len()
            )));
        }

        // 3. 写 content 层（set_entry 自动同步 manifest sha256）
        let mut bytes = serde_json::to_vec_pretty(&converted.content_file)
            .map_err(|e| ApiError::Doc(format!("content 序列化失败: {e}")))?;
        bytes.push(b'\n');
        c.set_entry(CONTENT_ENTRY, bytes)
            .map_err(|e| ApiError::Doc(e.friendly()))?;

        // 4. 标注自动重定位（层不存在 → None）
        let stats = athanor_cli::commands_m3::relocate_annotations_layer(&mut c)
            .map_err(|e| ApiError::Doc(e.friendly()))?;

        // 5. 落链——author_type 硬编码 "human"（A1 验收③）
        let revision = c
            .commit(&CommitInfo {
                author_type: "human",
                author_id: &author_id,
                message: &message,
            })
            .map_err(|e| ApiError::Doc(e.friendly()))?;

        // 6. 回写文件
        let out = c.write().map_err(|e| ApiError::Doc(e.friendly()))?;
        std::fs::write(&self.doc_path, out)
            .map_err(|e| ApiError::Doc(format!("容器回写失败: {e}")))?;

        // 7. 响应
        Ok(json!({
            "ok": true,
            "revision": revision,
            "ids_assigned": converted.ids_assigned,
            "ids_deduplicated": converted.ids_deduplicated,
            "relocate": stats.map(|s| json!({
                "unchanged": s.unchanged,
                "reanchored": s.reanchored,
                "moved": s.moved,
                "detached": s.detached,
            })),
            "warnings": warnings,
        }))
    }

    /// POST /api/verify —— 复用 `athanor verify`（spec/azodoc-package.md §9 全检查）。
    pub fn verify(&self) -> Value {
        let code = athanor_cli::verify_cmd::run(&self.doc_path);
        json!({ "ok": code == 0, "code": code })
    }
}

/// unknown 块盘点 + 失配标注计数（损失摘要的最小形态）。
fn loss_summary(pm_doc: &Value, annotations: &Value) -> Value {
    fn walk(v: &Value, out: &mut Vec<Value>) {
        match v {
            Value::Array(arr) => arr.iter().for_each(|x| walk(x, out)),
            Value::Object(obj) => {
                if obj.get("type").and_then(Value::as_str) == Some("unknown_block") {
                    out.push(json!({
                        "id": obj.get("id"),
                        "origin": obj.get("origin"),
                        "loss_class": obj.get("loss_class"),
                        "summary": obj.get("summary"),
                    }));
                }
                for val in obj.values() {
                    walk(val, out);
                }
            }
            _ => {}
        }
    }
    let mut unknown_blocks = Vec::new();
    walk(pm_doc, &mut unknown_blocks);

    let detached = annotations
        .get("annotations")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter(|a| a.get("detached") == Some(&Value::Bool(true)))
                .count()
        })
        .unwrap_or(0);

    json!({
        "unknown_blocks": unknown_blocks,
        "detached_annotations": detached,
    })
}

/// 路由（静态冒烟页 + 三个 API）。
pub fn route(app: &App, req: &Request) -> Response {
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/") => Response::html(crate::INDEX_HTML),
        ("GET", "/api/doc") => respond(app.open()),
        ("POST", "/api/save") => match serde_json::from_slice::<Value>(&req.body) {
            Ok(v) => respond(app.save(&v)),
            Err(e) => Response::json_with_status(
                400,
                &json!({ "error": format!("请求体不是合法 JSON: {e}") }),
            ),
        },
        ("POST", "/api/verify") => Response::json(&app.verify()),
        ("GET", _) | ("POST", _) => {
            Response::json_with_status(404, &json!({ "error": "未知路径" }))
        }
        _ => Response::json_with_status(405, &json!({ "error": "方法不允许" })),
    }
}

fn respond(result: Result<Value, ApiError>) -> Response {
    match result {
        Ok(v) => Response::json(&v),
        Err(e) => e.to_response(),
    }
}
