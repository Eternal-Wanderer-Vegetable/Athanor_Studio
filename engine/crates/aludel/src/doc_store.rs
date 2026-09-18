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

//! 文档存储后端：文件路径或内存草稿（未命名新文档）。
//!
//! - `File(path)`：常规 .azodoc 文件；保存走原子替换（同目录临时文件 →
//!   flush+sync → rename 覆盖），外部改动可通过 fingerprint 检测。
//! - `Draft(bytes)`：新建文档尚未落盘的容器字节；`save` 拒绝（须先 save_as），
//!   `save_at` 成功后把 store 重新绑定为 `File`。
//!
//! 原子语义：rename(2)/MoveFileEx(REPLACE_EXISTING) 在同文件系统内是原子替换；
//! 临时文件先创建、flush、sync 再 rename，写失败时原文件保持不动。Windows 上
//! 目标被占用时 rename 失败，同样不破坏原文件。

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// 文档字节来源。
pub enum Store {
    /// 绑定到磁盘路径。
    File(PathBuf),
    /// 内存草稿（新建未命名文档）。
    Draft(Vec<u8>),
}

impl Store {
    /// 读取当前容器字节。
    pub fn read_bytes(&self) -> Result<Vec<u8>, std::io::Error> {
        match self {
            Store::File(p) => fs::read(p),
            Store::Draft(b) => Ok(b.clone()),
        }
    }

    /// 绑定的文件路径（草稿为 None）。
    pub fn path(&self) -> Option<&Path> {
        match self {
            Store::File(p) => Some(p.as_path()),
            Store::Draft(_) => None,
        }
    }

    /// 草稿直接更新内存字节（不暴露为保存语义——持久化必须经 save_at）。
    pub fn update_draft(&mut self, bytes: Vec<u8>) {
        if let Store::Draft(b) = self {
            *b = bytes;
        }
    }

    /// 保存完成后把草稿绑定为新文件路径。
    pub fn bind_file(&mut self, path: PathBuf) {
        *self = Store::File(path);
    }
}

/// 文件内容的 SHA-256 十六进制摘要（冲突检测指纹）。
pub fn sha256_hex(data: &[u8]) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(data);
    let mut s = String::with_capacity(digest.len() * 2);
    for b in digest {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// 文件指纹：存在 → Some(sha256)；不存在 → None；读取失败 → Err。
pub fn fingerprint_of(path: &Path) -> Result<Option<String>, std::io::Error> {
    match fs::read(path) {
        Ok(data) => Ok(Some(sha256_hex(&data))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// 原子写入：同目录临时文件 → flush + sync → rename 覆盖。
/// 写失败时原文件不动；临时文件被清理（NamedTempFile drop）。
pub fn atomic_write(path: &Path, data: &[u8]) -> Result<(), std::io::Error> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "缺少目标目录"))?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.write_all(data)?;
    tmp.flush()?;
    tmp.as_file().sync_all()?;
    // persist = rename；目标已存在时替换（Windows MoveFileEx REPLACE_EXISTING）。
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

/// 规范化路径用于会话键比较；canonicalize 失败时退回原路径。
pub fn canonical_key(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_replaces_existing_content() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("doc.azodoc");
        fs::write(&p, b"old").unwrap();
        atomic_write(&p, b"new-bytes").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"new-bytes");
    }

    #[test]
    fn fingerprint_detects_change() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("doc.azodoc");
        fs::write(&p, b"v1").unwrap();
        let f1 = fingerprint_of(&p).unwrap().unwrap();
        fs::write(&p, b"v2").unwrap();
        let f2 = fingerprint_of(&p).unwrap().unwrap();
        assert_ne!(f1, f2);
        assert_eq!(fingerprint_of(&dir.path().join("none")).unwrap(), None);
    }

    #[test]
    fn draft_roundtrip_and_rebind() {
        let mut s = Store::Draft(b"draft".to_vec());
        assert!(s.path().is_none());
        assert_eq!(s.read_bytes().unwrap(), b"draft");
        s.update_draft(b"draft2".to_vec());
        assert_eq!(s.read_bytes().unwrap(), b"draft2");
        let p = PathBuf::from("/tmp/x.azodoc");
        s.bind_file(p.clone());
        assert_eq!(s.path(), Some(p.as_path()));
    }
}
