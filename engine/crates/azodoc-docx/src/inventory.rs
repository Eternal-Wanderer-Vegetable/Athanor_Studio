// This file is part of Athanor, the Azodoc document engine.
// Copyright (C) 2026 Athanor Studio
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

//! DOCX 清点：直接把 docx 当 ZIP 打开，盘点 Pandoc AST 里看不到的部件
//! （页眉/页脚/批注/核心属性标题）。丢失部件必须报告（R6），据此发损失事件。

use serde_json::Value;
use std::io::{Read, Seek};

#[derive(Debug, Default, Clone)]
pub struct Inventory {
    /// docProps/core.xml 的 dc:title
    pub title: Option<String>,
    pub has_headers: bool,
    pub has_footers: bool,
    pub has_comments: bool,
    /// word/media/ 下的媒体文件数
    pub media_count: usize,
}

pub fn inventory(docx_bytes: &[u8]) -> Result<Inventory, String> {
    let mut inv = Inventory::default();
    let cursor = std::io::Cursor::new(docx_bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| format!("docx 不是有效 ZIP: {e}"))?;

    for i in 0..archive.len() {
        let name = archive
            .by_index_raw(i)
            .map_err(|e| format!("docx 条目读取失败: {e}"))?
            .name()
            .to_string();
        let lower = name.to_ascii_lowercase();
        if lower.starts_with("word/header") {
            inv.has_headers = true;
        } else if lower.starts_with("word/footer") {
            inv.has_footers = true;
        } else if lower.starts_with("word/comments") {
            inv.has_comments = true;
        } else if lower.starts_with("word/media/") {
            inv.media_count += 1;
        } else if lower == "docprops/core.xml" {
            inv.title = read_core_title(&name, &mut archive);
        }
    }
    Ok(inv)
}

/// 从 docProps/core.xml 提取 dc:title（小文件，朴素解析即可，不引 XML 依赖）。
fn read_core_title<R: Read + Seek>(name: &str, archive: &mut zip::ZipArchive<R>) -> Option<String> {
    let mut file = archive.by_name(name).ok()?;
    let mut xml = String::new();
    file.read_to_string(&mut xml).ok()?;
    let start = xml.find("<dc:title")?;
    let after_open = &xml[start..];
    let tag_end = after_open.find('>')?;
    let rest = &after_open[tag_end + 1..];
    let close = rest.find("</dc:title>")?;
    let inner = rest[..close].trim();
    if inner.is_empty() {
        return None;
    }
    Some(
        inner
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">"),
    )
}

/// 依据清点结果生成损失事件（loss 规范：Pandoc 会静默丢弃这些部件）。
pub fn loss_issues(inv: &Inventory) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    if inv.has_headers {
        out.push((
            "docx_headers".to_string(),
            "页眉".to_string(),
            "页眉在 Pandoc AST 中无对应表达，已丢弃".to_string(),
        ));
    }
    if inv.has_footers {
        out.push((
            "docx_footers".to_string(),
            "页脚".to_string(),
            "页脚在 Pandoc AST 中无对应表达，已丢弃".to_string(),
        ));
    }
    if inv.has_comments {
        out.push((
            "docx_comments".to_string(),
            "批注".to_string(),
            "批注在 Pandoc AST 中无对应表达，已丢弃".to_string(),
        ));
    }
    out
}

/// 供测试与报告使用的结构化摘要。
pub fn inventory_summary(inv: &Inventory) -> Value {
    serde_json::json!({
        "title": inv.title,
        "headers": inv.has_headers,
        "footers": inv.has_footers,
        "comments": inv.has_comments,
        "media": inv.media_count,
    })
}
