// Copyright (C) 2026 The Athanor Studio Developers
// SPDX-License-Identifier: AGPL-3.0-only

//! 文档会话注册表：路径去重、草稿会话、另存为换绑。
//!
//! 会话键：文件文档用规范化路径；未命名草稿用 `doc_<ULID>`。
//! 同一路径再次打开聚焦已有会话（`open_path` 幂等），前端在丢弃未保存
//! 改动时才显式重建会话。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use aludel::doc_store::canonical_key;
use aludel::session::DocumentSession;
use serde::Serialize;
use serde_json::Value;

/// 命令层错误（可序列化给前端）。
#[derive(Debug, Serialize)]
pub struct SessionError {
    pub code: &'static str,
    pub message: String,
}

impl SessionError {
    pub fn new(code: &'static str, message: impl ToString) -> Self {
        Self {
            code,
            message: message.to_string(),
        }
    }
}

impl From<aludel::api::ApiError> for SessionError {
    fn from(error: aludel::api::ApiError) -> Self {
        let code = match error {
            aludel::api::ApiError::BadRequest(_) => "bad_request",
            aludel::api::ApiError::Doc(_) => "document_error",
        };
        Self {
            code,
            message: error.to_string(),
        }
    }
}

fn poisoned() -> SessionError {
    SessionError::new("state_poisoned", "studio session state is unavailable")
}

/// 会话注册表（Tauri State）。内部无锁顺序保证：所有经 sessions 的操作
/// 在同一 Mutex 临界区内完成键查找与插入。
#[derive(Default)]
pub struct SessionRegistry {
    sessions: Mutex<HashMap<String, Arc<DocumentSession>>>,
    /// 规范化路径 → session id（草稿会话不入此表）。
    paths: Mutex<HashMap<String, String>>,
}

impl SessionRegistry {
    /// 新建未命名草稿文档。
    pub fn new_document(&self, title: Option<&str>) -> Result<(String, Value), SessionError> {
        let bytes = azodoc_container::builder::build_minimal_document(
            title.unwrap_or("未命名文档"),
            "zh-CN",
        )
        .map_err(|e| SessionError::new("document_error", e.friendly()))?;
        let session = Arc::new(DocumentSession::new_draft(bytes));
        let opened = session.open().map_err(SessionError::from)?;
        let id = DocumentSession::draft_key();
        self.sessions
            .lock()
            .map_err(|_| poisoned())?
            .insert(id.clone(), session);
        Ok((id, opened))
    }

    /// 打开已有文档；同一路径已有会话时直接复用并重新读取磁盘。
    pub fn open_path(&self, path: &Path) -> Result<(String, Value), SessionError> {
        if !path.is_file() {
            return Err(SessionError::new(
                "document_not_found",
                format!("document does not exist: {}", path.display()),
            ));
        }
        let key = canonical_key(path);
        if let Some(existing) = self
            .paths
            .lock()
            .map_err(|_| poisoned())?
            .get(&key)
            .cloned()
        {
            if let Some(session) = self
                .sessions
                .lock()
                .map_err(|_| poisoned())?
                .get(&existing)
                .cloned()
            {
                let opened = session.open().map_err(SessionError::from)?;
                return Ok((existing, opened));
            }
        }
        let session = Arc::new(DocumentSession::new(path.to_path_buf()));
        let opened = session.open().map_err(SessionError::from)?;
        self.sessions
            .lock()
            .map_err(|_| poisoned())?
            .insert(key.clone(), session);
        self.paths
            .lock()
            .map_err(|_| poisoned())?
            .insert(key.clone(), key.clone());
        Ok((key, opened))
    }

    /// 按会话键取会话。
    pub fn session(&self, id: &str) -> Result<Arc<DocumentSession>, SessionError> {
        self.sessions
            .lock()
            .map_err(|_| poisoned())?
            .get(id)
            .cloned()
            .ok_or_else(|| {
                SessionError::new(
                    "session_not_found",
                    format!("no open document session for {id}"),
                )
            })
    }

    /// 保存到当前路径（草稿更新内存容器并返回 draft 标记）。
    pub fn save(&self, id: &str, body: &Value) -> Result<Value, SessionError> {
        self.session(id)?.save(body).map_err(SessionError::from)
    }

    /// 另存为：成功后把会话绑定迁移到新路径并返回新会话键。
    pub fn save_as(
        &self,
        id: &str,
        target: PathBuf,
        body: &Value,
        overwrite: bool,
    ) -> Result<(String, Value), SessionError> {
        let session = self.session(id)?;
        let resp = session
            .save_as(target.clone(), body, overwrite)
            .map_err(|e| match &e {
                aludel::api::ApiError::BadRequest(m) if m.starts_with("目标已存在") => {
                    SessionError::new("target_exists", m)
                }
                _ => SessionError::from(e),
            })?;
        let new_key = canonical_key(&target);

        let mut sessions = self.sessions.lock().map_err(|_| poisoned())?;
        let mut paths = self.paths.lock().map_err(|_| poisoned())?;
        if id != new_key {
            sessions.remove(id);
            sessions.insert(new_key.clone(), session);
            paths.retain(|_, v| v != id);
        }
        paths.insert(new_key.clone(), new_key.clone());
        Ok((new_key, resp))
    }

    /// 关闭并释放会话。脏检查在前端完成；此操作幂等。
    pub fn close(&self, id: &str) -> Result<(), SessionError> {
        let mut sessions = self.sessions.lock().map_err(|_| poisoned())?;
        if sessions.remove(id).is_some() {
            self.paths
                .lock()
                .map_err(|_| poisoned())?
                .retain(|_, v| v != id);
        }
        Ok(())
    }

    /// 当前活跃会话数。
    pub fn session_count(&self) -> usize {
        self.sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }
}
