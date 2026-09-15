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

//! azodoc-container — Azodoc 物理容器（spec/azodoc-container.md）。
//!
//! 核心保证：
//! - 探测算法按规范 §4（magic → 版本策略 → CRC → Plain-ZIP 兜底）；
//! - 未修改的容器原样返回字节（不重写 = 零风险）；
//! - 重写时未知条目 `raw_copy` 字节保真（R1），manifest 未知字段经
//!   typed-roundtrip 保真（R2）；
//! - 资源限制与加密/压缩方法检查（规范 §5–§6）。

pub mod builder;
pub mod header;
pub mod recover;
pub mod revisions;

use azodoc_model::manifest::Manifest;
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};
use zip::ZipArchive;

/// 本实现支持的主版本。
pub const SUPPORTED_MAJOR: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    /// 32 字节 Header + ZIP（标准）
    Prefixed,
    /// 无前缀（兼容回退 / 前缀受损）
    PlainZip,
}

#[derive(Debug, thiserror::Error)]
pub enum ContainerError {
    #[error("不是 Azodoc 文档")]
    NotAzodoc,
    #[error("文件过短或已截断")]
    TooShort,
    #[error("容器版本过新: 文件 v{found_major}.{found_minor}，本工具支持 v{supported_major}.x")]
    VersionRefused {
        found_major: u8,
        found_minor: u8,
        supported_major: u8,
    },
    #[error("ZIP 结构损坏: {0}")]
    Zip(String),
    #[error("manifest 无效: {0}")]
    Manifest(String),
    #[error("存在加密条目: {0}")]
    EncryptedEntries(String),
    #[error("不支持的压缩方法: {0}")]
    UnsupportedCompression(String),
    #[error("超出资源限制: {0}")]
    LimitExceeded(String),
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
}

impl ContainerError {
    /// 四段式友好输出（spec/azodoc-container.md §4.1）。
    pub fn friendly(&self) -> String {
        let headline = self.to_string();
        let (state, advice) = match self {
            ContainerError::NotAzodoc => (
                "文件没有损坏——它可能不是 Azodoc 文档（当前工具只认识 .azodoc）。",
                "用创建该文件的应用打开；若你确认它是 .azodoc，可尝试 athanor recover <file> -o <dir> 抢救内容。",
            ),
            ContainerError::TooShort => (
                "文件可能被截断。",
                "从原始来源重新获取；或用 athanor recover <file> -o <dir> 尝试抢救。",
            ),
            ContainerError::VersionRefused { .. } => (
                "文件本身没有损坏。",
                "1) 升级 Athanor 后重试；2) athanor recover <file> -o <dir> 抢救内容。",
            ),
            ContainerError::Zip(_) => (
                "ZIP 结构损坏，部分内容可能仍可抢救。",
                "athanor recover <file> -o <dir>。",
            ),
            ContainerError::EncryptedEntries(_) => (
                "容器包含加密条目，而 Azodoc 规范不允许加密。",
                "联系提供方获取未加密版本。",
            ),
            ContainerError::UnsupportedCompression(_) => (
                "条目使用了本工具不支持的压缩方法。",
                "升级 Athanor 到更新版本。",
            ),
            ContainerError::LimitExceeded(_) => (
                "文件超出本工具的安全资源限制（可能是 ZIP 炸弹或超大文档）。",
                "如确认文件可信，请使用可配置更高限制的版本。",
            ),
            ContainerError::Manifest(_) => (
                "manifest 层无效，容器结构可能被破坏。",
                "athanor recover <file> -o <dir> 抢救其余内容。",
            ),
            ContainerError::Io(_) => ("读取/写入文件失败。", "检查文件路径与权限。"),
        };
        format!("错误：{headline}\n{state}\n建议：{advice}")
    }
}

// Header CRC 失败不作为独立错误：按规范 §4 走「警告 + Plain-ZIP 兜底」路径。

