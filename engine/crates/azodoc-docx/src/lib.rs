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

//! azodoc-docx — DOCX 读写器。
//!
//! 读取有两条路径（RFC B2）：
//! - 原生 OOXML 读取（[`ooxml::import_native`]）：进程内解析
//!   document.xml/relationships/styles.xml，样式→theme、批注→annotations、
//!   tracked changes→终稿视图+原件 preserved；无外部依赖。
//! - Pandoc 子进程桥（[`import`]）：GPL 隔离（进程边界 + 通用 JSON 流），
//!   运行时可选，未安装时返回带安装指引的友好错误。
//!
//! 统一入口 [`ooxml::import_docx`]：`--reader auto|native|pandoc`，
//! auto = 原生优先、硬失败回落 Pandoc 并在报告记录回落事件。

pub mod ast_in;
pub mod ast_out;
pub mod bridge;
pub mod inventory;
pub mod ooxml;

use azodoc_convert::{ExportDoc, ImportJob, ImportOutput, LossClass, LossLog};
use std::path::{Path, PathBuf};

pub fn converter_name() -> &'static str {
    "athanor-docx"
}

fn unique_tmp(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("athanor-docx-{tag}-{}-{nanos}", std::process::id()))
}

/// DOCX 导入：pandoc 解析 → Prima；清点 Pandoc 丢失的部件并报告。
/// 资产与 preserved 载荷写入调用方的 `job`（CLI 装配容器时使用）。
pub fn import(docx: &[u8], job: &mut ImportJob) -> Result<ImportOutput, bridge::DocxError> {
    let pandoc = bridge::find_pandoc()?;
    let tmp = unique_tmp("import");
    std::fs::create_dir_all(&tmp).map_err(bridge::DocxError::Io)?;

    let result = (|| -> Result<ImportOutput, bridge::DocxError> {
        let ast = bridge::docx_to_ast(&pandoc, docx, &tmp, bridge::DEFAULT_TIMEOUT)?;
        let inv = inventory::inventory(docx).unwrap_or_default();

        let mut log = LossLog::new();
        let mut ctx = ast_in::ImportCtx {
            job,
            extract_dir: tmp.clone(),
            log: &mut log,
            pending_footnotes: Vec::new(),
            pending_blocks: Vec::new(),
        };
        let content = ast_in::ast_to_prima(&ast, &mut ctx);
        drop(ctx);

        // 清点出的丢失部件（Pandoc 静默丢弃，必须在此补报）
        for (feature, what, msg) in inventory::loss_issues(&inv) {
            log.record(
                LossClass::Unsupported,
                &feature,
                None,
                "dropped",
                &what,
                msg,
            );
        }

        let title = inv.title.clone().or_else(|| ast_in::meta_title(&ast));
        Ok(ImportOutput {
            content,
            title,
            language: None,
            doc_extra: Default::default(),
            annotations: Vec::new(),
            theme: None,
            log,
        })
    })();

    let _ = std::fs::remove_dir_all(&tmp);
    result
}

/// DOCX 导出：Prima → pandoc AST → docx 字节。
pub fn export(doc: &ExportDoc, log: &mut LossLog) -> Result<Vec<u8>, bridge::DocxError> {
    let pandoc = bridge::find_pandoc()?;
    let tmp = unique_tmp("export");
    std::fs::create_dir_all(&tmp).map_err(bridge::DocxError::Io)?;

    let result = (|| -> Result<Vec<u8>, bridge::DocxError> {
        let ast = ast_out::prima_to_ast(doc, &tmp.join("assets"), log).map_err(|e| {
            bridge::DocxError::Io(std::io::Error::other(format!("AST 构建失败: {e}")))
        })?;
        bridge::ast_to_docx(&pandoc, &ast, bridge::DEFAULT_TIMEOUT)
    })();

    let _ = std::fs::remove_dir_all(&tmp);
    result
}

/// 供 CLI 探测 Pandoc 可用性（不执行转换）。
pub fn pandoc_available() -> bool {
    bridge::find_pandoc().is_ok()
}

/// 供 CLI 错误输出使用。
pub fn not_found_friendly(path: &Path) -> String {
    let _ = path;
    match bridge::find_pandoc() {
        Ok(p) => format!("Pandoc 位于 {}", p.display()),
        Err(e) => e.friendly(),
    }
}
