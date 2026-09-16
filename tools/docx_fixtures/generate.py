#!/usr/bin/env python3
# This file is part of Athanor, the Azodoc document engine.
# Copyright (C) 2026 The Athanor Studio Developers
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU Affero General Public License as published
# by the Free Software Foundation, version 3 of the License only.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
# GNU Affero General Public License for more details.
#
# You should have received a copy of the GNU Affero General Public License
# along with this program. If not, see <https://www.gnu.org/licenses/>.

"""Deterministically build corpus/docx/{basic,lossy}.docx (B2 test corpus).

Fixed ZipInfo timestamps + fixed entry order -> byte-stable .docx fixtures.
Regenerate: python tools/docx_fixtures/generate.py
"""

import os
import zipfile

ROOT = os.path.join(os.path.dirname(__file__), "..", "..")
OUT = os.path.join(ROOT, "corpus", "docx")

CT = (
    '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
    '<Default Extension="xml" ContentType="application/xml"/>'
    '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
    '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
    '<Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>'
    "</Types>"
)

RELS_ROOT = (
    '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
    '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>'
    '<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/>'
    "</Relationships>"
)

CORE = (
    '<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" '
    'xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/">'
    "<dc:title>{title}</dc:title><dc:creator>Athanor</dc:creator>"
    "<dc:language>zh-CN</dc:language>"
    "<dcterms:created>2026-01-01T00:00:00Z</dcterms:created>"
    "<dcterms:modified>2026-01-02T00:00:00Z</dcterms:modified>"
    "</cp:coreProperties>"
)

W = 'xmlns:w="urn:w" xmlns:r="urn:r"'


def build_basic():
    doc = f"""<w:document {W}><w:body>"""
    doc += '<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>炼金术导论</w:t></w:r></w:p>'
    doc += (
        "<w:p><w:r><w:t>普通文本与</w:t></w:r>"
        "<w:r><w:rPr><w:b/></w:rPr><w:t>粗体</w:t></w:r>"
        "<w:r><w:rPr><w:i/></w:rPr><w:t>斜体</w:t></w:r>"
        "<w:r><w:t>混合段落。</w:t></w:r></w:p>"
    )
    doc += '<w:p><w:r><w:t>参见</w:t></w:r><w:hyperlink r:id="rIdH1"><w:r><w:t>Athanor 主页</w:t></w:r></w:hyperlink><w:r><w:t>。</w:t></w:r><w:r><w:footnoteReference w:id="2"/></w:r></w:p>'
    doc += (
        '<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>项目甲</w:t></w:r></w:p>'
        '<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>项目乙</w:t></w:r></w:p>'
    )
    doc += (
        '<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="2"/></w:numPr></w:pPr><w:r><w:t>第一步</w:t></w:r></w:p>'
        '<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="2"/></w:numPr></w:pPr><w:r><w:t>第二步</w:t></w:r></w:p>'
    )
    doc += (
        "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol/><w:gridCol/><w:gridCol/></w:tblGrid>"
        '<w:tr><w:trPr><w:tblHeader/></w:trPr>'
        '<w:tc><w:p><w:r><w:t>元素</w:t></w:r></w:p></w:tc>'
        '<w:tc><w:p><w:r><w:t>符号</w:t></w:r></w:p></w:tc>'
        '<w:tc><w:p><w:r><w:t>性质</w:t></w:r></w:p></w:tc></w:tr>'
        "<w:tr>"
        '<w:tc><w:p><w:r><w:t>金</w:t></w:r></w:p></w:tc>'
        '<w:tc><w:p><w:r><w:t>Au</w:t></w:r></w:p></w:tc>'
        '<w:tc><w:p><w:r><w:t>贵重金属</w:t></w:r></w:p></w:tc></w:tr>'
        "<w:tr>"
        '<w:tc><w:p><w:r><w:t>银</w:t></w:r></w:p></w:tc>'
        '<w:tc><w:p><w:r><w:t>Ag</w:t></w:r></w:p></w:tc>'
        '<w:tc><w:p><w:r><w:t>贵重金属</w:t></w:r></w:p></w:tc></w:tr>'
        "</w:tbl>"
    )
    doc += '<w:p><w:pPr><w:pStyle w:val="Heading2"/></w:pPr><w:r><w:t>第二章</w:t></w:r></w:p>'
    doc += '<w:p><w:r><w:t>结尾段落。</w:t></w:r></w:p>'
    doc += '<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1800" w:bottom="1440" w:left="1800"/></w:sectPr>'
    doc += "</w:body></w:document>"

    styles = (
        '<w:styles xmlns:w="urn:w">'
        '<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri"/><w:sz w:val="22"/></w:rPr></w:rPrDefault></w:docDefaults>'
        '<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>'
        '<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:pPr><w:outlineLvl w:val="0"/></w:pPr><w:rPr><w:b/><w:sz w:val="32"/></w:rPr></w:style>'
        '<w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:pPr><w:outlineLvl w:val="1"/></w:pPr></w:style>'
        "</w:styles>"
    )
    numbering = (
        '<w:numbering xmlns:w="urn:w">'
        '<w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="bullet"/></w:lvl></w:abstractNum>'
        '<w:abstractNum w:abstractNumId="1"><w:lvl w:ilvl="0"><w:start w:val="3"/><w:numFmt w:val="decimal"/></w:lvl></w:abstractNum>'
        '<w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>'
        '<w:num w:numId="2"><w:abstractNumId w:val="1"/></w:num>'
        "</w:numbering>"
    )
    footnotes = (
        '<w:footnotes xmlns:w="urn:w">'
        '<w:footnote w:id="-1"><w:p/></w:footnote>'
        '<w:footnote w:id="2"><w:p><w:r><w:t>脚注：出自《炼金术初步》。</w:t></w:r></w:p></w:footnote>'
        "</w:footnotes>"
    )
    rels = (
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rIdH1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://athanor.example/" TargetMode="External"/>'
        "</Relationships>"
    )
    return [
        ("[Content_Types].xml", CT),
        ("_rels/.rels", RELS_ROOT),
        ("docProps/core.xml", CORE.format(title="炼金术导论")),
        ("word/document.xml", doc),
        ("word/_rels/document.xml.rels", rels),
        ("word/styles.xml", styles),
        ("word/numbering.xml", numbering),
        ("word/footnotes.xml", footnotes),
    ]


