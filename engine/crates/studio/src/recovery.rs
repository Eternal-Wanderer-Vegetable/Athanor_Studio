// Copyright (C) 2026 The Athanor Studio Developers
// SPDX-License-Identifier: AGPL-3.0-only

//! 恢复草稿管理：每个文档会话对应一个 `<app_data>/recovery/<hash>.json`
//! 草稿文件。草稿与正式 human 修订分离——写草稿不落链，正式保存成功后
//! 由前端删除对应草稿。
//!
//! 文件内容（JSON）：
//! `{ session_path, generation, saved_at, pm_doc, settings? }`

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::sessions::SessionError;

/// 草稿文件名：会话路径/ID 的 SHA-256 截断，避免路径字符与长度问题。
fn recovery_file_name(key: &str) -> String {
    let digest = aludel::doc_store::sha256_hex(key.as_bytes());
    format!("{}.recovery.json", &digest[..24])
}

fn recovery_dir(base: &Path) -> PathBuf {
    base.join("recovery")
}

/// 写入恢复草稿（覆盖同一会话的旧草稿）。原子写入，写失败不影响文档本体。
pub fn write_draft(base: &Path, key: &str, draft: &Value) -> Result<PathBuf, SessionError> {
    let dir = recovery_dir(base);
    fs::create_dir_all(&dir)
        .map_err(|e| SessionError::new("recovery_unavailable", format!("无法创建恢复目录: {e}")))?;
    let file = dir.join(recovery_file_name(key));
    let bytes = serde_json::to_vec(draft)
        .map_err(|e| SessionError::new("recovery_encode", e.to_string()))?;
    aludel::doc_store::atomic_write(&file, &bytes)
        .map_err(|e| SessionError::new("recovery_write", format!("草稿写入失败: {e}")))?;
    Ok(file)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryInfo {
    /// 草稿文件名（不含目录）。
    pub file: String,
    /// 草稿中记录的会话路径或会话键。
    pub session_path: Option<String>,
    /// 草稿中的编辑代数。
    pub generation: u64,
    /// 草稿写入时间（RFC3339 或平台时间戳字符串）。
    pub saved_at: Option<String>,
}

/// 列出全部恢复草稿（元信息，不读完整 pm_doc）。
pub fn list_drafts(base: &Path) -> Result<Vec<RecoveryInfo>, SessionError> {
    let dir = recovery_dir(base);
    let mut out = Vec::new();
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => {
            return Err(SessionError::new(
                "recovery_unavailable",
                format!("无法读取恢复目录: {e}"),
            ))
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.ends_with(".recovery.json"))
            .unwrap_or(false)
        {
            continue;
        }
        let Ok(bytes) = fs::read(&path) else { continue };
        let Ok(v) = serde_json::from_slice::<Value>(&bytes) else {
            continue;
        };
        out.push(RecoveryInfo {
            file: path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string(),
            session_path: v
                .get("session_path")
                .and_then(Value::as_str)
                .map(str::to_string),
            generation: v.get("generation").and_then(Value::as_u64).unwrap_or(0),
            saved_at: v
                .get("saved_at")
                .and_then(Value::as_str)
                .map(str::to_string),
        });
    }
    Ok(out)
}

/// 读取一个恢复草稿的完整内容。
pub fn read_draft(base: &Path, file: &str) -> Result<Value, SessionError> {
    // 只允许叶子文件名，拒绝路径穿越。
    if file.contains(['/', '\\']) || file.is_empty() {
        return Err(SessionError::new("invalid_path", "非法草稿文件名"));
    }
    let path = recovery_dir(base).join(file);
    let bytes = fs::read(&path)
        .map_err(|e| SessionError::new("recovery_read", format!("草稿读取失败: {e}")))?;
    serde_json::from_slice(&bytes)
        .map_err(|e| SessionError::new("recovery_read", format!("草稿不是合法 JSON: {e}")))
}

/// 删除恢复草稿；不存在视为成功。
pub fn delete_draft(base: &Path, key_or_file: &str) -> Result<(), SessionError> {
    let dir = recovery_dir(base);
    let named = if key_or_file.ends_with(".recovery.json") {
        dir.join(key_or_file)
    } else {
        dir.join(recovery_file_name(key_or_file))
    };
    match fs::remove_file(&named) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(SessionError::new(
            "recovery_delete",
            format!("草稿删除失败: {e}"),
        )),
    }
}
