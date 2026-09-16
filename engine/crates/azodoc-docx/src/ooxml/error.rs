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

//! 原生 OOXML 读取的错误类型（四段式友好输出，体例同 bridge::DocxError）。

#[derive(Debug, thiserror::Error)]
pub enum OoxmlError {
    #[error("输入不是有效的 ZIP/OOXML 包: {0}")]
    NotZip(String),
    #[error("OOXML 包缺少必需部件: {0}")]
    MissingPart(String),
    #[error("部件 {part} 不是合法 XML: {msg}")]
    BadXml { part: String, msg: String },
    #[error("部件 {part} 已加密，无法读取")]
    Encrypted { part: String },
    #[error("部件 {part} 解压后 {size} 字节，超过安全上限 {limit} 字节")]
    TooLarge { part: String, size: u64, limit: u64 },
    #[error("OOXML 包条目数超过安全上限 {limit}")]
    TooManyEntries { limit: usize },
    #[error("条目名不安全（疑似路径穿越）: {0}")]
    UnsafeName(String),
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
}

impl OoxmlError {
    /// 四段式：发生了什么 / 文件状态 / 建议 / 恢复途径。
    pub fn friendly(&self) -> String {
        match self {
            OoxmlError::NotZip(_) => format!(
                "错误：{self}\n\
                 文件可能不是 .docx（OOXML）格式，或已损坏；Athanor 其余功能不受影响。你可以：\n\
                 1. 用 Word/LibreOffice 打开确认文件可读\n\
                 2. 重新另存为 .docx 后重试\n\
                 3. 需要最高兼容性时可安装 Pandoc 后用 --reader pandoc"
            ),
            OoxmlError::Encrypted { .. } => format!(
                "错误：{self}\n\
                 输入是带密码保护的 DOCX；Athanor 不做密码求解。你可以：\n\
                 1. 在 Word/LibreOffice 中另存为无密码副本后重试"
            ),
            OoxmlError::TooLarge { .. } | OoxmlError::TooManyEntries { .. } => format!(
                "错误：{self}\n\
                 触发的是安全护栏而非文件损坏。如确认来源可信且确需处理，可拆分文档后重试。"
            ),
            OoxmlError::UnsafeName(_) => format!(
                "错误：{self}\n\
                 输入包结构异常，已拒绝处理以保护本机文件系统。建议用 Word/LibreOffice 重新另存。"
            ),
            OoxmlError::MissingPart(_) => format!(
                "错误：{self}\n\
                 文件可能不是 Word 生成的完整 OOXML 包。你可以：\n\
                 1. 用 Word/LibreOffice 打开确认并另存为 .docx\n\
                 2. 安装 Pandoc 后用 --reader pandoc 走兼容桥"
            ),
            _ => format!(
                "错误：{self}\n\
                 建议检查输入文件后重试；需要时可安装 Pandoc 用 --reader pandoc 走兼容桥。"
            ),
        }
    }
}