#[derive(Debug, Clone)]
pub struct EntryInfo {
    pub name: String,
    pub size: u64,
    pub compressed_size: u64,
    pub compression: &'static str,
}

pub struct Container {
    pub profile: Profile,
    pub header: Option<header::Header>,
    manifest: Value,
    manifest_typed: Manifest,
    archive: ZipArchive<Cursor<Vec<u8>>>,
    original_file: Vec<u8>,
    modified: BTreeMap<String, Vec<u8>>,
    manifest_modified: bool,
}

/// 打开容器（spec §4 探测流程）。返回容器与警告列表。
pub fn open(data: Vec<u8>) -> Result<(Container, Vec<String>), ContainerError> {
    let mut warnings: Vec<String> = Vec::new();
    if data.len() < 4 {
        return Err(ContainerError::TooShort);
    }
    let is_magic = data.len() >= header::HEADER_SIZE && data[..8] == header::MAGIC;
    let is_pk = data[..4] == *b"PK\x03\x04";

    let (hdr, zip_start, profile) = if is_magic {
        let h = header::Header::parse_fields(&data);
        if h.version_major > SUPPORTED_MAJOR {
            return Err(ContainerError::VersionRefused {
                found_major: h.version_major,
                found_minor: h.version_minor,
                supported_major: SUPPORTED_MAJOR,
            });
        }
        let crc_ok = h.crc_matches(&data);
        if !crc_ok {
            warnings.push(
                "Header CRC 校验失败（文件可能被截断或篡改），已按兜底路径尝试打开".to_string(),
            );
        }
        (Some(h), h.zip_offset as usize, Profile::Prefixed)
    } else if is_pk {
        (None, 0, Profile::PlainZip)
    } else {
        return Err(ContainerError::NotAzodoc);
    };

    if zip_start >= data.len() {
        return Err(ContainerError::TooShort);
    }
    let mut archive = ZipArchive::new(Cursor::new(data[zip_start..].to_vec()))
        .map_err(|e| ContainerError::Zip(e.to_string()))?;

    // manifest 读取
    let manifest_value: Value = {
        let mut buf = Vec::new();
        let mut f = archive
            .by_name("manifest.json")
            .map_err(|_| ContainerError::Manifest("容器缺少 manifest.json 条目".into()))?;
        f.read_to_end(&mut buf)?;
        serde_json::from_slice(&buf)
            .map_err(|e| ContainerError::Manifest(format!("manifest.json 解析失败: {e}")))?
    };

    // Plain-ZIP 探测路径（或 CRC 受损后的兜底）：必须能认出 Azodoc 血统
    if profile == Profile::PlainZip || !warnings.is_empty() {
        let looks_azodoc = manifest_value
            .get("azodoc")
            .and_then(|a| a.get("format_version"))
            .is_some();
        if !looks_azodoc {
            return Err(ContainerError::NotAzodoc);
        }
        if profile == Profile::PlainZip
            && manifest_value
                .pointer("/azodoc/container_profile")
                .and_then(Value::as_str)
                == Some("prefixed")
        {
            warnings.push(
                "容器前缀缺失（文件可能被截断或被第三方工具改造），已按 Plain-ZIP 打开".to_string(),
            );
        }
    }

    enforce_limits(&mut archive)?;
    let manifest_typed = Manifest::from_value(&manifest_value)
        .map_err(|e| ContainerError::Manifest(format!("manifest 结构无效: {e}")))?;

    Ok((
        Container {
            profile,
            header: hdr,
            manifest: manifest_value,
            manifest_typed,
            archive,
            original_file: data,
            modified: BTreeMap::new(),
            manifest_modified: false,
        },
        warnings,
    ))
}

