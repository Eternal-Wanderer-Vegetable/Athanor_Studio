# Azodoc v0.1 — 落地方案

> **Status:** Implementation Plan（基于《SDOC Document Model v0.1 — 草案》）
> **Project:** Athanor Studio / Athanor Engine
> **Date:** 2026-09-14
> **本轮已定决策：**
> 1. 先交付规范与路线图，不写代码（符合草案第 31 节「先规范后实现」）。
> 2. 引擎核心语言：**Rust**。
> 3. MVP 转换范围：**Markdown + HTML**（DOCX、PDF 放入后续里程碑）。
> 4. 命名：结合草案与炼金术命名体系，且必须包含 "doc" → 提案 **Azodoc**（见 §2，可否决）。

---

## 0. TL;DR

草案回答了「SDOC 是什么」，本方案回答「怎么把它做出来」：

- **一个格式名**：Azodoc（`.azodoc`），格式层承接命名表的 Azoth，引擎叫 Athanor，GUI 叫 Athanor Studio。
- **一个容器设计**：32 字节固定 Header + 标准 ZIP。Header 满足草案 §4.2「文件开头稳定可识别」；因为 ZIP 读取器从文件尾部定位中央目录，前置 Header 不破坏 ZIP 兼容性（自解压档案即此原理）——改名为 `.zip` 依然能打开，满足 §4.1。
- **一套五层文件布局**：content（内容）/ semantics（语义）/ presentation（表现）/ publication（出版）/ revisions（修订），外加 assets、compatibility、preserved、reports。compatibility 目录本身就是「陌生软件的恢复通道」（`unzip -p x.azodoc compatibility/text/document.txt` 即可读出正文）。
- **七条保真规则（R1–R7）**：把草案的八条原则翻译成实现者可执行、CI 可验证的规范条款，核心是「未知条目/字段/节点一律原样保留并报告，绝不静默丢弃」。
- **一条工程路线**：M0 规范冻结 → M1 容器+模型核心 → M2 Markdown/HTML 转换（MVP）→ M3 修订+语义 → M4 DOCX（Pandoc 桥接）→ M5 PDF 出版 → M6 编辑器。
- **一个测试理念**：草案 §30 的 Degradation Test 不是口号，是 CI 里的黄金文件测试集。

---

## 1. 决议记录

| # | 决策点 | 决议 | 状态 |
|---|---|---|---|
| D1 | 本轮交付 | 落地方案文档（本文件），不写代码 | 已定 |
| D2 | 引擎语言 | Rust（workspace 多 crate） | 已定 |
| D3 | MVP 转换 | Markdown 导入/导出 + HTML 导入/导出 | 已定 |
| D4 | 格式命名 | Azodoc / `.azodoc`（见 §2） | 已定（2026-09-14 确认） |
| D5 | DOCX 策略 | Pandoc 进程桥接（GPL 隔离），放 M4 | 本方案建议，可调 |
| D6 | PDF 策略 | 首选 HTML→Paged.js→headless Chromium；Typst 为备选后端，放 M5 | 本方案建议，可调 |

---

## 2. 命名提案：Azodoc

约束：结合《SDOC Document Model v0.1》与 `name.md` 炼金术命名体系，且名称必须包含 "doc"。

**提案：Azodoc**（Azoth + doc），读作 /ˈæzədɒk/。

| 层 | 原命名表 | 本提案 |
|---|---|---|
| 格式 | Azoth / `.azoth` | **Azodoc / `.azodoc`**（保留 Azoth 词根 + doc 语义） |
| 引擎/CLI | Athanor | Athanor（不变），二进制名 `athanor` |
| 中间模型 | Prima Materia | Prima（不变；指 content 层的文档模型） |
| 导出器 | Transmuter | CLI 子命令 `athanor transmute`（`export` 为别名） |
| 编辑器 | Aludel | Aludel（不变，M6） |
| GUI | Athanor Studio | Athanor Studio（不变，仓库即此） |

配套标识：

- 扩展名：`.azodoc`
- MIME（临时）：`application/x-azodoc+zip`
- 魔数：ASCII `"AZODOC\r\n"`（8 字节，见 §4.1）
- 规范术语：文中 Azodoc 指格式与容器；Prima 指内容模型。草案中的 "SDOC" 一律映射为 Azodoc。

> 若你不喜欢 Azodoc，备选：`Azdoc`（更短）、`Azothdoc`（更完整）。确定后只需全局替换，规范与代码中所有出现点集中在常量与规范文件里。

---

## 3. 总体路线

```text
M0 规范冻结 ──► M1 容器+模型核心 ──► M2 Markdown/HTML 转换（MVP）
                                              │
                       M3 修订+语义层 ◄───────┘
                              │
              M4 DOCX(Pandoc桥) ──► M5 PDF 出版 ──► M6 Aludel 编辑器 / Athanor Studio
```

- M0–M2 是「格式成立」的最小闭环：能建文件、能验证、能与文本生态互通。
- M3–M5 是草案差异化的兑现：修订/语义/出版三层的真正实现。
- M6 才进入编辑器，避免过早被 UI 需求绑架模型设计。

---

## 4. 规范包（M0 的产出内容）

M0 阶段把本节内容拆成四份规范文件放入 `spec/`（见 §5 仓库结构），本节给出全部关键决议与示例。

### 4.1 容器与字节布局（Container Specification）

