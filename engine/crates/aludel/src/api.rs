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

//! API 层：open / save / save_as / verify。App 绑定一个存储后端（文件或
//! 内存草稿），HTTP 原型只监听回环地址，桌面经 `DocumentSession` 复用同一实现。
//!
//! 保存管线（RFC §决策 3 / 落地方案 M6 验收）：
//! 1. `pm_to_content_file`：PM JSON → Prima（ID 缺失/重复/非法按 IdKind 补发）
//!    另含 `assets::persist_assets`：staged → embedded、`data:` → 迁移、
//!    http(s) → external（spec/azodoc-package.md §4.2）；失败保留原引用并报告
//! 2. `azodoc_model::validate::check_content`：规范级校验，Error 级拒绝保存
//! 3. `set_entry("document/content.json")`
//! 4. `relocate_annotations_layer`：标注随编辑自动重定位（验收④）
//! 5. `Container::commit(author_type: "human")`（验收③，硬编码）
//! 6. 可靠落盘：原子替换（临时文件 → sync → rename）；可携带
//!    `expected_fingerprint` 在落盘前一刻检测外部改动
//! 7. 响应：修订号 + ID 统计 + 重定位统计 + 资产统计 + 文件指纹 + 容器警告

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use azodoc_container::revisions::CommitInfo;
use azodoc_container::Container;
use azodoc_model::id::{AzodocId, IdKind};
use azodoc_model::validate::{self, Level};
use serde_json::{json, Value};

use crate::assets::{self, StagedAssets};
use crate::doc_store::{self, Store};
use crate::http::{Request, Response};

const CONTENT_ENTRY: &str = "document/content.json";
const ANNOTATIONS_ENTRY: &str = "semantics/annotations.json";
const THEME_ENTRY: &str = "presentation/theme.json";

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
    /// 串行化 open/save/落盘，避免进程内并发读写同一文件
    state: Mutex<Store>,
    /// 会话级资产暂存（未提交进容器；随 App 释放即清理）。
    staged: StagedAssets,
}

impl App {
    /// 绑定到磁盘文档路径。
    pub fn new(doc_path: PathBuf) -> Self {
        App {
            state: Mutex::new(Store::File(doc_path)),
            staged: StagedAssets::default(),
        }
    }

    /// 新建内存草稿（未命名文档）；首次保存须走 `save_at`。
    pub fn new_draft(container_bytes: Vec<u8>) -> Self {
        App {
            state: Mutex::new(Store::Draft(container_bytes)),
            staged: StagedAssets::default(),
        }
    }

    /// 暂存一条资产（mime 白名单 + 字节上限），返回 (as_ id, asset:// 引用)。
    /// 引用写入 PM 正文；字节在下次保存落库。
    pub fn stage_asset(
        &self,
        filename: &str,
        mime: &str,
        bytes: Vec<u8>,
    ) -> Result<Value, ApiError> {
        let (id, url) = assets::stage(&self.staged, filename, mime, bytes)?;
        Ok(json!({ "id": id, "url": url }))
    }

    /// 读取资产供渲染：暂存区或 registry（embedded → 字节；external → url）。
    pub fn read_asset(&self, id: &str) -> Result<assets::AssetRead, ApiError> {
        let store = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let data = store
            .read_bytes()
            .map_err(|e| ApiError::Doc(format!("无法读取文档: {e}")))?;
        let (mut c, _) = azodoc_container::open(data).map_err(|e| ApiError::Doc(e.friendly()))?;
        assets::read_asset(
            &mut |p| c.read_entry(p).map_err(|e| ApiError::Doc(e.friendly())),
            &self.staged,
            id,
        )
    }

    /// 绑定的文件路径；草稿返回 None。
    pub fn doc_path(&self) -> Option<PathBuf> {
        let store = self.state.lock().unwrap_or_else(|e| e.into_inner());
        store.path().map(Path::to_path_buf)
    }

