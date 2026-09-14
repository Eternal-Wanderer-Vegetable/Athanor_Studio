# SDOC Document Model v0.1

> **Status:** Draft / Design Proposal  
> **Project:** SDOC — Structured Document Container  
> **Version:** 0.1  
> **Purpose:** 建立一种兼容 Markdown、HTML、DOC/DOCX、PDF 等现有文档生态，同时具备结构化编辑、版本控制、语义标注、出版状态和 AI 原生能力的新型单文件文档格式。

---

# 1. 设计目标

SDOC 的目标不是成为“另一种 Markdown”，也不是试图取代 DOCX、HTML 或 PDF。

SDOC 的核心目标是：

> **建立一个独立于具体编辑器和输出格式的结构化文档模型，并以单文件容器保存该模型及其相关资源。**

SDOC 应同时具备：

- Markdown 的可读性与语义性
- HTML 的丰富表现能力
- DOCX 的可编辑性与 Office 生态兼容能力
- PDF 的确定性出版能力
- Git 式 Revision 能力
- AI 可理解、可操作的结构化语义
- 单文件、自包含资源能力
- 面向未来扩展的插件与 Schema 机制

---

# 2. 核心设计原则

## 2.1 Content First

文档内容必须独立于具体渲染器。

```text
Content
    ↓
Presentation
    ↓
Renderer
    ↓
Output
```

不得将 HTML、PDF、DOCX 等任一具体输出格式作为 SDOC 的唯一真源。

---

## 2.2 One File, Structured Container

用户看到的是：

```text
document.sdoc
```

内部则是一个结构化 Container。

推荐采用 ZIP-based Container。

```text
document.sdoc
│
├── manifest.json
├── document/
├── presentation/
├── publication/
├── semantics/
├── revisions/
├── assets/
├── metadata/
└── compatibility/
```

ZIP 仅负责物理封装，不定义 Document Model 本身。

---

## 2.3 Graceful Degradation

这是 SDOC 的核心原则之一。

> **任何不理解 SDOC 的软件，都不应因为 SDOC 包含未知结构而导致崩溃、异常行为或静默数据损坏。**

当外部软件无法完整理解 SDOC 时，应尽可能提供：

1. 可读取的正文；
2. 可读取的附件；
3. 可识别的兼容表示；
4. 明确的“不支持 SDOC”提示；
5. 推荐的打开方式；
6. 在可能的情况下提供自动转换入口。

---

## 2.4 Never Silently Lose Data

SDOC 不要求所有旧格式都能无损表达。

但是：

> **任何无法表达的内容都不得在转换过程中静默丢失。**

转换器应采用以下策略：

```text
Fully Supported
        ↓
    Preserve

Partially Supported
        ↓
   Preserve what is possible
        +
   Mark degradation

Unsupported
        ↓
 Preserve original representation
        +
 Report unsupported feature
```

这一原则与 OpenDoc 当前强调的 loss-aware 设计高度一致。OpenDoc 明确提出，语义模型暂时无法表示的内容应被保留、原样重现或报告，而不能静默丢弃。

---

# 3. Compatibility Model

SDOC 不要求第三方软件直接理解 `.sdoc`。

兼容性定义为多个等级。

## Level 0 — Native

软件原生理解 SDOC。

能够访问：

- Content
- Presentation
- Semantic Annotation
- Revision
- Publication
- Asset
- Metadata

---

## Level 1 — Rich Document

通过转换为：

- DOCX
- ODT
- HTML

等格式进行编辑。

目标：

> 最大限度保持文档结构和编辑能力。

---

## Level 2 — Publication

转换为：

```text
PDF
```

目标：

> 最大限度保持视觉布局和出版状态。

---

## Level 3 — Web

转换为：

```text
HTML
```

目标：

> 任何现代浏览器都能够阅读。

---

## Level 4 — Source

转换为：

```text
Markdown
```

目标：

> 保留尽可能多的语义内容。

---

## Level 5 — Plain Text

转换为：

```text
TXT
```

目标：

> 即使用户使用最基础的文本查看器，也能获得正文内容。

---

# 4. Unknown Application Behavior

SDOC 规范必须考虑“不认识 SDOC 的软件”这一情况。

## 4.1 不允许依赖扩展名改变应用行为

