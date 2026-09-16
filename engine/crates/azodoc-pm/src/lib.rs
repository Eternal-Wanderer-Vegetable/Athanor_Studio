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

//! azodoc-pm — Prima ↔ ProseMirror JSON 双向转换与 schema 生成（M6.1）。
//!
//! 设计与映射表见 `design_docs/M6 — Aludel 编辑器原型计划.md`。要点：
//! - 唯一人工维护点是 [`mapping`] 映射表；生成物（`gen/aludel-schema.json`、
//!   `gen/schema.mjs`）由 `cargo run -p azodoc-pm --bin generate` 重造并以黄金
//!   测试锁定；
//! - 转换在 Value 层进行（与 `azodoc-model::validate` 同一哲学）：load 侧接受
//!   content.json 的 `Value`，save 侧产出 content.json 的 `Value`，是否结构合规
//!   由调用方（M6.2 保存管线）走 `azodoc_model::validate` 裁决；
//! - 恒等标准是**归一化恒等**：嵌套 span 与 PM flat marks 非结构双射，两端都
//!   产出规范形（rank 序嵌套、相邻同 marks 文本合并），规范形之间 Value 恒等。

pub mod from_prima;
pub mod gen;
pub mod mapping;
pub mod to_prima;

pub use from_prima::content_file_to_pm;
pub use to_prima::{pm_to_content_file, ToPrimaResult};

/// 转换层错误。
#[derive(Debug, thiserror::Error)]
pub enum PmError {
    #[error("未知的 ProseMirror 节点类型: `{0}`")]
    UnknownPmNode(String),
    #[error("未知的 ProseMirror 标记类型: `{0}`")]
    UnknownPmMark(String),
    #[error("结构不符合预期: {0}")]
    BadStructure(String),
    #[error("JSON 处理失败: {0}")]
    Json(#[from] serde_json::Error),
}
