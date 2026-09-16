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

//! numbering.xml → 列表映射数据源（numId/ilvl → bullet|ordered + start）。
//!
//! v1 仅建模 numFmt 与 start；lvlOverride/多级重试等边角由调用方聚合报告
//! （`docx_numbering_edge`）。

use super::error::OoxmlError;
use super::package::OoxmlPackage;
use super::xml::El;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct NumLevel {
    /// bullet | decimal | lowerLetter | ... （非 bullet 一律按 ordered 处理）
    pub fmt: String,
    pub start: i64,
}

#[derive(Debug, Default)]
pub struct Numbering {
    /// (numId, ilvl) → 级别信息
    levels: BTreeMap<(String, i64), NumLevel>,
}

impl Numbering {
    pub fn parse(pkg: &mut OoxmlPackage) -> Result<Numbering, OoxmlError> {
        let mut out = Numbering::default();
        if !pkg.has_part("word/numbering.xml") {
            return Ok(out);
        }
        let doc = pkg.part_tree("word/numbering.xml")?;

        // abstractNumId → (ilvl → NumLevel)
        let mut abstracts: BTreeMap<String, BTreeMap<i64, NumLevel>> = BTreeMap::new();
        // numId → abstractNumId
        let mut num_to_abstract: BTreeMap<String, String> = BTreeMap::new();

        for c in &doc.children {
            match c.local.as_str() {
                "abstractNum" => {
                    let Some(abs_id) = c.attr("w:abstractNumId") else {
                        continue;
                    };
                    let mut lvls = BTreeMap::new();
                    for lvl in &c.children {
                        if !lvl.is("w", "lvl") {
                            continue;
                        }
                        let Some(ilvl) = lvl.attr("w:ilvl").and_then(|v| v.parse::<i64>().ok())
                        else {
                            continue;
                        };
                        let fmt = lvl
                            .child("w", "numFmt")
                            .and_then(|f| f.attr("w:val"))
                            .unwrap_or("bullet")
                            .to_string();
                        let start = lvl
                            .child("w", "start")
                            .and_then(|s| s.attr("w:val"))
                            .and_then(|v| v.parse::<i64>().ok())
                            .unwrap_or(1);
                        lvls.insert(ilvl, NumLevel { fmt, start });
                    }
                    abstracts.insert(abs_id.to_string(), lvls);
                }
                "num" => {
                    let Some(num_id) = c.attr("w:numId") else {
                        continue;
                    };
                    if let Some(abs) = c.child("w", "abstractNumId").and_then(|a| a.attr("w:val")) {
                        num_to_abstract.insert(num_id.to_string(), abs.to_string());
                    }
                }
                _ => {}
            }
        }

        for (num_id, abs_id) in &num_to_abstract {
            if let Some(lvls) = abstracts.get(abs_id) {
                for (ilvl, info) in lvls {
                    out.levels.insert((num_id.clone(), *ilvl), info.clone());
                }
            }
        }
        Ok(out)
    }

    pub fn level(&self, num_id: &str, ilvl: i64) -> Option<&NumLevel> {
        self.levels.get(&(num_id.to_string(), ilvl))
    }

    /// 是否有序列表；编号信息缺失时按无序处理并让调用方报告边角。
    pub fn is_ordered(&self, num_id: &str, ilvl: i64) -> bool {
        self.level(num_id, ilvl)
            .map(|l| l.fmt != "bullet" && l.fmt != "none")
            .unwrap_or(false)
    }

    pub fn start(&self, num_id: &str, ilvl: i64) -> i64 {
        self.level(num_id, ilvl).map(|l| l.start).unwrap_or(1)
    }
}

/// 从 `w:pPr/w:numPr` 提取 (numId, ilvl)。
pub fn num_pr_of(ppr: &El) -> Option<(String, i64)> {
    let num_pr = ppr.child("w", "numPr")?;
    let num_id = num_pr
        .child("w", "numId")
        .and_then(|n| n.attr("w:val"))
        .map(String::from)?;
    let ilvl = num_pr
        .child("w", "ilvl")
        .and_then(|i| i.attr("w:val"))
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0);
    Some((num_id, ilvl))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ooxml::package::OoxmlPackage;
    use crate::ooxml::xml::parse_xml;
    use std::io::Cursor;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn pkg_with_numbering(xml: &[u8]) -> Vec<u8> {
        let cursor = Cursor::new(Vec::new());
        let mut w = ZipWriter::new(cursor);
        w.start_file("word/numbering.xml", SimpleFileOptions::default())
            .unwrap();
        std::io::Write::write_all(&mut w, xml).unwrap();
        w.finish().unwrap().into_inner()
    }

    #[test]
    fn num_pr_extraction() {
        let ppr = parse_xml(
            "p.xml",
            br#"<w:pPr xmlns:w="urn"><w:numPr><w:ilvl w:val="1"/><w:numId w:val="7"/></w:numPr></w:pPr>"#,
        )
        .unwrap();
        assert_eq!(num_pr_of(&ppr), Some(("7".to_string(), 1)));
    }

    #[test]
    fn parses_num_fmt_and_start_via_package() {
        let bytes = pkg_with_numbering(
            br#"<w:numbering xmlns:w="urn"><w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="3"/><w:numFmt w:val="decimal"/></w:lvl><w:lvl w:ilvl="1"><w:numFmt w:val="bullet"/></w:lvl></w:abstractNum><w:num w:numId="7"><w:abstractNumId w:val="0"/></w:num></w:numbering>"#,
        );
        let mut pkg = OoxmlPackage::open(&bytes).unwrap();
        let n = Numbering::parse(&mut pkg).unwrap();
        assert!(n.is_ordered("7", 0));
        assert_eq!(n.start("7", 0), 3);
        assert!(!n.is_ordered("7", 1));
        // 未知 numId 按无序处理
        assert!(!n.is_ordered("9", 0));
    }
}
