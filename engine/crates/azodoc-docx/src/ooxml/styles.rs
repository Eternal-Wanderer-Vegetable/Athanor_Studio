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

//! styles.xml → 样式表与 presentation/theme.json（B2 决策 7）。
//!
//! slug 规则：theme schema 限定 `name` 匹配 `^[a-z][a-z0-9-]*$`；样式名 slug 化，
//! 非 ASCII/空名回退 `style-<序号>`（按 styles.xml 出现序，确定性）；原始名存
//! 额外字段 `title`。

use super::error::OoxmlError;
use super::package::OoxmlPackage;
use super::xml::El;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default)]
pub struct RunFlags {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
}

impl RunFlags {
    pub fn is_plain(self) -> bool {
        !self.bold && !self.italic && !self.underline && !self.strike
    }
}

#[derive(Debug, Clone)]
pub struct StyleInfo {
    pub style_id: String,
    pub name: String,
    /// paragraph | character | table | numbering
    pub kind: String,
    pub is_default: bool,
    pub outline_level: Option<i64>,
    pub based_on: Option<String>,
    pub flags: RunFlags,
    pub font: Option<String>,
    /// w:sz 半磅值
    pub size_half_points: Option<i64>,
    /// theme 用的 slug（解析期确定性分配）
    pub slug: String,
}

#[derive(Debug, Default)]
pub struct DocDefaults {
    pub font: Option<String>,
    pub size_half_points: Option<i64>,
}

#[derive(Debug, Default)]
pub struct PageProps {
    /// twips
    pub width: i64,
    pub height: i64,
    pub margin_top: i64,
    pub margin_right: i64,
    pub margin_bottom: i64,
    pub margin_left: i64,
}

#[derive(Debug, Default)]
pub struct Styles {
    /// styleId → info（BTreeMap：确定性遍历）
    pub by_id: BTreeMap<String, StyleInfo>,
    /// styles.xml 出现序的 styleId（确定性）
    pub order: Vec<String>,
    pub defaults: DocDefaults,
    /// 默认段落样式（w:default="1"），块 style 引用跳过它
    pub default_paragraph: Option<String>,
}

impl Styles {
    pub fn parse(pkg: &mut OoxmlPackage) -> Result<Styles, OoxmlError> {
        let mut styles = Styles::default();
        if !pkg.has_part("word/styles.xml") {
            return Ok(styles);
        }
        let doc = pkg.part_tree("word/styles.xml")?;
        if let Some(dd) = doc.child("w", "docDefaults") {
            styles.defaults = parse_doc_defaults(dd);
        }
        // slug 分配序 = styles.xml 出现序
        let mut slug_seq = 0usize;
        let mut used_slugs: BTreeMap<String, ()> = BTreeMap::new();
        for st in &doc.children {
            if !st.is("w", "style") {
                continue;
            }
            let style_id = st.attr("w:styleId").unwrap_or("").to_string();
            if style_id.is_empty() {
                continue;
            }
            let kind = st.attr("w:type").unwrap_or("paragraph").to_string();
            let is_default = st.attr("w:default").map(|v| v == "1").unwrap_or(false);
            let name = st
                .child("w", "name")
                .and_then(|n| n.attr("w:val"))
                .unwrap_or(&style_id)
                .to_string();
            let outline_level = st
                .child("w", "pPr")
                .and_then(|p| p.child("w", "outlineLvl"))
                .and_then(|o| o.attr("w:val"))
                .and_then(|v| v.parse::<i64>().ok());
            let based_on = st
                .child("w", "basedOn")
                .and_then(|b| b.attr("w:val"))
                .map(String::from);
            let rpr = st.child("w", "rPr");
            let flags = rpr.map(parse_run_flags).unwrap_or_default();
            let font = rpr
                .and_then(|r| r.child("w", "rFonts"))
                .and_then(|f| f.attr("w:ascii"))
                .map(String::from);
            let size_half_points = rpr
                .and_then(|r| r.child("w", "sz"))
                .and_then(|s| s.attr("w:val"))
                .and_then(|v| v.parse::<i64>().ok());
            let slug = assign_slug(&name, &mut used_slugs, &mut slug_seq);
            if is_default && kind == "paragraph" && styles.default_paragraph.is_none() {
                styles.default_paragraph = Some(style_id.clone());
            }
            styles.order.push(style_id.clone());
            styles.by_id.insert(
                style_id.clone(),
                StyleInfo {
                    style_id,
                    name,
                    kind,
                    is_default,
                    outline_level,
                    based_on,
                    flags,
                    font,
                    size_half_points,
                    slug,
                },
            );
        }
        Ok(styles)
    }

    pub fn get(&self, style_id: &str) -> Option<&StyleInfo> {
        self.by_id.get(style_id)
    }

