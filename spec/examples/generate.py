#!/usr/bin/env python3
"""生成并校验 Azodoc 黄金样例（M0 验收物）。

用法:
    python generate.py            # 重新生成三个 .azodoc 样例（输出确定性字节）
    python generate.py --verify   # 只校验，不生成

说明:
- 兼容缓存（compatibility/*）为手工固定的夹具内容；由内容模型程序化生成缓存是 M2 的范围。
- ID 由确定性计数器生成，满足 ULID 字母表（Crockford Base32，无 I/L/O/U）。
- 每个样例都是: 32 字节 Azodoc Header + 标准 ZIP（manifest.json 首条目且 STORED）。
"""

import base64
import hashlib
import io
import json
import struct
import sys
import zipfile
import zlib
from pathlib import Path

HERE = Path(__file__).resolve().parent
SCHEMA_DIR = HERE.parent / "json-schema"

MAGIC = b"AZODOC\r\n"
FIXED_ZIP_TIME = (1980, 1, 1, 0, 0, 0)
T0 = "2026-09-14T12:00:00Z"
T1 = "2026-09-14T12:05:00Z"
T2 = "2026-09-14T12:10:00Z"
GENERATOR = {"name": "athanor-sample", "version": "0.1.0"}

ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"  # Crockford Base32, 无 I L O U


# ---------------------------------------------------------------- 基础设施

class IdGen:
    """确定性 ID 生成器: <prefix>_<26 位 Crockford Base32>。"""

    def __init__(self):
        self.n = 0

    def uid(self, prefix: str) -> str:
        self.n += 1
        n = self.n
        chars = []
        for _ in range(26):
            chars.append(ALPHABET[n % 32])
            n //= 32
        return f"{prefix}_{''.join(chars)}"


def jdump(obj) -> bytes:
    return (json.dumps(obj, ensure_ascii=False, indent=2) + "\n").encode("utf-8")


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def make_header(flags: int = 0) -> bytes:
    body = (
        MAGIC
        + bytes([1, 0])              # version_major.minor
        + struct.pack("<H", 32)      # header_size
        + struct.pack("<I", flags)   # flags
        + struct.pack("<Q", 32)      # zip_offset
    )
    assert len(body) == 24
    return body + struct.pack("<I", zlib.crc32(body) & 0xFFFFFFFF) + b"\x00" * 4


def build_azodoc(entries) -> bytes:
    """entries: [(name, data: bytes, stored: bool)]；manifest.json 必须首个且 STORED。"""
    assert entries[0][0] == "manifest.json"
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as zf:
        for name, data, stored in entries:
            zi = zipfile.ZipInfo(name, date_time=FIXED_ZIP_TIME)
            zi.external_attr = 0o100644 << 16
            zi.compress_type = zipfile.ZIP_STORED if stored else zipfile.ZIP_DEFLATED
            zf.writestr(zi, data)
    return make_header() + buf.getvalue()


def make_png(width: int = 8, height: int = 8, rgb=(178, 58, 44)) -> bytes:
    """程序化生成最小合法 PNG（8-bit truecolor）。"""

    def chunk(typ: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data)) + typ + data
            + struct.pack(">I", zlib.crc32(typ + data) & 0xFFFFFFFF)
        )

    ihdr = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    raw = b"".join(b"\x00" + bytes(rgb) * width for _ in range(height))
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")


def layer_entry(path: str, sha: str) -> dict:
    return {"path": path, "sha256": sha}


def compat_entry(fmt: str, path: str, revision, generated_at: str) -> dict:
    return {
        "format": fmt,
        "path": path,
        "revision": revision,
        "generated_at": generated_at,
        "generator": GENERATOR,
        "status": "fresh",
    }


# ---------------------------------------------------------------- 样例 1: minimal

