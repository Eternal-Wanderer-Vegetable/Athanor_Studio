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

//! Tauri command 薄层：把 IPC 参数转发给 [`SessionRegistry`] / recovery /
//! recent，统一返回 `{ session_id, doc }` 形态。业务逻辑全部在 lib 侧的
//! 可测模块中。

use std::path::PathBuf;

use serde_json::{json, Value};
use tauri::Manager;

use crate::recent;
use crate::recovery;
use crate::sessions::{SessionError, SessionRegistry};

fn app_data_dir(app: &tauri::AppHandle) -> Result<PathBuf, SessionError> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| SessionError::new("app_data_unavailable", e.to_string()))?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| SessionError::new("app_data_unavailable", e.to_string()))?;
    Ok(dir)
}

#[tauri::command]
pub fn new_document(
    state: tauri::State<'_, SessionRegistry>,
    title: Option<String>,
) -> Result<Value, SessionError> {
    let (session_id, doc) = state.new_document(title.as_deref())?;
    Ok(json!({ "session_id": session_id, "doc": doc }))
}

#[tauri::command]
pub fn open_document(
    app: tauri::AppHandle,
    state: tauri::State<'_, SessionRegistry>,
    path: String,
) -> Result<Value, SessionError> {
    let path = PathBuf::from(path);
    let (session_id, doc) = state.open_path(&path)?;
    if let Ok(base) = app_data_dir(&app) {
        if let Some(p) = state.session(&session_id).ok().and_then(|s| s.doc_path()) {
            recent::touch(&base, &p);
        }
    }
    Ok(json!({ "session_id": session_id, "doc": doc }))
}

#[tauri::command]
pub fn save_document(
    state: tauri::State<'_, SessionRegistry>,
    jobs: tauri::State<'_, crate::jobs::JobManager>,
    session_id: String,
    body: Value,
) -> Result<Value, SessionError> {
    let _commit = jobs
        .commit_gate
        .lock()
        .map_err(|_| SessionError::new("state_poisoned", "studio commit state is unavailable"))?;
    state.save(&session_id, &body)
}

#[tauri::command]
pub fn save_document_as(
    app: tauri::AppHandle,
    state: tauri::State<'_, SessionRegistry>,
    jobs: tauri::State<'_, crate::jobs::JobManager>,
    session_id: String,
    target: String,
    overwrite: bool,
    body: Value,
) -> Result<Value, SessionError> {
    let _commit = jobs
        .commit_gate
        .lock()
        .map_err(|_| SessionError::new("state_poisoned", "studio commit state is unavailable"))?;
    let (new_id, resp) = state.save_as(&session_id, PathBuf::from(&target), &body, overwrite)?;
    if let Ok(base) = app_data_dir(&app) {
        recent::touch(&base, &PathBuf::from(&target));
    }
    Ok(json!({ "session_id": new_id, "doc": resp }))
}

/// 把一条资产暂存进会话（bytes 以 base64 传入），返回 `{id, url}`，
/// url 为写入正文的 `asset://<id>/<filename>`。
#[tauri::command]
pub fn stage_asset(
    state: tauri::State<'_, SessionRegistry>,
    session_id: String,
    filename: String,
    mime: String,
    data: String,
) -> Result<Value, SessionError> {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data.trim())
        .map_err(|e| SessionError::new("bad_request", format!("data 不是合法 base64: {e}")))?;
    state
        .session(&session_id)?
        .stage_asset(&filename, &mime, bytes)
        .map_err(SessionError::from)
}

/// 读取资产字节（暂存区或 registry）供渲染。internal：前端渲染走
/// `azodoc-asset://` 自定义协议（lib.rs 注册），此命令主要供测试。
#[tauri::command]
pub fn read_asset(
    state: tauri::State<'_, SessionRegistry>,
    session_id: String,
    id: String,
) -> Result<Value, SessionError> {
    use base64::Engine as _;
    match state.session(&session_id)?.read_asset(&id)? {
        aludel::assets::AssetRead::Embedded { mime, bytes } => Ok(json!({
            "kind": "embedded",
            "mime": mime,
            "data": base64::engine::general_purpose::STANDARD.encode(bytes),
        })),
        aludel::assets::AssetRead::External { url } => Ok(json!({
            "kind": "external",
            "url": url,
        })),
        aludel::assets::AssetRead::Missing => Err(SessionError::new(
            "asset_not_found",
            format!("资产不存在: {id}"),
        )),
    }
}

/// 印刷预览：对当前会话快照（未保存的 PM 状态也参与）产出 Paged.js 增强
/// 的印刷 HTML + 快照指纹；不写容器、不出版（E4）。
#[tauri::command]
pub fn preview_document(
    state: tauri::State<'_, SessionRegistry>,
    session_id: String,
    body: Value,
) -> Result<Value, SessionError> {
    state
        .session(&session_id)?
        .preview(&body)
        .map_err(SessionError::from)
}

#[tauri::command]
pub fn close_document(
    state: tauri::State<'_, SessionRegistry>,
    session_id: String,
) -> Result<(), SessionError> {
    state.close(&session_id)
}

#[tauri::command]
pub fn verify_document(
    state: tauri::State<'_, SessionRegistry>,
    session_id: String,
) -> Result<Value, SessionError> {
    state
        .session(&session_id)?
        .verify()
        .map_err(SessionError::from)
}

#[tauri::command]
pub fn get_history(
    state: tauri::State<'_, SessionRegistry>,
    session_id: String,
) -> Result<Value, SessionError> {
    let opened = state
        .session(&session_id)?
        .open()
        .map_err(SessionError::from)?;
    Ok(opened
        .get("history")
        .cloned()
        .unwrap_or(Value::Array(Vec::new())))
}

// ---------------------------------------------------------------- 恢复草稿

#[tauri::command]
pub fn write_recovery(
    app: tauri::AppHandle,
    session_key: String,
    generation: u64,
    pm_doc: Value,
    session_path: Option<String>,
    theme: Option<Value>,
) -> Result<Value, SessionError> {
    let base = app_data_dir(&app)?;
    let file = recovery::write_draft(
        &base,
        &session_key,
        &json!({
            "session_path": session_path,
            "generation": generation,
            "saved_at": azodoc_container::builder::rfc3339_now(),
            "pm_doc": pm_doc,
            "theme": theme.unwrap_or_else(|| json!({})),
        }),
    )?;
    Ok(json!({ "file": file }))
}

#[tauri::command]
pub fn list_recoveries(app: tauri::AppHandle) -> Result<Value, SessionError> {
    Ok(json!(recovery::list_drafts(&app_data_dir(&app)?)?))
}

#[tauri::command]
pub fn read_recovery(app: tauri::AppHandle, file: String) -> Result<Value, SessionError> {
    recovery::read_draft(&app_data_dir(&app)?, &file)
}

#[tauri::command]
pub fn delete_recovery(app: tauri::AppHandle, key: String) -> Result<(), SessionError> {
    recovery::delete_draft(&app_data_dir(&app)?, &key)
}

// ---------------------------------------------------------------- 最近文件

#[tauri::command]
pub fn list_recent(app: tauri::AppHandle) -> Result<Value, SessionError> {
    Ok(recent::list_json(&app_data_dir(&app)?))
}

#[tauri::command]
pub fn remove_recent(app: tauri::AppHandle, path: String) -> Result<(), SessionError> {
    recent::remove(&app_data_dir(&app)?, &path)
}