    /// 标题级别：outlineLvl 优先（0–5 → 1–6），其次样式名 `heading N`，再次
    /// styleId `HeadingN`/`Heading N`（兼容中文 Word 的 styleId 命名）。
    pub fn heading_level(&self, style_id: &str) -> Option<i64> {
        let info = self.by_id.get(style_id)?;
        if let Some(l) = info.outline_level {
            if (0..=5).contains(&l) {
                return Some(l + 1);
            }
        }
        let lower = info.name.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("heading ") {
            if let Ok(n) = rest.trim().parse::<i64>() {
                if (1..=6).contains(&n) {
                    return Some(n);
                }
            }
        }
        let id_lower = info.style_id.to_ascii_lowercase();
        let digits = id_lower
            .strip_prefix("heading")
            .map(str::trim)
            .and_then(|s| s.parse::<i64>().ok());
        match digits {
            Some(n) if (1..=6).contains(&n) => Some(n),
            _ => None,
        }
    }

    /// 沿 basedOn 链解析运行 flags/字体/字号（带环保护）。
    pub fn resolve_effective(&self, style_id: &str) -> (RunFlags, Option<String>, Option<i64>) {
        let mut flags = RunFlags::default();
        let mut font = None;
        let mut size = None;
        let mut cur = Some(style_id.to_string());
        let mut hops = 0;
        while let Some(id) = cur {
            hops += 1;
            if hops > 16 {
                break;
            }
            let Some(info) = self.by_id.get(&id) else {
                break;
            };
            if info.flags.bold {
                flags.bold = true;
            }
            if info.flags.italic {
                flags.italic = true;
            }
            if info.flags.underline {
                flags.underline = true;
            }
            if info.flags.strike {
                flags.strike = true;
            }
            if font.is_none() {
                font = info.font.clone();
            }
            if size.is_none() {
                size = info.size_half_points;
            }
            cur = info.based_on.clone();
        }
        (flags, font, size)
    }
}

fn parse_doc_defaults(dd: &El) -> DocDefaults {
    let mut out = DocDefaults::default();
    if let Some(rpr) = dd
        .child("w", "rPrDefault")
        .and_then(|d| d.child("w", "rPr"))
    {
        out.font = rpr
            .child("w", "rFonts")
            .and_then(|f| f.attr("w:ascii"))
            .map(String::from);
        out.size_half_points = rpr
            .child("w", "sz")
            .and_then(|s| s.attr("w:val"))
            .and_then(|v| v.parse::<i64>().ok());
    }
    out
}

/// rPr 子元素 → 运行 flags（w:val="false"/"0"/"none" 视为关闭）。
pub fn parse_run_flags(rpr: &El) -> RunFlags {
    fn on(el: Option<&El>) -> bool {
        match el {
            None => false,
            Some(e) => match e.attr("w:val") {
                None => true,
                Some(v) => !matches!(v, "false" | "0" | "none"),
            },
        }
    }
    RunFlags {
        bold: on(rpr.child("w", "b")),
        italic: on(rpr.child("w", "i")),
        underline: on(rpr.child("w", "u")),
        strike: on(rpr.child("w", "strike")) || on(rpr.child("w", "dstrike")),
    }
}

fn assign_slug(name: &str, used: &mut BTreeMap<String, ()>, seq: &mut usize) -> String {
    let mut slug: String = name
        .to_ascii_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() {
                c
            } else {
                '-'
            }
        })
        .collect();
    while slug.starts_with('-') {
        slug.remove(0);
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    let valid = !slug.is_empty()
        && slug.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && !used.contains_key(&slug);
    if !valid {
        loop {
            *seq += 1;
            let candidate = format!("style-{seq}");
            if !used.contains_key(&candidate) {
                slug = candidate;
                break;
            }
        }
    }
    used.insert(slug.clone(), ());
    slug
}

fn half_points_to_pt(hp: i64) -> String {
    fmt_pt(hp as f64 / 2.0)
}

fn twips_to_pt(tw: i64) -> String {
    fmt_pt(tw as f64 / 20.0)
}

/// 点值格式化：整数不带小数，小数保留一位。
fn fmt_pt(pt: f64) -> String {
    if (pt - pt.round()).abs() < f64::EPSILON {
        format!("{}pt", pt.round() as i64)
    } else {
        format!("{pt:.1}pt")
    }
}