SDOC 不应要求：

```text
改名为 .zip
```

才能恢复内容。

---

## 4.2 Container Header

文件开头应存在稳定、可识别的 SDOC Header。

建议：

```text
[SDOC Signature]
[Format Version]
[Container Information]
[Manifest Locator]
```

具体字节结构在 Binary Container Specification 中定义。

---

## 4.3 Friendly Failure

如果软件不支持 SDOC：

```text
Unsupported document format: SDOC

This file is a Structured Document Container.

Recommended applications:
- SDOC-compatible editor
- Web importer
- DOCX exporter
- PDF exporter
```

应尽可能让用户知道：

> **文件没有损坏，只是当前软件不认识这种格式。**

---

## 4.4 Recovery Path

未来应提供官方：

```text
sdoc-open
sdoc-convert
sdoc-export
```

工具。

例如：

```bash
sdoc export document.sdoc --format pdf
sdoc export document.sdoc --format docx
sdoc export document.sdoc --format html
sdoc export document.sdoc --format markdown
```

---

# 5. Document Model

SDOC 的核心不是文件，而是 Document Model。

```text
Document
│
├── Metadata
├── Semantics
├── Sections
│
├── Presentation
├── Publication
│
├── Revisions
└── Assets
```

---

# 6. Document

`Document` 是整个文档模型的根节点。

建议属性：

```text
Document
├── id
├── schema_version
├── title
├── language
├── metadata
├── sections
├── semantics
├── presentation
├── publication
├── revision
└── assets
```

Document ID 必须稳定。

---

# 7. Section

Section 表示文档的逻辑章节。

```text
Document
└── Section
    ├── Section
    ├── Paragraph
    ├── Table
    └── Figure
```

Section 不等同于 PDF Page。

必须明确：

```text
Section ≠ Page
```

Section 属于 Content Layer。

Page 属于 Publication Layer。

---

# 8. Block Model

Block 是文档中的基本块级结构。

第一版建议支持：

```text
Paragraph
Heading
Quote
List
ListItem
CodeBlock
Table
Figure
Image
HorizontalRule
MathBlock
Callout
Embed
```

未来可扩展：

```text
Equation
Diagram
Form
Citation
Footnote
Endnote
Media
Widget
```

---

# 9. Inline Model

Inline 用于表达文本内部结构。

```text
Text
Link
Emphasis
Strong
Underline
Strike
Code
Mention
Citation
FootnoteReference
InlineImage
InlineMath
```

---

# 10. Schema

Document Model 必须具有明确 Schema。

Schema 定义：

- 节点类型
- 节点属性
- 父子关系
- 内容约束
- 可选字段
- 默认值
- Extension Point

ProseMirror 的 Schema 机制是重要参考。ProseMirror 为每个文档定义 Schema，并通过 Node Type 和 Content Expression 约束节点及其嵌套关系。

SDOC 应吸收这一思想，但不应直接复制 ProseMirror Schema。

---

# 11. Semantic Layer

Semantic Annotation 与视觉表现分离。

例如：

```text
Text:
"Rust"

Semantic:
{
    "type": "ProgrammingLanguage",
    "value": "Rust"
}
```

Annotation 可以附着于：

- Document
- Section
- Block
- Inline
- Asset

属性：

```text
id
type
target
value
confidence
source
author
created_at
```

---

# 12. Presentation Layer

Presentation 描述：

> 内容应该如何表现。

包括：

```text
Theme
Style
Font
Color
Spacing
Border
Background
Alignment
Columns
```

Presentation 不应该修改 Content 的语义。

例如：

```text
Heading(level=1)
```

而不是：

```text
FontSize=32
Bold=true
```

来定义“标题”。

---

# 13. Layout Model

Layout 属于 Presentation/Publication 之间的边界层。

支持：

```text
Page Size
Margins
Columns
Flow
Position
Anchor
Float
Wrap
Break
Widow/Orphan
```

但是：

```text
Logical Content
```

不得依赖具体页面坐标才能存在。

---

# 14. Publication Layer

Publication 表示：

> 文档某个 Revision 的冻结出版状态。

```text
Document
    ↓
Publish
    ↓
Publication
```

Publication 可以记录：

```text
source_revision
renderer_version
timestamp
page_count
content_hash
layout_hash
signature
```