    /// 当前内容的 SHA-256 指纹（草稿为草稿字节的指纹）。
    pub fn fingerprint(&self) -> Result<String, ApiError> {
        let store = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let data = store
            .read_bytes()
            .map_err(|e| ApiError::Doc(format!("无法读取文档: {e}")))?;
        Ok(doc_store::sha256_hex(&data))
    }

    fn open_container_from(store: &Store) -> Result<(Container, Vec<String>), ApiError> {
        let data = store
            .read_bytes()
            .map_err(|e| ApiError::Doc(format!("无法读取文档: {e}")))?;
        azodoc_container::open(data).map_err(|e| ApiError::Doc(e.friendly()))
    }

    /// GET /api/doc —— 文档全貌：PM 状态 + 语义层 + 修订链 + 损失盘点。
    /// 修订层/语义层/损失报告第一次面向真人的出口。
    pub fn open(&self) -> Result<Value, ApiError> {
        let store = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let fingerprint = store
            .read_bytes()
            .map_err(|e| ApiError::Doc(format!("无法读取文档: {e}")))?;
        let fingerprint = doc_store::sha256_hex(&fingerprint);
        let (mut c, warnings) = Self::open_container_from(&store)?;

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

        let theme = if c.has_entry(THEME_ENTRY) {
            let bytes = c
                .read_entry(THEME_ENTRY)
                .map_err(|e| ApiError::Doc(e.friendly()))?;
            serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({}))
        } else {
            json!({})
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
            "path": store.path().map(|p| p.display().to_string()),
            "fingerprint": fingerprint,
            "revision": c.manifest_typed().current_revision,
            "pm_doc": pm_doc,
            "annotations": annotations,
            "theme": theme,
            "history": history,
            "warnings": warnings,
            "loss_summary": loss_summary(&pm_doc, &annotations),
        }))
    }

    /// 校验请求体：pm_doc + 可选 theme / message / author_id / expected_fingerprint。
    fn parse_save_body(body: &Value) -> Result<(&Value, Option<&Value>, String, String), ApiError> {
        let Some(pm_doc) = body.get("pm_doc") else {
            return Err(ApiError::BadRequest("缺少 pm_doc 字段".into()));
        };
        if !pm_doc.is_object() {
            return Err(ApiError::BadRequest("pm_doc 必须是 JSON 对象".into()));
        }
        let theme = body.get("theme");
        if let Some(theme) = theme {
            if !theme.is_object() {
                return Err(ApiError::BadRequest("theme 必须是 JSON 对象".into()));
            }
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
        Ok((pm_doc, theme, message, author_id))
    }

    /// 校验→管线→产出容器字节（不写盘）。七步中第 1–5 步。
    /// 返回 (容器字节, 响应 JSON, 已落库的暂存 id)；落库 id 供调用方
    /// 在写盘成功后清理暂存表。
    fn produce_container_bytes(
        store: &Store,
        staged: &StagedAssets,
        pm_doc: &Value,
        theme: Option<&Value>,
        message: &str,
        author_id: &str,
    ) -> Result<(Vec<u8>, Value, Vec<String>), ApiError> {
        let (mut c, warnings) = Self::open_container_from(store)?;

        // 1. PM → Prima（ID 补发走引擎的 ULID 生成器）
        let mut idgen = |k: IdKind| -> String { AzodocId::generate(k).as_str().to_string() };
        let converted = azodoc_pm::pm_to_content_file(pm_doc, &mut idgen)
            .map_err(|e| ApiError::BadRequest(format!("ProseMirror → Prima 转换失败: {e}")))?;
        let mut content_file = converted.content_file;

        // 1b. 资产落库：staged → embedded、data: → 迁移、http(s) → external。
        // 失败只保留原引用并报告，不产生悬空 asset://（spec §4.2.3）。
        let assets_report = assets::persist_assets(&mut c, &mut content_file, staged)?;

        // 2. 规范级校验（Error 级拒绝；payload_ref/资产存在性经容器条目回调检查）
        let entry_exists = |p: &str| c.has_entry(p);
        let ctx = validate::Ctx {
            entry_exists: &entry_exists,
        };
        let (issues, _) = validate::check_content(&content_file, &ctx);
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
        let mut bytes = serde_json::to_vec_pretty(&content_file)
            .map_err(|e| ApiError::Doc(format!("content 序列化失败: {e}")))?;
        bytes.push(b'\n');
        c.set_entry(CONTENT_ENTRY, bytes)
            .map_err(|e| ApiError::Doc(e.friendly()))?;

        if let Some(theme) = theme {
            let mut theme_bytes = serde_json::to_vec_pretty(theme)
                .map_err(|e| ApiError::Doc(format!("theme 序列化失败: {e}")))?;
            theme_bytes.push(b'\n');
            c.set_entry(THEME_ENTRY, theme_bytes)
                .map_err(|e| ApiError::Doc(e.friendly()))?;
            let mut manifest = c.manifest_value().clone();
            if let Some(layers) = manifest.get_mut("layers").and_then(Value::as_object_mut) {
                layers.insert(
                    "presentation".to_string(),
                    json!({
                        "path": THEME_ENTRY,
                        "sha256": doc_store::sha256_hex(&c.read_entry(THEME_ENTRY).unwrap_or_default()),
                    }),
                );
            }
            c.set_manifest(manifest)
                .map_err(|e| ApiError::Doc(e.friendly()))?;
        }

        // 4. 标注自动重定位（层不存在 → None）
        let stats = athanor_cli::commands_m3::relocate_annotations_layer(&mut c)
            .map_err(|e| ApiError::Doc(e.friendly()))?;

        // 5. 落链——author_type 硬编码 "human"（A1 验收③）
        let revision = c
            .commit(&CommitInfo {
                author_type: "human",
                author_id,
                message,
            })
            .map_err(|e| ApiError::Doc(e.friendly()))?;

        let out = c.write().map_err(|e| ApiError::Doc(e.friendly()))?;

        Ok((
            out,
            json!({
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
                "assets": assets_report.to_json(),
                "warnings": warnings,
            }),
            assets_report.persisted_ids,
        ))
    }

    /// 落盘前一刻的外部改动检测：`expected_fingerprint` 非空且与当前文件不一致时拒绝。
    fn check_external_change(store: &Store, expected: Option<&str>) -> Result<(), ApiError> {
        let (Some(expected), Some(path)) = (expected, store.path()) else {
            return Ok(());
        };
        match doc_store::fingerprint_of(path) {
            Ok(Some(actual)) if actual == expected => Ok(()),
            Ok(_) => Err(ApiError::Doc(
                "文件在外部被修改，为避免覆盖未保存；请重新打开或另存为".into(),
            )),
            Err(e) => Err(ApiError::Doc(format!("无法读取文件用于冲突检测: {e}"))),
        }
    }

    /// POST /api/save —— 七步保存管线（保存到当前绑定路径）。
    pub fn save(&self, body: &Value) -> Result<Value, ApiError> {
        let (pm_doc, theme, message, author_id) = Self::parse_save_body(body)?;
        let expected = body.get("expected_fingerprint").and_then(Value::as_str);

        let mut store = self.state.lock().unwrap_or_else(|e| e.into_inner());
        match &mut *store {
            Store::Draft(bytes) => {
                // 草稿更新内存容器字节，不写盘——落盘必须经 save_at 明确选路径。
                let (out, mut resp, persisted) = Self::produce_container_bytes(
                    &Store::Draft(bytes.clone()),
                    &self.staged,
                    pm_doc,
                    theme,
                    &message,
                    &author_id,
                )?;
                *bytes = out;
                assets::drop_persisted(&self.staged, &persisted);
                resp["fingerprint"] = json!(doc_store::sha256_hex(bytes));
                resp["draft"] = json!(true);
                Ok(resp)
            }
            Store::File(_) => {
                // 指纹复核在读容器之前：外部把文件改成非 Azodoc 时也能给出
                // 冲突错误而不是容器解析错误。
                Self::check_external_change(&store, expected)?;
                let (out, mut resp, persisted) = Self::produce_container_bytes(
                    &store,
                    &self.staged,
                    pm_doc,
                    theme,
                    &message,
                    &author_id,
                )?;
                let path = store.path().expect("File store 必有 path").to_path_buf();
                // 第 6 步：原子替换（管线与落盘之间仍同一把锁，无窗口）
                doc_store::atomic_write(&path, &out)
                    .map_err(|e| ApiError::Doc(format!("容器回写失败: {e}")))?;
                assets::drop_persisted(&self.staged, &persisted);
                resp["fingerprint"] = json!(doc_store::sha256_hex(&out));
                resp["path"] = json!(path.display().to_string());
                Ok(resp)
            }
        }
    }

    /// 另存为：管线产出 → 原子写入 `target` → 成功后才把会话绑定到新路径。
    /// `overwrite=false` 且目标已存在时拒绝；失败时旧会话绑定不变。
    pub fn save_at(
        &self,
        target: PathBuf,
        body: &Value,
        overwrite: bool,
    ) -> Result<Value, ApiError> {
        let (pm_doc, theme, message, author_id) = Self::parse_save_body(body)?;
        if !overwrite && target.exists() {
            return Err(ApiError::BadRequest(format!(
                "目标已存在: {}",
                target.display()
            )));
        }
        let mut store = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let (out, mut resp, persisted) = Self::produce_container_bytes(
            &store,
            &self.staged,
            pm_doc,
            theme,
            &message,
            &author_id,
        )?;
        doc_store::atomic_write(&target, &out)
            .map_err(|e| ApiError::Doc(format!("写入 {} 失败: {e}", target.display())))?;
        assets::drop_persisted(&self.staged, &persisted);
        store.bind_file(target.clone());
        resp["fingerprint"] = json!(doc_store::sha256_hex(&out));
        resp["path"] = json!(target.display().to_string());
        resp["saved_as"] = json!(true);
        Ok(resp)
    }

    /// POST /api/verify —— 复用 `athanor verify`（spec/azodoc-package.md §9 全检查）。
    pub fn verify(&self) -> Result<Value, ApiError> {
        let store = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let Some(path) = store.path() else {
            return Err(ApiError::BadRequest(
                "未命名草稿没有落盘文件，请先另存为再运行检查".into(),
            ));
        };
        let code = athanor_cli::verify_cmd::run(path);
        Ok(json!({ "ok": code == 0, "code": code }))
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

/// 路由（静态冒烟页 + API）。
pub fn route(app: &App, req: &Request) -> Response {
    // 资产读取是动态路径（/api/asset/<as_id>），先于固定表匹配。
    if req.method == "GET" {
        if let Some(id) = req.path.strip_prefix("/api/asset/") {
            if id.is_empty() || id.contains('/') {
                return Response::json_with_status(400, &json!({ "error": "资产 id 非法" }));
            }
            return match app.read_asset(id) {
                Ok(assets::AssetRead::Embedded { mime, bytes }) => Response::bytes(mime, bytes),
                Ok(assets::AssetRead::External { url }) => Response::redirect(&url),
                Ok(assets::AssetRead::Missing) => {
                    Response::json_with_status(404, &json!({ "error": "资产不存在" }))
                }
                Err(e) => e.to_response(),
            };
        }
    }
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
        ("POST", "/api/asset/stage") => match serde_json::from_slice::<Value>(&req.body) {
            Ok(v) => {
                let filename = v.get("filename").and_then(Value::as_str).unwrap_or("");
                let mime = v.get("mime").and_then(Value::as_str).unwrap_or("");
                let data_b64 = v.get("data").and_then(Value::as_str).unwrap_or("");
                use base64::Engine as _;
                match base64::engine::general_purpose::STANDARD.decode(data_b64) {
                    Ok(bytes) => respond(app.stage_asset(filename, mime, bytes)),
                    Err(e) => Response::json_with_status(
                        400,
                        &json!({ "error": format!("data 不是合法 base64: {e}") }),
                    ),
                }
            }
            Err(e) => Response::json_with_status(
                400,
                &json!({ "error": format!("请求体不是合法 JSON: {e}") }),
            ),
        },
        ("POST", "/api/verify") => respond(app.verify()),
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