def build_lossy():
    doc = f"""<w:document {W} xmlns:m="urn:m" xmlns:mc="urn:mc"><w:body>"""
    doc += '<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:bookmarkStart w:id="9" w:name="_Toc9"/><w:r><w:t>有损特性示例</w:t></w:r><w:bookmarkEnd w:id="9"/></w:p>'
    # tracked changes
    doc += (
        "<w:p>"
        '<w:ins w:id="1" w:author="张三" w:date="2026-01-05T00:00:00Z"><w:r><w:t>新增文字</w:t></w:r></w:ins>'
        '<w:del w:id="2" w:author="张三" w:date="2026-01-05T00:00:00Z"><w:r><w:delText>删除文字</w:delText></w:r></w:del>'
        "</w:p>"
    )
    # comment anchored on a range
    doc += (
        "<w:p>"
        '<w:commentRangeStart w:id="1"/>'
        "<w:r><w:t>被批注的句子。</w:t></w:r>"
        '<w:commentRangeEnd w:id="1"/>'
        '<w:r><w:commentReference w:id="1"/></w:r>'
        "</w:p>"
    )
    # highlight + vertAlign (run props dropped)
    doc += (
        "<w:p><w:r><w:rPr><w:highlight w:val=\"yellow\"/><w:vertAlign w:val=\"superscript\"/></w:rPr>"
        "<w:t>高亮上标</w:t></w:r></w:p>"
    )
    # OMML display math
    doc += '<w:p><m:oMathPara><m:oMath><m:r><m:t>a²+b²=c²</m:t></m:r></m:oMath></m:oMathPara></w:p>'
    # field (TOC)
    doc += '<w:p><w:fldSimple w:instr=" TOC \\o &quot;1-3&quot; "><w:r><w:t>目录占位</w:t></w:r></w:fldSimple></w:p>'
    # sdt
    doc += '<w:p><w:sdt><w:sdtPr/><w:sdtContent><w:r><w:t>控件文本</w:t></w:r></w:sdtContent></w:sdt></w:p>'
    # tab + soft break + page break
    doc += "<w:p><w:r><w:t>列</w:t></w:r><w:r><w:tab/><w:t>表</w:t></w:r><w:r><w:br/><w:t>换行后</w:t></w:r><w:r><w:br w:type=\"page\"/></w:r></w:p>"
    # vMerge table
    doc += (
        "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol/><w:gridCol/></w:tblGrid>"
        "<w:tr>"
        '<w:tc><w:tcPr><w:vMerge w:val="restart"/></w:tcPr><w:p><w:r><w:t>跨行</w:t></w:r></w:p></w:tc>'
        '<w:tc><w:p><w:r><w:t>R1C2</w:t></w:r></w:p></w:tc></w:tr>'
        "<w:tr>"
        '<w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p/></w:tc>'
        '<w:tc><w:p><w:r><w:t>R2C2</w:t></w:r></w:p></w:tc></w:tr>'
        "</w:tbl>"
    )
    # endnote + internal anchor hyperlink
    doc += '<w:p><w:r><w:endnoteReference w:id="2"/></w:r><w:hyperlink w:anchor="_Toc9"><w:r><w:t>回到开头</w:t></w:r></w:hyperlink></w:p>'
    doc += "</w:body></w:document>"

    styles = (
        '<w:styles xmlns:w="urn:w">'
        '<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>'
        '<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:pPr><w:outlineLvl w:val="0"/></w:pPr></w:style>'
        '<w:style w:type="paragraph" w:styleId="EmphBox"><w:name w:val="强调框"/></w:style>'
        "</w:styles>"
    )
    comments = (
        '<w:comments xmlns:w="urn:w">'
        '<w:comment w:id="1" w:author="李四" w:date="2026-01-06T08:00:00Z"><w:p><w:r><w:t>这句话需要润色</w:t></w:r></w:p></w:comment>'
        "</w:comments>"
    )
    endnotes = (
        '<w:endnotes xmlns:w="urn:w">'
        '<w:endnote w:id="-1"><w:p/></w:endnote>'
        '<w:endnote w:id="2"><w:p><w:r><w:t>尾注内容。</w:t></w:r></w:p></w:endnote>'
        "</w:endnotes>"
    )
    settings = '<w:settings xmlns:w="urn:w"><w:trackChanges/></w:settings>'
    header = '<w:hdr xmlns:w="urn:w"><w:p><w:r><w:t>页眉文字</w:t></w:r></w:p></w:hdr>'
    footer = '<w:ftr xmlns:w="urn:w"><w:p><w:r><w:t>页脚文字</w:t></w:r></w:p></w:ftr>'
    return [
        ("[Content_Types].xml", CT),
        ("_rels/.rels", RELS_ROOT),
        ("docProps/core.xml", CORE.format(title="有损特性示例")),
        ("word/document.xml", doc),
        ("word/styles.xml", styles),
        ("word/comments.xml", comments),
        ("word/endnotes.xml", endnotes),
        ("word/settings.xml", settings),
        ("word/header1.xml", header),
        ("word/footer1.xml", footer),
        ("word/embeddings/oleObject1.bin", bytes(range(64))),
    ]


def write_docx(path, entries):
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        for name, data in entries:
            zi = zipfile.ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
            zi.compress_type = zipfile.ZIP_DEFLATED
            if isinstance(data, str):
                data = data.encode("utf-8")
            z.writestr(zi, data)


def main():
    os.makedirs(OUT, exist_ok=True)
    write_docx(os.path.join(OUT, "basic.docx"), build_basic())
    write_docx(os.path.join(OUT, "lossy.docx"), build_lossy())
    print("written:", os.path.join(OUT, "basic.docx"))
    print("written:", os.path.join(OUT, "lossy.docx"))


if __name__ == "__main__":
    main()
