# Azodoc Prima Content Model Specification v1.0-draft

> 规范 `document/content.json`：文档树的节点目录、ID 与引用系统、纯文本化算法。
> Prima = 内容唯一真源；不包含样式、页面与任何表现概念（那些属于 presentation 层，见 azodoc-package.md）。
> Schema：[json-schema/content.schema.json](json-schema/content.schema.json)。

---

## 1. 文件形态

```json
{
  "schema_version": "1.0",
  "content": [ <Node>, … ]
}
```

- `content` 是顶层节点数组，元素为 Section 或 Block；空数组合法（空文档）。
- 节点通用形态：`{ "id": <ID>, "type": <类型名>, …类型专属字段 }`。
- 行内内容（Inline）以**嵌套 span 数组**表达：容器 span 持有 `content` 数组，叶子 span 持有 `text`/`latex` 等载荷。不使用「纯文本 + 偏移标注」表示法。

## 2. 冲突裁决

JSON Schema（content.schema.json）是本规范的机器可校验投影。两者不一致时，**以本规范为准**，并视为 Schema 缺陷立即修复（M1 起 CI 保证同步，见落地方案 §13「规范与代码漂移」）。

---

## 3. ID 系统

### 3.1 形态

```text
<前缀>_<26 位 ULID>
```

前缀表（封闭集，新增需 minor 版本）：

| 前缀 | 节点 |
|---|---|
| `doc_` | 文档（manifest.document.id） |
| `sec_` | section |
| `blk_` | 其余一切 Block（含 heading、paragraph、unknown 等） |
| `li_` | list_item |
| `row_` / `cel_` / `col_` | 表格行 / 单元格 / 列 |
| `as_` | 资产（assets/registry.json） |
| `ann_` | 语义标注 |
| `rev_` | 修订 |
| `pub_` | 出版记录 |
| `iss_` | 转换报告 issue |

ULID 采用 Crockford Base32 字母表（`0-9` + `A-Z` 去 `I L O U`），26 位。生成器 SHOULD 使用单调 ULID（同毫秒内递增），使 ID 排序 ≈ 创建顺序。

### 3.2 义务

1. ID 在**文档全生存期**（含全部修订历史）内唯一、稳定、永不复用。
2. 导入器在建模时立即分配 ID；此后任何编辑/转换/回滚都 MUST 保留既有 ID。
3. 删除节点 ≠ 回收 ID：被删 ID 不得再次分配。
4. 「另存为副本」「从修订恢复」MUST 保留原 ID；「从内容重新生成新文档」才允许全量重编 ID（生成器 MUST 记录这是一次新 `doc_` 诞生）。

## 4. 引用协议

| 引用对象 | 语法 | 示例 |
|---|---|---|
| 资产 | `asset://<as_id>`，可带文件名提示 | `asset://as_01JAVG7X4C6E8G0H2J4K6M8P0R2T4/photo.jpg` |
| 文档内部 | URL 片段 | `#blk_01JAVG3QK2M4N6P8R0T2V4X6Z8` |
| 外部资源 | 原样 URL | `https://example.com/` |

- `asset://` 文件名提示部分仅供人读，解析以 ID 为准。
- link 的 `url` 是 URI-reference；`#…` 片段目标 MUST 是本容器内存在的节点 ID（verify 可校验，悬空片段为错误）。

---

## 5. Section 与大纲

```json
{ "id": "sec_…", "type": "section", "children": [ <Section|Block>, … ] }
```

- section 是纯组织节点，没有自身文本。
- **大纲（outline）由 heading 块序列唯一决定**；渲染器不得依赖 section 才能产生正确大纲。section 是编辑器/导入器的组织便利。
- 约定（SHOULD）：导入器遇 heading 时把该 heading 及后续同级内容装入新 section，按 heading 层级嵌套；section 首子节点通常为 heading，但**允许无 heading 的 section**（前言、附录包裹）。
- section.children 深度无上限；工具 SHOULD 能处理 ≥ 32 层。

---

## 6. Block 目录（v1）

### 6.1 段落与标题

**paragraph** — `content: Span[]`

**heading** — `level: 1–6`，`content: Span[]`
- 文档 SHOULD 恰有一个 `level: 1` heading 作为题目（惯例，不强制）；层级 MUST 不跳级使用（h1 之后直接 h3 为坏味道，工具 MAY 警告）。

### 6.2 引用块

**quote** — `children: Block[]`（嵌套引用 = quote 内含 quote）

### 6.3 列表

**list** — `style: "ordered" | "bullet"`，`start?: integer ≥ 1`（ordered 专用，默认 1），`items: ListItem[]`