```text
偏移   大小   字段            v1 取值
0      8     magic           "AZODOC\r\n"  (0x41 0x5A 0x4F 0x44 0x4F 0x43 0x0D 0x0A)
8      1     version_major   0x01
9      1     version_minor   0x00
10     2     header_size     u16 LE，v1 = 32
12     4     flags           u32 LE，bit0 = 使用 ZIP64；其余必须为 0
16     8     zip_offset      u64 LE，= header_size（内嵌 ZIP 起始偏移）
24     4     header_crc32    前 24 字节的 CRC-32（IEEE）
28     4     reserved        0x00000000
```

Header 之后是**一个标准 ZIP 档案**（无偏移差）。

**读取规则（normative MUST）：**

1. magic 不匹配 → 不是 Azodoc。若 magic 为 `PK\x03\x04`，进入「Plain-ZIP 探测」：扫描 ZIP 条目寻找含 `"azodoc"` 键的 `manifest.json`，命中则按 Azodoc 处理（这同时是受损文件的恢复路径）。
2. `version_major` 高于本实现支持版本 → **拒绝解析但给出完整恢复指引**（提示可用工具与官方转换命令）。宁可拒绝，不得半解析（草案 Principle 1）。
3. `header_crc32` 校验失败 → 警告 Header 可能损坏，尝试 Plain-ZIP 探测兜底。

**写入规则：**

1. 必须先写 32 字节 Header，再写 ZIP。
2. `manifest.json` 必须是 ZIP 第一个条目且 **STORED（不压缩）**——使 `unzip -p x.azodoc manifest.json` 无需全量解压即可工作。
3. 默认 Deflate 压缩；图片/音视频等已压缩资产用 STORED。
4. 条目名规则：UTF-8、正斜杠、禁止 `..` 与绝对路径（防路径穿越）。

**兼容性论证（回应草案 §4.1/§4.2）：** ZIP 读取器通过文件尾部的 End of Central Directory 定位，允许文件头存在任意前缀字节（自解压档案即工作于此机制上）。32 字节前缀在 7-Zip / Windows 资源管理器 / Info-ZIP unzip / Python zipfile / Java ZipFile 下均可直接打开——**改名为 .zip 就能读**，无需依赖扩展名识别 Azodoc。M1 验收包含上述工具的实测矩阵；若某主流工具实测失败，降级方案为 **Profile B（plain-zip）**：去掉前缀，魔数信息移入首条目，manifest 中以 `container_profile` 字段区分。

### 4.2 包目录布局（Package Layout）

```text
x.azodoc (ZIP 内部)
├── manifest.json                      # 包清单（第一个条目，STORED）
├── document/
│   └── content.json                   # ★ Prima 内容模型（唯一内容真源）
├── semantics/
│   └── annotations.json               # 语义标注
├── presentation/
│   └── theme.json                     # 主题/样式/Layout hints
├── publication/
│   └── publication.json               # 出版记录（冻结状态索引）
├── revisions/
│   ├── chain.json                     # 修订链索引
│   └── <rev_id>/content.json          # 各修订的快照
├── assets/
│   ├── registry.json                  # 资产注册表
│   └── <asset_id>/<filename>          # 资产二进制
├── preserved/                         # 无法建模的原始内容（只进不出）
│   └── <origin>/<seq>.<ext>           # 如 preserved/docx/part-0007.xml
├── compatibility/                     # 派生表示（可随时删除重建）
│   ├── html/index.html
│   ├── markdown/document.md
│   ├── text/document.txt
│   ├── docx/document.docx
│   └── pdf/document.pdf
└── reports/
    └── *.json                         # conversion-report
```

要点：

- `document/content.json` 是**唯一内容真源**；`compatibility/` 一律视为可丢弃缓存（草案 Principle 5）。
- `preserved/` 是"只进不出"区：任何工具不得修改或删除其中内容，只能新增。
- 未知目录/条目（未来版本新增的层）按 R1 原样保留。

### 4.3 manifest.json（Package Manifest）

```json
{
  "azodoc": {
    "format_version": "1.0",
    "container_profile": "prefixed",
    "generator": { "name": "athanor", "version": "0.1.0" }
  },
  "document": {
    "id": "doc_01JAVG2Z7Q4M8XK3N5P9R2T6W8",
    "schema_version": "1.0",
    "title": "示例文档",
    "language": "zh-CN",
    "created_at": "2026-09-14T12:00:00Z",
    "modified_at": "2026-09-14T12:00:00Z"
  },
  "current_revision": "rev_01JAVG3A9B2C4D6E8F0H1J3K5M7",
  "layers": {
    "content":      { "path": "document/content.json",     "sha256": "…" },
    "assets":       { "path": "assets/registry.json",      "sha256": "…" },
    "semantics":    { "path": "semantics/annotations.json","sha256": "…" },
    "presentation": { "path": "presentation/theme.json",   "sha256": "…" },
    "revisions":    { "path": "revisions/chain.json",      "sha256": "…" },
    "publication":  { "path": "publication/publication.json", "sha256": "…" }
  },
  "compatibility": [
    {
      "format": "html",
      "path": "compatibility/html/index.html",
      "revision": "rev_01JAVG3A9B2C4D6E8F0H1J3K5M7",
      "generated_at": "2026-09-14T12:05:00Z",
      "generator": { "name": "athanor", "version": "0.1.0" },
      "status": "fresh"
    }
  ],
  "reports": [
    { "path": "reports/import-markdown-0001.json", "kind": "conversion", "sha256": "…" }
  ],
  "extra": {}
}
```

规则：