def build_minimal():
    g = IdGen()
    doc_id = g.uid("doc")
    h1, p1 = g.uid("blk"), g.uid("blk")

    content = {
        "schema_version": "1.0",
        "content": [
            {"id": h1, "type": "heading", "level": 1,
             "content": [{"type": "text", "text": "最小样例"}]},
            {"id": p1, "type": "paragraph",
             "content": [{"type": "text", "text": "这是一个最小合法 Azodoc 容器：只有内容层与 TXT 兼容缓存。"}]},
        ],
    }
    txt = (
        "最小样例\n"
        "========\n\n"
        "这是一个最小合法 Azodoc 容器：只有内容层与 TXT 兼容缓存。\n"
    )

    content_b, txt_b = jdump(content), txt.encode("utf-8")
    manifest = {
        "azodoc": {"format_version": "1.0", "container_profile": "prefixed", "generator": GENERATOR},
        "document": {
            "id": doc_id, "schema_version": "1.0", "title": "最小样例", "language": "zh-CN",
            "created_at": T0, "modified_at": T0,
        },
        "current_revision": None,
        "layers": {"content": layer_entry("document/content.json", sha256(content_b))},
        "compatibility": [compat_entry("text", "compatibility/text/document.txt", None, T0)],
        "reports": [],
    }
    entries = [
        ("manifest.json", jdump(manifest), True),
        ("document/content.json", content_b, False),
        ("compatibility/text/document.txt", txt_b, False),
    ]
    return "minimal.azodoc", entries


# ---------------------------------------------------------------- 样例 2: rich