**list_item** — `checked?: boolean`（任务列表），`children: Block[]`
- children 允许嵌套 list（实现多级列表）；
- `checked` 出现在 `style: "bullet"` 的 list 中即任务列表语义；
- ordered + checked 合法但罕见，写回 GFM 时以任务列表优先。

### 6.4 代码

**code_block** — `language?: string`，`text: string`
- `text` 是唯一载荷（不使用 Span），MUST 保留原始换行与空白；`language` 为自由 token（如 `rust`）。
- 内部不再有任何子节点。

### 6.5 表格

```json
{
  "id": "blk_…", "type": "table",
  "columns": [ { "id": "col_…", "name": "块类型", "width": 2 }, … ],
  "header_row": true,
  "rows": [
    { "id": "row_…", "cells": [
        { "id": "cel_…", "column": 0, "colSpan": 1, "rowSpan": 1, "role": "header", "children": [ … ] }, …
    ] }
  ]
}
```

- `column` 是该单元格**起始列下标（0 起）**——配合 colSpan/rowSpan 可无歧义重建网格；
- `colSpan`/`rowSpan` ≥ 1，默认 1；
- `columns[].width`：可选非负整数，**相对宽度单位**（仅比例有意义，非像素/磅等绝对单位）；它是表格结构属性的有限例外（见 §11），缺失时按等宽。删除列时 MUST 同步移除对应 `columns` 项；编辑器拖拽列宽写回本字段。
- `cells[].role`：可选 `"header" | "body"`，默认 `"body"`；与 `header_row` 正交——`header_row: true` 声明首行是表头，`role: "header"` 声明任意单元格的表头角色（如首列作表头）。
- `header_row: true` 表示首行是表头；
- 单元格 children 为 Block[]（允许段落、列表甚至嵌套表格）；
- 网格重建算法（normative）：建立 R×C 矩阵，按 rows 顺序、每行按 `column` 升序放置，遇占位跳过——与 HTML 表格布局规则一致。

**不规则旧表（冻结规则）**：网格重建中单元格相互重叠、`column`/`colSpan`/`rowSpan` 越界或与 `columns` 数量不一致的表称为**不规则表**。读取器 MUST 原样读入并展示为可诊断状态（工具给出诊断提示），MUST NOT 静默重写其结构；任何修复（拆分、重排、补列）只能在用户显式操作下发生，且 MUST 可撤销。verify 对不规则结构输出警告级诊断（`table.irregular`），不判错误。

### 6.6 图

**figure** — `asset: <asset-url>`，`alt: string`，`caption?: Span[]`（带题注的正式图）

**image** — `asset: <asset-url>`，`alt: string`（简单图，无题注）

### 6.7 分隔与数学

**horizontal_rule** — 无附加字段

**page_break** — 无附加字段（手动分页符；连续视图为渲染 hint，印刷/DOCX 导出为真实分页）

**math_block** — `latex: string`（LaTeX 数学子集）

### 6.8 提示块

**callout** — `variant: "note" | "tip" | "warning" | "important"`，`children: Block[]`

### 6.9 内嵌

**embed** — `asset: <asset-url>`
- 资产 mime 决定呈现（图片/音视频/PDF/二进制附件/内嵌文档）；`relationship` 元数据见 assets 层。

### 6.10 脚注

**footnote** — `children: Block[]`；由行内 `footnote_ref`（§7）按 `id` 引用。
- footnote 块 SHOULD 集中置于 content 末尾（惯例）；
- 引用解析按 ID 全局查找，**与位置无关**；悬空 `footnote_ref` 为 verify 错误。

### 6.11 unknown（保真包装节点）

见 §8。

---

## 7. Inline 目录（v1）

| 类型 | 字段 | 嵌套 | 语义 |
|---|---|---|---|
| `text` | `text: string` | 叶子 | 普通文本；换行不得存于此（用 hard_break） |
| `hard_break` | — | 叶子 | 硬换行 |
| `strong` | `content: Span[]` | 容器 | 强调（粗） |
| `em` | `content: Span[]` | 容器 | 强调（斜） |
| `underline` | `content: Span[]` | 容器 | 下划线 |
| `strike` | `content: Span[]` | 容器 | 删除线 |
| `code` | `text: string` | 叶子 | 行内代码；内部不解释任何标记 |
| `link` | `url: string`，`title?: string`，`content: Span[]` | 容器 | 超链接 |
| `footnote_ref` | `id: <footnote 的 blk_id>` | 叶子 | 脚注引用 |
| `inline_math` | `latex: string` | 叶子 | 行内数学 |
| `inline_image` | `asset: <asset-url>`，`alt: string` | 叶子 | 行内图 |
| `mention` | `target: string` | 叶子 | 提及；v1 中 target 为不透明字符串（约定 `person:<id>` 等，规范不定义） |
| `cite` | `key: string` | 叶子 | 引用键；参考文献管理是未来扩展 |
| `unknown` | 同 §8 | 叶子（`content: []`） | 保真包装 |