- `layers.*.sha256` 覆盖内容完整性；ZIP CRC 之下再加一层可校验的哈希（`athanor verify` 用）。
- `compatibility[].revision` 绑定生成时的修订号；`current_revision` 不一致 → 工具必须将其视为 `stale`（草案 §18），`athanor upgrade` 负责重建。
- **宽容解析**：所有 JSON 层文件遇到未知键一律收入 `extra` 并在写回时原样输出（R2）。这是前向兼容的机制化。

### 4.4 内容模型 Prima（Content Model）

`document/content.json`，纯内容，无样式无页面概念。

**节点约定：**

- 顶层：`{ "schema_version": "1.0", "content": [ <Section|Block>… ] }`
- 每个 Section/Block 必须有稳定 `id`；Inline span 的 `id` 可选（被语义标注引用时必需）。
- Inline 用**嵌套 span 数组**表达（ProseMirror 式），不用「纯文本+偏移标注」。
- 偏移量问题（草案未覆盖）：语义层锚定**以引用文本选择器为主**（§4.6），避免字符偏移在 UTF-16/码点/字素间的歧义。

**Block 类型（v1）：**

| 类型 | 关键属性 | 说明 |
|---|---|---|
| `section` | `children: (Section\|Block)[]` | 逻辑章节；`Section ≠ Page` |
| `paragraph` | `content: Span[]` | |
| `heading` | `level: 1–6, content: Span[]` | |
| `quote` | `children: Block[]` | |
| `list` | `style: ordered\|bullet, items[]` | |
| `list_item` | `checked?: bool, children: Block[]` | checked 支持任务列表 |
| `code_block` | `language?, text` | 文本走独立字段，不走 Span |
| `table` | `columns[], rows[]` | 单元格 `colSpan/rowSpan`（DOM 习惯）；cell 内是 Block[] |
| `figure` | `asset: asset-url, alt, caption: Span[]` | 块级图 |
| `image` | `asset: asset-url, alt` | 简单块图 |
| `horizontal_rule` | | |
| `math_block` | `latex` | |
| `callout` | `variant: note\|tip\|warning\|important, children: Block[]` | |
| `embed` | `asset: asset-url` | 内嵌文档/媒体 |
| `footnote` | `children: Block[]` | 与 `footnote_ref` 对应 |
| `unknown` | `origin, loss_class, payload_ref?, summary, content:[]` | **保真包装节点**，见下 |

**Inline 类型（v1）：**
`text`（含 `hard_break` 变体）、`strong`、`em`、`underline`、`strike`、`code`、`link{url,title?}`、`footnote_ref{id}`、`inline_math{latex}`、`inline_image{asset,alt}`、`mention{target}`、`cite{key}`、`unknown`（同块级规则）。

**示例——一个段落：**

```json
{
  "id": "blk_01JAVG3QK2M4N6P8R0T2V4X6Z8",
  "type": "paragraph",
  "content": [
    { "type": "text", "text": "Rust 是一门" },
    { "type": "strong", "content": [ { "type": "text", "text": "系统级" } ] },
    { "type": "text", "text": "编程语言。" }
  ]
}
```

**示例——无法建模的内容（Never Silently Lose Data 的落点）：**

```json
{
  "id": "blk_01JAVG4S8A0B2C4D6E8F0H2J4K6",
  "type": "unknown",
  "origin": "html",
  "loss_class": "preserved_raw",
  "summary": "自定义元素 <x-diagram>（交互式图表）",
  "payload_ref": "preserved/html/part-0003.html",
  "content": []
}
```

各 writer 对 `unknown` 的输出义务：

| 目标 | 义务 |
|---|---|
| HTML | 尽力输出 payload 原文（`preserved/` 内容本就是 HTML 时）或占位框 |
| Markdown | `<!-- azodoc: unsupported <summary>，原文见 preserved/… -->` |
| TXT | `[未能显示的内容: <summary>]` |
| DOCX/PDF | 占位段落 + 报告条目 |

### 4.5 ID 与引用系统

- **ULID**（26 位 Crockford Base32，时间有序）+ 类型前缀：`doc_` `sec_` `blk_` `li_` `cel_` `col_` `row_` `as_`(asset) `ann_` `rev_` `pub_` `iss_`(报告条目)。
- ID 一经分配永不复用、永不变更；导入即分配，此后所有层通过 ID 交叉引用。
- 引用协议：
  - 资产：`asset://<asset_id>`，可带文件名提示 `asset://<asset_id>/photo.jpg`；
  - 文档内链接：URL 片段 `#<block_id>`；
  - 外部链接：原样 URL。

### 4.6 语义层（Semantic Layer）

`semantics/annotations.json`。**直接复用 W3C Web Annotation Data Model 的选择器思想，不自造锚定协议**：

```json
{
  "schema_version": "1.0",
  "annotations": [
    {
      "id": "ann_01JAVG5T0A2C4E6G8H0J2K4M6P8",
      "type": "ProgrammingLanguage",
      "target": {
        "kind": "text_quote",
        "block": "blk_01JAVG3QK2M4N6P8R0T2V4X6Z8",
        "selector": { "type": "text_quote", "exact": "Rust", "prefix": "", "suffix": " 是一门" }
      },
      "value": { "name": "Rust" },
      "confidence": 0.97,
      "source": "athanor-semantic/0.1",
      "author": { "type": "ai", "id": "model:example" },
      "created_at": "2026-09-14T12:06:00Z"
    }
  ]
}
```