fn enforce_limits(archive: &mut ZipArchive<Cursor<Vec<u8>>>) -> Result<(), ContainerError> {
    const MAX_ENTRIES: usize = 65_536;
    const MAX_SINGLE: u64 = 512 * 1024 * 1024;
    const MAX_TOTAL: u64 = 2 * 1024 * 1024 * 1024;
    const MAX_RATIO: u64 = 200;
    const MIN_RATIO_SAMPLE: u64 = 64;

    if archive.len() > MAX_ENTRIES {
        return Err(ContainerError::LimitExceeded(format!(
            "条目数 {} > {MAX_ENTRIES}",
            archive.len()
        )));
    }
    let mut total: u64 = 0;
    let mut encrypted: Vec<String> = Vec::new();
    let mut bad_method: Vec<String> = Vec::new();
    for i in 0..archive.len() {
        let f = archive
            .by_index_raw(i)
            .map_err(|e| ContainerError::Zip(e.to_string()))?;
        let name = f.name().to_string();
        if f.encrypted() {
            encrypted.push(name.clone());
        }
        let method = f.compression();
        if !matches!(
            method,
            zip::CompressionMethod::Stored | zip::CompressionMethod::Deflated
        ) {
            bad_method.push(format!("{name} ({method:?})"));
        }
        let uncomp = f.size();
        let comp = f.compressed_size();
        if uncomp > MAX_SINGLE {
            return Err(ContainerError::LimitExceeded(format!(
                "单条目超限: {name} ({uncomp} 字节)"
            )));
        }
        total = total.saturating_add(uncomp);
        if total > MAX_TOTAL {
            return Err(ContainerError::LimitExceeded("解压后总尺寸超 2 GiB".into()));
        }
        let is_stored = matches!(method, zip::CompressionMethod::Stored);
        if !is_stored && comp > MIN_RATIO_SAMPLE && uncomp / comp > MAX_RATIO {
            return Err(ContainerError::LimitExceeded(format!(
                "压缩比异常（{name}: {uncomp}/{comp}），疑似 ZIP 炸弹"
            )));
        }
    }
    if !encrypted.is_empty() {
        return Err(ContainerError::EncryptedEntries(encrypted.join(", ")));
    }
    if !bad_method.is_empty() {
        return Err(ContainerError::UnsupportedCompression(
            bad_method.join(", "),
        ));
    }
    Ok(())
}

impl Container {
    pub fn manifest_value(&self) -> &Value {
        &self.manifest
    }

    pub fn manifest_typed(&self) -> &Manifest {
        &self.manifest_typed
    }

    pub fn is_modified(&self) -> bool {
        self.manifest_modified || !self.modified.is_empty()
    }

    /// 替换 manifest（重新做类型解析；未知字段照常保留）。
    pub fn set_manifest(&mut self, v: Value) -> Result<(), ContainerError> {
        self.manifest_typed = Manifest::from_value(&v)
            .map_err(|e| ContainerError::Manifest(format!("manifest 结构无效: {e}")))?;
        self.manifest = v;
        self.manifest_modified = true;
        Ok(())
    }

    /// 替换/新增条目。替换已登记层文件时自动同步 manifest 的 sha256。
    /// 注意：替换既有未知条目违反 R1 精神，调用方自律。
    pub fn set_entry(&mut self, name: &str, data: Vec<u8>) -> Result<(), ContainerError> {
        if name == "manifest.json" {
            let v: Value = serde_json::from_slice(&data).map_err(|e| {
                ContainerError::Manifest(format!("manifest.json 需要 JSON 字节: {e}"))
            })?;
            return self.set_manifest(v);
        }
        self.modified.insert(name.to_string(), data.clone());

        // 已登记层 → 同步 layers.<key>.sha256（保持容器自洽）
        let mut updated = self.manifest.clone();
        let mut touched = false;
        if let Some(layers) = updated.get_mut("layers").and_then(Value::as_object_mut) {
            for (_key, v) in layers.iter_mut() {
                if v.get("path").and_then(Value::as_str) == Some(name) {
                    if let Some(obj) = v.as_object_mut() {
                        obj.insert(
                            "sha256".to_string(),
                            Value::String(crate::builder::sha256_hex(&data)),
                        );
                        touched = true;
                    }
                }
            }
        }
        if touched {
            self.set_manifest(updated)?;
        }
        Ok(())
    }

