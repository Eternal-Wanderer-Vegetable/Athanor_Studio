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

//! Prima ↔ ProseMirror 映射表 —— 本 crate 唯一的人工维护点。
//!
//! 设计说明见 `design_docs/M6 — Aludel 编辑器原型计划.md` §4。约定：
//! - `attrs` 列表按 Prima 字段声明序排列（生成 JSON 的键序即此序）；
//!   `id` 恒为第一个 attr，`extra` 恒为最后一个（ExtraMap 透传，R2）；
//! - `prima_fields` 是该 Prima 类型的**全部**已声明字段（声明序，不含 `type`
//!   与 flatten 的 extra），与 schemars 产物逐一锁定
//!   （`tests/mapping_consistency.rs`）——映射表跟不上 `azodoc-model` 改动时
//!   CI 直接失败；
//! - 修改本表后必须 `cargo run -p azodoc-pm --bin generate` 重造 `gen/` 黄金文件。

use azodoc_model::id::IdKind;

/// attr 规格兼默认值（`gen` 侧据此写 PM attrs default，`to_prima` 侧据此补缺）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttrSpec {
    /// 非空字符串，默认值随表给出
    Text(&'static str),
    /// 非空整数，默认值随表给出
    Int(i64),
    /// 可空字符串
    OptText,
    /// 可空整数
    OptInt,
    /// 跨度整数（colSpan/rowSpan）：PM 侧默认值是 1（prosemirror-tables
    /// 的 TableMap 按 attrs.colspan 做算术，null 会算出 zero_sized 网格）。
    /// 与 OptInt 的区别只在 schema 默认；写回逻辑同 OptInt（null/1 即默认）。
    Span,
    /// 可空布尔
    OptBool,
    /// 透传 JSON（对象/数组），可空：columns / caption / extra / span_extra.data
    OptJson,
}

/// 单个 attr 定义。`name` 与 Prima 字段同名；PM attr 名默认相同，
/// prosemirror-tables 惯例用小写名的见 [`pm_attr_name`]。
#[derive(Debug, Clone, Copy)]
pub struct AttrDef {
    pub name: &'static str,
    pub spec: AttrSpec,
}

/// PM attr 名：Prima `colSpan`/`rowSpan` 在 PM 侧沿用 prosemirror-tables
/// 惯例的 `colspan`/`rowspan`/`colwidth`，其余同名。
pub fn pm_attr_name(name: &str) -> &str {
    match name {
        "colSpan" => "colspan",
        "rowSpan" => "rowspan",
        other => other,
    }
}

/// prosemirror-tables 角色：table/row/cell/header_cell（含 isolating 语义）。
/// 由 pm_name 查表，不进入 NodeDef 字段（避免每个定义重复声明）。
pub fn table_role(pm_name: &str) -> Option<&'static str> {
    Some(match pm_name {
        "table" => "table",
        "table_row" => "row",
        "table_cell" => "cell",
        "table_header" => "header_cell",
        _ => return None,
    })
}

const A_ID: AttrDef = AttrDef {
    name: "id",
    spec: AttrSpec::Text(""),
};
const A_EXTRA: AttrDef = AttrDef {
    name: "extra",
    spec: AttrSpec::OptJson,
};
const A_ASSET: AttrDef = AttrDef {
    name: "asset",
    spec: AttrSpec::Text(""),
};
const A_ALT: AttrDef = AttrDef {
    name: "alt",
    spec: AttrSpec::Text(""),
};
const A_ORIGIN: AttrDef = AttrDef {
    name: "origin",
    spec: AttrSpec::Text(""),
};
const A_LOSS_CLASS: AttrDef = AttrDef {
    name: "loss_class",
    spec: AttrSpec::Text(""),
};
const A_SUMMARY: AttrDef = AttrDef {
    name: "summary",
    spec: AttrSpec::Text(""),
};
const A_PAYLOAD_REF: AttrDef = AttrDef {
    name: "payload_ref",
    spec: AttrSpec::OptText,
};