/// 组装 presentation/theme.json（RFC B2 决策 7）。
/// `used_style_ids` 为正文实际引用的 styleId（按首次引用序，确定性）。
pub fn build_theme(styles: &Styles, used_style_ids: &[String], page: Option<&PageProps>) -> Value {
    let mut theme = Map::new();
    theme.insert("schema_version".into(), json!("1.0"));

    let mut defaults = Map::new();
    if let Some(f) = &styles.defaults.font {
        defaults.insert("base_font".into(), json!(f));
    }
    if let Some(sz) = styles.defaults.size_half_points {
        defaults.insert("base_size".into(), json!(half_points_to_pt(sz)));
    }
    if let Some(p) = page {
        let mut page_obj = Map::new();
        page_obj.insert(
            "size".into(),
            json!(format!(
                "{} × {}",
                twips_to_pt(p.width),
                twips_to_pt(p.height)
            )),
        );
        let (t, r, b, l) = (
            twips_to_pt(p.margin_top),
            twips_to_pt(p.margin_right),
            twips_to_pt(p.margin_bottom),
            twips_to_pt(p.margin_left),
        );
        let margin = if t == r && r == b && b == l {
            t
        } else {
            format!("{t} {r} {b} {l}")
        };
        page_obj.insert("margin".into(), json!(margin));
        defaults.insert("page".into(), Value::Object(page_obj));
    }
    if !defaults.is_empty() {
        theme.insert("defaults".into(), Value::Object(defaults));
    }

    let mut style_entries = Vec::new();
    for id in used_style_ids {
        let Some(info) = styles.by_id.get(id) else {
            continue;
        };
        let (flags, font, size) = styles.resolve_effective(id);
        let mut css = Map::new();
        let mut deco = Vec::new();
        if flags.underline {
            deco.push("underline");
        }
        if flags.strike {
            deco.push("line-through");
        }
        if !deco.is_empty() {
            css.insert("text-decoration".into(), json!(deco.join(" ")));
        }
        if flags.bold {
            css.insert("font-weight".into(), json!("bold"));
        }
        if flags.italic {
            css.insert("font-style".into(), json!("italic"));
        }
        if let Some(f) = font {
            css.insert("font-family".into(), json!(f));
        }
        if let Some(sz) = size {
            css.insert("font-size".into(), json!(half_points_to_pt(sz)));
        }
        let applies = if info.kind == "character" {
            "text"
        } else {
            "paragraph"
        };
        let mut entry = Map::new();
        entry.insert("name".into(), json!(info.slug));
        entry.insert("title".into(), json!(info.name));
        entry.insert("applies_to".into(), json!([applies]));
        if !css.is_empty() {
            entry.insert("css_like".into(), Value::Object(css));
        }
        style_entries.push(Value::Object(entry));
    }
    if !style_entries.is_empty() {
        theme.insert("styles".into(), Value::Array(style_entries));
    }
    Value::Object(theme)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_rules_match_theme_schema() {
        let mut used = BTreeMap::new();
        let mut seq = 0usize;
        assert_eq!(assign_slug("Heading 1", &mut used, &mut seq), "heading-1");
        assert_eq!(assign_slug("标题 1", &mut used, &mut seq), "style-1");
        assert_eq!(assign_slug("Quote", &mut used, &mut seq), "quote");
        // 冲突回退序号
        assert_eq!(assign_slug("Quote", &mut used, &mut seq), "style-2");
        // 数字开头不合法 → 回退
        assert_eq!(assign_slug("9 Point", &mut used, &mut seq), "style-3");
    }

    #[test]
    fn pt_formatting() {
        assert_eq!(half_points_to_pt(22), "11pt");
        assert_eq!(half_points_to_pt(21), "10.5pt");
        assert_eq!(twips_to_pt(1440), "72pt");
        assert_eq!(twips_to_pt(1684), "84.2pt");
    }

    #[test]
    fn build_theme_emits_slug_titles_and_css() {
        let mut used = BTreeMap::new();
        let mut seq = 0usize;
        let mut styles = Styles::default();
        styles.defaults.font = Some("Calibri".into());
        styles.defaults.size_half_points = Some(22);
        styles.order.push("Heading1".into());
        styles.by_id.insert(
            "Heading1".into(),
            StyleInfo {
                style_id: "Heading1".into(),
                name: "heading 1".into(),
                kind: "paragraph".into(),
                is_default: false,
                outline_level: Some(0),
                based_on: None,
                flags: RunFlags {
                    bold: true,
                    ..Default::default()
                },
                font: None,
                size_half_points: Some(32),
                slug: super::assign_slug("heading 1", &mut used, &mut seq),
            },
        );
        let theme = build_theme(&styles, &["Heading1".to_string()], None);
        assert_eq!(theme["defaults"]["base_font"], "Calibri");
        assert_eq!(theme["defaults"]["base_size"], "11pt");
        let entry = &theme["styles"][0];
        assert_eq!(entry["name"], "heading-1");
        assert_eq!(entry["title"], "heading 1");
        assert_eq!(entry["css_like"]["font-weight"], "bold");
        assert_eq!(entry["css_like"]["font-size"], "16pt");
    }
}
