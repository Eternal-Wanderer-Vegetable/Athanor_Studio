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

//! Pandoc 子进程桥（许可隔离边界：以独立进程调用 GPL 工具，经标准流交换
//! 通用 JSON，不链接、不分发、不依赖其源码——见落地方案 §8.1）。
//!
//! 运行时可选：用户未安装 Pandoc 时返回带安装指引的友好错误，核心功能不受影响。
//! 查找顺序：`AZODOC_PANDOC_PATH` 环境变量 → PATH → 可执行文件/当前目录的
//! `tools/pandoc-*/pandoc[.exe]` 逐级向上搜索。

use serde_json::Value;
use std::io::Write as IoWrite;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, thiserror::Error)]
pub enum DocxError {
    #[error("未找到 Pandoc（已搜索: {searched}）")]
    PandocNotFound { searched: String },
    #[error("Pandoc 执行失败（退出码 {exit:?}）: {stderr}")]
    PandocFailed { exit: Option<i32>, stderr: String },
    #[error("Pandoc 超时（{timeout_secs}s），进程已终止")]
    Timeout { timeout_secs: u64 },
    #[error("Pandoc 输出不是合法 JSON: {0}")]
    BadJson(String),
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
}

impl DocxError {
    /// 四段式友好输出。
    pub fn friendly(&self) -> String {
        match self {
            DocxError::PandocNotFound { searched } => format!(
                "错误：DOCX 转换需要 Pandoc，但未找到（已搜索: {searched}）。\n\
                 文件不受影响；Athanor 其余功能不受影响。你可以：\n\
                 1. 安装 Pandoc: winget install --id JohnMacFarlane.Pandoc\n   \
                 （或 choco install pandoc，或从 https://github.com/jgm/pandoc/releases 下载便携版\n   \
                 解压到仓库 tools/pandoc-3.11/ 即可被自动发现）\n\
                 2. 指定路径: 设置环境变量 AZODOC_PANDOC_PATH 指向 pandoc 可执行文件"
            ),
            DocxError::PandocFailed { .. } => format!(
                "错误：{self}\n输入文件可能不是有效的 DOCX。建议：\n\
                 1. 用 Word/LibreOffice 打开确认文件可读\n\
                 2. 将完整错误信息提交 issue"
            ),
            DocxError::Timeout { timeout_secs: _ } => format!(
                "错误：{self}\n文件可能异常复杂或损坏。建议：\n\
                 1. 用 Word/LibreOffice 确认文件可读\n\
                 2. 尝试拆分文档后重试"
            ),
            _ => format!("错误：{self}\n建议：检查输入与运行环境后重试。"),
        }
    }
}

/// 定位 pandoc 可执行文件。找不到时返回搜索过的位置（用于错误信息）。
pub fn find_pandoc() -> Result<PathBuf, DocxError> {
    let mut searched: Vec<String> = Vec::new();

    if let Ok(p) = std::env::var("AZODOC_PANDOC_PATH") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Ok(p);
        }
        searched.push(format!("AZODOC_PANDOC_PATH={}", p.display()));
    }

    // PATH 上的 pandoc
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            for exe in ["pandoc.exe", "pandoc"] {
                let candidate = dir.join(exe);
                if candidate.is_file() {
                    return Ok(candidate);
                }
            }
        }
    }
    searched.push("PATH".to_string());

    // exe 目录与当前目录的各级祖先下的 tools/pandoc-*/
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            roots.push(dir.to_path_buf());
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd);
    }
    for root in &roots {
        let mut dir: Option<&Path> = Some(root.as_path());
        while let Some(d) = dir {
            let tools = d.join("tools");
            if let Ok(entries) = std::fs::read_dir(&tools) {
                let mut found: Vec<PathBuf> = entries
                    .flatten()
                    .filter(|e| e.file_name().to_string_lossy().starts_with("pandoc-"))
                    .map(|e| e.path())
                    .collect();
                found.sort();
                if let Some(latest) = found.pop() {
                    for exe in ["pandoc.exe", "pandoc"] {
                        let candidate = latest.join(exe);
                        if candidate.is_file() {
                            return Ok(candidate);
                        }
                    }
                }
            }
            searched.push(format!("{}", tools.display()));
            dir = d.parent();
        }
    }

    Err(DocxError::PandocNotFound {
        searched: searched.join("; "),
    })
}

/// 运行 pandoc 子进程：输入走 stdin，输出取 stdout；超时强杀。
pub fn run_pandoc(
    pandoc: &Path,
    args: &[String],
    input: &[u8],
    timeout: Duration,
) -> Result<Vec<u8>, DocxError> {
    let mut child = Command::new(pandoc)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(DocxError::Io)?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input).map_err(DocxError::Io)?;
    }
    drop(child.stdin.take());

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait().map_err(DocxError::Io)? {
            Some(status) => {
                let output = child.wait_with_output().map_err(DocxError::Io)?;
                if !status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let tail: String = stderr
                        .lines()
                        .rev()
                        .take(6)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect::<Vec<_>>()
                        .join(" | ");
                    return Err(DocxError::PandocFailed {
                        exit: status.code(),
                        stderr: tail,
                    });
                }
                return Ok(output.stdout);
            }
            None => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(DocxError::Timeout {
                        timeout_secs: timeout.as_secs(),
                    });
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        }
    }
}

/// 导入：docx → pandoc JSON AST（附带 --extract-media 提取嵌入媒体）。
pub fn docx_to_ast(
    pandoc: &Path,
    docx_bytes: &[u8],
    extract_media_dir: &Path,
    timeout: Duration,
) -> Result<Value, DocxError> {
    let media = extract_media_dir.to_string_lossy().to_string();
    let args = vec![
        "-f".to_string(),
        "docx".to_string(),
        "-t".to_string(),
        "json".to_string(),
        "--track-changes=all".to_string(),
        "--extract-media".to_string(),
        media,
    ];
    let stdout = run_pandoc(pandoc, &args, docx_bytes, timeout)?;
    let ast: Value =
        serde_json::from_slice(&stdout).map_err(|e| DocxError::BadJson(e.to_string()))?;
    Ok(ast)
}

/// 导出：pandoc AST JSON → docx 字节（docx 是二进制，经 -o 临时文件取回）。
pub fn ast_to_docx(
    pandoc: &Path,
    ast: &serde_json::Value,
    timeout: Duration,
) -> Result<Vec<u8>, DocxError> {
    let mut input = serde_json::to_vec(ast).map_err(|e| DocxError::BadJson(e.to_string()))?;
    input.push(b'\n');

    let out_path = unique_tmp("out").join("document.docx");
    std::fs::create_dir_all(out_path.parent().expect("有父目录")).map_err(DocxError::Io)?;
    let args: Vec<String> = vec![
        "-f".into(),
        "json".into(),
        "-t".into(),
        "docx".into(),
        "-o".into(),
        out_path.to_string_lossy().to_string(),
    ];
    let bytes = run_pandoc(pandoc, &args, &input, timeout)
        .and_then(|_| std::fs::read(&out_path).map_err(DocxError::Io));
    let _ = std::fs::remove_dir_all(out_path.parent().expect("有父目录"));
    bytes
}

fn unique_tmp(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("athanor-docx-{tag}-{}-{nanos}", std::process::id()))
}
