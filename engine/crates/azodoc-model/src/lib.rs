//! azodoc-model — Azodoc Prima 内容模型与 manifest 类型。
//!
//! 设计要点（对应 spec/azodoc-model.md 与 spec/azodoc-package.md）：
//! - 所有类型都带 `#[serde(flatten)] extra` 捕获未知字段（保真规则 R2）；
//! - 类型层是宽容的（保证往返不丢数据），规范级的结构校验在 [`validate`] 中以
//!   Value 层实现（严格），供 `athanor verify` 使用；
//! - [`schema_gen`] 用 schemars 生成 JSON Schema，测试与 spec/json-schema 做双向
//!   接受性比对（落地方案 M1 验收 ④）。

pub mod content;
pub mod id;
pub mod manifest;
pub mod schema_gen;
pub mod validate;

/// 模型层统一错误。
#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("JSON 解析失败: {0}")]
    Json(#[from] serde_json::Error),
    #[error("ID 无效: {0}")]
    Id(#[from] id::IdError),
    #[error("结构不符合 v1 规范: {0}")]
    Structure(String),
}

/// JSON 对象便捷别名（开启 preserve_order，保持键序以支持字节稳定重写）。
pub type JsonMap = serde_json::Map<String, serde_json::Value>;
