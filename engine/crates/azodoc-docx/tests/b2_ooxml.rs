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

//! B2 原生 OOXML 读取：映射表逐行断言（RFC B2 §4.2）+ schema 校验 + 友好失败。
//! 全部测试不依赖 Pandoc（验收 ⑥）。

use azodoc_convert::{ImportJob, LossClass};
use serde_json::Value;
use std::io::Cursor;
use std::io::Write as IoWrite;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

/// 组装最小 docx（ZIP）。
fn make_docx(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let cursor = Cursor::new(Vec::new());
    let mut w = ZipWriter::new(cursor);
    for (name, data) in entries {
        w.start_file(*name, SimpleFileOptions::default()).unwrap();
        w.write_all(data).unwrap();
    }
    w.finish().unwrap().into_inner()
}

fn xml(s: &str) -> Vec<u8> {
    s.as_bytes().to_vec()
}

/// 标准包壳：Content-Types + document.xml（可选附加部件）。
fn pkg(doc_body: &str, extra: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut entries: Vec<(String, Vec<u8>)> = vec![
        (
            "[Content_Types].xml".to_string(),
            xml(
                r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#,
            ),
        ),
        (
            "word/document.xml".to_string(),
            xml(&format!(
                r#"<w:document xmlns:w="urn:w" xmlns:r="urn:r" xmlns:a="urn:a" xmlns:wp="urn:wp" xmlns:m="urn:m" xmlns:mc="urn:mc" xmlns:v="urn:v">{doc_body}</w:document>"#
            )),
        ),
    ];
    for (n, b) in extra {
        entries.push((n.to_string(), b.clone()));
    }
    let refs: Vec<(&str, Vec<u8>)> = entries
        .iter()
        .map(|(n, b)| (n.as_str(), b.clone()))
        .collect();
    make_docx(&refs)
}

fn import(docx: &[u8]) -> azodoc_convert::ImportOutput {
    let mut job = ImportJob::new(None);
    azodoc_docx::ooxml::import_native(docx, &mut job).unwrap()
}

fn loss_features(out: &azodoc_convert::ImportOutput) -> Vec<(String, String)> {
    out.log
        .events()
        .iter()
        .map(|e| (e.feature.clone(), e.action.clone()))
        .collect()
}

fn has_feature(out: &azodoc_convert::ImportOutput, feature: &str) -> bool {
    loss_features(out).iter().any(|(f, _)| f == feature)
}

fn blocks(out: &azodoc_convert::ImportOutput) -> Vec<&Value> {
    let mut all = Vec::new();
    if let Some(arr) = out.content.get("content").and_then(Value::as_array) {
        collect_blocks(arr, &mut all);
    }
    all
}