- `target.kind`：`document` | `section` | `block` | `asset` | `text_quote`（block + 引用选择器）。
- 用 `exact/prefix/suffix` 三元组锚定而非数字偏移：编辑发生后仍可模糊重定位（Web Annotation 生态的成熟做法），彻底避开码点/UTF-16 歧义。
- `author.type`：`human | ai | importer | converter | system`（草案 §15 的 Author Type 在语义层同样适用）。
- 语义标注**永不参与渲染决策**；表现层可以选择"响应"某类标注，但依赖必须显式声明。

### 4.7 表现层与 Layout 边界（Presentation Layer）

`presentation/theme.json`。v1 明确"三不"：不含内容、不按块内联存储样式、不出现绝对页面坐标。

```json
{
  "schema_version": "1.0",
  "theme": "default",
  "defaults": {
    "page": { "size": "A4", "margin": "25mm" },
    "base_font": "Source Han Serif SC",
    "base_size": "11pt"
  },
  "styles": [
    { "name": "warn-red", "applies_to": ["callout:warning"], "css_like": { "color": "#B00" } }
  ],
  "layout_hints": [
    { "block": "blk_01JAVG…", "hint": "page-before" }
  ]
}
```

- Block/span 可携带 `style: ["class-name"]` 引用此处定义的类；类的定义用 **CSS 子集**（颜色/字体/间距/对齐），保证可映射到 DOCX 样式与 PDF。
- Layout 概念（page size、margins、break、widow/orphan、float）在此层以**提示（hint）**形式存在；内容模型永远不依赖它们存在（草案 §13）。

### 4.8 出版层（Publication Layer）

`publication/publication.json`：

```json
{
  "schema_version": "1.0",
  "publications": [
    {
      "id": "pub_01JAVG6V2B4D6F8H0J2K4M6P8R2",
      "source_revision": "rev_01JAVG3A9B2C4D6E8F0H1J3K5M7",
      "created_at": "2026-09-14T12:10:00Z",
      "renderer": { "name": "athanor-pdf", "version": "0.1.0", "engine": "pagedjs+chromium" },
      "artifact": { "path": "compatibility/pdf/document.pdf", "sha256": "…" },
      "page": { "count": 12, "size": "A4" },
      "content_hash": "…", "layout_hash": "…",
      "signature": null
    }
  ]
}
```

- Publication 是「某修订 × 某渲染器版本」的冻结记录，PDF 工件放 `compatibility/pdf/` 并由 `artifact.sha256` 锚定。
- 重建同一 Publication 要求：同修订 + 同渲染器版本 + 同主题 → `layout_hash` 必须一致（确定性出版，向 Typst 的可重现性看齐）。

### 4.9 修订层（Revision Layer）

`revisions/chain.json`：

```json
{
  "schema_version": "1.0",
  "policy": { "mode": "snapshot", "trigger": ["explicit_save", "before_convert"], "max_snapshots": 200 },
  "head": "rev_01JAVG3A9B2C4D6E8F0H1J3K5M7",
  "revisions": [
    {
      "id": "rev_01JAVG3A9B2C4D6E8F0H1J3K5M7",
      "parent": null,
      "author": { "type": "importer", "id": "athanor-md/0.1" },
      "timestamp": "2026-09-14T12:05:00Z",
      "message": "import from input.md",
      "kind": "snapshot",
      "path": "revisions/rev_01JAVG…/content.json",
      "changes": { "blocks_added": 42 }
    }
  ]
}
```

- **v1 只做快照模式**：显式保存/转换前落快照；内容文件不大（纯内容 JSON），快照成本可控；资产不可变（R7），快照无需复制资产。
- 增量/delta 模式留待 v0.3 决议（需先定 diff 格式），规范层面现在只保留 `kind: "delta"` 枚举位。
- 分支允许（`parent` 构成 DAG），但 v1 工具只做线性展示。
- 此层是「AI 作为作者」的一等公民入口：每次 AI 批量改写必须落 `author.type: "ai"` 的修订，这是 Athanor 差异化能力的基础。

### 4.10 资产层（Asset Model）

`assets/registry.json`：

```json
{
  "schema_version": "1.0",
  "assets": [
    {
      "id": "as_01JAVG7X4C6E8G0H2J4K6M8P0R2T4",
      "filename": "architecture.png",
      "mime": "image/png",
      "size": 148233,
      "sha256": "…",
      "storage": "embedded",
      "path": "assets/as_01JAVG7X…/architecture.png",
      "relationship": "inline"
    }
  ]
}
```

- **资产不可变（R7）**：写入后内容与 ID 绑定（sha256 双保险）；"替换图片"= 新资产 + 新 ID + 更新引用。
- `storage: embedded | external`：外链资产记录原始 URL 与抓取元数据，不强制内嵌。
- 垃圾回收（无引用资产清理）不是 v1 范围；宁多勿删。

### 4.11 降级分级与转换报告（Loss Model & Conversion Report）

损失分级沿用草案 §24 五级：`NONE / PARTIAL / DEGRADED / UNSUPPORTED / PRESERVED_RAW`。

`reports/*.json`（conversion-report v1）：

