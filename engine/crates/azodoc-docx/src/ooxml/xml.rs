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

//! 轻量 XML 元素树（OOXML 映射用）。
//!
//! 以 quick-xml 事件流构建一次树，供部件级多遍访问。命名空间不做解析，
//! 按限定名的 `前缀:local` 匹配——OOXML 生态约定俗成的前缀（w/r/a/wp/pic/m/mc/v）
//! 即为匹配键；无前缀元素（如 `[Content_Types].xml` 的 `Types`）前缀为空串。
//! quick-xml 不展开自定义实体，XXE/实体炸弹攻击面天然极小。

use super::error::OoxmlError;

#[derive(Debug, Clone, Default)]
pub struct El {
    pub prefix: String,
    pub local: String,
    /// 限定名（`前缀:local` 或 `local`）→ 未转义值，按出现序保留
    pub attrs: Vec<(String, String)>,
    pub children: Vec<El>,
    /// 本元素直接文本（不含子元素内文本）
    pub text: String,
}

impl El {
    pub fn is(&self, prefix: &str, local: &str) -> bool {
        self.prefix == prefix && self.local == local
    }

    pub fn attr(&self, qname: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == qname)
            .map(|(_, v)| v.as_str())
    }

    /// 按属性 local 名匹配任意前缀（如 `embed` 命中 `r:embed`）。
    pub fn attr_local(&self, local: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k.rsplit(':').next() == Some(local))
            .map(|(_, v)| v.as_str())
    }

    pub fn child(&self, prefix: &str, local: &str) -> Option<&El> {
        self.children.iter().find(|c| c.is(prefix, local))
    }

    pub fn child_local(&self, local: &str) -> Option<&El> {
        self.children.iter().find(|c| c.local == local)
    }

    pub fn has_child(&self, prefix: &str, local: &str) -> bool {
        self.child(prefix, local).is_some()
    }

    pub fn descendants<'a>(&'a self, prefix: &str, local: &str, out: &mut Vec<&'a El>) {
        for c in &self.children {
            if c.is(prefix, local) {
                out.push(c);
            }
            c.descendants(prefix, local, out);
        }
    }

    pub fn find_descendant(&self, prefix: &str, local: &str) -> Option<&El> {
        let mut out = Vec::new();
        self.descendants(prefix, local, &mut out);
        out.into_iter().next()
    }

    /// 递归收集全部文本（含子元素），用于批注文本、标题名等。
    pub fn all_text(&self) -> String {
        let mut s = self.text.clone();
        for c in &self.children {
            s.push_str(&c.all_text());
        }
        s
    }
}

/// 实体引用解析（`amp` / `#38` / `#x26` 等；未知引用原样保留）。
fn resolve_entity(name: &str) -> String {
    match name {
        "amp" => "&".to_string(),
        "lt" => "<".to_string(),
        "gt" => ">".to_string(),
        "quot" => "\"".to_string(),
        "apos" => "'".to_string(),
        _ => {
            let code =
                if let Some(hex) = name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
                    u32::from_str_radix(hex, 16).ok()
                } else if let Some(dec) = name.strip_prefix('#') {
                    dec.parse::<u32>().ok()
                } else {
                    None
                };
            match code.and_then(char::from_u32) {
                Some(ch) => ch.to_string(),
                None => format!("&{name};"),
            }
        }
    }
}

fn split_qn(name: &str) -> (String, String) {
    match name.split_once(':') {
        Some((p, l)) => (p.to_string(), l.to_string()),
        None => (String::new(), name.to_string()),
    }
}

