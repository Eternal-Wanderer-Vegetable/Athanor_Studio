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

//! 尽最大努力抢救（spec/azodoc-container.md §8）。永不修改源文件。
//!
//! 模式 A：标准 ZIP 中央目录读取（对前缀容忍）；
//! 模式 B：Local File Header 逐签名扫描 + 逐条目解压（中央目录损坏时）。

use crate::ContainerError;
use flate2::read::DeflateDecoder;
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_ENTRY: u64 = 512 * 1024 * 1024;

#[derive(Debug)]
pub struct RecoverOutcome {
    /// 使用的恢复模式描述
    pub mode: String,
    /// 每条目一行：Ok(说明) / Err(原因)
    pub lines: Vec<Result<String, String>>,
    pub salvaged: usize,
    pub failed: usize,
    pub out_dir: PathBuf,
}

/// 抢救内容到 out_dir，并写 recovery-report.txt。
pub fn recover(data: &[u8], out_dir: &Path) -> Result<RecoverOutcome, ContainerError> {
    std::fs::create_dir_all(out_dir).map_err(ContainerError::Io)?;

    let (mode, lines) = match try_central_directory(data, out_dir) {
        Some((mode, lines)) => (mode, lines),
        None => scan_local_headers(data, out_dir)?,
    };

    let salvaged = lines.iter().filter(|l| l.is_ok()).count();
    let failed = lines.iter().filter(|l| l.is_err()).count();

    let mut report = String::new();
    report.push_str("Athanor 恢复报告\n");
    report.push_str(&format!("模式: {mode}\n源文件未被修改。\n条目:\n"));
    for line in &lines {
        match line {
            Ok(msg) => report.push_str(&format!("  OK    {msg}\n")),
            Err(msg) => report.push_str(&format!("  失败  {msg}\n")),
        }
    }
    report.push_str(&format!("汇总: 成功 {salvaged} · 失败 {failed}\n"));
    std::fs::write(out_dir.join("recovery-report.txt"), report).map_err(ContainerError::Io)?;

    Ok(RecoverOutcome {
        mode,
        lines,
        salvaged,
        failed,
        out_dir: out_dir.to_path_buf(),
    })
}

fn try_central_directory(
    data: &[u8],
    out_dir: &Path,
) -> Option<(String, Vec<Result<String, String>>)> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(data.to_vec())).ok()?;
    let mut lines = Vec::new();
    for i in 0..archive.len() {
        let name = match archive.by_index_raw(i) {
            Ok(f) => f.name().to_string(),
            Err(e) => {
                lines.push(Err(format!("<条目 {i}>: {e}")));
                continue;
            }
        };
        match sanitize_rel_path(&name) {
            Err(e) => lines.push(Err(format!("{name}: {e}"))),
            Ok(rel) => match read_entry_decompressed(&mut archive, i) {
                Err(e) => lines.push(Err(format!("{name}: {e}"))),
                Ok(bytes) => match write_out(out_dir, &rel, &bytes) {
                    Ok(n) => lines.push(Ok(format!("{name}（{n} 字节）"))),
                    Err(e) => lines.push(Err(format!("{name}: 写入失败 {e}"))),
                },
            },
        }
    }
    Some(("ZIP 中央目录读取".to_string(), lines))
}

fn read_entry_decompressed(
    archive: &mut zip::ZipArchive<std::io::Cursor<Vec<u8>>>,
    i: usize,
) -> Result<Vec<u8>, String> {
    let mut f = archive.by_index(i).map_err(|e| e.to_string())?;
    if f.size() > MAX_ENTRY {
        return Err("条目超出 512 MiB 上限".to_string());
    }
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}