def build_rich():
    g = IdGen()
    doc_id = g.uid("doc")
    sec_root = g.uid("sec")
    h1 = g.uid("blk")
    p1 = g.uid("blk")
    p2 = g.uid("blk")
    h2a = g.uid("blk")
    p3 = g.uid("blk")
    quote = g.uid("blk")
    quote_p = g.uid("blk")
    list1 = g.uid("blk")
    li1, li2, li3, li4 = g.uid("li"), g.uid("li"), g.uid("li"), g.uid("li")
    li1_p, li2_p, li3_p, li4_p = (g.uid("blk") for _ in range(4))
    nested = g.uid("blk")
    li_n1, li_n2 = g.uid("li"), g.uid("li")
    li_n1_p, li_n2_p = g.uid("blk"), g.uid("blk")
    list2 = g.uid("blk")
    li5 = g.uid("li")
    li5_p = g.uid("blk")
    code = g.uid("blk")
    h2b = g.uid("blk")
    table = g.uid("blk")
    col1, col2 = g.uid("col"), g.uid("col")
    row1, row2, row3 = g.uid("row"), g.uid("row"), g.uid("row")
    cel = [g.uid("cel") for _ in range(6)]
    cel_p = [g.uid("blk") for _ in range(6)]
    figure = g.uid("blk")
    asset_id = g.uid("as")
    callout = g.uid("blk")
    callout_p = g.uid("blk")
    math = g.uid("blk")
    p4 = g.uid("blk")
    fn1 = g.uid("blk")
    fn1_p = g.uid("blk")
    hr = g.uid("blk")
    h2c = g.uid("blk")
    p5 = g.uid("blk")
    ann1, ann2, ann3 = g.uid("ann"), g.uid("ann"), g.uid("ann")
    rev_a, rev_b = g.uid("rev"), g.uid("rev")
    pub_none = None  # rich 样例不含 publication 层

    title = "Azodoc 示例：炼金术士的工作台"

    content = {
        "schema_version": "1.0",
        "content": [
            {
                "id": sec_root, "type": "section",
                "children": [
                    {"id": h1, "type": "heading", "level": 1,
                     "content": [{"type": "text", "text": title}]},
                    {"id": p1, "type": "paragraph", "content": [
                        {"type": "text", "text": "Azodoc 是一种"},
                        {"type": "strong", "content": [{"type": "text", "text": "结构化文档容器"}]},
                        {"type": "text", "text": "，目标是让"},
                        {"type": "em", "content": [{"type": "text", "text": "不同生态"}]},
                        {"type": "text", "text": " 之间安全降级与恢复。"},
                        {"type": "link", "url": "https://example.com/azodoc",
                         "content": [{"type": "text", "text": "项目主页"}]},
                    ]},
                    {"id": p2, "type": "paragraph", "content": [
                        {"type": "text", "text": "行内代码示例："},
                        {"type": "code", "text": f"asset://{asset_id}/athanor-banner.png"},
                        {"type": "hard_break"},
                        {"type": "text", "text": "第二行展示硬换行。"},
                    ]},
                    {"id": h2a, "type": "heading", "level": 2,
                     "content": [{"type": "text", "text": "基础块"}]},
                    {"id": p3, "type": "paragraph",
                     "content": [{"type": "text", "text": "下面展示 v1 支持的主要块类型。"}]},
                    {"id": quote, "type": "quote", "children": [
                        {"id": quote_p, "type": "paragraph",
                         "content": [{"type": "text", "text": "格式可以降级，文档不应消失。"}]},
                    ]},
                    {"id": list1, "type": "list", "style": "bullet", "items": [
                        {"id": li1, "type": "list_item",
                         "children": [{"id": li1_p, "type": "paragraph",
                                       "content": [{"type": "text", "text": "导入现有文档"}]}]},
                        {"id": li2, "type": "list_item", "children": [
                            {"id": li2_p, "type": "paragraph",
                             "content": [{"type": "text", "text": "支持的导入格式："}]},
                            {"id": nested, "type": "list", "style": "bullet", "items": [
                                {"id": li_n1, "type": "list_item",
                                 "children": [{"id": li_n1_p, "type": "paragraph",
                                               "content": [{"type": "text", "text": "Markdown"}]}]},
                                {"id": li_n2, "type": "list_item",
                                 "children": [{"id": li_n2_p, "type": "paragraph",
                                               "content": [{"type": "text", "text": "HTML"}]}]},
                            ]},
                        ]},
                        {"id": li3, "type": "list_item", "checked": True,
                         "children": [{"id": li3_p, "type": "paragraph",
                                       "content": [{"type": "text", "text": "设计容器格式"}]}]},
                        {"id": li4, "type": "list_item", "checked": False,
                         "children": [{"id": li4_p, "type": "paragraph",
                                       "content": [{"type": "text", "text": "实现转换器"}]}]},
                    ]},
                    {"id": list2, "type": "list", "style": "ordered", "start": 3, "items": [
                        {"id": li5, "type": "list_item",
                         "children": [{"id": li5_p, "type": "paragraph",
                                       "content": [{"type": "text", "text": "从 3 开始编号的有序列表（start 属性）。"}]}]},
                    ]},
                    {"id": code, "type": "code_block", "language": "rust",
                     "text": 'fn main() { println!("万物入炉，一炼成书。"); }\n'},
                    {"id": h2b, "type": "heading", "level": 2,
                     "content": [{"type": "text", "text": "表格与图"}]},
                    {"id": table, "type": "table", "header_row": True,
                     "columns": [{"id": col1, "name": "块类型"}, {"id": col2, "name": "说明"}],
                     "rows": [
                         {"id": row1, "cells": [
                             {"id": cel[0], "column": 0, "children": [
                                 {"id": cel_p[0], "type": "paragraph",
                                  "content": [{"type": "text", "text": "paragraph / heading"}]}]},
                             {"id": cel[1], "column": 1, "children": [
                                 {"id": cel_p[1], "type": "paragraph",
                                  "content": [{"type": "text", "text": "文本与大纲"}]}]},
                         ]},
                         {"id": row2, "cells": [
                             {"id": cel[2], "column": 0, "children": [
                                 {"id": cel_p[2], "type": "paragraph",
                                  "content": [{"type": "text", "text": "figure / image"}]}]},
                             {"id": cel[3], "column": 1, "children": [
                                 {"id": cel_p[3], "type": "paragraph",
                                  "content": [{"type": "text", "text": "引用 assets/ 注册表资产"}]}]},
                         ]},
                         {"id": row3, "cells": [
                             {"id": cel[4], "column": 0, "children": [
                                 {"id": cel_p[4], "type": "paragraph",
                                  "content": [{"type": "text", "text": "callout / math_block"}]}]},
                             {"id": cel[5], "column": 1, "children": [
                                 {"id": cel_p[5], "type": "paragraph",
                                  "content": [{"type": "text", "text": "提示块与公式"}]}]},
                         ]},
                     ]},
                    {"id": figure, "type": "figure", "asset": f"asset://{asset_id}",
                     "alt": "8×8 像素占位图",
                     "caption": [{"type": "text", "text": "图 1：程序生成的占位资产"}]},
                    {"id": callout, "type": "callout", "variant": "note", "children": [
                        {"id": callout_p, "type": "paragraph",
                         "content": [{"type": "text", "text": "这是 callout 提示块，样式在 presentation/theme.json 中定义。"}]},
                    ]},
                    {"id": math, "type": "math_block", "latex": "E = mc^2"},
                    {"id": p4, "type": "paragraph", "content": [
                        {"type": "text", "text": "Azodoc 的修订层支持 AI 作者身份。"},
                        {"type": "footnote_ref", "id": fn1},
                    ]},
                    {"id": hr, "type": "horizontal_rule"},
                    {"id": h2c, "type": "heading", "level": 2,
                     "content": [{"type": "text", "text": "结语"}]},
                    {"id": p5, "type": "paragraph",
                     "content": [{"type": "text", "text": "本样例用于 M0 验收。"}]},
                    {"id": fn1, "type": "footnote", "children": [
                        {"id": fn1_p, "type": "paragraph",
                         "content": [{"type": "text", "text": "author 字段记录修订作者类型（human/ai/importer/converter/system）。"}]},
                    ]},
                ],
            }
        ],
    }

    initial = {
        "schema_version": "1.0",
        "content": [
            {"id": h1, "type": "heading", "level": 1,
             "content": [{"type": "text", "text": title}]},
            content["content"][0]["children"][1],
        ],
    }

    png = make_png()
    asset_path = f"assets/{asset_id}/athanor-banner.png"
    registry = {
        "schema_version": "1.0",
        "assets": [{
            "id": asset_id, "filename": "athanor-banner.png", "mime": "image/png",
            "size": len(png), "sha256": sha256(png), "storage": "embedded",
            "path": asset_path, "relationship": "inline",
        }],
    }

    annotations = {
        "schema_version": "1.0",
        "annotations": [
            {
                "id": ann1, "type": "Concept",
                "target": {"kind": "text_quote", "block": p1,
                           "selector": {"type": "text_quote", "exact": "结构化文档容器",
                                        "prefix": "Azodoc 是一种", "suffix": "，目标是让"}},
                "value": {"label": "格式定位"}, "confidence": 0.98,
                "source": "athanor-sample/0.1", "author": {"type": "ai", "id": "model:example"},
                "created_at": T1,
            },
            {
                "id": ann2, "type": "ImageRole",
                "target": {"kind": "asset", "asset": asset_id},
                "value": {"role": "placeholder"}, "confidence": 1.0,
                "source": "athanor-sample/0.1", "author": {"type": "human", "id": "vegetable"},
                "created_at": T1,
            },
            {
                "id": ann3, "type": "ReviewStatus",
                "target": {"kind": "block", "block": callout},
                "value": {"status": "approved"},
                "source": "athanor-sample/0.1", "author": {"type": "human", "id": "vegetable"},
                "created_at": T2,
            },
        ],
    }

    theme = {
        "schema_version": "1.0",
        "theme": "default",
        "defaults": {"page": {"size": "A4", "margin": "25mm"},
                     "base_font": "Source Han Serif SC", "base_size": "11pt"},
        "styles": [{"name": "brand-red", "applies_to": ["strong"],
                    "css_like": {"color": "#8B0000"}}],
        "layout_hints": [{"block": h2b, "hint": "page-before"}],
    }

    content_b = jdump(content)
    initial_b = jdump(initial)
    chain = {
        "schema_version": "1.0",
        "policy": {"mode": "snapshot", "trigger": ["explicit_save", "before_convert"], "max_snapshots": 200},
        "head": rev_b,
        "revisions": [
            {"id": rev_a, "parent": None, "author": {"type": "importer", "id": "athanor-sample/0.1"},
             "timestamp": T0, "message": "import from sample.md", "kind": "snapshot",
             "path": f"revisions/{rev_a}/content.json", "sha256": sha256(initial_b),
             "changes": {"blocks_added": 2}},
            {"id": rev_b, "parent": rev_a, "author": {"type": "human", "id": "vegetable"},
             "timestamp": T1, "message": "补充示例块", "kind": "snapshot",
             "path": f"revisions/{rev_b}/content.json", "sha256": sha256(content_b),
             "changes": {"blocks_added": 24, "blocks_modified": 1}},
        ],
    }

    png_b64 = base64.b64encode(png).decode("ascii")
    txt = (
        f"{title}\n{'=' * 30}\n\n"
        "Azodoc 是一种结构化文档容器，目标是让不同生态 之间安全降级与恢复。（项目主页）\n"
        "行内代码示例：asset://…/athanor-banner.png\n"
        "第二行展示硬换行。\n\n"
        "基础块\n------\n\n"
        "下面展示 v1 支持的主要块类型。\n\n"
        "  格式可以降级，文档不应消失。\n\n"
        "  1. 导入现有文档\n"
        "  2. 支持的导入格式：\n     - Markdown\n     - HTML\n"
        "  3. [x] 设计容器格式\n"
        "  4. [ ] 实现转换器\n\n"
        "  3. 从 3 开始编号的有序列表（start 属性）。\n\n"
        "  fn main() { println!(\"万物入炉，一炼成书。\"); }\n\n"
        "表格与图\n--------\n\n"
        "块类型 / 说明\n"
        "paragraph / heading / 文本与大纲\n"
        "figure / image / 引用 assets/ 注册表资产\n"
        "callout / math_block / 提示块与公式\n\n"
        "[图: 8×8 像素占位图]\n图 1：程序生成的占位资产\n\n"
        "[NOTE] 这是 callout 提示块，样式在 presentation/theme.json 中定义。\n\n"
        "E = mc^2\n\n"
        "Azodoc 的修订层支持 AI 作者身份。[1]\n\n"
        "──────────\n\n结语\n----\n\n本样例用于 M0 验收。\n\n"
        "脚注：\n[1] author 字段记录修订作者类型（human/ai/importer/converter/system）。\n"
    )
    md = f"""# {title}

Azodoc 是一种**结构化文档容器**，目标是让*不同生态* 之间安全降级与恢复。[项目主页](https://example.com/azodoc)

行内代码示例：`asset://{asset_id}/athanor-banner.png`\
第二行展示硬换行。

## 基础块

下面展示 v1 支持的主要块类型。

> 格式可以降级，文档不应消失。

- 导入现有文档
- 支持的导入格式：
  - Markdown
  - HTML
- [x] 设计容器格式
- [ ] 实现转换器

3. 从 3 开始编号的有序列表（start 属性）。

```rust
fn main() {{ println!("万物入炉，一炼成书。"); }}
```

## 表格与图

| 块类型 | 说明 |
| --- | --- |
| paragraph / heading | 文本与大纲 |
| figure / image | 引用 assets/ 注册表资产 |
| callout / math_block | 提示块与公式 |

![8×8 像素占位图](data:image/png;base64,{png_b64})*图 1：程序生成的占位资产*

> [!NOTE]
> 这是 callout 提示块，样式在 presentation/theme.json 中定义。

$$E = mc^2$$

Azodoc 的修订层支持 AI 作者身份。[^1]

---

## 结语

本样例用于 M0 验收。

[^1]: author 字段记录修订作者类型（human/ai/importer/converter/system）。
"""
    html = f"""<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="azodoc.document-id" content="{doc_id}">
<meta name="azodoc.revision" content="{rev_b}">
<title>{title}</title>
<style>
body{{max-width:44em;margin:2rem auto;padding:0 1rem;font-family:"Source Han Serif SC",serif;line-height:1.7;color:#222}}
h1,h2{{line-height:1.3}} .callout{{border-left:4px solid #36c;background:#f0f6ff;padding:.5rem 1rem;margin:1rem 0}}
code,pre{{background:#f5f5f5;border-radius:4px}} pre{{padding:.75rem;overflow:auto}}
figure{{margin:1.5rem 0;text-align:center}} figcaption{{font-size:.9em;color:#666}}
table{{border-collapse:collapse}} td,th{{border:1px solid #ccc;padding:.35rem .6rem}}
.footnotes{{font-size:.9em;color:#555;border-top:1px solid #ddd;margin-top:2rem;padding-top:.5rem}}
</style>
</head>
<body>
<article>
<h1 id="{h1}">{title}</h1>
<p>Azodoc 是一种<strong>结构化文档容器</strong>，目标是让<em>不同生态</em> 之间安全降级与恢复。<a href="https://example.com/azodoc">项目主页</a></p>
<p>行内代码示例：<code>asset://{asset_id}/athanor-banner.png</code><br>第二行展示硬换行。</p>
<h2 id="{h2a}">基础块</h2>
<p>下面展示 v1 支持的主要块类型。</p>
<blockquote><p>格式可以降级，文档不应消失。</p></blockquote>
<ul>
<li><p>导入现有文档</p></li>
<li><p>支持的导入格式：</p><ul><li><p>Markdown</p></li><li><p>HTML</p></li></ul></li>
<li><p><input type="checkbox" checked disabled> 设计容器格式</p></li>
<li><p><input type="checkbox" disabled> 实现转换器</p></li>
</ul>
<ol start="3"><li><p>从 3 开始编号的有序列表（start 属性）。</p></li></ol>
<pre><code class="language-rust">fn main() {{ println!("万物入炉，一炼成书。"); }}</code></pre>
<h2 id="{h2b}">表格与图</h2>
<table>
<thead><tr><th>块类型</th><th>说明</th></tr></thead>
<tbody>
<tr><td>paragraph / heading</td><td>文本与大纲</td></tr>
<tr><td>figure / image</td><td>引用 assets/ 注册表资产</td></tr>
<tr><td>callout / math_block</td><td>提示块与公式</td></tr>
</tbody>
</table>
<figure><img src="data:image/png;base64,{png_b64}" alt="8×8 像素占位图"><figcaption>图 1：程序生成的占位资产</figcaption></figure>
<div class="callout" data-variant="note"><p>这是 callout 提示块，样式在 presentation/theme.json 中定义。</p></div>
<p class="math" data-latex="E = mc^2">E = mc^2</p>
<p>Azodoc 的修订层支持 AI 作者身份。<sup><a href="#{fn1}" id="fnref-{fn1}">[1]</a></sup></p>
<hr>
<h2 id="{h2c}">结语</h2>
<p>本样例用于 M0 验收。</p>
<section class="footnotes"><ol><li id="{fn1}">author 字段记录修订作者类型（human/ai/importer/converter/system）。<a href="#fnref-{fn1}">↩</a></li></ol></section>
</article>
</body>
</html>
"""

    report = {
        "report_version": "1.0",
        "direction": "import",
        "source": {"format": "markdown", "path": "sample.md", "sha256": sha256(md.encode("utf-8"))},
        "target": {"format": "azodoc", "document_id": doc_id, "revision": rev_a},
        "engine": {"name": "athanor", "version": "0.1.0", "converter": "athanor-md/0.1.0"},
        "status": "complete",
        "summary": {
            "counts": {"blocks": 2, "spans": 6, "assets": 0},
            "loss": {"none": 2, "partial": 0, "degraded": 0, "unsupported": 0, "preserved_raw": 0},
        },
        "issues": [],
    }

    files = {
        "document/content.json": jdump(content),
        f"revisions/{rev_a}/content.json": initial_b,
        f"revisions/{rev_b}/content.json": content_b,
        "assets/registry.json": jdump(registry),
        asset_path: png,
        "semantics/annotations.json": jdump(annotations),
        "presentation/theme.json": jdump(theme),
        "revisions/chain.json": jdump(chain),
        "compatibility/text/document.txt": txt.encode("utf-8"),
        "compatibility/markdown/document.md": md.encode("utf-8"),
        "compatibility/html/index.html": html.encode("utf-8"),
        "reports/import-markdown-0001.json": jdump(report),
    }
    manifest = {
        "azodoc": {"format_version": "1.0", "container_profile": "prefixed", "generator": GENERATOR},
        "document": {
            "id": doc_id, "schema_version": "1.0", "title": title, "language": "zh-CN",
            "created_at": T0, "modified_at": T2,
        },
        "current_revision": rev_b,
        "layers": {
            "content": layer_entry("document/content.json", sha256(files["document/content.json"])),
            "assets": layer_entry("assets/registry.json", sha256(files["assets/registry.json"])),
            "semantics": layer_entry("semantics/annotations.json", sha256(files["semantics/annotations.json"])),
            "presentation": layer_entry("presentation/theme.json", sha256(files["presentation/theme.json"])),
            "revisions": layer_entry("revisions/chain.json", sha256(files["revisions/chain.json"])),
        },
        "compatibility": [
            compat_entry("html", "compatibility/html/index.html", rev_b, T2),
            compat_entry("markdown", "compatibility/markdown/document.md", rev_b, T2),
            compat_entry("text", "compatibility/text/document.txt", rev_b, T2),
        ],
        "reports": [{
            "path": "reports/import-markdown-0001.json", "kind": "conversion",
            "sha256": sha256(files["reports/import-markdown-0001.json"]),
        }],
    }
    entries = [("manifest.json", jdump(manifest), True)]
    stored = {asset_path}
    entries += [(name, data, name in stored) for name, data in files.items()]
    return "rich.azodoc", entries


