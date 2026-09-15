//! azodoc-docx — DOCX 读写器（Pandoc 子进程桥接）。
//!
//! 许可隔离：以独立子进程调用 GPL 工具 Pandoc，经标准流交换通用 JSON，
//! 不链接、不分发其代码，核心许可（MIT OR Apache-2.0）不受传染。
//! Pandoc 为运行时可选依赖：未安装时返回带安装指引的友好错误。

pub mod ast_in;
pub mod ast_out;
pub mod bridge;
pub mod inventory;

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
        let mut log = log;

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
            bridge::DocxError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("AST 构建失败: {e}"),
            ))
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