Publication 不应该成为唯一内容源。

---

# 15. Revision Layer

Revision 与 Publication 必须分离。

```text
Revision
    ↓
Editable history

Publication
    ↓
Frozen output
```

Revision 可以包含：

```text
id
parent
author
timestamp
changes
message
```

未来允许：

```text
Human
AI
Importer
Converter
System
```

作为不同 Author Type。

---

# 16. Asset Model

所有外部资源都应拥有稳定 Asset ID。

```text
asset://image-001
asset://font-002
asset://attachment-003
```

Asset 可以是：

```text
Image
SVG
Font
Audio
Video
Binary Attachment
Embedded Document
```

Asset 元数据：

```text
id
mime
size
hash
filename
relationship
embedded/external
```

---

# 17. Compatibility Representation

Compatibility Representation 是 SDOC 非核心、但极其重要的一层。

```text
compatibility/
├── html/
├── markdown/
├── text/
├── docx/
└── pdf/
```

这些内容均属于：

> Derived Representation

不是 Source of Truth。

---

# 18. Compatibility Cache

每一个 Compatibility Representation 都应绑定 Revision：

```json
{
    "format": "pdf",
    "revision": "rev-42",
    "path": "compatibility/pdf/document.pdf"
}
```

如果：

```text
Current Revision != Cached Revision
```

则缓存标记为：

```text
stale
```

而不是继续宣称其代表当前文档。

---

# 19. HTML Compatibility

HTML 是最重要的开放兼容目标之一。

SDOC 应提供：

```text
SDOC
 ↓
HTML
```

转换。

HTML 应尽可能：

- 自包含
- 不依赖外部网络
- 使用标准 HTML
- 使用标准 CSS
- 使用稳定资源 URL
- 尽可能内嵌 Asset

这样：

```text
Chrome
Edge
Firefox
Safari
```

均可以阅读。

---

# 20. Markdown Compatibility

Markdown 用于：

- Source
- Git
- IDE
- 文本编辑器
- AI Pipeline

但 Markdown 并不是 SDOC 的完整表达能力。

无法用 Markdown 表达的内容必须：

```text
Preserve
+
Annotate
```

而不能静默丢失。

---

# 21. DOCX Compatibility

DOCX 是 SDOC 的重要 Import/Export Target。

```text
DOCX
 ↓
DOCX Importer
 ↓
SDOC Model
```

以及：

```text
SDOC
 ↓
DOCX Exporter
 ↓
DOCX
```

导入时应尽可能保留：

- Paragraph
- Heading
- Table
- Image
- Style
- Header
- Footer
- Comment
- Revision
- Relationship
- Unknown OOXML parts

无法转换的 OOXML 内容应：

```text
Preserve original representation
```

或：

```text
Create UnsupportedFeature annotation
```

而非直接丢失。

A3S Office 当前的架构尤其值得研究：其结构化模型与 HTML compatibility representation 同时存在，并明确计划向 loss-preserving OOXML package state 发展。

---

# 22. PDF Compatibility

PDF 主要作为：

> Publication Target

而不是 Editable Source。

```text
SDOC
 ↓
Publication
 ↓
PDF
```

PDF 应最大限度保持：

- Page
- Font
- Image
- Vector
- Layout
- Header
- Footer
- Page Number

---

# 23. Plain Text Compatibility

即使所有高级能力全部丢失：

```text
SDOC
 ↓
TXT
```

仍然应该能够得到：

```text
标题

正文。

列表：

1. Item A
2. Item B
```

这是 SDOC 的最后一道内容恢复机制。

---

# 24. Loss Classification

SDOC 应定义标准 Loss Classification。

```text
NONE
PARTIAL
DEGRADED
UNSUPPORTED
PRESERVED_RAW
```

例如：

```text
DOCX feature:
Tracked Changes

SDOC:
Fully supported

→ NONE
```

而：

```text
DOCX feature:
Advanced DrawingML effect

SDOC:
Not represented

→ PRESERVED_RAW
```

如果无法保存：

```text
→ UNSUPPORTED
```

并产生明确报告。

---

# 25. Conversion Report

任何跨格式转换都可以产生：

```text
conversion-report.json
```

例如：