```json
{
  "report_version": "1.0",
  "direction": "import",
  "source": { "format": "markdown", "path": "input.md", "sha256": "…" },
  "target": { "format": "azodoc", "document_id": "doc_01JAVG2Z…", "revision": "rev_01JAVG3A…" },
  "engine": { "name": "athanor", "version": "0.1.0", "converter": "athanor-md/0.1.0" },
  "status": "partial",
  "summary": {
    "counts": { "blocks": 120, "assets": 4 },
    "loss": { "none": 118, "partial": 1, "degraded": 0, "unsupported": 0, "preserved_raw": 1 }
  },
  "issues": [
    {
      "id": "iss_01JAVG8Y6D8F0H2J4K6M8P0R2T4V6",
      "feature": "inline_html",
      "loss_class": "preserved_raw",
      "location": { "source_line": 57 },
      "action": "preserved",
      "detail": "原始 HTML 存入 preserved/html/part-0001.html",
      "message": "内联 HTML 以原始形式保留，未参与建模"
    }
  ]
}
```

- 每次跨格式转换**必须**产出报告：写入容器 `reports/`，同时可按需输出到旁边供流水线读取。
- `status` 规则：`summary.loss` 中出现任何非 `none` 项 → 至少 `partial`；出现 `unsupported` → `partial` 并在 CLI 输出显著提示。
- 报告是给人看的极少、给机器看的多——编辑器可以在 UI 角标显示「本档含 1 处降级」。

**安全例外（必须显式写进规范）：** 活动内容——`<script>`、事件处理器属性、`javascript:` URL——**默认不保留、不执行**，记为 `unsupported`（原因：安全）。提供显式的 `--preserve-raw-active` 取证开关。「不静默丢失」原则让位于安全，但必须留痕。

### 4.12 版本策略与七条保真规则（Serialization Rules）

**版本策略：**

- Header `version_major` 变更 = 破坏性变更（新容器布局/模型断裂）；`version_minor` = 向后兼容增量。
- 读取器必须接受 `major 相同` 的一切文件；遇到更高的 `minor` → 正常打开 + 提示升级。
- 每层 JSON 有独立 `schema_version`，随层独立演进。

**七条保真规则（R1–R7，全部为 normative MUST，是 CI 测试对象）：**

| 规则 | 内容 | 对应草案原则 |
|---|---|---|
| R1 | 读取器→写入器往返，**未知 ZIP 条目字节不变保留** | Principle 1, 7 |
| R2 | JSON 层**未知字段收入 `extra` 原样回写** | Principle 1, 7 |
| R3 | 未知节点类型包装为 `unknown` + payload 入 `preserved/` + 报告条目，**绝不丢弃** | Principle 2, 6 |
| R4 | magic/主版本不符 → 拒绝解析并给恢复指引，**不得半解析** | Principle 1 |
| R5 | 转换器只能写 `compatibility/` 与 `reports/`；**重建永远不触碰其余层** | Principle 3, 5 |
| R6 | 任何 writer 遇到无法表达的内容 → 损失分级 + 报告条目；**宁过度报告**（唯一例外：§4.11 安全条款） | Principle 2, 6 |
| R7 | 资产不可变；ID 永不复用 | Principle 4, 8 |

> R1+R2 合起来是「旧软件不毁新文档，新软件不毁旧文档」的双向前向兼容：一个 v2 文件被 v1 工具打开再保存，v2 的未知条目与字段必须原样回来。这条规则将作为 M1 的固定测试夹具（forward-compat fixture）。

---

## 5. 工程结构与依赖选型（Rust）

### 5.1 仓库结构

```text
Athanor_Studio/
├── design_docs/                  # 草案 + 本方案 + 后续 RFC
├── spec/
│   ├── README.md                 # 规范索引与阅读顺序 + M0 验收清单
│   ├── azodoc-container.md       # §4.1 + §4.2 展开
│   ├── azodoc-package.md         # manifest + 各层文件规范
│   ├── azodoc-model.md           # Prima 内容模型规范
│   ├── azodoc-loss.md            # 损失分级/报告/安全例外
│   ├── json-schema/              # 各层 JSON Schema（M0 手写；M1 起 CI 与代码对齐）
│   └── examples/                 # 黄金样例 .azodoc + 生成/校验脚本（generate.py）
├── engine/                       # Athanor 引擎（Rust workspace）
│   ├── Cargo.toml
│   └── crates/
│       ├── azodoc-model/         # Prima 类型 + schema 校验 + schemars 导出
│       ├── azodoc-container/     # Header/ZIP/manifest/完整性
│       ├── azodoc-convert/       # Reader/Writer 注册表 + 损失报告引擎
│       ├── azodoc-md/            # Markdown reader/writer
│       ├── azodoc-html/          # HTML reader/writer
│       ├── azodoc-docx/          # M4：Pandoc 桥接
│       ├── azodoc-pdf/           # M5：出版管线
│       └── athanor-cli/          # 二进制 `athanor`
├── corpus/                       # 测试语料（含 forward-compat fixtures）
└── docs/                         # 用户文档
```

### 5.2 依赖选型

| 用途 | 选择 | 理由 / 备注 |
|---|---|---|
| ZIP | `zip` crate | 支持 ZIP64/Deflate；前置 Header 只是字节前缀，无耦合 |
| JSON | `serde` + `serde_json` | 生态标配 |
| Schema | `schemars` | 从 Rust 类型生成 JSON Schema；**CI 断言与 spec/json-schema 一致**，规范与代码永不漂移 |
| ID | `ulid` | §4.5 |
| Markdown | `comrak` | 有 AST 也有渲染器（GFM/表格/任务列表/脚注/数学扩展），往返可控；`pulldown-cmark` 只有解析，写回要自造，故不选 |
| HTML 解析 | `html5ever`（+ rcdom） | 真 HTML5 容错解析，规范行为与浏览器一致 |
| HTML 输出 | 手写序列化 | 输出结构完全受控，无模板依赖 |
| CLI | `clap` | |
| 错误/诊断 | `miette` | 友好报错 + 源码位置标注，直接服务「Friendly Failure」 |
| 哈希 | `sha2` | |
| 测试 | `insta`（快照）+ `proptest`（性质测试） | |
| DOCX（M4） | Pandoc **子进程**桥接 | GPL 隔离：进程边界调用不构成衍生作品；桥接层做成可选 feature，核心保持 MIT/Apache 自定 |
| PDF（M5） | headless Chromium + Paged.js；Typst 备选 | 见 §8 |

