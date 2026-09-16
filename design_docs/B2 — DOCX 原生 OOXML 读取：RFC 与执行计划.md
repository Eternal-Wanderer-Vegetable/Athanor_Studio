# B2 — DOCX 原生 OOXML 读取：RFC 与执行计划

> **Status:** **已交付**（2026-09-16，P0–P4 全阶段；全仓 169 测试绿）。本文存档自
> 《Future Work — 可选项与后续计划》§B2 的重启规划，含完整 RFC、OOXML → Azodoc
> 映射表、分阶段计划与各阶段实施记录；**后续修订直接改本文**。
> **上游依据:** 《Azodoc v0.1 — 落地方案》§8.1（"DOCX 直读列为 v0.3+ 的 RFC 议题"）
> + 《Future Work — 可选项与后续计划》§B2 + SDOC 草案 §21（DOCX Compatibility）。
> **基线:** M0–M5 + C7/A2/B1/A1(M6) 已交付，CI 双平台，全仓 114 测试绿。

---

## 0. TL;DR

在 `azodoc-docx` 内新增 `ooxml` 原生读取模块（不建新 crate）：直接解析
`word/document.xml` + relationships + styles.xml + numbering.xml 等部件，
建立**样式→presentation 层、批注→语义层、tracked changes→修订层**的真正映射
（Pandoc 桥永远做不到的三条）。CLI `athanor import` 增加 `--reader auto|native|pandoc`
（默认 auto：原生优先，硬解析失败回落 Pandoc 并在报告中显式记录）；
Pandoc 保留为导出路径与导入 fallback。分四阶段（P1 包遍历器与正文核心映射 →
P2 表现层与保留部件 → P3 语义层与修订层 → P4 加固与验收），总量 L。

**对 GUI（A3）的意义**：原生读取是纯进程内解析（zip + XML，无外部二进制），
Tauri/GUI 无需随应用分发 Pandoc（~50MB）即可导入 DOCX；`azodoc_docx::import`
的库形态签名不变，GUI 直接调用；行为跨平台确定，进度可观测（无黑盒子进程）。

---

## 1. 验收标准（可执行）

Future Work §B2 验收要点 + 落地方案 §10 测试体系合并：

| # | 验收 | 落点 |
|---|---|---|
| ① | `corpus/docx/` 语料（basic + lossy）原生导入后 `athanor verify` 通过 | P4 e2e |
| ② | §4 映射表逐行有对应损失级别断言（每行至少一个用例） | P1–P3 单元测试 |
| ③ | 批注落 `semantics/annotations.json`、通过 annotations schema 校验、可被 `relocate_annotations` 重定位 | P3 |
| ④ | styles→theme 产物通过 theme schema 校验；块挂 `style` 类引用 | P2 |
| ⑤ | tracked changes：终稿视图导入 + 原始 document.xml 整件 preserved + 聚合报告 | P3 |
| ⑥ | 无 Pandoc 环境下原生路径全部测试绿（原生测试不依赖 Pandoc） | P1–P4 |
| ⑦ | `--reader pandoc` 回退路径既有 M4 测试不回归 | P1 |
| ⑧ | 友好失败套件：截断 zip / 缺 `[Content_Types].xml` / 加密条目 / OOXML 内路径穿越 → 四段式报错而非 panic | P4 |
| ⑨ | 确定性：同字节 docx → 同 content.json（黄金比对，`AZODOC_WRITE_GOLDEN` 模式） | P4 |

沿用落地方案 §10 测试体系：语料恒等（含幂等）、前向兼容夹具（R1/R2）、友好失败。

---

## 2. 调研结论：引擎侧现有能力（直接复用）

