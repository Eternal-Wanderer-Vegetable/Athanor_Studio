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

//! 无头 Chromium/Edge 打印管线（PDF 出版，落地方案 §8.2）。
//!
//! 浏览器查找顺序：`AZODOC_BROWSER_PATH` → PATH（msedge/chrome）→
//! Windows 标准安装位置 → 各级 `tools/` 目录。Windows 上 Edge 随系统自带，
//! 通常无需额外安装。
//!
//! 两条打印路径：
//! - [`paged`]：CDP + Paged.js 分页（页码边盒/运行头，出版品质，默认）；
//! - CLI 直印（`print_html_to_pdf`）：无头 `--print-to-pdf`，作为回退。

mod cdp;
pub mod paged;

pub use paged::{
    augment_print_html, print_html_to_pdf_paged, write_polyfill_assets, PagedRenderInfo, PAGED_CSS,
};

use std::io::Write as IoWrite;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    #[error("未找到可用的 Chromium/Edge 浏览器（已搜索: {searched}）")]
    BrowserNotFound { searched: String },
    #[error("浏览器打印失败（退出码 {exit:?}）: {stderr}")]
    PrintFailed { exit: Option<i32>, stderr: String },
    #[error("打印超时（{timeout_secs}s）")]
    Timeout { timeout_secs: u64 },
    #[error("PDF 产物异常: {0}")]
    BadOutput(String),
    #[error("CDP 会话失败: {0}")]
    Cdp(String),
    #[error("Paged.js 分页失败: {0}")]
    Paged(String),
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
}

impl PdfError {
    pub fn friendly(&self) -> String {
        match self {
            PdfError::BrowserNotFound { searched } => format!(
                "错误：PDF 出版需要无头 Chromium/Edge，但未找到（已搜索: {searched}）。\n\
                 文件不受影响；Athanor 其余功能不受影响。你可以：\n\
                 1. Windows 自带的 Microsoft Edge 即可使用（确认未卸载）\n\
                 2. 或安装 Chrome\n\
                 3. 或指定路径: 设置环境变量 AZODOC_BROWSER_PATH 指向 msedge.exe/chrome.exe"
            ),
            PdfError::PrintFailed { .. } => {
                format!(
                    "错误：{self}\n建议：确认浏览器版本较新（需支持 --headless --print-to-pdf）。"
                )
            }
            PdfError::Cdp(_) | PdfError::Paged(_) => format!(
                "错误：{self}\n建议：确认浏览器版本较新（Chromium 90+）；\
                 或用 --no-paged 回退 Chromium 直印（无页码边盒）。"
            ),
            _ => format!("错误：{self}\n建议：检查运行环境后重试。"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Browser {
    pub path: PathBuf,
    /// 渲染器指纹：路径 + 文件尺寸 + 修改时间的 SHA-256（同安装内确定）
    pub fingerprint: String,
}

fn fingerprint(path: &Path) -> String {
    use sha2::Digest;
    let meta = std::fs::metadata(path).ok();
    let len = meta.as_ref().map(|m| m.len()).unwrap_or(0);
    let mtime = meta
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut hasher = sha2::Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());
    hasher.update(len.to_le_bytes());
    hasher.update(mtime.to_le_bytes());
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// 定位可用的 Chromium 系浏览器。
pub fn find_browser() -> Result<Browser, PdfError> {
    let mut searched: Vec<String> = Vec::new();

    if let Ok(p) = std::env::var("AZODOC_BROWSER_PATH") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Ok(Browser {
                fingerprint: fingerprint(&p),
                path: p,
            });
        }
        searched.push(format!("AZODOC_BROWSER_PATH={}", p.display()));
    }

    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            for exe in [
                "msedge.exe",
                "chrome.exe",
                "msedge",
                "chrome",
                "google-chrome",
                "google-chrome-stable",
                "chromium",
            ] {
                let candidate = dir.join(exe);
                if candidate.is_file() {
                    return Ok(Browser {
                        fingerprint: fingerprint(&candidate),
                        path: candidate,
                    });
                }
            }
        }
    }
    searched.push("PATH".to_string());