/// PM 节点定义。
#[derive(Debug, Clone, Copy)]
pub struct NodeDef {
    pub pm_name: &'static str,
    /// 对应的 Prima `type` 标签；`None` = 无标签的结构（doc / list_item / table_row /
    /// table_cell，Prima 中它们是不带 type 的普通对象）
    pub prima_type: Option<&'static str>,
    /// PM group：`"block"` / `"inline"` / `""`（不编组，仅按名引用）
    pub group: &'static str,
    /// PM content 表达式；`None` = 原子节点
    pub content: Option<&'static str>,
    /// code 语义（code_block 的 text 子节点按等宽处理）
    pub code: bool,
    /// ID 补发种类；`None` = 无自身身份（doc/text）或是引用字段（footnote_ref.id）
    pub id_kind: Option<IdKind>,
    pub attrs: &'static [AttrDef],
    /// Prima 全部字段（声明序，不含 type 与 flatten 的 extra）
    pub prima_fields: &'static [&'static str],
}

/// PM 标记定义。
#[derive(Debug, Clone, Copy)]
pub struct MarkDef {
    pub pm_name: &'static str,
    /// 对应的 Prima span `type`；`None` = 合成标记（span_extra，Text.extra 的载体）
    pub prima_type: Option<&'static str>,
    /// 嵌套重建序（升序为从外到内）；`span_extra` 恒排最后且不参与嵌套
    pub rank: u8,
    pub attrs: &'static [AttrDef],
    pub prima_fields: &'static [&'static str],
}

/// code mark 的 rank（最内/叶——Prima 的 Code 是 text 叶，不可包裹子 span）。
pub const CODE_RANK: u8 = 5;
/// span_extra 的 rank（隐形内部标记，恒不参与嵌套重建）。
pub const SPAN_EXTRA_RANK: u8 = 99;