| B2 需要的能力 | 现状 | 位置 |
|---|---|---|
| 导入契约 | `ImportJob`（idgen/assets/preserved）/`ImportOutput`/`LossLog` | `azodoc-convert/src/lib.rs:187/228/75` |
| preserved 机制 | `add_preserved` → `preserved/<origin>/part-NNNN.ext`；origin token `docx/ooxml` 已被 package spec §5 预留 | `azodoc-convert/src/lib.rs:219` |
| DOCX 当 ZIP 读 | `inventory.rs` 已用 zip 8.6 打开 docx 清点部件（原生遍历器的种子） | `azodoc-docx/src/inventory.rs:33` |
| XML 解析 | **quick-xml 0.42 已在 Cargo.lock**（plist 传递依赖），直接引用零新增传递重量 | `engine/Cargo.lock` |
| 内容树工具 | `wrap_sections` / `merge_text_spans` / `plain_text_of_block`（text_quote 锚定基准） | `azodoc-convert/src/lib.rs:458/509/634` |
| 确定性 ID | `IdGen`（黄金测试可重现）；资产 id 走 `AzodocId::generate`（随机，既有约定） | `azodoc-convert/src/lib.rs:138` |
| 标注重定位 | `relocate_annotations`（unchanged/reanchor/moved/detached） | `azodoc-convert/src/semantics.rs:38` |
| 修订落链 | `Container::commit(CommitInfo)`（snapshot 模式，自动块 ID diff） | `azodoc-container/src/revisions.rs:101` |
| 表现层 schema | `spec/json-schema/theme.schema.json`（styles[].name 限 `^[a-z][a-z0-9-]*$`） | spec/ |
| 语义层 schema | `spec/json-schema/annotations.schema.json`（text_quote 选择器） | spec/ |
| CLI 装配 | `cmd_import` 全流程格式无关，docx 分支在 `lib.rs:312` | `athanor-cli/src/lib.rs` |

**已知缺陷（顺手修复）**：docx 导入的报告/落链 converter 名落入 `_` 臂，
误用 `athanor-html`（`athanor-cli/src/lib.rs:409-412, 483-486`）；transmute 的 docx
报告误用 `athanor-txt`。B2 落地时改为：原生读取 `athanor-docx`、Pandoc 路径
`athanor-docx-pandoc`，报告可审计地区分两条读取路径。

---

## 3. 核心架构决策（RFC）

### 决策 1：`azodoc-docx::ooxml` 子模块，而非新 crate

DOCX 关注点单处收敛（原生读取 + Pandoc 桥 + 清点），CLI 装配与 crate 数量不变；
C7 许可证统一（AGPL-3.0-only）后，Pandoc 的 GPL 隔离已非许可问题，无需 feature 门。
否决备选：新 `azodoc-ooxml` crate（依赖隔离收益小，装配面反而变大）。

### 决策 2：XML 栈 = quick-xml 0.42 + 内部轻量元素树

OOXML 处理需要多遍访问（样式解析、编号解析、关系解析后再处理正文），
pull 事件流不够，故以 quick-xml 事件流构建一次轻量树（前缀 + local-name 匹配，
保留属性限定名），部件级复用。quick-xml 不展开自定义实体 → XXE/十亿笑话攻击面
天然极小。否决备选：roxmltree（新依赖，传递重量不为零）、xml5ever（HTML 语义，
非命名空间严格）。

### 决策 3：包模型 = OoxmlPackage 三层解析

`zip` 打开 → `[Content_Types].xml`（Default/Override → 部件类型）→
`_rels/.rels` + `word/_rels/document.xml.rels`（rId → target/TargetMode）。
每个部件的去向三选一：**建模映射**（进 Prima/annotations/theme）、
**整件 preserved**（`preserved/docx/`）、**已知支持部件静默忽略**
（fontTable/theme1/webSettings 等无用户内容的部件）；未知部件一律 preserved + 报告。

### 决策 4：损失报告 = R6 宁过度报告 + 按 feature 聚合

直排格式的千处小损失（如每处 `w:highlight`）聚合成单条事件（detail 带计数与样本），
防报告爆炸；每个被评估节点仍计数（`node_none`/`record`）。

### 决策 5：批注 → 语义层（首个写 semantics 层的导入器）

`word/comments.xml` + `commentRangeStart/End/Reference` → annotation：
`type: "docx_comment"`、target 优先 `text_quote`（exact/prefix/suffix 取自块内
锚定区间的 §9 纯文本），跨块锚定降级 block target、无法锚定降级 document target
（各发 Partial 事件）；`w:author` → `author{type:"human", id}`、`w:date` →
`created_at`（缺失回退导入时刻）、评注文本 → `value.text`。
`ImportOutput` 增加 `annotations: Vec<Value>`（默认空），`cmd_import` 在非空时写
`semantics/annotations.json` 并登记 manifest.layers.semantics。

