//! schemars JSON Schema 生成（落地方案 M1 验收 ④）。
//!
//! 生成 Schema 是 spec/json-schema 手写规范的机器投影；CI 测试断言：
//! 黄金样例的各层文件在两套 Schema 下都被接受（防止规范与代码漂移）。
//! 规范文件始终是 normative；两者冲突时修 spec（见 azodoc-model.md §2 的裁决规则类推）。

use crate::content::ContentFile;
use crate::manifest::Manifest;
use serde_json::Value;

pub fn manifest_schema() -> Value {
    serde_json::to_value(schemars::schema_for!(Manifest)).expect("manifest schema 生成失败")
}

pub fn content_schema() -> Value {
    serde_json::to_value(schemars::schema_for!(ContentFile)).expect("content schema 生成失败")
}

pub fn manifest_schema_json_pretty() -> String {
    serde_json::to_string_pretty(&manifest_schema()).expect("manifest schema 序列化失败")
}

pub fn content_schema_json_pretty() -> String {
    serde_json::to_string_pretty(&content_schema()).expect("content schema 序列化失败")
}