嵌套规则：

- 容器 span 可任意嵌套（`strong > em > link` 合法）；
- `code`、`inline_math`、`text`、`hard_break`、`footnote_ref`、`inline_image`、`mention`、`cite` 为叶子，MUST 无 `content`；
- 同义嵌套（如 `strong > strong`）合法但工具 MAY 在写回时展平（展平属 NONE 级往返，因为语义不变）。

---

## 8. unknown 节点（R3 的载体）

无法建模的内容**绝不丢弃**，包装为：

```json
{
  "id": "blk_…",
  "type": "unknown",
  "origin": "html",
  "loss_class": "preserved_raw",
  "summary": "自定义元素 <x-chart>（交互式图表）",
  "payload_ref": "preserved/html/part-0001.html",
  "content": []
}
```

| 字段 | 约束 |
|---|---|
| `origin` | 来源格式 token（`html`/`docx`/`ooxml`/…） |
| `loss_class` | `"preserved_raw"`（payload 已存）或 `"unsupported"`（无法/依安全政策不存） |
| `summary` | 单行人读摘要；MUST 足以让用户判断要不要恢复它 |
| `payload_ref` | `loss_class == "preserved_raw"` 时**必须**存在，指向 preserved/ 文件 |
| `content` | 恒为 `[]`（保持节点形态统一，占位渲染由此节点自身完成） |

- unknown 块与 unknown span 同构（span 版出现在某 block 的 `content` 数组中）。
- 块级 unknown 参与 outline/TXT 导出的占位规则见 azodoc-loss.md §5。
- 工具 MUST NOT「顺手清理」unknown 节点；只有用户显式操作（编辑器删除、专用降级命令）才可移除，且 SHOULD 提示。

---

## 9. 纯文本化算法（Plain Text Extraction）

本算法是 normative 的，供三处使用：TXT 兼容导出、语义层 text_quote 锚定的匹配基准、`athanor info` 预览。

### 9.1 span 级

```text
text        → 其 text 原文
hard_break  → "\n"
code        → 其 text
inline_math → 其 latex
inline_image→ 其 alt
mention     → 其 target
cite        → 其 key
footnote_ref→ ""
link/strong/em/underline/strike → 递归拼接其 content
unknown     → ""
```

### 9.2 block 级

```text
paragraph/heading        → span 级拼接
quote                    → 各子块文本以 "\n" 连接
list                     → 各 item 子块文本以 "\n" 连接
code_block               → 其 text
math_block               → 其 latex
figure/image             → 其 alt
table                    → 逐行：单元格文本以 "\t" 连接；行间 "\n"
callout                  → 各子块文本以 "\n" 连接
footnote                 → 各子块文本以 "\n" 连接
embed                    → ""
horizontal_rule          → ""
unknown                  → ""
```

### 9.3 文档级

块序列以 `\n\n` 连接即为「文档纯文本」。TXT 导出的排版增强（标题下划线、列表编号等）在本算法结果上叠加，且增强不得改变算法产出的字符序列语义（增强规则见 azodoc-loss.md §5.4）。

---

## 10. 锚定配合约定（供语义层）

语义标注（annotations.json）的 `text_quote` 选择器以**本文件 §9 的纯文本**为匹配基准：

- `prefix + exact + suffix` 三元组拼接 MUST 在目标 block 纯文本中**恰好出现一次**（匹配位置唯一性由写入标注的工具负责）；
- `prefix`/`suffix` SHOULD 提供 ≥ 4 个字符的上下文（可用时）；
- 编辑发生后选择器失配（因文本改变）时，工具 SHOULD 尝试模糊重定位（仅 exact 子串匹配），失败则将标注标记为 `detached`（该状态字段由未来 minor 引入，v1.0 中失配标注保持原样并警告）。

---

## 11. 表现字段的缺席声明（负面清单）

Prima 中 **MUST NOT 出现**以下概念；出现即 Schema 校验失败：

- 字体、字号、颜色、行距、对齐、边距等视觉属性（属 presentation/theme）；
- 页码、页面尺寸、绝对坐标（属 publication/layout）；
- 任何「渲染器指令」类字段。

有限例外：`table.columns[].width` 的相对宽度单位（§6.5）与 Block 的 `style` 软引用（§11 末段）——两者都是结构属性而非视觉属性，渲染器可安全忽略。

Block MAY 携带 `style: ["<类名>"]` 引用 theme 中定义的样式类（软引用；theme 缺席时安全忽略——这满足草案 Principle 8「去掉高级特性仍有用」）。