```json
{
    "source": "document.docx",
    "target": "document.sdoc",
    "status": "partial",
    "warnings": [
        {
            "feature": "DrawingMLEffect",
            "status": "preserved_raw"
        },
        {
            "feature": "CustomField",
            "status": "degraded"
        }
    ]
}
```

用户不需要阅读它。

但高级用户、编辑器、自动化工具和 AI 可以读取它。

---

# 26. Reference Architecture

```text
                         SDOC
                          │
             ┌────────────┴────────────┐
             │                         │
        Container                 Document Model
             │                         │
             │          ┌──────────────┼──────────────┐
             │          │              │              │
             │      Semantic        Content      Presentation
             │          │              │              │
             │          │         Section/Block      Style
             │          │              │              │
             │          │            Inline          Layout
             │          │
             │          └──────────────┬──────────────┘
             │                         │
             │                    Publication
             │                         │
             │                       Page
             │
             ├── Assets
             ├── Revisions
             ├── Metadata
             └── Compatibility
                     │
       ┌─────────────┼─────────────┐
       ▼             ▼             ▼
     DOCX           HTML          PDF
       │             │             │
       └─────────────┼─────────────┘
                     ▼
               Markdown / TXT
```

---

# 27. Reference Projects

SDOC 不应重复发明已经成熟的概念。

以下项目作为设计参考。

## 27.1 Pandoc

核心参考：

> Universal document conversion + intermediate AST

```text
Input
 ↓
AST
 ↓
Output
```

SDOC 应借鉴其：

- Reader/Writer architecture
- AST
- Format independence
- 多格式转换

但 SDOC 的 Document Model 必须比 Pandoc AST 更丰富，因为需要支持 Revision、Layout、Publication、Asset 和 Semantic Annotation。

---

## 27.2 ProseMirror

核心参考：

> Structured editable document model

ProseMirror 使用 Schema、Node、Content Expression 构建结构化文档模型，并支持 HTML DOM serialization。

SDOC 可以借鉴：

- Node
- Schema
- Content constraints
- Inline/Block distinction
- Serialization boundary

但不能让浏览器 DOM 成为 SDOC 的 Source of Truth。

---

## 27.3 Typst

核心参考：

> Layout + Publication

重点研究：

- Layout Engine
- Pagination
- Typography
- PDF generation
- Compiler architecture

Typst 更接近 SDOC 的 Publication/Presentation Layer。

---

## 27.4 Paged.js

核心参考：

> HTML → paginated publication

重点研究：

- CSS Paged Media
- Page breaking
- Header/Footer
- Print layout

---

## 27.5 OpenDocument / ODF

核心参考：

> Structured ZIP-based document package

重点研究：

- Package
- Manifest
- Metadata
- Assets
- Relationships

---

## 27.6 OOXML / DOCX

核心参考：

> Office-grade document package architecture

重点研究：

- Parts
- Relationships
- Styles
- Themes
- Embedded assets
- Revision/comment representation
- Compatibility behavior

---

## 27.7 CasualOffice OpenDoc

核心参考：

> Loss-aware document engine

尤其关注：

```text
Unsupported
    ≠
Delete
```

OpenDoc 当前明确采用“未知内容保留或报告，而非静默丢失”的设计原则。

---

## 27.8 A3S Office

这是目前与 SDOC 总体方向最值得重点研究的项目之一。

其当前架构已经涉及：

```text
Structured Document Model
+
HTML Compatibility Representation
+
Pagination
+
DOCX
+
PDF
+
Markdown
+
Revision
+
Comments
```

并且明确将 HTML 与结构化模型作为不同层次处理，而不是让 HTML 单独成为永久 Source of Truth。

尤其值得研究：

- `WorkDocumentModel`
- HTML compatibility representation
- deterministic snapshot
- OOXML preservation
- pagination
- loss-preserving package state

---

# 28. SDOC 与现有项目的关系

SDOC 不应该试图取代：

```text
Pandoc
ProseMirror
Typst
Paged.js
OpenDoc
A3S Office
```

而应该形成：

```text
                    SDOC
                      │
        ┌─────────────┼─────────────┐
        │             │             │
      Import        Core          Export
        │             │             │
     Pandoc       SDOC Model      Pandoc
     DOCX         Rust Core       DOCX
     HTML                         HTML
     ODT                          PDF
                                  Markdown
```

