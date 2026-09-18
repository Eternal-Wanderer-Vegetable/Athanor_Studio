// Copyright (C) 2026 The Athanor Studio Developers
// SPDX-License-Identifier: AGPL-3.0-only

//! 最近文件列表：`<app_data>/recent.json`，最多保留 20 条，最新在前。
//! 打开/另存为成功时由命令层记录；打开失败不清理（用户决定何时移除）。

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::sessions::SessionError;

const MAX_RECENT: usize = 20;

fn recent_file(base: &Path) -> PathBuf {
    base.join("recent.json")
}

/// 读取最近文件列表（路径字符串数组，最新在前）。损坏文件返回空表。
pub fn list(base: &Path) -> Vec<String> {
    let Ok(bytes) = fs::read(recent_file(base)) else {
        return Vec::new();
    };
    serde_json::from_slice::<Vec<String>>(&bytes).unwrap_or_default()
}

/// 把路径提升到最近列表首位（去重、截断）。写失败静默——最近列表不是关键数据。
pub fn touch(base: &Path, path: &Path) {
    let mut items = list(base);
    let s = path.to_string_lossy().into_owned();
    items.retain(|p| p != &s);
    items.insert(0, s);
    items.truncate(MAX_RECENT);
    let _ = fs::write(
        recent_file(base),
        serde_json::to_vec(&items).unwrap_or_default(),
    );
}

/// 从最近列表移除路径（文件已被删/移动时调用）。
pub fn remove(base: &Path, path: &str) -> Result<(), SessionError> {
    let mut items = list(base);
    items.retain(|p| p != path);
    fs::write(
        recent_file(base),
        serde_json::to_vec(&items).unwrap_or_default(),
    )
    .map_err(|e| SessionError::new("recent_write", format!("最近列表写入失败: {e}")))
}

/// 序列化为前端可用的 JSON。
pub fn list_json(base: &Path) -> Value {
    json!(list(base))
}