# ---------------------------------------------------------------- 样例 3: forward-compat

def build_forward_compat():
    g = IdGen()
    doc_id = g.uid("doc")
    h1 = g.uid("blk")
    p1 = g.uid("blk")
    p2 = g.uid("blk")
    unknown_blk = g.uid("blk")

    payload1 = b'<x-chart data-series="[1,2,3]"><canvas></canvas></x-chart>\n'
    payload2 = b'<x-tooltip text="\xe6\x8f\x90\xe7\xa4\xba">\xe8\xaf\x8d</x-tooltip>\n'
    future_bin = b"FUTURE-FEATURE\x00\x01\x02\x03"
    ai_state = {"model": "example-1", "tokens": 42, "note": "未知层（R1/R2 往返保真测试用）"}

    content = {
        "schema_version": "1.0",
        "content": [
            {"id": h1, "type": "heading", "level": 1,
             "content": [{"type": "text", "text": "前向兼容夹具"}]},
            {"id": p1, "type": "paragraph",
             "content": [{"type": "text", "text": "本文件包含 v1 工具不认识的构造，全部必须原样保留。"}]},
            {"id": p2, "type": "paragraph", "content": [
                {"type": "text", "text": "本段落含一个"},
                {"type": "unknown", "origin": "html", "loss_class": "preserved_raw",
                 "summary": "自定义行内元素 <x-tooltip>",
                 "payload_ref": "preserved/html/part-0002.html", "content": []},
                {"type": "text", "text": "占位。"},
            ]},
            {"id": unknown_blk, "type": "unknown", "origin": "html", "loss_class": "preserved_raw",
             "summary": "自定义元素 <x-chart>（交互式图表）",
             "payload_ref": "preserved/html/part-0001.html", "content": []},
        ],
    }
    txt = (
        "前向兼容夹具\n"
        "============\n\n"
        "本文件包含 v1 工具不认识的构造，全部必须原样保留。\n\n"
        "本段落含一个[未能显示的内容: 自定义行内元素 <x-tooltip>]占位。\n\n"
        "[未能显示的内容: 自定义元素 <x-chart>（交互式图表）]\n"
    )

    files = {
        "document/content.json": jdump(content),
        "preserved/html/part-0001.html": payload1,
        "preserved/html/part-0002.html": payload2,
        "ai/state.json": jdump(ai_state),
        "future/feature.bin": future_bin,
        "compatibility/text/document.txt": txt.encode("utf-8"),
    }
    manifest = {
        "azodoc": {
            "format_version": "1.0", "container_profile": "prefixed", "generator": GENERATOR,
            "vendor_note": {"hint": "未知字段：R2 前向兼容往返保真测试用"},
        },
        "document": {
            "id": doc_id, "schema_version": "1.0", "title": "前向兼容夹具", "language": "zh-CN",
            "created_at": T0, "modified_at": T0,
        },
        "current_revision": None,
        "layers": {
            "content": layer_entry("document/content.json", sha256(files["document/content.json"])),
            "ai_state": layer_entry("ai/state.json", sha256(files["ai/state.json"])),
        },
        "compatibility": [compat_entry("text", "compatibility/text/document.txt", None, T0)],
        "reports": [],
        "provenance": {"chain": ["html", "azodoc"]},
    }
    entries = [("manifest.json", jdump(manifest), True)]
    stored = {"preserved/html/part-0001.html", "preserved/html/part-0002.html", "future/feature.bin"}
    entries += [(name, data, name in stored) for name, data in files.items()]
    return "forward-compat.azodoc", entries