    // Windows 标准安装位置
    let mut candidates: Vec<PathBuf> = Vec::new();
    for base in [
        r"C:\Program Files (x86)\Microsoft\Edge\Application",
        r"C:\Program Files\Microsoft\Edge\Application",
        r"C:\Program Files\Google\Chrome\Application",
        r"C:\Program Files (x86)\Google\Chrome\Application",
    ] {
        candidates.push(PathBuf::from(base).join("msedge.exe"));
        candidates.push(PathBuf::from(base).join("chrome.exe"));
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        candidates.push(PathBuf::from(local).join(r"Google\Chrome\Application\chrome.exe"));
    }
    // 仓库 tools/ 目录（便携版浏览器）
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
            candidates.push(d.join("tools/msedge/msedge.exe"));
            candidates.push(d.join("tools/chrome/chrome.exe"));
            dir = d.parent();
        }
    }

    for candidate in candidates {
        if candidate.is_file() {
            return Ok(Browser {
                fingerprint: fingerprint(&candidate),
                path: candidate,
            });
        }
    }
    searched.push("Windows 标准安装位置".to_string());
    searched.push("tools/".to_string());

    Err(PdfError::BrowserNotFound {
        searched: searched.join("; "),
    })
}

/// HTML 文件 → PDF 文件（无头打印）。
pub fn print_html_to_pdf(
    browser: &Browser,
    html_path: &Path,
    pdf_path: &Path,
    timeout: Duration,
) -> Result<(), PdfError> {
    let mut child = Command::new(&browser.path)
        .args([
            "--headless",
            "--disable-gpu",
            "--no-first-run",
            "--disable-extensions",
            "--no-pdf-header-footer",
        ])
        .arg(format!("--print-to-pdf={}", pdf_path.display()))
        .arg(html_path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(PdfError::Io)?;

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait().map_err(PdfError::Io)? {
            Some(s) => break s,
            None => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(PdfError::Timeout {
                        timeout_secs: timeout.as_secs(),
                    });
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    };

    let output = child.wait_with_output().map_err(PdfError::Io)?;
    if !status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail: String = stderr
            .lines()
            .rev()
            .take(5)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join(" | ");
        return Err(PdfError::PrintFailed {
            exit: status.code(),
            stderr: tail,
        });
    }
    if !pdf_path.is_file() {
        return Err(PdfError::BadOutput("浏览器未产出 PDF 文件".to_string()));
    }
    Ok(())
}

/// 最佳努力统计 PDF 页数（扫描页树 /Count；扫描不到时返回 None）。
pub fn pdf_page_count(pdf_bytes: &[u8]) -> Option<u64> {
    // 优先取页树 /Count 的最大值
    let mut best: Option<u64> = None;
    let mut rest = pdf_bytes;
    while let Some(pos) = find(rest, b"/Count") {
        let after = &rest[pos + 6..];
        let mut num = Vec::new();
        for b in after.iter().take(16) {
            if b.is_ascii_digit() {
                num.push(*b);
            } else if !num.is_empty() {
                break;
            }
        }
        if !num.is_empty() {
            let n: u64 = String::from_utf8_lossy(&num).parse().unwrap_or(0);
            best = Some(best.map_or(n, |b| b.max(n)));
        }
        rest = &rest[pos + 6..];
        if rest.is_empty() {
            break;
        }
    }
    best.filter(|n| *n > 0)
}

fn find(data: &[u8], needle: &[u8]) -> Option<usize> {
    data.windows(needle.len()).position(|w| w == needle)
}

/// 控制台小工具（调试用）。
pub fn write_stdout(file: &Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::File::create(file)?.write_all(bytes)
}
