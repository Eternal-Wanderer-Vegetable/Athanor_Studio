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

//! Prima 内容模型类型（spec/azodoc-model.md）。
//!
//! 类型层刻意保持宽容（未知字段进 `ExtraMap`，确保往返无损）；
//! 规范级强校验在 [`crate::validate`] 的 Value 层实现。
//! v1.0 中未知节点**类型**不做类型化捕获——遇到未知 type 时 `serde` 会报错；
//! 读取方应先经 [`crate::validate`] 盘点（R3 警告），需要处理未知类型时走 Value 层。

use crate::manifest::ExtraMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// content.json 顶层文件。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ContentFile {
    pub schema_version: String,
    pub content: Vec<Node>,
    #[serde(flatten)]
    pub extra: ExtraMap,
}

/// 已知块/结构节点类型名（含 section 与 unknown）。
pub const KNOWN_NODE_TYPES: &[&str] = &[
    "section",
    "paragraph",
    "heading",
    "quote",
    "list",
    "list_item",
    "code_block",
    "table",
    "figure",
    "image",
    "horizontal_rule",
    "page_break",
    "math_block",
    "callout",
    "embed",
    "footnote",
    "unknown",
];

/// 已知行内 span 类型名。
pub const KNOWN_SPAN_TYPES: &[&str] = &[
    "text",
    "hard_break",
    "strong",
    "em",
    "underline",
    "strike",
    "code",
    "link",
    "footnote_ref",
    "inline_math",
    "inline_image",
    "mention",
    "cite",
    "unknown",
];

/// 节点（顶层与 children 通用）：section + 全部块类型。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum Node {
    #[serde(rename = "section")]
    Section {
        id: String,
        children: Vec<Node>,
        #[serde(flatten)]
        extra: ExtraMap,
    },
    #[serde(rename = "paragraph")]
    Paragraph {
        id: String,
        content: Vec<Span>,
        #[serde(flatten)]
        extra: ExtraMap,
    },
    #[serde(rename = "heading")]
    Heading {
        id: String,
        level: i64,
        content: Vec<Span>,
        #[serde(flatten)]
        extra: ExtraMap,
    },
    #[serde(rename = "quote")]
    Quote {
        id: String,
        children: Vec<Node>,
        #[serde(flatten)]
        extra: ExtraMap,
    },
    #[serde(rename = "list")]
    List {
        id: String,
        style: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start: Option<i64>,
        items: Vec<ListItem>,
        #[serde(flatten)]
        extra: ExtraMap,
    },
    #[serde(rename = "code_block")]
    CodeBlock {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        language: Option<String>,
        text: String,
        #[serde(flatten)]
        extra: ExtraMap,
    },
    #[serde(rename = "table")]
    Table {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        header_row: Option<bool>,
        columns: Vec<TableColumn>,
        rows: Vec<TableRow>,
        #[serde(flatten)]
        extra: ExtraMap,
    },
    #[serde(rename = "figure")]
    Figure {
        id: String,
        asset: String,
        alt: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        caption: Option<Vec<Span>>,
        #[serde(flatten)]
        extra: ExtraMap,
    },
    #[serde(rename = "image")]
    Image {
        id: String,
        asset: String,
        alt: String,
        #[serde(flatten)]
        extra: ExtraMap,
    },
    #[serde(rename = "horizontal_rule")]
    HorizontalRule {
        id: String,
        #[serde(flatten)]
        extra: ExtraMap,
    },
    #[serde(rename = "math_block")]
    MathBlock {
        id: String,
        latex: String,
        #[serde(flatten)]
        extra: ExtraMap,
    },
    #[serde(rename = "callout")]
    Callout {
        id: String,
        variant: String,
        children: Vec<Node>,
        #[serde(flatten)]
        extra: ExtraMap,
    },
    #[serde(rename = "embed")]
    Embed {
        id: String,
        asset: String,
        #[serde(flatten)]
        extra: ExtraMap,
    },
    #[serde(rename = "footnote")]
    Footnote {
        id: String,
        children: Vec<Node>,
        #[serde(flatten)]
        extra: ExtraMap,
    },
    #[serde(rename = "unknown")]
    Unknown {
        id: String,
        origin: String,
        loss_class: String,
        summary: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        payload_ref: Option<String>,
        #[serde(default)]
        content: Vec<Span>,
        #[serde(flatten)]
        extra: ExtraMap,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ListItem {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    pub children: Vec<Node>,
    #[serde(flatten)]
    pub extra: ExtraMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TableColumn {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 相对宽度单位（仅比例有意义，非绝对单位；azodoc-model.md §6.5）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<i64>,
    #[serde(flatten)]
    pub extra: ExtraMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TableRow {
    pub id: String,
    pub cells: Vec<TableCell>,
    #[serde(flatten)]
    pub extra: ExtraMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TableCell {
    pub id: String,
    pub column: i64,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "colSpan")]
    pub col_span: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "rowSpan")]
    pub row_span: Option<i64>,
    /// 表头角色（"header" | "body"，默认 body；azodoc-model.md §6.5）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    pub children: Vec<Node>,
    #[serde(flatten)]
    pub extra: ExtraMap,
}

/// 行内 span。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum Span {
    #[serde(rename = "text")]
    Text {
        text: String,
        #[serde(flatten)]
        extra: ExtraMap,
    },
    #[serde(rename = "hard_break")]
    HardBreak {
        #[serde(flatten)]
        extra: ExtraMap,
    },
    #[serde(rename = "strong")]
    Strong {
        content: Vec<Span>,
        #[serde(flatten)]
        extra: ExtraMap,
    },
    #[serde(rename = "em")]
    Em {
        content: Vec<Span>,
        #[serde(flatten)]
        extra: ExtraMap,
    },
    #[serde(rename = "underline")]
    Underline {
        content: Vec<Span>,
        #[serde(flatten)]
        extra: ExtraMap,
    },
    #[serde(rename = "strike")]
    Strike {
        content: Vec<Span>,
        #[serde(flatten)]
        extra: ExtraMap,
    },
    #[serde(rename = "code")]
    Code {
        text: String,
        #[serde(flatten)]
        extra: ExtraMap,
    },
    #[serde(rename = "link")]
    Link {
        url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        content: Vec<Span>,
        #[serde(flatten)]
        extra: ExtraMap,
    },
    #[serde(rename = "footnote_ref")]
    FootnoteRef {
        id: String,
        #[serde(flatten)]
        extra: ExtraMap,
    },
    #[serde(rename = "inline_math")]
    InlineMath {
        latex: String,
        #[serde(flatten)]
        extra: ExtraMap,
    },
    #[serde(rename = "inline_image")]
    InlineImage {
        asset: String,
        alt: String,
        #[serde(flatten)]
        extra: ExtraMap,
    },
    #[serde(rename = "mention")]
    Mention {
        target: String,
        #[serde(flatten)]
        extra: ExtraMap,
    },
    #[serde(rename = "cite")]
    Cite {
        key: String,
        #[serde(flatten)]
        extra: ExtraMap,
    },
    #[serde(rename = "unknown")]
    Unknown {
        origin: String,
        loss_class: String,
        summary: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        payload_ref: Option<String>,
        #[serde(default)]
        content: Vec<Span>,
        #[serde(flatten)]
        extra: ExtraMap,
    },
}

impl ContentFile {
    pub fn from_value(v: &serde_json::Value) -> Result<Self, serde_json::Error> {
        serde_json::from_value(v.clone())
    }
}
