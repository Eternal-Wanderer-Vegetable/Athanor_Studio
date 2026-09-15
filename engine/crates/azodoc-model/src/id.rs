// This file is part of Athanor, the Azodoc document engine.
// Copyright (C) 2026 Athanor Studio
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

//! ID 系统（spec/azodoc-model.md §3）：`<前缀>_<26 位 ULID>`。
//! ULID 采用 Crockford Base32 字母表（`0-9` + `A-Z` 去 `I L O U`）。

use serde::{Deserialize, Serialize};
use std::fmt;

pub const ULID_ALPHABET: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
pub const ULID_LEN: usize = 26;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IdKind {
    Doc,
    Sec,
    Blk,
    Li,
    Cel,
    Col,
    Row,
    As,
    Ann,
    Rev,
    Pub,
    Iss,
}

impl IdKind {
    pub fn prefix(self) -> &'static str {
        match self {
            IdKind::Doc => "doc",
            IdKind::Sec => "sec",
            IdKind::Blk => "blk",
            IdKind::Li => "li",
            IdKind::Cel => "cel",
            IdKind::Col => "col",
            IdKind::Row => "row",
            IdKind::As => "as",
            IdKind::Ann => "ann",
            IdKind::Rev => "rev",
            IdKind::Pub => "pub",
            IdKind::Iss => "iss",
        }
    }

    pub fn from_prefix(prefix: &str) -> Option<IdKind> {
        Some(match prefix {
            "doc" => IdKind::Doc,
            "sec" => IdKind::Sec,
            "blk" => IdKind::Blk,
            "li" => IdKind::Li,
            "cel" => IdKind::Cel,
            "col" => IdKind::Col,
            "row" => IdKind::Row,
            "as" => IdKind::As,
            "ann" => IdKind::Ann,
            "rev" => IdKind::Rev,
            "pub" => IdKind::Pub,
            "iss" => IdKind::Iss,
            _ => return None,
        })
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IdError {
    #[error("缺少下划线分隔的前缀: `{0}`")]
    MissingPrefix(String),
    #[error("未知 ID 前缀: `{0}`")]
    UnknownPrefix(String),
    #[error("ULID 部分无效（须为 {ULID_LEN} 位 Crockford Base32）: `{0}`")]
    BadUlid(String),
}

/// 校验 ULID 段：26 位、字符全部落在 Crockford 字母表内。
pub fn is_valid_ulid(s: &str) -> bool {
    s.len() == ULID_LEN && s.bytes().all(|b| ULID_ALPHABET.contains(&b))
}

/// 拆分 `<前缀>_<ULID>`，校验两段合法性。
pub fn split_id(s: &str) -> Result<(IdKind, &str), IdError> {
    let (prefix, ulid_part) = s
        .split_once('_')
        .ok_or_else(|| IdError::MissingPrefix(s.to_string()))?;
    let kind =
        IdKind::from_prefix(prefix).ok_or_else(|| IdError::UnknownPrefix(prefix.to_string()))?;
    if !is_valid_ulid(ulid_part) {
        return Err(IdError::BadUlid(ulid_part.to_string()));
    }
    Ok((kind, ulid_part))
}

/// 任意 ID 字符串是否合法（不限种类）。
pub fn is_valid_id(s: &str) -> bool {
    split_id(s).is_ok()
}

/// 已校验的 Azodoc ID。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AzodocId {
    kind: IdKind,
    raw: String,
}

impl AzodocId {
    pub fn parse(s: &str) -> Result<Self, IdError> {
        let (kind, _) = split_id(s)?;
        Ok(AzodocId {
            kind,
            raw: s.to_string(),
        })
    }

    /// 生成新 ID（随机 ULID）。同毫秒内不保证单调；排序稳定性非目标（v1 快照模式）。
    pub fn generate(kind: IdKind) -> Self {
        let raw = format!("{}_{}", kind.prefix(), ulid::Ulid::new());
        AzodocId { kind, raw }
    }

    pub fn kind(&self) -> IdKind {
        self.kind
    }

    pub fn as_str(&self) -> &str {
        &self.raw
    }
}

impl fmt::Display for AzodocId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_ids() {
        let id = AzodocId::parse("doc_01JAVG2Z7Q4M8XK3N5P9R2T6W8").unwrap();
        assert_eq!(id.kind(), IdKind::Doc);
        assert!(is_valid_id("blk_01JAVG3QK2M4N6P8R0T2V4X6Z8"));
        assert!(is_valid_id("li_ABCDEFGHJKMNPQRSTVWXYZ2345"));
    }

    #[test]
    fn rejects_bad_ids() {
        // 含 I/L/O/U
        assert!(matches!(
            split_id("doc_01JAVG2Z7Q4M8XK3N5P9R2T6WI"),
            Err(IdError::BadUlid(_))
        ));
        // 长度不对
        assert!(matches!(split_id("doc_SHORT"), Err(IdError::BadUlid(_))));
        // 未知前缀
        assert!(matches!(
            split_id("xyz_01JAVG2Z7Q4M8XK3N5P9R2T6W8"),
            Err(IdError::UnknownPrefix(_))
        ));
        // 无前缀
        assert!(matches!(
            split_id("nounderscore"),
            Err(IdError::MissingPrefix(_))
        ));
        // 小写不合法
        assert!(matches!(
            split_id("doc_01javg2z7q4m8xk3n5p9r2t6w8"),
            Err(IdError::BadUlid(_))
        ));
    }

    #[test]
    fn generates_valid_ids() {
        for kind in [
            IdKind::Doc,
            IdKind::Blk,
            IdKind::As,
            IdKind::Rev,
            IdKind::Iss,
        ] {
            let id = AzodocId::generate(kind);
            assert_eq!(id.kind(), kind);
            assert!(is_valid_id(id.as_str()), "生成的 ID 非法: {id}");
        }
    }
}