### 决策 6：tracked changes → 修订层（分步）

- **P3 核心**：内容取**终稿视图**（`w:ins` 内容保留、`w:del`/`w:delText` 内容剔除、
  `w:moveTo` 保留、`w:moveFrom` 剔除）；原始 `word/document.xml` 整件 preserved；
  聚合事件 `docx_tracked_changes`（Partial/degraded，detail 列作者与计数）。
- **后续扩展（不在 P1–P4 承诺内）**：用现成快照机制落双快照修订链
  （原始视图快照 → 接受快照，`w:author` 落 author 字段）——snapshot 模式即可，
  不碰未实现的 delta 模式。

### 决策 7：styles → presentation 层（首个 theme.json 写入器）

`word/styles.xml` 的被引用样式 → `presentation/theme.json`
（`styles[].{name, applies_to, css_like}`），`docDefaults` → `defaults.base_font/base_size`，
`w:sectPr` 的 pgSz/pgMar（twips→pt）→ `defaults.page`。**slug 规则**：schema 限定
`name` 匹配 `^[a-z][a-z0-9-]*$`，故样式名 slug 化（ASCII 小写连字符；非 ASCII 名
回退 `style-<序号>`），原始名存入额外字段 `title`（schema additionalProperties 允许）；
块/行内挂 `style: ["<slug>"]` 软引用（模型 §11 约定，落在 ExtraMap）。
仅收**实际被引用**的样式。`ImportOutput` 增加 `theme: Option<Value>`（默认 None），
`cmd_import` 在 Some 时写 `presentation/theme.json` 并登记 manifest.layers.presentation。

### 决策 8：页眉/页脚/文本框/OLE 等无法建模部件 → 整件 preserved

- 页眉/页脚部件 → `preserved/docx/` 整件 + `PreservedRaw/preserved` 事件
  （终结现状"清点后报 Unsupported/dropped 但内容真丢"）。
- OLE 对象（`word/embeddings/*`）与宏（`vbaProject.bin`）→ preserved +
  `preserved_quarantined`（detail 注明 security）——即 C4 的 `--preserve-raw-active`
  语义在 DOCX 侧的直接落地；**渲染侧约束不变：preserved 载荷永不参与渲染**。
- 文本框/VML/SmartArt/图表 → preserved + unknown 块/span + 报告。
- `mc:AlternateContent` → 取 `mc:Fallback`（确定性优先，老格式表达力足够），
  无 Fallback 取 Choice，发聚合 Partial 事件。

### 决策 9：数学 OMML → v1 保留 + Degraded

OMML→LaTeX 转换是独立工程（Pandoc 桥恰是数学场景的最优解）；v1 原生读取将
`m:oMath`/`m:oMathPara` 的 OMML 片段整件 preserved + unknown 节点（content[] 带
提取的纯文本），发 `docx_omml` Degraded 事件；数学密集文档建议 `--reader pandoc`。
OMML→LaTeX 子集转换列为后续 RFC。

### 决策 10：确定性与安全

- 遍历按 ZIP 归档序（central directory 序）；输出映射用 Vec/BTreeMap，禁 HashMap
  迭代序外泄 → 同字节输入产出逐字节一致的 content.json（黄金测试锁定）。
- 媒体/部件名消毒：拒绝/消毒含 `..`、绝对路径、盘符的条目名（zip-slip）。
- 尺寸护栏：单部件解压上限 256MB、累计解压上限 1GB、条目数上限 10000
  （超限四段式报错）；加密条目（`encrypted()`）→ 明确报错（无密码求解能力）。
- 错误类型 `OoxmlError` 实现 `friendly()` 四段式（发生了什么 / 文件没有损坏 /
  建议工具 / 恢复命令）。

### 决策 11：CLI 接线 = `--reader auto|native|pandoc`

- `native`：仅原生；`OoxmlError` 直接四段式报错。
- `pandoc`：仅 Pandoc 桥（现行为，M4 测试锁定）；Pandoc 缺席时友好报错。
- `auto`（默认）：原生优先；原生**硬失败**（OoxmlError）且 Pandoc 可用时回落，
  并向报告注入 `docx_reader_fallback` 事件（Partial，detail 带原生失败原因）；
  Pandoc 不可用时返回原生错误的友好信息。