---

## 6. CLI 设计（`athanor`）

```bash
athanor new doc.azodoc --title "新文档" --lang zh-CN
athanor info doc.azodoc [--json]          # 层清单、修订头、缓存新鲜度
athanor verify doc.azodoc                 # header CRC、ZIP 完整性、各层 sha256、schema 校验
athanor import input.md -o doc.azodoc [--report report.json]
athanor import input.html -o doc.azodoc
athanor transmute doc.azodoc --to html --out out/    # alias: export
athanor transmute doc.azodoc --to md | txt | html | pdf | docx
athanor upgrade doc.azodoc                # 重建全部 stale 的 compatibility 缓存
athanor history doc.azodoc                # 修订链
athanor checkout doc.azodoc --revision rev_01J… [-o out/]
athanor recover broken.azodoc -o recovered/   # Plain-ZIP 扫描抢救（R4 的出口）
```

交互原则（草案 §4.3 Friendly Failure 的 CLI 形态）：所有错误输出包含四段式——`发生了什么 / 文件没有损坏 / 推荐工具 / 恢复命令`。例：

```text
错误：此文件由更新版本的 Azodoc (v2.0) 创建，当前工具支持到 v1.x。
文件本身没有损坏。你可以：
  1. 升级 Athanor: https://…
  2. 抢救内容:   athanor recover doc.azodoc -o recovered/
```

---

## 7. MVP 转换器设计：Markdown 与 HTML（M2）

### 7.1 架构

```text
            ┌──────── azodoc-convert ────────┐
input ──►  Reader ──► Prima 模型 ──► Writer ──► output
            │                                │
            └──► 损失事件 ──► conversion-report ◄──┘
```

- Reader/Writer 是注册表插件；每个转换器只负责「格式 ↔ Prima」，损失事件统一交给报告引擎（R6 的机制化）。
- 转换前自动落修订快照（`policy.trigger: before_convert`），保证转换永远可回退。

### 7.2 Markdown 映射表（v1）

| Markdown | Prima | 往返级别 |
|---|---|---|
| ATX 标题 `#`~`######` | heading(1–6) | NONE |
| Setext 标记 | heading | NONE（写回归一化为 ATX） |
| `**bold**` / `*em*` / `` `code` `` / `~~s~~` | strong / em / code / strike | NONE |
| `<u>`（GFM 扩展） | underline | PARTIAL（非所有 MD 方言支持，写回标注） |
| 硬换行（行尾双空格/反斜杠） | text.hard_break | NONE |
| 嵌套列表 / 有序列表 | list + list_item | NONE |
| 任务列表 `[ ]` / `[x]` | list_item.checked | NONE（GFM）/ PARTIAL（严格 CommonMark 语境） |
| GFM 表格 | table | NONE（无合并单元格；发现合并 → 见 HTML 导入链路） |
| 引用块 / 水平线 | quote / horizontal_rule | NONE |
| 围栏代码 ```lang | code_block | NONE |
| 图片 | figure + asset（本地文件复制入 assets/） | NONE |
| 链接 / 自动链接 | link | NONE |
| 脚注 `[^1]` | footnote + footnote_ref | NONE（M2 可后置，先报告） |
| `$…$` / `$$…$$` | inline_math / math_block | MD 往返 NONE；HTML 导出 DEGRADED（v1 输出 `<span data-latex>` + 报告） |
| YAML front-matter | manifest.document 元数据 | NONE（保留未知键） |
| 内联 HTML / HTML 注释 | unknown + preserved/ | PRESERVED_RAW |

### 7.3 HTML 映射表（v1）

| HTML | Prima | 级别 |
|---|---|---|
| h1–h6 / p / blockquote / hr / br | heading / paragraph / quote / horizontal_rule / hard_break | NONE |
| strong/b、em/i、u、s/del、code | 对应 span（b→strong、i→em 归一化，报告 PARTIAL 备注） | NONE* |
| pre>code | code_block（语言从 class="language-x" 提取） | NONE |
| ul/ol/li 嵌套 | list / list_item | NONE |
| table（含 colspan/rowspan） | table（含 colSpan/rowSpan） | NONE |
| img `src="data:"` | asset（embedded） | NONE |
| img `src="http…"` | asset（external，记录 URL） | NONE（引用保留） |
| a[href] / `#anchor` | link / `#<block_id>` | NONE |
| div | **透明节点**：children 上提，`class` 记入可选语义标注 | NONE |
| span + class | 保留 class 为 `style_classes` | PARTIAL（样式语义取决于 theme） |
| `<style>` | 提取选择器 → theme.json 可映射子集 | PARTIAL |
| `<script>` / 事件属性 / `javascript:` | **不保留不执行** → unsupported + 报告（§4.11 安全例外） | UNSUPPORTED（安全） |
| iframe/video/audio/source | embed + asset | PARTIAL |
| 自定义元素 / 未知标签 | unknown + preserved/ | PRESERVED_RAW |

