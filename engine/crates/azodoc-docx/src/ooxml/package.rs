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

//! OOXML 包模型：zip 打开 → `[Content_Types].xml` → `_rels` 关系解析。
//!
//! 安全护栏（RFC B2 决策 10）：条目数上限、单部件/累计解压上限、加密条目拒绝、
//! 条目名消毒（zip-slip）。遍历一律按 ZIP 归档序，保证确定性。

use super::error::OoxmlError;
use super::xml::{parse_xml, El};
use std::io::{Cursor, Read};

pub const MAX_ENTRIES: usize = 10_000;
pub const MAX_PART_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_TOTAL_BYTES: u64 = 1024 * 1024 * 1024;

pub struct OoxmlPackage {
    archive: zip::ZipArchive<Cursor<Vec<u8>>>,
    content_types: ContentTypes,
    total_read: u64,
}

#[derive(Debug, Default)]
struct ContentTypes {
    defaults: Vec<(String, String)>,  // 扩展名（小写）→ content type
    overrides: Vec<(String, String)>, // /part/name → content type
}

impl ContentTypes {
    fn parse(doc: &El) -> Self {
        let mut ct = ContentTypes::default();
        for c in &doc.children {
            match c.local.as_str() {
                "Default" => {
                    if let (Some(ext), Some(ty)) = (c.attr("Extension"), c.attr("ContentType")) {
                        ct.defaults.push((ext.to_ascii_lowercase(), ty.to_string()));
                    }
                }
                "Override" => {
                    if let (Some(pn), Some(ty)) = (c.attr("PartName"), c.attr("ContentType")) {
                        ct.overrides.push((normalize_part_name(pn), ty.to_string()));
                    }
                }
                _ => {}
            }
        }
        ct
    }

    fn of(&self, part: &str) -> Option<&str> {
        let norm = normalize_part_name(part);
        for (pn, ty) in &self.overrides {
            if *pn == norm {
                return Some(ty);
            }
        }
        let ext = norm.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        for (e, ty) in &self.defaults {
            if *e == ext {
                return Some(ty);
            }
        }
        None
    }
}

#[derive(Debug, Clone)]
pub struct Relationship {
    pub id: String,
    pub rel_type: String,
    pub target: String,
    pub external: bool,
}

impl Relationship {
    /// 内部关系 → 以包根为基准的部件名（External 返回原 URL）。
    pub fn resolve(&self, source_part: &str) -> String {
        if self.external {
            return self.target.clone();
        }
        let base = match source_part.rfind('/') {
            Some(i) => &source_part[..i],
            None => "",
        };
        let mut segments: Vec<&str> = base.split('/').filter(|s| !s.is_empty()).collect();
        for seg in self.target.split('/') {
            match seg {
                "" | "." => {}
                ".." => {
                    segments.pop();
                }
                s => segments.push(s),
            }
        }
        let joined = segments.join("/");
        normalize_part_name(&joined)
    }
}

fn normalize_part_name(p: &str) -> String {
    let p = p.replace('\\', "/");
    let p = p.strip_prefix('/').unwrap_or(&p);
    p.to_string()
}

/// OPC 命名消毒：拒绝路径穿越 / 绝对路径 / 盘符（zip-slip）。
pub fn sanitize_part_name(name: &str) -> Result<String, OoxmlError> {
    let norm = normalize_part_name(name);
    if norm.is_empty() {
        return Err(OoxmlError::UnsafeName(name.to_string()));
    }
    for seg in norm.split('/') {
        // OPC 部件名不允许空段、`..` 段或 `:`（盘符形态）；`\` 已在规范化时转换
        if seg.is_empty() || seg == ".." || seg.contains(':') {
            return Err(OoxmlError::UnsafeName(name.to_string()));
        }
    }
    if norm.contains('\0') {
        return Err(OoxmlError::UnsafeName(name.to_string()));
    }
    Ok(norm)
}

impl OoxmlPackage {
    pub fn open(bytes: &[u8]) -> Result<Self, OoxmlError> {
        let cursor = Cursor::new(bytes.to_vec());
        let archive =
            zip::ZipArchive::new(cursor).map_err(|e| OoxmlError::NotZip(e.to_string()))?;
        if archive.len() > MAX_ENTRIES {
            return Err(OoxmlError::TooManyEntries { limit: MAX_ENTRIES });
        }
        let mut pkg = OoxmlPackage {
            archive,
            content_types: ContentTypes::default(),
            total_read: 0,
        };
        if pkg.has_part("[Content_Types].xml") {
            let doc = pkg.part_tree("[Content_Types].xml")?;
            pkg.content_types = ContentTypes::parse(&doc);
        }
        // 缺 [Content_Types].xml 时容忍：部件按名称规则识别（记录在 classify）。
        Ok(pkg)
    }

    pub fn content_type(&self, part: &str) -> Option<String> {
        self.content_types.of(part).map(String::from)
    }

    /// 归档序的条目名列表（确定性遍历的基准）。
    pub fn part_names(&mut self) -> Vec<String> {
        let mut names = Vec::new();
        for i in 0..self.archive.len() {
            if let Ok(f) = self.archive.by_index_raw(i) {
                names.push(f.name().to_string());
            }
        }
        names
    }

    pub fn has_part(&mut self, part: &str) -> bool {
        self.archive.by_name(part).is_ok() || self.archive.by_name(&format!("/{part}")).is_ok()
    }

