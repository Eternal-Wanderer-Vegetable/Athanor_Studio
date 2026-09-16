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

//! word/comments.xml → 批注源数据（正文锚定见 document.rs；B2 决策 5）。

use super::error::OoxmlError;
use super::package::OoxmlPackage;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct CommentInfo {
    pub id: String,
    pub author: String,
    pub date: Option<String>,
    pub text: String,
}

#[derive(Debug, Default)]
pub struct Comments {
    pub by_id: BTreeMap<String, CommentInfo>,
}

impl Comments {
    pub fn parse(pkg: &mut OoxmlPackage) -> Result<Comments, OoxmlError> {
        let mut out = Comments::default();
        if !pkg.has_part("word/comments.xml") {
            return Ok(out);
        }
        let doc = pkg.part_tree("word/comments.xml")?;
        for c in &doc.children {
            if !c.is("w", "comment") {
                continue;
            }
            let Some(id) = c.attr("w:id") else { continue };
            // 多段批注文本按 \n 连接
            let mut paras = Vec::new();
            for p in &c.children {
                if p.is("w", "p") {
                    paras.push(p.all_text());
                }
            }
            let text = if paras.is_empty() {
                c.all_text()
            } else {
                paras.join("\n")
            };
            out.by_id.insert(
                id.to_string(),
                CommentInfo {
                    id: id.to_string(),
                    author: c.attr("w:author").unwrap_or("unknown").to_string(),
                    date: c.attr("w:date").map(String::from),
                    text,
                },
            );
        }
        Ok(out)
    }

    pub fn get(&self, id: &str) -> Option<&CommentInfo> {
        self.by_id.get(id)
    }
}