/// 模式 B：扫描 Local File Header 签名。
fn scan_local_headers(
    data: &[u8],
    out_dir: &Path,
) -> Result<(String, Vec<Result<String, String>>), ContainerError> {
    const LFH: &[u8] = b"PK\x03\x04";
    let mut lines = Vec::new();
    let mut pos = 0usize;
    let mut order = 0usize;

    while let Some(rel) = find(data, pos, LFH) {
        let i = rel;
        if i + 30 > data.len() {
            break;
        }
        let flags = u16::from_le_bytes([data[i + 6], data[i + 7]]);
        let method = u16::from_le_bytes([data[i + 8], data[i + 9]]);
        let csize =
            u32::from_le_bytes([data[i + 18], data[i + 19], data[i + 20], data[i + 21]]) as u64;
        let _uncomp =
            u32::from_le_bytes([data[i + 22], data[i + 23], data[i + 24], data[i + 25]]) as u64;
        let nlen = u16::from_le_bytes([data[i + 26], data[i + 27]]) as usize;
        let elen = u16::from_le_bytes([data[i + 28], data[i + 29]]) as usize;
        let name_start = i + 30;
        let data_start = name_start + nlen + elen;
        if data_start > data.len() {
            pos = i + 4;
            continue;
        }
        let name = String::from_utf8_lossy(&data[name_start..name_start + nlen]).to_string();
        // 已压缩尺寸已知时，直接跳过数据区，避免在压缩数据里误匹配签名
        let next = if flags & 0x8 == 0
            && csize > 0
            && data_start.saturating_add(csize as usize) <= data.len()
        {
            data_start + csize as usize
        } else {
            data_start
        };
        pos = next.max(i + 4);

        order += 1;
        let label = if name.is_empty() {
            format!("<未命名#{order}>")
        } else {
            name.clone()
        };
        let result = (|| -> Result<usize, String> {
            let rel_path = sanitize_rel_path(&name).map_err(|e| e.to_string())?;
            let payload = match method {
                0 => data
                    .get(data_start..data_start.saturating_add(csize as usize))
                    .ok_or_else(|| "数据区越界".to_string())?
                    .to_vec(),
                8 => {
                    let raw = data
                        .get(data_start..)
                        .ok_or_else(|| "数据区越界".to_string())?;
                    let capped = if csize > 0 {
                        let n = (csize as usize).min(raw.len());
                        &raw[..n]
                    } else {
                        raw
                    };
                    let mut out = Vec::new();
                    DeflateDecoder::new(capped)
                        .take(MAX_ENTRY)
                        .read_to_end(&mut out)
                        .map_err(|e| format!("解压失败: {e}"))?;
                    out
                }
                m => return Err(format!("不支持的压缩方法 {m}")),
            };
            if payload.len() as u64 > MAX_ENTRY {
                return Err("解压后超出 512 MiB 上限".to_string());
            }
            let n = write_out(out_dir, &rel_path, &payload)?;
            Ok(n)
        })();
        lines.push(
            result
                .map(|n| format!("{label}（{n} 字节）"))
                .map_err(|e| format!("{label}: {e}")),
        );
    }

    Ok(("Local Header 签名扫描（中央目录不可用）".to_string(), lines))
}

fn find(data: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if from >= data.len() {
        return None;
    }
    data[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

/// 条目名 → 输出目录内的安全相对路径（spec/azodoc-container.md §5.1）。
fn sanitize_rel_path(name: &str) -> Result<PathBuf, String> {
    if name.contains('\\') {
        return Err("路径含反斜杠".to_string());
    }
    let mut out = PathBuf::new();
    for seg in name.split('/') {
        if seg.is_empty() || seg == "." {
            continue;
        }
        if seg == ".." {
            return Err("路径含 `..`".to_string());
        }
        out.push(seg);
    }
    if out.as_os_str().is_empty() {
        return Err("空路径".to_string());
    }
    Ok(out)
}

fn write_out(out_dir: &Path, rel: &Path, bytes: &[u8]) -> Result<usize, String> {
    let target = out_dir.join(rel);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&target, bytes).map_err(|e| e.to_string())?;
    Ok(bytes.len())
}