/// 解析一段 XML 部件为元素树。畸形 XML / 编码错误按 BadXml 报告。
pub fn parse_xml(part: &str, bytes: &[u8]) -> Result<El, OoxmlError> {
    use quick_xml::events::Event;
    let text = std::str::from_utf8(bytes)
        .map_err(|e| OoxmlError::BadXml {
            part: part.to_string(),
            msg: e.to_string(),
        })?
        .to_owned();
    let mut reader = quick_xml::Reader::from_str(&text);
    let mut stack: Vec<El> = vec![El::default()]; // [0] 为合成根
    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                let (prefix, local) = split_qn(e.name().as_ref());
                let mut el = El {
                    prefix,
                    local,
                    ..El::default()
                };
                for attr in e.attributes().flatten() {
                    let key = attr.key.as_ref().to_string();
                    let value = attr
                        .normalized_value(quick_xml::XmlVersion::Explicit1_0)
                        .map(|v| v.into_owned())
                        .unwrap_or_else(|_| attr.value.clone().into_owned());
                    el.attrs.push((key, value));
                }
                stack.push(el);
            }
            Ok(Event::Empty(ref e)) => {
                let (prefix, local) = split_qn(e.name().as_ref());
                let mut el = El {
                    prefix,
                    local,
                    ..El::default()
                };
                for attr in e.attributes().flatten() {
                    let key = attr.key.as_ref().to_string();
                    let value = attr
                        .normalized_value(quick_xml::XmlVersion::Explicit1_0)
                        .map(|v| v.into_owned())
                        .unwrap_or_else(|_| attr.value.clone().into_owned());
                    el.attrs.push((key, value));
                }
                stack.last_mut().expect("根常在").children.push(el);
            }
            Ok(Event::End(_)) => {
                match stack.pop() {
                    Some(el) => {
                        if stack.is_empty() {
                            // 超出合成根：畸形，恢复并继续
                            stack.push(El::default());
                        } else {
                            stack.last_mut().expect("根常在").children.push(el);
                        }
                    }
                    None => break,
                }
            }
            Ok(Event::Text(ref t)) => {
                let txt = t
                    .xml_content(quick_xml::XmlVersion::Explicit1_0)
                    .into_owned();
                stack.last_mut().expect("根常在").text.push_str(&txt);
            }
            Ok(Event::CData(ref t)) => {
                let txt = t.as_ref().to_string();
                stack.last_mut().expect("根常在").text.push_str(&txt);
            }
            Ok(Event::GeneralRef(ref r)) => {
                // 0.42 把实体引用拆为独立事件；预定义/数字引用就地展开，
                // 未知实体原样保留（保守，不展开自定义实体）
                let resolved = resolve_entity(r.as_ref());
                stack.last_mut().expect("根常在").text.push_str(&resolved);
            }
            Ok(Event::Eof) => break,
            Ok(_) => {} // Decl/Comment/PI/DocType
            Err(e) => {
                return Err(OoxmlError::BadXml {
                    part: part.to_string(),
                    msg: e.to_string(),
                })
            }
        }
    }
    let root = stack.into_iter().next().expect("根常在");
    match root.children.into_iter().next() {
        Some(doc) => Ok(doc),
        None => Err(OoxmlError::BadXml {
            part: part.to_string(),
            msg: "无文档根元素".to_string(),
        }),
    }
}

/// 元素树 → XML 字节（preserved OMML/VML 片段留档用；属性值做最小转义）。
pub fn el_to_xml(el: &El) -> String {
    let mut out = String::new();
    write_el(el, &mut out);
    out
}

fn write_el(el: &El, out: &mut String) {
    let qname = if el.prefix.is_empty() {
        el.local.clone()
    } else {
        format!("{}:{}", el.prefix, el.local)
    };
    out.push('<');
    out.push_str(&qname);
    for (k, v) in &el.attrs {
        out.push(' ');
        out.push_str(k);
        out.push_str("=\"");
        for ch in v.chars() {
            match ch {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '"' => out.push_str("&quot;"),
                _ => out.push(ch),
            }
        }
        out.push('"');
    }
    if el.children.is_empty() && el.text.is_empty() {
        out.push_str("/>");
        return;
    }
    out.push('>');
    for ch in el.text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
    for c in &el.children {
        write_el(c, out);
    }
    out.push_str("</");
    out.push_str(&qname);
    out.push('>');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_namespaces_and_attrs() {
        let doc = parse_xml(
            "t.xml",
            br#"<w:document xmlns:w="urn:x"><w:body><w:p w:numId="3"><w:r><w:t xml:space="preserve">a &amp; b</w:t></w:r></w:p></w:body></w:document>"#,
        )
        .unwrap();
        assert!(doc.is("w", "document"));
        let p = doc.child("w", "body").unwrap().child("w", "p").unwrap();
        assert_eq!(p.attr("w:numId"), Some("3"));
        let t = p.child("w", "r").unwrap().child("w", "t").unwrap();
        assert_eq!(t.text, "a & b");
        assert_eq!(t.attr("xml:space"), Some("preserve"));
    }

    #[test]
    fn parses_unprefixed_content_types_shape() {
        let doc = parse_xml(
            "[Content_Types].xml",
            br#"<Types xmlns="http://x"><Default Extension="xml" ContentType="a/b"/><Override PartName="/w.xml" ContentType="c/d"/></Types>"#,
        )
        .unwrap();
        assert_eq!(doc.prefix, "");
        assert_eq!(doc.local, "Types");
        let d = doc.child("", "Default").unwrap();
        assert_eq!(d.attr("ContentType"), Some("a/b"));
    }

    #[test]
    fn empty_elements_and_descendants() {
        let doc = parse_xml("t.xml", br#"<a><b><c v="1"/><c v="2"/></b><c v="3"/></a>"#).unwrap();
        let mut cs = Vec::new();
        doc.descendants("", "c", &mut cs);
        assert_eq!(cs.len(), 3);
        assert_eq!(cs[0].attr("v"), Some("1"));
        assert!(doc
            .find_descendant("", "b")
            .unwrap()
            .child("", "c")
            .is_some());
    }

    #[test]
    fn serializes_roundtrip_minimal() {
        let doc = parse_xml("t.xml", br#"<m:oMath><m:r><m:t>x+1</m:t></m:r></m:oMath>"#).unwrap();
        let xml = el_to_xml(&doc);
        assert!(xml.contains("<m:oMath>"));
        assert!(xml.contains("x+1"));
        assert!(xml.ends_with("</m:oMath>"));
    }

    #[test]
    fn bad_xml_is_reported() {
        let err = parse_xml("t.xml", b"<a><b></a>").unwrap_err();
        assert!(matches!(err, OoxmlError::BadXml { .. }));
    }
}