- converter 名：原生 `athanor-docx`；Pandoc 路径 `athanor-docx-pandoc`
  （同时修复 §2 所列 `_` 臂误用）。

---

## 4. OOXML → Azodoc 映射表（SDOC 草案 §21 逐行落实）

判定流程遵循 spec/azodoc-loss.md §1；class ∈ none/partial/degraded/unsupported/preserved_raw。

### 4.1 部件级（word/ 包 → 容器层）

| OOXML 部件 | Azodoc 去向 | class | action |
|---|---|---|---|
| `word/document.xml` 正文 | content.json（§4.2） | — | — |
| `word/styles.xml` | presentation/theme.json + 块 style 引用 | none | — |
| `word/numbering.xml` | 列表映射数据源（§4.2） | none | — |
| `word/comments.xml` + 正文批注锚 | semantics/annotations.json | none | — |
| `word/footnotes.xml` | footnote 块 + footnote_ref | none | — |
| `word/endnotes.xml` | footnote 块（编号语义差异报告） | partial | degraded（`docx_endnotes`） |
| `word/header*.xml` / `word/footer*.xml` | `preserved/docx/` 整件 | preserved_raw | preserved（`docx_headers`/`docx_footers`） |
| `word/media/*` | assets 注册表（内嵌资产） | none | — |
| `word/embeddings/*`、`vbaProject.bin`、activeX | `preserved/docx/`（隔离） | preserved_raw | preserved_quarantined（`docx_ole_object`，security） |
| `word/theme/*`、`fontTable`、`webSettings`、`settings`、`stylesWithEffects` | 支持部件，静默忽略（无用户内容） | — | — |
| `docProps/core.xml` | title/language/created_at/modified_at → DocumentMeta；creator → document.extra | none | — |
| `docProps/app.xml`、`custom.xml` | custom → preserved；app 忽略 | preserved_raw | preserved（`docx_customxml`） |
| `m:oMath`（正文内） | preserved OMML + unknown 节点 | degraded | degraded（`docx_omml`） |
| 未知部件（按 Content-Type 无法归类） | `preserved/docx/` | preserved_raw | preserved（`docx_unknown_part`） |

### 4.2 正文元素级（document.xml → Prima）

| OOXML 构造 | Prima 去向 | class | action |
|---|---|---|---|
| `w:p` + pStyle→Heading1-6 / outlineLvl | heading(1–6) | none | — |
| `w:p` 普通段落 | paragraph（空段丢弃，计数） | none | — |
| run rPr `b/i/u/strike`（w:val≠false） | strong/em/underline/strike | none | — |
| run 其余 rPr（color/sz/fonts/highlight/caps/vertAlign…） | 丢弃并聚合报告 | partial | dropped（`docx_run_props`） |
| `w:tab` | 空格 | partial | degraded（`docx_tab`） |
| `w:br`（textWrapping） | hard_break | none | — |
| `w:br w:type="page"` | horizontal_rule | partial | degraded（`docx_page_break`） |
| `w:hyperlink @r:id`（External） | link{url} | none | — |
| `w:hyperlink @w:anchor` | link{url:"#anchor"} | partial | degraded（`docx_internal_link`） |
| `w:bookmarkStart/End` | 丢弃并聚合报告 | partial | dropped（`docx_bookmark`） |
| `w:fldSimple` / `w:fldChar`+`w:instrText` | 结果文本保留；指令聚合报告 | partial | degraded（`docx_field`） |
| `w:tbl`（gridSpan） | table + colSpan | none | — |
| `w:tbl`（vMerge restart/continue） | table + rowSpan（续标记合并计数） | none | — |
| `w:tblHeader` | header_row + columns[].name | none | — |
| 嵌套 `w:tbl` | table（cell 内直接嵌套——模型 `TableCell.children` 可表达，实施期决议升级为真映射） | none | — |
| `w:drawing`（pic/blip @r:embed） | figure（独段）/inline_image + asset | none | — |
| `wp:anchor` 浮动定位 | 同上映射 + 报告定位丢失 | partial | degraded（`docx_floating_image`） |
| `w:pict` / VML | preserved + unknown span/块 | preserved_raw | preserved（`docx_vml`） |
| `mc:AlternateContent` | 取 Fallback（无则 Choice） | partial | degraded（`docx_alternate_content`） |
| `w:sdt` | 透明解包 sdtContent | partial | degraded（`docx_sdt`） |
| `w:numPr`（numId+ilvl）+ numbering.xml | list(bullet/ordered, start)；多级经 list_item 嵌套表达 | none | — |
| `w:ins`（终稿视图=保留） | 子内容照常映射 | partial | degraded（`docx_tracked_changes` 聚合） |
| `w:del`/`w:delText`/`w:moveFrom`（终稿视图=剔除） | 不产出节点 | partial | degraded（同上聚合） |
| `w:footnoteReference` | footnote_ref + 文末 footnote 块（含 label） | none | — |
| 批注锚 `commentRangeStart/End` + `commentReference` | annotation（text_quote → block → document 降级链） | partial | degraded（跨块 `docx_comment_cross_block`） |
| `w:sectPr`（pgSz/pgMar） | theme defaults.page | none | — |
| 段落直接格式（pPr 非 pStyle 部分） | 聚合报告（样式引用仍挂） | partial | dropped（`docx_paragraph_props`） |