**HTML 导出（Writer）义务：** 自包含单文件——资产转 data URI；内嵌极简只读样式；数学按 §7.2 降级策略；文档头部嵌入 `<meta name="azodoc.id">` 与修订号，保证「浏览器另存 → 再导入」可识别回 Azodoc 血统。

### 7.4 TXT：最后一道恢复

- 标题 → 文字 + `====/----` 下划线；列表 → 编号/符号；表格 → 逐行 `标签: 内容`；代码块 → 缩进保留；unknown → `[未能显示的内容: <summary>]`。
- 固化在容器里：`compatibility/text/document.txt` 随每次保存刷新。任何拿到 `.azodoc` 的人，`unzip -p x.azodoc compatibility/text/document.txt` 即得全文——这就是草案 §2.3/§23 的物理落点。

---

## 8. 后续转换策略

### 8.1 DOCX（M4）：Pandoc 进程桥接

```text
docx ──► pandoc -t json ──► Pandoc AST ──► 映射层 ──► Prima + 损失报告
Prima ──► Pandoc AST ──► pandoc -f json -o docx
```

- 自研 DOCX 直解（OOXML parts/relationships）预计 4–6 人周起步且质量难达标；Pandoc 的 docx reader 是目前开源最佳。桥接层把 Pandoc AST 的子集映射到 Prima，Pandoc AST 表达不了的原 OOXML 部件 → `preserved/docx/`（草案 §21 的 PRESERVED_RAW 政策）。
- 许可：Pandoc 为 GPL-2.0+；以**独立子进程**调用并在桥接 crate 做可选 feature（`--features pandoc`），Athanor 核心许可不受传染。用户未装 Pandoc → 功能优雅缺席 + 明确提示，核心 MVP 不受影响。
- DOCX 直读/直写（native OOXML reader）列为 v0.3+ 的 RFC 议题，不在本路线图承诺。

### 8.2 PDF（M5）：出版管线

首选路径：**Prima →（theme）→ 语义 HTML → Paged.js 分页 → headless Chromium 打印 → PDF**。

- 理由：M2 已有高质量 HTML writer，出版管线复用它；Paged.js 解决分页/页眉页脚/页码（草案 §27.4 正是此用途）；Chromium 打印到 PDF 免费且确定性好。
- 备选后端：Typst（`Prima → Typst 标记生成器`）。排版质量上限更高、可重现性强，但需要维护一套代码生成器。作为 `--engine typst` 可选后端保留，M5 做一次对比评估后定主推。
- 确定性要求（§4.8）：同输入同版本 → `layout_hash` 一致；Chromium 打印路径需锁定版本并在 publication.json 记录引擎指纹。

---

## 9. 里程碑与验收标准

| 里程碑 | 内容 | 验收标准（可执行） | 粗估* |
|---|---|---|---|
| **M0 规范冻结** | `spec/` 四份规范 + JSON Schema + 3 个黄金 `.azodoc` 样例 | 样例可被 7-Zip/Explorer/unzip 直接打开；Schema 校验样例通过；你签署确认 | 1 周 |
| **M1 容器+模型** | azodoc-model / azodoc-container / CLI（new/info/verify/recover） | ①任意 v1 文件读→写字节级稳定（确定性字段除外）②forward-compat fixture 的未知条目/字段往返保真（R1/R2）③损坏文件全走友好报错（R4）④schemars 生成 Schema 与 spec/ 目录 CI 比对一致 | 2–3 周 |
| **M2 MVP 转换** | azodoc-md / azodoc-html / 报告引擎 / 兼容缓存 | ①语料库 `md→azodoc→md` 与 `html→azodoc→html` 归一化恒等 ②每条映射表行的损失级别有对应用例断言 ③`--to txt` 黄金文件 ④stale 缓存标记与 upgrade 生效 | 3–4 周 |
| **M3 修订+语义** | 快照链 / history / checkout / annotations 读写 | AI/人工修订均落链；checkout 后 content 与快照哈希一致；标注经「编辑后重定位」用例存活率 ≥ 目标 | 2 周 |
| **M4 DOCX** | Pandoc 桥接 import/export + OOXML 保留 | 标准测试 docx 集合导入后报告零 unsupported 误报；无法建模部件 100% 落 preserved/ | 3–4 周 |
| **M5 PDF 出版** | Paged.js+Chromium 管线 / publication.json / signature 位 | 同输入重复出版 layout_hash 稳定；出版记录与 PDF sha256 互相锚定 | 2–3 周 |
| **M6 编辑器起步** | Aludel 原型（ProseMirror schema 由 azodoc-model 生成） | 编辑→保存→verify 通过；编辑器状态与 Prima 双向无损（以 M2 语料为准） | 另行规划 |

\* 粗估按一名全职工程师；仅供排期感知，不做承诺。

---

## 10. 测试与验收体系（把 Degradation Test 变成代码）

草案 §30 要求每个新功能回答「六种目标 + 未知软件各自表现如何」。落地为三层测试资产：