# ---------------------------------------------------------------- 校验

LAYER_SCHEMA = {
    "manifest.json": "manifest.schema.json",
    "document/content.json": "content.schema.json",
    "semantics/annotations.json": "annotations.schema.json",
    "presentation/theme.json": "theme.schema.json",
    "publication/publication.json": "publication.schema.json",
    "revisions/chain.json": "revisions.schema.json",
    "assets/registry.json": "assets.schema.json",
}


def schema_for(name: str):
    if name in LAYER_SCHEMA:
        return LAYER_SCHEMA[name]
    if name.startswith("revisions/") and name.endswith("/content.json"):
        return "content.schema.json"
    if name.startswith("reports/") and name.endswith(".json"):
        return "conversion-report.schema.json"
    return None


def verify(path: Path) -> bool:
    from jsonschema import Draft202012Validator

    ok = True

    def fail(msg):
        nonlocal ok
        ok = False
        print(f"  ✗ {msg}")

    data = path.read_bytes()
    print(f"校验 {path.name}（{len(data)} 字节）")

    # Header
    if len(data) < 40:
        fail("文件过短"); return False
    if data[:8] != MAGIC:
        fail("magic 不符")
    major, minor = data[8], data[9]
    header_size, flags = struct.unpack("<H", data[10:12])[0], struct.unpack("<I", data[12:16])[0]
    zip_offset, crc = struct.unpack("<Q", data[16:24])[0], struct.unpack("<I", data[24:28])[0]
    if (major, minor, header_size, zip_offset) != (1, 0, 32, 32):
        fail(f"Header 字段异常: major={major} minor={minor} header_size={header_size} zip_offset={zip_offset}")
    if crc != (zlib.crc32(data[:24]) & 0xFFFFFFFF):
        fail("header_crc32 不符")
    if data[28:32] != b"\x00" * 4:
        fail("reserved 非零")
    if flags & ~1:
        fail("flags 保留位非零")

    # ZIP
    zf = zipfile.ZipFile(io.BytesIO(data[32:]))
    names = zf.namelist()
    if names[0] != "manifest.json":
        fail("manifest.json 不是第一个条目")
    if zf.getinfo("manifest.json").compress_type != zipfile.ZIP_STORED:
        fail("manifest.json 未以 STORED 存储")
    if zf.testzip() is not None:
        fail("ZIP CRC 校验失败")
    for info in zf.infolist():
        name = info.filename
        if name.startswith("/") or "\\" in name or ".." in name.split("/"):
            fail(f"条目路径不安全: {name}")
        if info.flag_bits & 0x1:
            fail(f"条目被加密: {name}")

    # manifest 与层一致性
    manifest = json.loads(zf.read("manifest.json"))
    for key, ref in manifest["layers"].items():
        p = ref["path"]
        if p not in names:
            fail(f"layers.{key} 指向不存在的条目: {p}")
            continue
        if sha256(zf.read(p)) != ref["sha256"]:
            fail(f"layers.{key} sha256 不匹配: {p}")

    # JSON 层解析 + Schema 校验
    validated = 0
    for name in names:
        if not name.endswith(".json"):
            continue
        obj = json.loads(zf.read(name))  # 语法校验
        schema_file = schema_for(name)
        if schema_file:
            schema = json.loads((SCHEMA_DIR / schema_file).read_text("utf-8"))
            errs = sorted(Draft202012Validator(schema).iter_errors(obj), key=lambda e: e.path)
            for e in errs:
                loc = "/".join(str(x) for x in e.absolute_path) or "<root>"
                fail(f"Schema 校验失败 [{schema_file}] {loc}: {e.message}")
            validated += 1
    print(f"  ✓ Header / ZIP / sha256 / Schema（{validated} 个 JSON 层）" if ok else "  存在失败项")
    return ok


def main():
    args = sys.argv[1:]
    builders = [build_minimal, build_rich, build_forward_compat]

    if "--verify" in args:
        labels = ["minimal.azodoc", "rich.azodoc", "forward-compat.azodoc"]
        all_ok = all(verify(HERE / label) for label in labels)
        sys.exit(0 if all_ok else 1)

    for builder in builders:
        name, entries = builder()
        out = HERE / name
        out.write_bytes(build_azodoc(entries))
        print(f"生成 {name}（{out.stat().st_size} 字节，{len(entries)} 个条目）")

    print("\n运行 --verify 进行全量校验。")


if __name__ == "__main__":
    main()