### 4.3 与草案 §21 的对照

| 草案 §21 要求 | 本 RFC 落实 |
|---|---|
| Paragraph / Heading / Table / Image | §4.2 直接建模（none） |
| Style | §4.1 styles.xml → theme.json（真映射，Pandoc 桥做不到） |
| Header / Footer | 整件 preserved + 报告（终结静默丢弃） |
| Comment | semantics/annotations.json（真映射） |
| Revision | 终稿视图 + 原件 preserved + 聚合报告；双快照修订链为后续扩展 |
| Relationship | assets 注册表 / link url / rels 解析 |
| Unknown OOXML parts | preserved + 报告（R3：绝不丢弃） |
| Preserve original representation | `preserved/docx/` 整件机制 |
| UnsupportedFeature annotation | 损失报告 issues[]（loss 规范 §4.2 字段） |

---

## 5. 分阶段计划

| 阶段 | 内容 | 收口 |
|---|---|---|
| P0 | 本 RFC 存档 + Future Work §B2 状态注记 | 提交 |
| P1 | `ooxml` 模块：XML 树 + 包遍历器（Content-Types/rels/消毒/尺寸护栏）+ 正文核心映射（段落/标题/run/链接/列表/表格/图片/脚注）+ `--reader` 接线 + converter 名修复 | 原生路径单元测试绿；M4 测试不回归 |
| P2 | styles→theme.json 写入器 + 块 style 引用；页眉/页脚 preserved；core.xml 元数据 | theme schema 校验测试绿 |
| P3 | comments→annotations（semantics 层写入）+ tracked changes（终稿视图 + 原件 preserved + 聚合报告） | annotations schema 校验 + relocate 测试绿 |
| P4 | `corpus/docx/` 语料 + 黄金测试 + 友好失败套件 + e2e（verify 通过、pandoc 回退不回归）+ README/loss spec token 登记更新 | 全仓 fmt/clippy/test 绿 |