/// 全部节点定义（顺序 = 生成 Schema 的键序）。
pub static NODES: &[NodeDef] = &[
    NodeDef {
        pm_name: "doc",
        prima_type: None,
        group: "",
        content: Some("block*"),
        code: false,
        id_kind: None,
        attrs: &[
            AttrDef {
                name: "schema_version",
                spec: AttrSpec::Text("1.0"),
            },
            A_EXTRA,
        ],
        prima_fields: &["schema_version"],
    },
    NodeDef {
        pm_name: "section",
        prima_type: Some("section"),
        group: "block",
        content: Some("block*"),
        code: false,
        id_kind: Some(IdKind::Sec),
        attrs: &[A_ID, A_EXTRA],
        prima_fields: &["children"],
    },
    NodeDef {
        pm_name: "paragraph",
        prima_type: Some("paragraph"),
        group: "block",
        content: Some("inline*"),
        code: false,
        id_kind: Some(IdKind::Blk),
        attrs: &[A_ID, A_EXTRA],
        prima_fields: &["content"],
    },
    NodeDef {
        pm_name: "heading",
        prima_type: Some("heading"),
        group: "block",
        content: Some("inline*"),
        code: false,
        id_kind: Some(IdKind::Blk),
        attrs: &[
            A_ID,
            AttrDef {
                name: "level",
                spec: AttrSpec::Int(1),
            },
            A_EXTRA,
        ],
        prima_fields: &["level", "content"],
    },
    NodeDef {
        pm_name: "quote",
        prima_type: Some("quote"),
        group: "block",
        content: Some("block*"),
        code: false,
        id_kind: Some(IdKind::Blk),
        attrs: &[A_ID, A_EXTRA],
        prima_fields: &["children"],
    },
    NodeDef {
        pm_name: "list",
        prima_type: Some("list"),
        group: "block",
        content: Some("list_item*"),
        code: false,
        id_kind: Some(IdKind::Blk),
        attrs: &[
            A_ID,
            AttrDef {
                name: "style",
                spec: AttrSpec::Text("bullet"),
            },
            AttrDef {
                name: "start",
                spec: AttrSpec::OptInt,
            },
            A_EXTRA,
        ],
        prima_fields: &["style", "start", "items"],
    },
    NodeDef {
        pm_name: "list_item",
        prima_type: None,
        group: "",
        content: Some("block*"),
        code: false,
        id_kind: Some(IdKind::Li),
        attrs: &[
            A_ID,
            AttrDef {
                name: "checked",
                spec: AttrSpec::OptBool,
            },
            A_EXTRA,
        ],
        prima_fields: &["checked", "children"],
    },
    NodeDef {
        pm_name: "code_block",
        prima_type: Some("code_block"),
        group: "block",
        content: Some("text*"),
        code: true,
        id_kind: Some(IdKind::Blk),
        attrs: &[
            A_ID,
            AttrDef {
                name: "language",
                spec: AttrSpec::OptText,
            },
            A_EXTRA,
        ],
        prima_fields: &["language", "text"],
    },
    NodeDef {
        pm_name: "table",
        prima_type: Some("table"),
        group: "block",
        content: Some("table_row*"),
        code: false,
        id_kind: Some(IdKind::Blk),
        attrs: &[
            A_ID,
            AttrDef {
                name: "header_row",
                spec: AttrSpec::OptBool,
            },
            AttrDef {
                name: "columns",
                spec: AttrSpec::OptJson,
            },
            A_EXTRA,
        ],
        prima_fields: &["header_row", "columns", "rows"],
    },
    NodeDef {
        pm_name: "table_row",
        prima_type: None,
        group: "",
        content: Some("(table_cell | table_header)*"),
        code: false,
        id_kind: Some(IdKind::Row),
        attrs: &[A_ID, A_EXTRA],
        prima_fields: &["cells"],
    },
    NodeDef {
        pm_name: "table_cell",
        prima_type: None,
        group: "",
        content: Some("block*"),
        code: false,
        id_kind: Some(IdKind::Cel),
        attrs: &[
            A_ID,
            AttrDef {
                name: "column",
                spec: AttrSpec::Int(0),
            },
            AttrDef {
                name: "colSpan",
                spec: AttrSpec::Span,
            },
            AttrDef {
                name: "rowSpan",
                spec: AttrSpec::Span,
            },
            AttrDef {
                name: "role",
                spec: AttrSpec::OptText,
            },
            // PM-only：列宽（像素），保存时归并进 columns[].width（spec §6.5）
            AttrDef {
                name: "colwidth",
                spec: AttrSpec::OptJson,
            },
            A_EXTRA,
        ],
        prima_fields: &["column", "colSpan", "rowSpan", "role", "children"],
    },
    // prosemirror-tables 的表头单元格：映射到 Prima cell role="header"
    NodeDef {
        pm_name: "table_header",
        prima_type: None,
        group: "",
        content: Some("block*"),
        code: false,
        id_kind: Some(IdKind::Cel),
        attrs: &[
            A_ID,
            AttrDef {
                name: "column",
                spec: AttrSpec::Int(0),
            },
            AttrDef {
                name: "colSpan",
                spec: AttrSpec::Span,
            },
            AttrDef {
                name: "rowSpan",
                spec: AttrSpec::Span,
            },
            AttrDef {
                name: "role",
                spec: AttrSpec::OptText,
            },
            AttrDef {
                name: "colwidth",
                spec: AttrSpec::OptJson,
            },
            A_EXTRA,
        ],
        prima_fields: &["column", "colSpan", "rowSpan", "role", "children"],
    },
    NodeDef {
        pm_name: "figure",
        prima_type: Some("figure"),
        group: "block",
        content: None,
        code: false,
        id_kind: Some(IdKind::Blk),
        attrs: &[
            A_ID,
            A_ASSET,
            A_ALT,
            AttrDef {
                name: "caption",
                spec: AttrSpec::OptJson,
            },
            A_EXTRA,
        ],
        prima_fields: &["asset", "alt", "caption"],
    },
    NodeDef {
        pm_name: "image",
        prima_type: Some("image"),
        group: "block",
        content: None,
        code: false,
        id_kind: Some(IdKind::Blk),
        attrs: &[A_ID, A_ASSET, A_ALT, A_EXTRA],
        prima_fields: &["asset", "alt"],
    },
    NodeDef {
        pm_name: "horizontal_rule",
        prima_type: Some("horizontal_rule"),
        group: "block",
        content: None,
        code: false,
        id_kind: Some(IdKind::Blk),
        attrs: &[A_ID, A_EXTRA],
        prima_fields: &[],
    },
    // E4：显式手动分页节点（分页是可观察 hint；DOCX ↔ w:br type="page"）。
    NodeDef {
        pm_name: "page_break",
        prima_type: Some("page_break"),
        group: "block",
        content: None,
        code: false,
        id_kind: Some(IdKind::Blk),
        attrs: &[A_ID, A_EXTRA],
        prima_fields: &[],
    },
    NodeDef {
        pm_name: "math_block",
        prima_type: Some("math_block"),
        group: "block",
        content: None,
        code: false,
        id_kind: Some(IdKind::Blk),
        attrs: &[
            A_ID,
            AttrDef {
                name: "latex",
                spec: AttrSpec::Text(""),
            },
            A_EXTRA,
        ],
        prima_fields: &["latex"],
    },
    NodeDef {
        pm_name: "callout",
        prima_type: Some("callout"),
        group: "block",
        content: Some("block*"),
        code: false,
        id_kind: Some(IdKind::Blk),
        attrs: &[
            A_ID,
            AttrDef {
                name: "variant",
                spec: AttrSpec::Text("info"),
            },
            A_EXTRA,
        ],
        prima_fields: &["variant", "children"],
    },
    NodeDef {
        pm_name: "embed",
        prima_type: Some("embed"),
        group: "block",
        content: None,
        code: false,
        id_kind: Some(IdKind::Blk),
        attrs: &[A_ID, A_ASSET, A_EXTRA],
        prima_fields: &["asset"],
    },
    NodeDef {
        pm_name: "footnote",
        prima_type: Some("footnote"),
        group: "block",
        content: Some("block*"),
        code: false,
        id_kind: Some(IdKind::Blk),
        attrs: &[A_ID, A_EXTRA],
        prima_fields: &["children"],
    },
    NodeDef {
        pm_name: "unknown_block",
        prima_type: Some("unknown"),
        group: "block",
        content: Some("inline*"),
        code: false,
        id_kind: Some(IdKind::Blk),
        attrs: &[
            A_ID,
            A_ORIGIN,
            A_LOSS_CLASS,
            A_SUMMARY,
            A_PAYLOAD_REF,
            A_EXTRA,
        ],
        prima_fields: &["origin", "loss_class", "summary", "payload_ref", "content"],
    },
    NodeDef {
        pm_name: "text",
        prima_type: Some("text"),
        group: "inline",
        content: None,
        code: false,
        id_kind: None,
        attrs: &[],
        prima_fields: &["text"],
    },
    NodeDef {
        pm_name: "hard_break",
        prima_type: Some("hard_break"),
        group: "inline",
        content: None,
        code: false,
        id_kind: None,
        attrs: &[A_EXTRA],
        prima_fields: &[],
    },
    NodeDef {
        pm_name: "footnote_ref",
        prima_type: Some("footnote_ref"),
        group: "inline",
        content: None,
        code: false,
        // footnote_ref.id 是对 footnote 块的**引用**，不参与补发
        id_kind: None,
        attrs: &[A_ID, A_EXTRA],
        prima_fields: &[],
    },
    NodeDef {
        pm_name: "inline_math",
        prima_type: Some("inline_math"),
        group: "inline",
        content: None,
        code: false,
        id_kind: None,
        attrs: &[
            AttrDef {
                name: "latex",
                spec: AttrSpec::Text(""),
            },
            A_EXTRA,
        ],
        prima_fields: &["latex"],
    },
    NodeDef {
        pm_name: "inline_image",
        prima_type: Some("inline_image"),
        group: "inline",
        content: None,
        code: false,
        id_kind: None,
        attrs: &[A_ASSET, A_ALT, A_EXTRA],
        prima_fields: &["asset", "alt"],
    },
    NodeDef {
        pm_name: "mention",
        prima_type: Some("mention"),
        group: "inline",
        content: None,
        code: false,
        id_kind: None,
        attrs: &[
            AttrDef {
                name: "target",
                spec: AttrSpec::Text(""),
            },
            A_EXTRA,
        ],
        prima_fields: &["target"],
    },
    NodeDef {
        pm_name: "cite",
        prima_type: Some("cite"),
        group: "inline",
        content: None,
        code: false,
        id_kind: None,
        attrs: &[
            AttrDef {
                name: "key",
                spec: AttrSpec::Text(""),
            },
            A_EXTRA,
        ],
        prima_fields: &["key"],
    },
    NodeDef {
        pm_name: "unknown_span",
        prima_type: Some("unknown"),
        group: "inline",
        content: Some("inline*"),
        code: false,
        id_kind: None,
        attrs: &[A_ORIGIN, A_LOSS_CLASS, A_SUMMARY, A_PAYLOAD_REF, A_EXTRA],
        prima_fields: &["origin", "loss_class", "summary", "payload_ref", "content"],
    },
];