    pub fn entry_names(&self) -> Vec<String> {
        self.archive.file_names().map(str::to_string).collect()
    }

    pub fn has_entry(&self, name: &str) -> bool {
        self.archive.file_names().any(|n| n == name)
    }

    /// 读取条目内容（优先返回修改后的字节）。
    pub fn read_entry(&mut self, name: &str) -> Result<Vec<u8>, ContainerError> {
        if let Some(bytes) = self.modified.get(name) {
            return Ok(bytes.clone());
        }
        let mut f = self
            .archive
            .by_name(name)
            .map_err(|e| ContainerError::Zip(format!("条目 `{name}` 读取失败: {e}")))?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;
        Ok(buf)
    }

    /// 输出容器字节。未修改 → 原样返回（零重写）；已修改 → 重建 ZIP。
    pub fn write(&mut self) -> Result<Vec<u8>, ContainerError> {
        if !self.is_modified() {
            return Ok(self.original_file.clone());
        }

        let manifest_bytes: Vec<u8> = if self.manifest_modified {
            let mut b = serde_json::to_vec_pretty(&self.manifest)
                .map_err(|e| ContainerError::Manifest(e.to_string()))?;
            b.push(b'\n');
            b
        } else {
            let mut f = self
                .archive
                .by_name("manifest.json")
                .map_err(|e| ContainerError::Zip(e.to_string()))?;
            let mut buf = Vec::new();
            f.read_to_end(&mut buf)?;
            buf
        };

        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let manifest_opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .unix_permissions(0o644);
        writer
            .start_file("manifest.json", manifest_opts)
            .map_err(|e| ContainerError::Zip(e.to_string()))?;
        writer
            .write_all(&manifest_bytes)
            .map_err(ContainerError::Io)?;

        for i in 0..self.archive.len() {
            let (name, method, mtime) = {
                let f = self
                    .archive
                    .by_index(i)
                    .map_err(|e| ContainerError::Zip(e.to_string()))?;
                (f.name().to_string(), f.compression(), f.last_modified())
            };
            if name == "manifest.json" {
                continue;
            }
            if let Some(bytes) = self.modified.get(&name) {
                let mut opts = zip::write::SimpleFileOptions::default()
                    .compression_method(method)
                    .unix_permissions(0o644);
                if let Some(dt) = mtime {
                    opts = opts.last_modified_time(dt);
                }
                writer
                    .start_file(&name, opts)
                    .map_err(|e| ContainerError::Zip(e.to_string()))?;
                writer.write_all(bytes).map_err(ContainerError::Io)?;
            } else {
                let f = self
                    .archive
                    .by_index(i)
                    .map_err(|e| ContainerError::Zip(e.to_string()))?;
                writer
                    .raw_copy_file(f)
                    .map_err(|e| ContainerError::Zip(e.to_string()))?;
            }
        }

        // 新增条目（原档案中不存在，如导出报告文件）
        let existing: std::collections::HashSet<String> =
            self.archive.file_names().map(str::to_string).collect();
        for (name, bytes) in &self.modified {
            if existing.contains(name) {
                continue;
            }
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .unix_permissions(0o644);
            writer
                .start_file(name, opts)
                .map_err(|e| ContainerError::Zip(e.to_string()))?;
            writer.write_all(bytes).map_err(ContainerError::Io)?;
        }

        let zip_bytes = writer
            .finish()
            .map_err(|e| ContainerError::Zip(e.to_string()))?
            .into_inner();

        let mut out = Vec::with_capacity(header::HEADER_SIZE + zip_bytes.len());
        if self.profile == Profile::Prefixed {
            let h = self.header.unwrap_or_else(header::Header::v1);
            out.extend_from_slice(&h.to_bytes());
        }
        out.extend_from_slice(&zip_bytes);
        Ok(out)
    }
}