> **实施记录（P0）**：2026-09-16 本 RFC 存档；Future Work §B2 加状态注记。
>
> **实施记录（P1）**：`azodoc-docx::ooxml` 全模块落地（error/xml/package/styles/
> numbering/comments/document/mod）。quick-xml 0.42 由传递依赖转为直接依赖（零新增
> 传递重量）；实体引用按 `Event::GeneralRef` 独立事件解析（0.42 语义），未知实体
> 原样保留不展开。CLI `--reader auto|native|pandoc` 接线；`cmd_import` 保持原签名
> （默认 auto，18 处既有测试调用零改动），新增 `cmd_import_reader`。
>
> **实施记录（P2）**：styles→theme.json 写入器（slug 规则 + `title` 原名 + 仅收
> 被引用样式；sectPr→defaults.page）；页眉/页脚/自定义 XML/未知部件整件 preserved；
> OLE/宏 `preserved_quarantined`；core.xml → DocumentMeta（creator 走 doc_extra，
> 覆盖 created_at/modified_at）。
>
> **实施记录（P3）**：批注 → annotations（text_quote→block 降级链；author/date
> 保留；schema 校验测试）；tracked changes 终稿视图 + 原件 preserved + 按作者聚合
> 事件。`ImportOutput` 增 `annotations`/`theme` 字段；`cmd_import` 装配 semantics/
> presentation 层并登记 manifest。
>
> **实施记录（P4）**：`corpus/docx/{basic,lossy}.docx` + golden（content.json +
> loss 概要，AZODOC_WRITE_GOLDEN 重新生成；夹具由 tools/docx_fixtures/generate.py
> 确定性生成）；友好失败套件（截断 zip / 缺 document.xml / 畸形 XML）；CLI e2e
> （verify 通过、semantics/presentation 层装配、preserved 载荷落容器、converter 名
> 断言、pandoc 回退）。converter 名缺陷修复：导入 `athanor-docx` /
> `athanor-docx-pandoc`，transmute 导出修正为 `athanor-docx`（原误报 athanor-txt，
> 导入曾误报 athanor-html）。
>
> **验收结果**：全仓 169 测试绿（新增 55：b2_ooxml 31 + b2_golden 4 + b2_tests 5 +
> 模块单元 15）；clippy `-D warnings` 清零；rustfmt 通过。token 表最终新增
> `docx_footnote_missing`、`docx_table_edge`（§7 清单之外），已同步 loss 规范 §4.2。

---

## 6. 关键难点与对策

| 难点 | 对策 |
|---|---|
| vMerge 是"续标记"而非跨数 | 逐列扫描：restart 记起点，连续 continue 向下计数 → rowSpan；被合并 cell 不产出 |
| 编号多级/重启语义（numId/abstractNum/lvlOverride） | 仅建模 numFmt→bullet/ordered + start；lvlOverride/重启等边角聚合报告 `docx_numbering_edge` |
| 批注锚定失败率（跨块/悬空） | text_quote → block → document 三级降级链，每级失败发事件；exact 为空不发 text_quote |
| 畸形 XML / 编码 | quick-xml 严格模式，部件级错误聚合为 `OoxmlError::BadXml` 四段式 |
| 跨平台确定性 | 归档序遍历 + Vec/BTreeMap + IdGen；黄金测试双平台 CI 锁定 |
| 中文 Word 样式名（styleId "1"/name "heading 1" 等） | 标题识别优先 outlineLvl，其次 name 正则，最后 styleId 前缀；slug 回退 `style-N` |

---

## 7. 影响面与工作流

- **代码**：`azodoc-docx`（主：ooxml 模块 + import 接线）、`azodoc-convert`
  （轻：ImportOutput 增 `annotations`/`theme` 默认字段）、`athanor-cli`
  （`--reader` flag、semantics/presentation 层装配、converter 名修复）。
- **spec**：无规范性变更（preserved origin token 已预留）；`spec/azodoc-loss.md`
  §4.2 feature token 注册表追加新 token（docx_tracked_changes、docx_omml、
  docx_field、docx_sdt、docx_alternate_content、docx_nested_table、docx_vml、
  docx_run_props、docx_paragraph_props、docx_bookmark、docx_tab、docx_page_break、
  docx_internal_link、docx_floating_image、docx_endnotes、docx_numbering_edge、
  docx_ole_object、docx_customxml、docx_unknown_part、docx_reader_fallback、
  docx_footnote_missing、docx_table_edge——见实施记录 P4）。
- **语料**：新增 `corpus/docx/{basic,lossy}.docx` + golden（确定性生成脚本入库）。
- **CI**：不改（原生测试 Pandoc-free；Pandoc 仍装给 M4 回退测试）。
- **工作流**：实施期每个既有符号编辑前跑 GitNexus impact；提交前
  `detect-changes --scope all`；提交信息一律英文。

---

## 8. v1 明确排除（防范围膨胀）

- 原生 DOCX **写出**（export 仍走 Pandoc 桥；B4 reference-doc 另行推进）。
- delta 修订模式；tracked changes 双快照修订链（决策 6 后续扩展）。
- OMML→LaTeX 转换；SmartArt/图表重建；主题颜色保真（theme1.xml 不读）。
- 宏分析/文档保护解密；`.doc`/`.rtf` 二进制格式。
- 内容控件（sdt）属性语义（仅透明解包）。