未来甚至可以：

```text
SDOC Editor
    │
    ├── ProseMirror-inspired editing model
    ├── Typst-inspired layout engine
    ├── Paged.js-compatible web publication
    ├── OpenDoc-inspired loss-aware import
    └── Pandoc-compatible conversion ecosystem
```

---

# 29. 最重要的设计原则

SDOC v0.1 应正式确立以下原则：

### Principle 1

> **Unknown software must not destroy document content merely because it does not understand SDOC.**

### Principle 2

> **Unsupported semantics must never be silently discarded.**

### Principle 3

> **Content is the source of truth; rendered formats are representations.**

### Principle 4

> **Publication is a frozen state, not the editable document itself.**

### Principle 5

> **Compatibility representations are disposable and regenerable.**

### Principle 6

> **Every degradation should be detectable.**

### Principle 7

> **Advanced SDOC features must be safely discardable.**

### Principle 8

> **The document must remain useful even when all advanced features are removed.**

---

# 30. Compatibility as a Design Test

以后任何 SDOC 新功能，都应该通过：

> **Degradation Test**

例如增加一个新节点：

```text
InteractiveDiagram
```

必须回答：

```text
SDOC Editor       → 完整支持
DOCX              → ?
HTML              → ?
PDF               → ?
Markdown          → ?
TXT               → ?
Unknown Software  → ?
```

如果不能回答，就不能加入核心规范。

---

# 31. v0.1 下一阶段

SDOC v0.1 不应该马上进入完整实现。

下一阶段应完成：

```text
1. Node Schema
2. ID / Reference System
3. Content Model
4. Asset Model
5. Semantic Model
6. Presentation Model
7. Publication Model
8. Revision Model
9. Loss / Degradation Model
10. Compatibility Model
11. ZIP Container Layout
12. Manifest Schema
13. Serialization Rules
14. DOCX Mapping
15. HTML Mapping
16. Markdown Mapping
17. PDF Mapping
```

完成以上内容后，才进入：

```text
Rust Core Prototype
```

---

# 32. 最终目标

SDOC 不应成为：

> “又一种文档格式”。

它应该成为：

> **一种能够容纳多种文档表示，并允许文档在不同生态之间安全降级和恢复的结构化文档容器。**

最终目标：

```text
                         SDOC
                           │
       ┌───────────────────┼───────────────────┐
       │                   │                   │
     Editor              Browser            Office
       │                   │                   │
     Native               HTML               DOCX
       │                   │                   │
       └───────────────────┼───────────────────┘
                           │
                         PDF
                           │
                     Publication
                           │
                   ┌───────┴───────┐
                   │               │
                Markdown          TXT
                   │               │
                   └───────┬───────┘
                           │
                     Content survives
```

最终原则：

> **格式可以降级，表现可以降级，编辑能力可以降级，语义可以部分降级；但文档本身不应该因为生态不兼容而消失。**

---

# Appendix A — 当前重点参考项目

| 项目 | 参考领域 | SDOC 关注重点 |
|---|---|---|
| Pandoc | 文档转换 | AST / Reader / Writer |
| ProseMirror | 编辑器 | Schema / Node / Document Model |
| Typst | 排版 | Layout / Publication |
| Paged.js | Web 出版 | Pagination / Print |
| OpenDocument | 文档格式 | Package / Manifest |
| OOXML | Office | Parts / Relationships |
| OpenDoc | DOCX Engine | Loss-aware Model |
| A3S Office | Office Runtime | Structured Model / HTML / PDF / DOCX |
| unified | 文档处理 | AST Transformation |

其中 A3S Office 当前公开架构尤其值得重点跟踪：它已经明确把结构化模型、HTML compatibility representation、页面布局、DOCX package state 和 PDF 输出放在不同边界上。

---

# Appendix B — 当前结论

SDOC 项目目前不应宣称：

> “世界上没有类似项目。”

更准确的描述是：

> **现有开源生态已经分别解决了文档 AST、编辑模型、Office 兼容、排版、PDF 出版、格式转换和 loss-aware preservation 等问题；SDOC 的创新目标是将这些能力统一到一个独立的、单文件、结构化、可版本化、可降级的文档模型中。**

这也是 SDOC 最值得努力的地方。