/// 全部标记定义（顺序 = 生成 Schema 的键序；rank 升序即嵌套从外到内）。
pub static MARKS: &[MarkDef] = &[
    MarkDef {
        pm_name: "link",
        prima_type: Some("link"),
        rank: 0,
        attrs: &[
            AttrDef {
                name: "url",
                spec: AttrSpec::Text(""),
            },
            AttrDef {
                name: "title",
                spec: AttrSpec::OptText,
            },
            A_EXTRA,
        ],
        prima_fields: &["url", "title", "content"],
    },
    MarkDef {
        pm_name: "strong",
        prima_type: Some("strong"),
        rank: 1,
        attrs: &[A_EXTRA],
        prima_fields: &["content"],
    },
    MarkDef {
        pm_name: "em",
        prima_type: Some("em"),
        rank: 2,
        attrs: &[A_EXTRA],
        prima_fields: &["content"],
    },
    MarkDef {
        pm_name: "underline",
        prima_type: Some("underline"),
        rank: 3,
        attrs: &[A_EXTRA],
        prima_fields: &["content"],
    },
    MarkDef {
        pm_name: "strike",
        prima_type: Some("strike"),
        rank: 4,
        attrs: &[A_EXTRA],
        prima_fields: &["content"],
    },
    MarkDef {
        pm_name: "code",
        prima_type: Some("code"),
        rank: CODE_RANK,
        attrs: &[A_EXTRA],
        prima_fields: &["text"],
    },
    MarkDef {
        pm_name: "span_extra",
        prima_type: None,
        rank: SPAN_EXTRA_RANK,
        attrs: &[AttrDef {
            name: "data",
            spec: AttrSpec::OptJson,
        }],
        prima_fields: &[],
    },
];