    pub fn read_part(&mut self, part: &str) -> Result<Vec<u8>, OoxmlError> {
        let name = sanitize_part_name(part)?;
        let actual = if self.archive.by_name(&name).is_ok() {
            name.clone()
        } else {
            format!("/{name}")
        };
        let mut file = self
            .archive
            .by_name(&actual)
            .map_err(|_| OoxmlError::MissingPart(name.clone()))?;
        if file.encrypted() {
            return Err(OoxmlError::Encrypted { part: name.clone() });
        }
        if file.size() > MAX_PART_BYTES {
            return Err(OoxmlError::TooLarge {
                part: name.clone(),
                size: file.size(),
                limit: MAX_PART_BYTES,
            });
        }
        self.total_read += file.size();
        if self.total_read > MAX_TOTAL_BYTES {
            return Err(OoxmlError::TooLarge {
                part: name.clone(),
                size: self.total_read,
                limit: MAX_TOTAL_BYTES,
            });
        }
        let mut buf = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut buf)?;
        Ok(buf)
    }

    pub fn part_tree(&mut self, part: &str) -> Result<El, OoxmlError> {
        let bytes = self.read_part(part)?;
        parse_xml(part, &bytes)
    }

    /// 某部件的关系列表（读 `<dir>/_rels/<name>.rels`；不存在返回空）。
    pub fn rels(&mut self, part: &str) -> Result<Vec<Relationship>, OoxmlError> {
        let (dir, file) = match part.rfind('/') {
            Some(i) => (&part[..i], &part[i + 1..]),
            None => ("", part),
        };
        let rels_part = if dir.is_empty() {
            format!("_rels/{file}.rels")
        } else {
            format!("{dir}/_rels/{file}.rels")
        };
        if !self.has_part(&rels_part) {
            return Ok(Vec::new());
        }
        let doc = self.part_tree(&rels_part)?;
        let mut out = Vec::new();
        for c in &doc.children {
            if c.local != "Relationship" {
                continue;
            }
            let mode = c.attr("TargetMode").unwrap_or("");
            out.push(Relationship {
                id: c.attr("Id").unwrap_or("").to_string(),
                rel_type: c.attr("Type").unwrap_or("").to_string(),
                target: c.attr("Target").unwrap_or("").to_string(),
                external: mode.eq_ignore_ascii_case("External"),
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn make_pkg(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let cursor = Cursor::new(Vec::new());
        let mut w = ZipWriter::new(cursor);
        for (name, data) in entries {
            w.start_file(*name, SimpleFileOptions::default()).unwrap();
            std::io::Write::write_all(&mut w, data).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    const CT: &[u8] = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;

    #[test]
    fn opens_and_resolves_content_types() {
        let bytes = make_pkg(&[
            ("[Content_Types].xml", CT),
            ("word/document.xml", b"<w:document/>"),
        ]);
        let mut pkg = OoxmlPackage::open(&bytes).unwrap();
        assert_eq!(
            pkg.content_type("word/document.xml").as_deref(),
            Some(
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"
            )
        );
        assert_eq!(
            pkg.content_type("docProps/core.xml").as_deref(),
            Some("application/xml")
        );
        assert!(pkg.has_part("word/document.xml"));
        assert!(!pkg.has_part("word/nope.xml"));
    }

    #[test]
    fn rejects_non_zip() {
        let err = match OoxmlPackage::open(b"not a zip at all") {
            Err(e) => e,
            Ok(_) => panic!("expected open failure"),
        };
        assert!(matches!(err, OoxmlError::NotZip(_)));
        assert!(err.friendly().contains("错误："));
    }

    #[test]
    fn rels_resolution_relative_and_external() {
        let bytes = make_pkg(&[
            ("[Content_Types].xml", CT),
            (
                "word/_rels/document.xml.rels",
                br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://x/image" Target="media/image1.png"/><Relationship Id="rId2" Type="http://x/hyper" Target="https://a.example/" TargetMode="External"/><Relationship Id="rId3" Type="http://x/core" Target="../docProps/core.xml"/></Relationships>"#,
            ),
        ]);
        let mut pkg = OoxmlPackage::open(&bytes).unwrap();
        let rels = pkg.rels("word/document.xml").unwrap();
        assert_eq!(rels.len(), 3);
        assert_eq!(
            rels[0].resolve("word/document.xml"),
            "word/media/image1.png"
        );
        assert_eq!(rels[1].resolve("word/document.xml"), "https://a.example/");
        assert!(rels[1].external);
        assert_eq!(rels[2].resolve("word/document.xml"), "docProps/core.xml");
    }

    #[test]
    fn sanitize_rejects_traversal() {
        assert!(sanitize_part_name("word/media/../evil").is_err());
        assert!(sanitize_part_name("/abs/path.xml").is_ok()); // 前导斜杠规范化
        assert_eq!(sanitize_part_name("/word/a.xml").unwrap(), "word/a.xml");
        assert!(sanitize_part_name("C:/evil.xml").is_err());
    }

    #[test]
    fn part_names_preserve_archive_order() {
        let bytes = make_pkg(&[("b.xml", b"<b/>"), ("a.xml", b"<a/>")]);
        let mut pkg = OoxmlPackage::open(&bytes).unwrap();
        assert_eq!(pkg.part_names(), vec!["b.xml", "a.xml"]);
    }
}