1. **语料库 `corpus/`**：Markdown/HTML/Azodoc 样例集，每个样例绑定期望损失档案（expected-loss profile）。
2. **往返恒等测试**：`A → azodoc → A'` 归一化后必须与 A 相等；`azodoc → A → azodoc'` 则必须与原文件模型层恒等（idempotence）。
3. **前向兼容夹具**：手工构造的 v2 样例（含未知条目、未知字段、未知节点）——v1 工具读入再保存后，未知内容必须字节/字段级保真（验证 R1/R2/R3）。
4. **友好失败套件**：截断文件、坏魔数、坏 CRC、ZIP 炸弹（超限比/超限解压尺寸一律拒绝）、路径穿越条目——全部必须产出四段式友好报错而非 panic。
5. **跨工具 ZIP 矩阵**：7-Zip / Windows Explorer / Info-ZIP / Python zipfile / Java / macOS Archive Utility 对前置 Header 文件的打开实测（M1 验收项，决定是否启用 Profile B）。

新提案节点的评审清单直接引用 §30 的表：六个目标格式各填 NONE/PARTIAL/DEGRADED/UNSUPPORTED/PRESERVED_RAW，填不出就不能进 schema——这条写进 `spec/` 的贡献指南。

---

## 11. 草案第 31 节清单对照

| 草案要求项 | 本方案落点 |
|---|---|
| 1 Node Schema | §4.4 类型表 + spec/azodoc-model.md + JSON Schema |
| 2 ID / Reference System | §4.5 |
| 3 Content Model | §4.4（Prima） |
| 4 Asset Model | §4.10 |
| 5 Semantic Model | §4.6（W3C Web Annotation 选择器） |
| 6 Presentation Model | §4.7 |
| 7 Publication Model | §4.8 |
| 8 Revision Model | §4.9 |
| 9 Loss / Degradation Model | §4.11 |
| 10 Compatibility Model | §4.11 + §7 映射表 + §8 |
| 11 ZIP Container Layout | §4.1 + §4.2 |
| 12 Manifest Schema | §4.3 |
| 13 Serialization Rules | §4.12（R1–R7） |
| 14 DOCX Mapping | §8.1（M4） |
| 15 HTML Mapping | §7.3（M2） |
| 16 Markdown Mapping | §7.2（M2） |
| 17 PDF Mapping | §8.2（M5） |

---

## 12. 草案八原则对照

| 草案原则 | 兑现机制 |
|---|---|
| P1 未知软件不得毁内容 | 前置 Header + ZIP 自身兼容 + R1/R2/R4 + `compatibility/` 恢复通道 |
| P2 不支持≠丢弃 | R3 + `unknown` 节点 + `preserved/`（唯一例外：安全条款 §4.11） |
| P3 内容是唯一真源 | `document/content.json` 单真源；R5 禁止转换器触碰非兼容层 |
| P4 出版是冻结态 | §4.8：Publication 绑定修订 + 渲染器指纹 + 双哈希 |
| P5 兼容表示可丢弃 | `compatibility/` 全量可删重建，`athanor upgrade` |
| P6 一切降级可检测 | R6 + conversion-report + summary.loss 计数 |
| P7 高级特性可安全丢弃 | R1/R2 保证未知层无损穿透；各层文件可整层移除仍为合法容器 |
| P8 去掉一切高级特性文档仍有用 | TXT 兜底（§7.4）+ content.json 本身即纯内容 |

---

## 13. 风险与对策

| 风险 | 影响 | 对策 |
|---|---|---|
| Windows Explorer 等工具对 ZIP 前缀的兼容性边缘案例 | 「改名 .zip 可读」承诺受损 | M1 实测矩阵；不行即切 Profile B（无前缀 plain-zip） |
| Pandoc GPL 传染顾虑 | 核心许可 | 子进程隔离 + 可选 feature；不装 Pandoc 时功能优雅缺席 |
| Chromium 打印的确定性不足 | layout_hash 不稳 | 锁版本 + 引擎指纹入 publication.json；必要时切 Typst 后端 |
| comrak/html5ever 往返保真度不足 | M2 恒等测试失败 | 语料库驱动迭代；映射表即测试清单，缺口显式降级而非掩盖 |
| 规范与代码漂移 | 格式失信 | schemars 生成 Schema 与 spec/ 目录 CI 比对（M1 起） |
| 编辑器需求过早反向绑架模型 | 重演 HTML 历史包袱 | M6 前不写编辑器；ProseMirror schema 从模型生成而非反之 |
| 数学/复杂内容跨格式衰减 | 用户体验 | v1 明示 DEGRADED 政策；KaTeX/Typst 内嵌列为后续 RFC |

---

## 14. 下一步行动

1. ~~确认 D4 命名提案~~ —— 已确认：**Azodoc / `.azodoc`**（2026-09-14）。
2. ~~M0 规范包~~ —— 已交付并签署（2026-09-14）：`spec/` 四份正式规范 + 8 份 JSON Schema + 3 个黄金样例。
3. ~~M1 容器+模型核心~~ —— 已交付（2026-09-14）：CLI new/info/verify/recover，26 项测试全绿。
4. **M2 已交付（2026-09-15）**：azodoc-convert / azodoc-md / azodoc-html + CLI import/transmute/upgrade，59 项测试全绿；M2 四项验收全部通过（往返恒等 / 映射断言 / TXT 黄金 / stale+upgrade），对照见 `engine/README.md`。规范增补 v1.0.1：兼容条目 `content_sha256` 岔度检测（spec/azodoc-package.md §6）。下一步进入 **M3**：修订层 + 语义层。

---

> 本方案的状态：待评审。文中标注「提案/建议」的条目（D4–D6、快照策略、CSS 子集样式等）均可在评审中推翻；标注「已定」的条目来自 2026-09-14 的决策沟通。