/// 按 PM 名查节点定义。
pub fn node_by_pm(pm_name: &str) -> Option<&'static NodeDef> {
    NODES.iter().find(|d| d.pm_name == pm_name)
}

/// 按 PM 名查标记定义。
pub fn mark_by_pm(pm_name: &str) -> Option<&'static MarkDef> {
    MARKS.iter().find(|d| d.pm_name == pm_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping_covers_all_known_types() {
        for t in azodoc_model::content::KNOWN_NODE_TYPES {
            // list_item 等在 Prima 中是不带 type 的结构体，按 PM 名对齐
            let covered = NODES
                .iter()
                .any(|d| d.prima_type == Some(*t) || d.pm_name == *t);
            assert!(covered, "块类型 `{t}` 没有节点映射");
        }
        for t in azodoc_model::content::KNOWN_SPAN_TYPES {
            let covered = MARKS.iter().any(|d| d.prima_type == Some(*t))
                || NODES.iter().any(|d| d.prima_type == Some(*t));
            assert!(covered, "span 类型 `{t}` 没有标记/原子映射");
        }
    }

    #[test]
    fn pm_names_unique() {
        let mut names: Vec<&str> = NODES.iter().map(|d| d.pm_name).collect();
        names.extend(MARKS.iter().map(|d| d.pm_name));
        let len = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), len, "PM 节点/标记名必须唯一");
    }

    #[test]
    fn attrs_are_well_formed() {
        let check = |attrs: &[AttrDef], fields: &[&str], synthetic: bool| {
            if attrs.is_empty() {
                return;
            }
            if let Some(pos) = attrs.iter().position(|a| a.name == "id") {
                assert_eq!(pos, 0, "id 存在时必须是第一个 attr");
            }
            if attrs.iter().any(|a| a.name == "extra") {
                assert_eq!(
                    attrs.last().unwrap().name,
                    "extra",
                    "extra 必须是最后一个 attr"
                );
            }
            for a in attrs {
                if a.name == "id" || a.name == "extra" {
                    continue;
                }
                // 合成定义（prima_type = None：doc / span_extra）的 attr 不对应 Prima 字段
                assert!(
                    fields.contains(&a.name) || synthetic,
                    "attr `{}` 不在 prima_fields 中",
                    a.name
                );
            }
        };
        for d in NODES {
            check(d.attrs, d.prima_fields, d.prima_type.is_none());
        }
        for d in MARKS {
            check(d.attrs, d.prima_fields, d.prima_type.is_none());
        }
    }
}