fn collect_blocks<'a>(arr: &'a [Value], out: &mut Vec<&'a Value>) {
    for v in arr {
        out.push(v);
        match v.get("type").and_then(Value::as_str) {
            Some("section") => {
                if let Some(children) = v.get("children").and_then(Value::as_array) {
                    collect_blocks(children, out);
                }
            }
            Some("quote") | Some("callout") | Some("footnote") => {
                if let Some(children) = v.get("children").and_then(Value::as_array) {
                    collect_blocks(children, out);
                }
            }
            Some("list") => {
                if let Some(items) = v.get("items").and_then(Value::as_array) {
                    for item in items {
                        if let Some(children) = item.get("children").and_then(Value::as_array) {
                            collect_blocks(children, out);
                        }
                    }
                }
            }
            Some("table") => {
                if let Some(rows) = v.get("rows").and_then(Value::as_array) {
                    for row in rows {
                        if let Some(cells) = row.get("cells").and_then(Value::as_array) {
                            for cell in cells {
                                if let Some(children) =
                                    cell.get("children").and_then(Value::as_array)
                                {
                                    collect_blocks(children, out);
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn first_block_of_type<'a>(out: &'a azodoc_convert::ImportOutput, ty: &str) -> Option<&'a Value> {
    blocks(out)
        .into_iter()
        .find(|b| b.get("type").and_then(Value::as_str) == Some(ty))
}

// ---------------------------------------------------------------- 正文核心

#[test]
fn heading_and_paragraph_via_style_and_outline() {
    let styles = r#"<w:styles xmlns:w="urn:w"><w:style w:type="paragraph" w:styleId="H1"><w:name w:val="heading 1"/><w:pPr><w:outlineLvl w:val="0"/></w:pPr></w:style><w:style w:type="paragraph" w:styleId="MyLvl"><w:name w:val="Custom"/><w:pPr><w:outlineLvl w:val="2"/></w:pPr></w:style></w:styles>"#;
    let docx = pkg(
        r#"<w:body>
<w:p><w:pPr><w:pStyle w:val="H1"/></w:pPr><w:r><w:t>标题一</w:t></w:r></w:p>
<w:p><w:pPr><w:pStyle w:val="MyLvl"/></w:pPr><w:r><w:t>三级标题</w:t></w:r></w:p>
<w:p><w:r><w:t>正文段落。</w:t></w:r></w:p>
</w:body>"#,
        &[("word/styles.xml", xml(styles))],
    );
    let out = import(&docx);
    let h1 = first_block_of_type(&out, "heading").expect("heading 应存在");
    assert_eq!(h1["level"], 1);
    let headings: Vec<&Value> = blocks(&out)
        .into_iter()
        .filter(|b| b.get("type").and_then(Value::as_str) == Some("heading"))
        .collect();
    assert_eq!(headings.len(), 2);
    assert_eq!(headings[1]["level"], 3); // outlineLvl 2 → level 3
    assert_eq!(
        headings[0]["content"][0]["text"], "标题一",
        "标题文本应映射"
    );
    assert!(
        first_block_of_type(&out, "paragraph").is_some(),
        "正文段落应映射"
    );
}

#[test]
fn run_flags_map_to_spans() {
    let docx = pkg(
        r#"<w:body><w:p><w:r><w:rPr><w:b/></w:rPr><w:t>粗</w:t></w:r><w:r><w:t>与</w:t></w:r><w:r><w:rPr><w:i/><w:u/><w:strike/></w:rPr><w:t>斜下删</w:t></w:r></w:p></w:body>"#,
        &[],
    );
    let out = import(&docx);
    let p = first_block_of_type(&out, "paragraph").expect("段落应存在");
    let content = p["content"].as_array().unwrap();
    assert_eq!(content[0]["type"], "strong");
    assert_eq!(content[1]["type"], "text");
    let last = &content[2];
    assert_eq!(last["type"], "strike"); // 最外层标志位（strike 优先级最高）
    assert_eq!(last["content"][0]["text"], "斜下删");
}

#[test]
fn empty_paragraph_is_dropped() {
    let docx = pkg(
        r#"<w:body><w:p/><w:p><w:r><w:t>only</w:t></w:r></w:p><w:p/></w:body>"#,
        &[],
    );
    let out = import(&docx);
    let paras = blocks(&out);
    assert_eq!(paras.len(), 1, "空段应丢弃，仅剩 1 块");
}

#[test]
fn tab_break_and_page_break() {
    let docx = pkg(
        r#"<w:body>
<w:p><w:r><w:t>a</w:t></w:r><w:r><w:tab/><w:t>b</w:t></w:r></w:p>
<w:p><w:r><w:t>l1</w:t></w:r><w:r><w:br/><w:t>l2</w:t></w:r></w:p>
<w:p><w:r><w:br w:type="page"/></w:r></w:p>
<w:p><w:r><w:t>after</w:t></w:r></w:p>
</w:body>"#,
        &[],
    );
    let out = import(&docx);
    assert!(has_feature(&out, "docx_tab"));
    assert!(has_feature(&out, "docx_page_break"));
    let pb = first_block_of_type(&out, "page_break").expect("分页应映射为 page_break");
    assert_eq!(pb["type"], "page_break");
    let para_with_break = blocks(&out)
        .into_iter()
        .find(|b| {
            b.get("content")
                .and_then(Value::as_array)
                .map(|c| c.iter().any(|s| s["type"] == "hard_break"))
                .unwrap_or(false)
        })
        .expect("软换行应为 hard_break");
    assert!(para_with_break.get("content").is_some());
    // tab 退化为空格
    let p0 = blocks(&out).into_iter().next().unwrap();
    let joined: String = p0["content"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| s.get("text").and_then(Value::as_str))
        .collect();
    assert_eq!(joined, "a b");
}

#[test]
fn hyperlinks_external_and_internal() {
    let docx = pkg(
        r#"<w:body><w:p><w:hyperlink r:id="rId1"><w:r><w:t>外链</w:t></w:r></w:hyperlink><w:hyperlink w:anchor="sec2"><w:r><w:t>内锚</w:t></w:r></w:hyperlink></w:p></w:body>"#,
        &[(
            "word/_rels/document.xml.rels",
            xml(
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://x/hyperlink" Target="https://example.com/" TargetMode="External"/></Relationships>"#,
            ),
        )],
    );
    let out = import(&docx);
    assert!(has_feature(&out, "docx_internal_link"));
    let p = first_block_of_type(&out, "paragraph").unwrap();
    let content = p["content"].as_array().unwrap();
    assert_eq!(content[0]["type"], "link");
    assert_eq!(content[0]["url"], "https://example.com/");
    assert_eq!(content[1]["url"], "#sec2");
}

#[test]
fn bookmarks_are_reported() {
    let docx = pkg(
        r#"<w:body><w:p><w:bookmarkStart w:id="1" w:name="_Toc1"/><w:r><w:t>t</w:t></w:r><w:bookmarkEnd w:id="1"/></w:p></w:body>"#,
        &[],
    );
    let out = import(&docx);
    assert!(has_feature(&out, "docx_bookmark"));
}

#[test]
fn field_result_text_kept_instruction_reported() {
    let docx = pkg(
        r#"<w:body><w:p><w:fldSimple w:instr=" TOC \o &quot;1-3&quot; "><w:r><w:t>目录文本</w:t></w:r></w:fldSimple></w:p></w:body>"#,
        &[],
    );
    let out = import(&docx);
    assert!(has_feature(&out, "docx_field"));
    let p = first_block_of_type(&out, "paragraph").unwrap();
    assert_eq!(p["content"][0]["text"], "目录文本");
}

// ---------------------------------------------------------------- 列表

#[test]
fn bullet_and_ordered_lists_with_start() {
    let numbering = r#"<w:numbering xmlns:w="urn:w"><w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="5"/><w:numFmt w:val="decimal"/></w:lvl><w:lvl w:ilvl="1"><w:numFmt w:val="bullet"/></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num></w:numbering>"#;
    let docx = pkg(
        r#"<w:body>
<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>首项</w:t></w:r></w:p>
<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>次项</w:t></w:r></w:p>
<w:p><w:r><w:t>列表外段落</w:t></w:r></w:p>
</w:body>"#,
        &[("word/numbering.xml", xml(numbering))],
    );
    let out = import(&docx);
    let list = first_block_of_type(&out, "list").expect("列表应存在");
    assert_eq!(list["style"], "ordered");
    assert_eq!(list["start"], 5);
    assert_eq!(list["items"].as_array().unwrap().len(), 2);
    assert!(first_block_of_type(&out, "paragraph").is_some());
}

#[test]
fn nested_list_deepens_via_ilvl() {
    let docx = pkg(
        r#"<w:body>
<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="9"/></w:numPr></w:pPr><w:r><w:t>外层1</w:t></w:r></w:p>
<w:p><w:pPr><w:numPr><w:ilvl w:val="1"/><w:numId w:val="9"/></w:numPr></w:pPr><w:r><w:t>内层1</w:t></w:r></w:p>
<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="9"/></w:numPr></w:pPr><w:r><w:t>外层2</w:t></w:r></w:p>
</w:body>"#,
        &[],
    );
    let out = import(&docx);
    let list = first_block_of_type(&out, "list").expect("外层列表");
    let items = list["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    let inner = items[0]["children"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["type"] == "list")
        .expect("内层列表应嵌套在外层首项内");
    assert_eq!(inner["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        inner["items"][0]["children"][0]["content"][0]["text"],
        "内层1"
    );
}

// ---------------------------------------------------------------- 表格

#[test]
fn table_gridspan_vmerge_and_header() {
    let docx = pkg(
        r#"<w:body><w:tbl>
<w:tblPr><w:tblStyle w:val="TableGrid"/></w:tblPr>
<w:tblGrid><w:gridCol w:w="5760"/><w:gridCol w:w="2880"/><w:gridCol w:w="1440"/></w:tblGrid>
<w:tr><w:trPr><w:tblHeader/></w:trPr>
  <w:tc><w:tcPr><w:gridSpan w:val="2"/></w:tcPr><w:p><w:r><w:t>宽头</w:t></w:r></w:p></w:tc>
  <w:tc><w:p><w:r><w:t>第三列</w:t></w:r></w:p></w:tc>
</w:tr>
<w:tr>
  <w:tc><w:tcPr><w:vMerge w:val="restart"/></w:tcPr><w:p><w:r><w:t>合并起点</w:t></w:r></w:p></w:tc>
  <w:tc><w:p><w:r><w:t>B2</w:t></w:r></w:p></w:tc>
  <w:tc><w:p><w:r><w:t>C2</w:t></w:r></w:p></w:tc>
</w:tr>
<w:tr>
  <w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p/></w:tc>
  <w:tc><w:p><w:r><w:t>B3</w:t></w:r></w:p></w:tc>
  <w:tc><w:p><w:r><w:t>C3</w:t></w:r></w:p></w:tc>
</w:tr>
</w:tbl></w:body>"#,
        &[],
    );
    let out = import(&docx);
    let tbl = first_block_of_type(&out, "table").expect("表格应存在");
    assert_eq!(tbl["header_row"], true);
    assert_eq!(tbl["columns"].as_array().unwrap().len(), 3);
    assert_eq!(tbl["columns"][0]["name"], "宽头");
    // E2：gridCol@w:w → columns[].width（twips 原样，相对单位）
    assert_eq!(tbl["columns"][0]["width"], 5760);
    assert_eq!(tbl["columns"][1]["width"], 2880);
    assert_eq!(tbl["columns"][2]["width"], 1440);
    let rows = tbl["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    // 表头行：第一格 colSpan=2；tblHeader → 首行 cells role:"header"
    let head_cells = rows[0]["cells"].as_array().unwrap();
    assert_eq!(head_cells[0]["colSpan"], 2);
    assert_eq!(head_cells.len(), 2, "gridSpan 合并后首行 2 格");
    assert_eq!(head_cells[0]["role"], "header");
    assert_eq!(head_cells[1]["role"], "header");
    // 数据行 cell 无 role（body 默认不写字段）
    assert!(rows[1]["cells"][0].get("role").is_none());
    // vMerge：续格不产出，起点 rowSpan=2
    let row1_cells = rows[1]["cells"].as_array().unwrap();
    assert_eq!(row1_cells[0]["rowSpan"], 2);
    assert_eq!(
        row1_cells[0]["children"][0]["content"][0]["text"],
        "合并起点"
    );
    let row2_cells = rows[2]["cells"].as_array().unwrap();
    assert_eq!(row2_cells.len(), 2, "续格不产出（占位列 0 的格被合并）");
}

#[test]
fn nested_table_maps_recursively() {
    let docx = pkg(
        r#"<w:body><w:tbl>
<w:tblGrid><w:gridCol/></w:tblGrid>
<w:tr><w:tc><w:p><w:r><w:t>外壳</w:t></w:r></w:p>
<w:tbl><w:tblGrid><w:gridCol/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>内壳</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
</w:tc></w:tr></w:tbl></w:body>"#,
        &[],
    );
    let out = import(&docx);
    let inner_tables: Vec<&Value> = blocks(&out)
        .into_iter()
        .filter(|b| b.get("type").and_then(Value::as_str) == Some("table"))
        .collect();
    assert_eq!(inner_tables.len(), 2, "外层表 + cell 内嵌套表都被建模");
}

// ---------------------------------------------------------------- 图片 / 脚注 / OMML

#[test]
fn standalone_image_becomes_figure_with_asset() {
    let png: Vec<u8> = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let docx = pkg(
        r#"<w:body><w:p><w:r><w:drawing><wp:inline><wp:docPr id="1" name="pic" descr="描述文字"/><a:graphic><a:graphicData><pic:pic xmlns:pic="urn:pic"><a:blip r:embed="rId5"/></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p></w:body>"#,
        &[
            (
                "word/_rels/document.xml.rels",
                xml(
                    r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId5" Type="http://x/image" Target="media/image1.png"/></Relationships>"#,
                ),
            ),
            ("word/media/image1.png", png),
        ],
    );
    let out = import(&docx);
    let fig = first_block_of_type(&out, "figure").expect("独段图片应为 figure");
    assert_eq!(fig["alt"], "描述文字");
    assert!(
        fig["asset"].as_str().unwrap().starts_with("asset://"),
        "asset 引用形态"
    );
}

#[test]
fn footnote_reference_creates_def_with_label() {
    let footnotes = r#"<w:footnotes xmlns:w="urn:w"><w:footnote w:id="-1"><w:p/></w:footnote><w:footnote w:id="2"><w:p><w:r><w:t>脚注内容</w:t></w:r></w:p></w:footnote></w:footnotes>"#;
    let docx = pkg(
        r#"<w:body><w:p><w:r><w:t>正文</w:t></w:r><w:r><w:footnoteReference w:id="2"/></w:r></w:p></w:body>"#,
        &[("word/footnotes.xml", xml(footnotes))],
    );
    let out = import(&docx);
    let para = first_block_of_type(&out, "paragraph").unwrap();
    assert_eq!(para["content"][1]["type"], "footnote_ref");
    let def = first_block_of_type(&out, "footnote").expect("脚注定义应追加在文末");
    assert_eq!(def["label"], "1");
    assert_eq!(def["children"][0]["content"][0]["text"], "脚注内容");
    assert!(!has_feature(&out, "docx_footnote_missing"));
}

#[test]
fn endnote_maps_as_footnote_with_report() {
    let endnotes = r#"<w:endnotes xmlns:w="urn:w"><w:endnote w:id="2"><w:p><w:r><w:t>尾注内容</w:t></w:r></w:p></w:endnote></w:endnotes>"#;
    let docx = pkg(
        r#"<w:body><w:p><w:r><w:endnoteReference w:id="2"/></w:r></w:p></w:body>"#,
        &[("word/endnotes.xml", xml(endnotes))],
    );
    let out = import(&docx);
    assert!(has_feature(&out, "docx_endnotes"));
    assert!(first_block_of_type(&out, "footnote").is_some());
}

#[test]
fn omml_preserved_with_unknown_block() {
    let docx = pkg(
        r#"<w:body><w:p><m:oMathPara><m:oMath><m:r><m:t>E=mc²+1</m:t></m:r></m:oMath></m:oMathPara></w:p><w:p><w:r><w:t>公式后</w:t></w:r></w:p></w:body>"#,
        &[],
    );
    let mut job = ImportJob::new(None);
    let out = azodoc_docx::ooxml::import_native(&docx, &mut job).unwrap();
    assert!(has_feature(&out, "docx_omml"));
    let unknown = first_block_of_type(&out, "unknown").expect("OMML 应产出 unknown 块");
    assert_eq!(unknown["loss_class"], "preserved_raw");
    let payload = unknown["payload_ref"].as_str().unwrap();
    assert!(payload.starts_with("preserved/docx/"));
    let bytes = job
        .preserved
        .iter()
        .find(|(p, _)| p == payload)
        .map(|(_, b)| b)
        .expect("preserved 载荷应登记在 job");
    assert!(
        String::from_utf8_lossy(bytes).contains("<m:oMath>"),
        "OMML 原文应整件保留"
    );
}

// ---------------------------------------------------------------- 批注 → 标注

#[test]
fn comments_become_text_quote_annotations() {
    let comments_xml = r#"<w:comments xmlns:w="urn:w"><w:comment w:id="1" w:author="张三" w:date="2026-01-15T09:30:00Z"><w:p><w:r><w:t>这里写得好</w:t></w:r></w:p></w:comment></w:comments>"#;
    let docx = pkg(
        r#"<w:body><w:p><w:commentRangeStart w:id="1"/><w:r><w:t>被批注文本</w:t></w:r><w:commentRangeEnd w:id="1"/><w:r><w:commentReference w:id="1"/></w:r><w:r><w:t>后续文字</w:t></w:r></w:p></w:body>"#,
        &[("word/comments.xml", xml(comments_xml))],
    );
    let out = import(&docx);
    assert_eq!(out.annotations.len(), 1, "一条批注 → 一条标注");
    let ann = &out.annotations[0];
    assert_eq!(ann["type"], "docx_comment");
    assert_eq!(ann["target"]["kind"], "text_quote");
    assert_eq!(ann["target"]["selector"]["exact"], "被批注文本");
    assert_eq!(ann["value"]["text"], "这里写得好");
    assert_eq!(ann["author"]["type"], "human");
    assert_eq!(ann["author"]["id"], "张三");
    assert_eq!(ann["created_at"], "2026-01-15T09:30:00Z");
    // 标注目标块必须真实存在于 content
    let block_id = ann["target"]["block"].as_str().unwrap();
    assert!(
        blocks(&out)
            .iter()
            .any(|b| b["id"].as_str() == Some(block_id)),
        "text_quote 目标块存在"
    );
}

#[test]
fn annotations_validate_against_spec_schema() {
    let comments_xml = r#"<w:comments xmlns:w="urn:w"><w:comment w:id="1" w:author="李四" w:date="2026-02-01T00:00:00Z"><w:p><w:r><w:t>note</w:t></w:r></w:p></w:comment></w:comments>"#;
    let docx = pkg(
        r#"<w:body><w:p><w:commentRangeStart w:id="1"/><w:r><w:t>范围</w:t></w:r><w:commentRangeEnd w:id="1"/></w:p></w:body>"#,
        &[("word/comments.xml", xml(comments_xml))],
    );
    let out = import(&docx);
    let schema_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../spec/json-schema/annotations.schema.json"
    );
    let schema: Value =
        serde_json::from_str(&std::fs::read_to_string(schema_path).unwrap()).unwrap();
    let doc = serde_json::json!({
        "schema_version": "1.0",
        "annotations": out.annotations,
    });
    jsonschema::validate(&schema, &doc).unwrap_or_else(|e| panic!("标注不符合 Schema: {e}\n{doc}"));
}

// ---------------------------------------------------------------- 修订

#[test]
fn tracked_changes_final_view_with_preserved_original() {
    let docx = pkg(
        r#"<w:body><w:p><w:ins w:id="1" w:author="王五" w:date="2026-01-01T00:00:00Z"><w:r><w:t>新增</w:t></w:r></w:ins><w:del w:id="2" w:author="王五" w:date="2026-01-01T00:00:00Z"><w:r><w:delText>删除</w:delText></w:r></w:del></w:p></w:body>"#,
        &[],
    );
    let out = import(&docx);
    assert!(has_feature(&out, "docx_tracked_changes"));
    let event = out
        .log
        .events()
        .iter()
        .find(|e| e.feature == "docx_tracked_changes")
        .unwrap();
    assert_eq!(event.class, LossClass::Partial);
    assert_eq!(event.action, "degraded");
    assert!(event.detail.contains("王五"));
    assert!(event.detail.contains("preserved/docx/"));
    // 终稿视图：新增保留、删除剔除
    let p = first_block_of_type(&out, "paragraph").unwrap();
    let text: String = p["content"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| s.get("text").and_then(Value::as_str))
        .collect();
    assert_eq!(text, "新增");
}

// ---------------------------------------------------------------- 侧部件 / 元数据 / 主题

#[test]
fn headers_and_footers_preserved_whole() {
    let docx = pkg(
        r#"<w:body><w:p><w:r><w:t>b</w:t></w:r></w:p></w:body>"#,
        &[
            (
                "word/header1.xml",
                xml("<w:hdr xmlns:w=\"urn:w\"><w:p><w:r><w:t>页眉</w:t></w:r></w:p></w:hdr>"),
            ),
            (
                "word/footer1.xml",
                xml("<w:ftr xmlns:w=\"urn:w\"><w:p><w:r><w:t>页脚</w:t></w:r></w:p></w:ftr>"),
            ),
        ],
    );
    let out = import(&docx);
    assert!(has_feature(&out, "docx_headers"));
    assert!(has_feature(&out, "docx_footers"));
    for e in out.log.events() {
        if e.feature == "docx_headers" || e.feature == "docx_footers" {
            assert_eq!(e.class, LossClass::PreservedRaw);
            assert_eq!(e.action, "preserved");
            assert!(e.detail.starts_with("preserved/docx/"));
        }
    }
}

#[test]
fn ole_embedding_quarantined() {
    let docx = pkg(
        r#"<w:body><w:p><w:r><w:t>b</w:t></w:r></w:p></w:body>"#,
        &[("word/embeddings/oleObject1.bin", vec![1, 2, 3, 4])],
    );
    let out = import(&docx);
    let event = out
        .log
        .events()
        .iter()
        .find(|e| e.feature == "docx_ole_object")
        .expect("OLE 应被隔离报告");
    assert_eq!(event.action, "preserved_quarantined");
    assert!(event.detail.starts_with("preserved/docx/"));
}

#[test]
fn core_props_flow_to_metadata() {
    let core = r#"<cp:coreProperties xmlns:cp="urn:cp" xmlns:dc="urn:dc" xmlns:dcterms="urn:dcterms"><dc:title>测试文档</dc:title><dc:creator>赵六</dc:creator><dc:language>zh-CN</dc:language><dcterms:created>2026-03-01T10:00:00Z</dcterms:created><dcterms:modified>2026-03-02T10:00:00Z</dcterms:modified></cp:coreProperties>"#;
    let docx = pkg(
        r#"<w:body><w:p><w:r><w:t>x</w:t></w:r></w:p></w:body>"#,
        &[("docProps/core.xml", xml(core))],
    );
    let out = import(&docx);
    assert_eq!(out.title.as_deref(), Some("测试文档"));
    assert_eq!(out.language.as_deref(), Some("zh-CN"));
    assert_eq!(
        out.doc_extra.get("creator").and_then(Value::as_str),
        Some("赵六")
    );
    assert_eq!(
        out.doc_extra.get("created_at").and_then(Value::as_str),
        Some("2026-03-01T10:00:00Z")
    );
    assert_eq!(
        out.doc_extra.get("modified_at").and_then(Value::as_str),
        Some("2026-03-02T10:00:00Z")
    );
}

#[test]
fn theme_validates_against_spec_schema_and_blocks_carry_style() {
    let styles = r#"<w:styles xmlns:w="urn:w"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="宋体"/><w:sz w:val="24"/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:type="paragraph" w:styleId="a1"><w:name w:val="标题 甲"/></w:style><w:style w:type="paragraph" w:styleId="Quote"><w:name w:val="Quote"/><w:rPr><w:i/></w:rPr></w:style></w:styles>"#;
    let docx = pkg(
        r#"<w:body><w:p><w:pPr><w:pStyle w:val="a1"/></w:pPr><w:r><w:t>带样式的段</w:t></w:r></w:p></w:body>"#,
        &[("word/styles.xml", xml(styles))],
    );
    let out = import(&docx);
    let theme = out.theme.as_ref().expect("有样式引用时必须产出 theme");
    let schema_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../spec/json-schema/theme.schema.json"
    );
    let schema: Value =
        serde_json::from_str(&std::fs::read_to_string(schema_path).unwrap()).unwrap();
    jsonschema::validate(&schema, theme)
        .unwrap_or_else(|e| panic!("theme 不符合 Schema: {e}\n{theme}"));
    assert_eq!(theme["defaults"]["base_font"], "宋体");
    assert_eq!(theme["defaults"]["base_size"], "12pt");
    let styles_arr = theme["styles"].as_array().unwrap();
    let zh = styles_arr.iter().find(|s| s["title"] == "标题 甲").unwrap();
    assert!(
        zh["name"].as_str().unwrap().starts_with("style-"),
        "非 ASCII 样式名回退 style-N slug"
    );
    // 块挂 style 软引用
    let para = first_block_of_type(&out, "paragraph").unwrap();
    assert!(para.get("style").is_some(), "块应带 style 引用");
}

#[test]
fn sect_pr_flows_to_theme_page_defaults() {
    let docx = pkg(
        r#"<w:body><w:p><w:r><w:t>x</w:t></w:r></w:p><w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1800" w:bottom="1440" w:left="1800"/></w:sectPr></w:body>"#,
        &[],
    );
    let out = import(&docx);
    let theme = out.theme.as_ref().expect("sectPr 应产出 theme page 默认");
    assert_eq!(theme["defaults"]["page"]["size"], "595.3pt × 841.9pt");
    assert_eq!(theme["defaults"]["page"]["margin"], "72pt 90pt 72pt 90pt");
}

// ---------------------------------------------------------------- sdt / AlternateContent / 跳过元素

#[test]
fn sdt_unwraps_transparently() {
    let docx = pkg(
        r#"<w:body><w:p><w:sdt><w:sdtPr><w:alias w:val="ctl"/></w:sdtPr><w:sdtContent><w:r><w:t>控件内文本</w:t></w:r></w:sdtContent></w:sdt></w:p></w:body>"#,
        &[],
    );
    let out = import(&docx);
    assert!(has_feature(&out, "docx_sdt"));
    let p = first_block_of_type(&out, "paragraph").unwrap();
    assert_eq!(p["content"][0]["text"], "控件内文本");
}

#[test]
fn alternate_content_prefers_fallback() {
    let docx = pkg(
        r#"<w:body><w:p><mc:AlternateContent><mc:Choice Requires="urn:new"><w:r><w:t>新表达</w:t></w:r></mc:Choice><mc:Fallback><w:r><w:t>旧表达</w:t></w:r></mc:Fallback></mc:AlternateContent></w:p></w:body>"#,
        &[],
    );
    let out = import(&docx);
    assert!(has_feature(&out, "docx_alternate_content"));
    let p = first_block_of_type(&out, "paragraph").unwrap();
    assert_eq!(p["content"][0]["text"], "旧表达");
}

#[test]
fn vml_pict_without_image_is_preserved() {
    let docx = pkg(
        r#"<w:body><w:p><w:r><w:pict><v:shape style="width:10pt;height:10pt"><v:textbox><w:p><w:r><w:t>框内</w:t></w:r></w:p></v:textbox></v:shape></w:pict></w:r></w:p></w:body>"#,
        &[],
    );
    let out = import(&docx);
    assert!(has_feature(&out, "docx_vml"));
    let p = first_block_of_type(&out, "paragraph").unwrap();
    assert_eq!(p["content"][0]["type"], "unknown");
    assert_eq!(p["content"][0]["loss_class"], "preserved_raw");
}

// ---------------------------------------------------------------- 确定性与友好失败

#[test]
fn deterministic_output_for_same_bytes() {
    let styles = r#"<w:styles xmlns:w="urn:w"><w:style w:type="paragraph" w:styleId="H1"><w:name w:val="heading 1"/></w:style></w:styles>"#;
    let make = || {
        pkg(
            r#"<w:body><w:p><w:pPr><w:pStyle w:val="H1"/></w:pPr><w:r><w:t>t1</w:t></w:r></w:p><w:p><w:r><w:t>t2</w:t></w:r></w:p></w:body>"#,
            &[("word/styles.xml", xml(styles))],
        )
    };
    let out1 = import(&make());
    let out2 = import(&make());
    assert_eq!(
        serde_json::to_string(&out1.content).unwrap(),
        serde_json::to_string(&out2.content).unwrap(),
        "同字节输入 → 逐字节一致 content（IdGen 确定性）"
    );
    assert_eq!(out1.log.events().len(), out2.log.events().len());
}

#[test]
fn friendly_failure_truncated_zip() {
    let docx = pkg(
        r#"<w:body><w:p><w:r><w:t>x</w:t></w:r></w:p></w:body>"#,
        &[],
    );
    let truncated = &docx[..docx.len() / 2];
    let mut job = ImportJob::new(None);
    let err = match azodoc_docx::ooxml::import_native(truncated, &mut job) {
        Err(e) => e,
        Ok(_) => panic!("expected failure"),
    };
    let friendly = err.friendly();
    assert!(friendly.starts_with("错误："), "四段式首段：{friendly}");
    assert!(
        friendly.contains("建议") || friendly.contains("另存"),
        "含建议段"
    );
}

#[test]
fn friendly_failure_missing_document() {
    let docx = make_docx(&[
        ("[Content_Types].xml", xml("<Types/>")),
        ("word/styles.xml", xml("<w:styles xmlns:w=\"u\"/>")),
    ]);
    let mut job = ImportJob::new(None);
    let err = match azodoc_docx::ooxml::import_native(&docx, &mut job) {
        Err(e) => e,
        Ok(_) => panic!("expected failure"),
    };
    assert!(matches!(
        err,
        azodoc_docx::ooxml::OoxmlError::MissingPart(_)
    ));
}

#[test]
fn friendly_failure_bad_xml() {
    let docx = pkg("<w:body><w:p><w:r><w:t>未闭合</w:r></w:p>", &[]);
    let mut job = ImportJob::new(None);
    let err = match azodoc_docx::ooxml::import_native(&docx, &mut job) {
        Err(e) => e,
        Ok(_) => panic!("expected failure"),
    };
    assert!(matches!(err, azodoc_docx::ooxml::OoxmlError::BadXml { .. }));
    assert!(err.friendly().contains("错误："));
}

#[test]
fn no_loss_import_reports_zero_issues() {
    let docx = pkg(
        r#"<w:body><w:p><w:r><w:t>干净文档</w:t></w:r></w:p><w:p><w:r><w:t>第二段</w:t></w:r></w:p></w:body>"#,
        &[],
    );
    let out = import(&docx);
    assert!(!out.log.has_loss(), "干净文档应零损失");
    assert!(out.theme.is_none(), "无样式引用不产 theme");
    assert!(out.annotations.is_empty());
}
