# Azoth/Aludel Future Work — 可选项与后续计划（存档）

> **Status:** Archived roadmap options — **not scheduled**; none of this is being executed in the current work stream.
> **Status（中文）:** 存档的未来计划——**未排期**；本文所列内容均不在当前工作流中执行。
> **Date archived:** 2026-09-15 · **Baseline:** M0–M5 complete (`git log`: 627147d), 77 tests green
> **Primary doc:** 《Azodoc v0.1 — 落地方案》（同目录）；本文是其"下一步"候选池的正式存档。

---

## 说明（如何使用本文）

落地方案 §9 的里程碑 M0–M5 已全部交付。本文把"可选后续"从对话记忆固化成文档，
每项记录：**背景 / 价值 / 验收要点 / 规模预估 / 前置依赖**。重启任一项时，
以此为起点写 RFC 或直接开里程碑即可，不需要考古历史对话。

预估规模按一名熟悉本代码库的工程师标注：S ≤ 1 周，M ≈ 1–3 周，L ≈ 1 月+。

---

## A. 产品方向类

### A1. Aludel 编辑器原型（M6 候选）

- **背景**：命名表中的编辑器。路线图 M6 一直悬置——"编辑器需求过早会反向绑架模型"，
  现在 M0–M5 已完成，模型足够稳定，值得接受这次实战检验。
- **方案**：ProseMirror schema **从 `azodoc-model` 类型生成**（而非手工维护），
  编辑器状态与 Prima 双向同步；保存走 `Container::commit`（AI/人工作者类型天然落链）。
- **价值**：格式从"能被机器读写"升级为"能被人编辑"；修订层/语义层/损失报告第一次
  面向真人暴露，是模型质量的最严苛检验。
- **验收要点**：M2 语料在编辑器中打开→编辑→保存→`athanor verify` 通过；
  编辑会话产生 `author: human` 修订；标注随编辑自动重定位。
- **规模**：L。**前置**：无硬前置；建议同时做 C1（schemars 权威化）以支撑 schema 生成。

### A2. 语义层 AI 管线演示（Prima ↔ LLM 示例）

- **背景**：草案的差异化卖点之一是"AI 原生"：修订作者类型有 `ai`、语义层允许
  `confidence/source` 标注。目前引擎提供了机制，缺一个端到端示例证明价值。
- **方案**：示例程序读取 `.azodoc` → 把纯文本喂给 LLM → 结构化输出（概念标注 +
  改写建议）→ `annotate`（`author: ai:…`, confidence）+ `commit`（`author: ai`）→
  人工 review 后 checkout/upgrade。一个 ~200 行的示例 crate 或 `examples/`。
- **价值**：给"AI 可理解、可操作"提供可运行证据；是文档、演示、以及未来 GUI 的素材。
- **验收要点**：示例跑通全流程；每次 AI 落链均可 `history` 审计；AI 标注带 confidence。
- **规模**：S。**前置**：无。

### A3. Athanor Studio GUI（原命名表中的"炼金工坊"）

- **背景**：仓库即以此命名，但 GUI 一直未启动。
- **方案**：依赖 A1（Aludel 编辑核心）。技术栈建议 Tauri（Rust 核心 + Web 前端），
  复用未来 Web 编辑器组件。
- **规模**：XL，独立立项。**前置**：A1。

---

## B. 转换与格式类

### B1. Paged.js / CDP 出版集成（页码、运行头）

- **背景**：M5 基线用 Chromium 原生 `--print-to-pdf`（无页码边盒/运行头）。
  Paged.js 注入已实测存在竞态：CLI 的 `--virtual-time-budget` 与 Paged.js 异步分页
  完成时机不匹配 → 产出空白页（tools/pagedjs/paged.polyfill.js 与
  `--paged` 实验代码均已就位但默认关闭）。
- **方案**：放弃 CLI 标志，改用 **Chrome DevTools Protocol**：`Page.printToPDF`
  前可注入 `window.PagedConfig = { auto: true }` 并等待 `pagedjs:rendered` 事件
  再打印。Rust 侧可用 `chromiumoxide`/`headless_chrome` crate，或最小化自实现
  （WebSocket + 两条 CDP 命令）。Paged.js 资产已在 `tools/pagedjs/`。
- **价值**：页码、`@page` 边盒（运行头/页脚）、封面页——出版品质的关键一步。
- **验收要点**：多页文档页脚含页码；分页点由 Paged.js 决定；确定性验收沿用 M5
  （layout_hash 稳定）；产出非空白。
- **规模**：M。**前置**：无。

### B2. DOCX 原生 OOXML 读取 RFC（摆脱 Pandoc）

- **背景**：M4 走 Pandoc 桥（质量好但有外部依赖 + AST 有损：样式/批注/tracked
  changes/页眉页脚被丢）。落地方案 §8.1 已把"原生读取"列为 v0.3+ RFC 议题。
- **方案**：直接解析 OOXML（`word/document.xml` + relationships + styles.xml）。
  现在比 M4 前更有利：容器已有 `preserved/` 机制与 `zip` 依赖，无法建模的 OOXML
  部件可整件进 `preserved/docx/`（甚至原始 `.docx` 整包留档）。
- **价值**：去掉外部依赖；样式→presentation 层、批注→语义层、tracked changes→
  修订层的**真正映射**（Pandoc 桥永远做不到）。
- **验收要点**：落地方案 §21 映射表逐行落实；styles→theme 类映射；
  批注→annotations；tracked changes→修订链；现有 Pandoc 路径保留为 fallback。
- **规模**：L。**前置**：建议先做 C4（活动内容政策已在 loss 规范定好）。

### B3. HTML 导出增强：KaTeX 数学与语义标注呈现

- **背景**：M2 中数学在 HTML 导出降级为 `data-latex` 文本（DEGRADED）；
  语义标注不参与呈现。
- **方案**：a) 内嵌 KaTeX（自包含约束下需内嵌字体/WASM，注意容器体积）；
  b) 可选模式把语义标注渲染为高亮/悬浮卡（像 Google Docs 评论）。
- **验收要点**：公式在无网环境可读；标注呈现不影响 `athanor verify`；
  体积增量可配置。
- **规模**：M（a）/ S（b）。**前置**：无。

### B4. DOCX 导出的 reference-doc 样式模板

- **背景**：M4 导出的 docx 用 Pandoc 默认样式。Pandoc 支持 `--reference-doc`。
- **方案**：theme.json 增加 docx 样式映射（heading 样式名、正文字体等），
  publish/transmute 时以用户模板出稿。
- **规模**：S。**前置**：无。

---

## C. 工程债与规范演进类（M0–M5 期间记录在案）

### C1. Schemars JSON Schema 权威化

- **背景**：M1 验收 ④ 采取"双向接受性测试"（黄金样例在生成 Schema 与手写 spec
  Schema 下都通过），因为 schemars 产物与手写规范结构不同。
- **方案**：把 `schema_gen` 产物打磨到与手写规范逐字段等价（或反过来以生成物为
  规范源、手写件退役），CI 比对从"接受性"升级为"结构性 diff"。
- **规模**：S–M。**前置**：无。

### C2. 跨工具 ZIP 矩阵接入 CI（Java `ZipFile` 项）

- **背景**：M0 验收矩阵中 Java 项一直未跑（本机无 Java CI 环境）。
  spec/azodoc-container.md §9 记录了这一悬置。
- **方案**：加一个 Java 冒烟用例（读 Header 前缀 + 解包 manifest），接入任意
  CI；同时补 macOS 归档实用工具人工验证记录。
- **规模**：S。**前置**：需要 CI 环境。

### C3. 诊断美化：引入 miette

- **背景**：M1 起四段式错误为自绘字符串。miette 可给源码级诊断（错误位置
  下划线标注），对 import 的语法错误尤其有用。
- **规模**：S。**前置**：无。

### C4. `--preserve-raw-active` 取证开关

- **背景**：loss 规范 §3 已定义：活动内容默认移除并报告，取证开关使其进入
  `preserved/`（隔离保存，仍不渲染）。M2 的 HTML/DOCX 导入器未实现该开关。
- **验收要点**：开关开启后 script 原文入 `preserved/`、HTML 导出仍不渲染
  （消毒规则不变）、报告 action 为 `preserved_quarantined`。
- **规模**：S。**前置**：无。

### C5. 修订层 delta 模式与快照裁剪

- **背景**：v1 只做快照模式（`policy.mode: "delta"` 是保留枚举位）。
  长文档高频保存下快照体积线性增长；`max_snapshots` 裁剪策略也未实现。
- **方案**：delta = 与父快照的结构化 diff（块 ID + 文本三级）；裁剪策略需
  先在 spec 层定义"不可破坏冻结出版引用的快照"。
- **规模**：M。**前置**：建议 M6 编辑器落地后按真实保存频率设计。

### C6. 孤儿资产垃圾回收

- **背景**：R7 资产不可变 + 宁多勿删，导致孤儿资产永久滞留（verify 仅提示）。
- **方案**：显式命令 `athanor gc --dry-run`：扫描所有修订快照与出版记录引用，
  仅清理"任何历史都不可达"的资产；默认 dry-run，`--apply` 才真删。
- **规模**：S–M。**前置**：无。

### C7. 许可证统一

- **背景**：仓库根 LICENSE 为 **AGPL-3.0**（初始提交带入），而 engine crates
  声明 `MIT OR Apache-2.0`——当前处于不一致状态。README 已如实标注。
- **待决问题**：a) 引擎走哪个许可（MIT/Apache 利于生态采纳；AGPL 利于
  Copyleft 护栏——注意 Pandoc 桥接已被进程隔离，不受 GPL 传染，两种选择都可行）；
  b) 根 LICENSE 与 crate 声明对齐；c) 是否需要 NOTICE / 第三方清单
  （zip/html5ever/comrak 等传递依赖）。
- **规模**：S（决策 + 文件调整）。**前置**：项目所有者拍板。

### C8. `athanor info` 增强：损失档案与出版历史汇总

- **背景**：verify 会聚合损失摘要，但 `info` 不显示；出版历史需单独看
  publication.json。
- **方案**：info 增加两段：历史出版列表（id/时间/页数/layout_hash）与
  全部转换报告的损失汇总。
- **规模**：S。**前置**：无。

---

## D. 已记录但不建议近期做的

| 项 | 原因 |
|---|---|
| PDF 字节级确定性（去除 CreationDate） | Chromium 元数据在压缩流中，剥离易损坏文件；layout_hash 已按输入定义，覆盖验收需求 |
| 自研 ZIP 写入器替代 `zip` crate | 现有依赖工作正常；raw-copy 保真已由测试锁定 |
| 段内 DisplayMath 顺序精确保持 | 当前"段后追加"策略损失极小且已报告；精确保持需要块切分器 |
| mention/cite 的结构化后端 | v1 定义为不透明字符串；等参考文献管理需求出现再设计 |

---

## 建议的重启顺序（当未来某天继续时）

1. **C7 许可证统一**（半天，解决仓库法律状态的不一致）
2. **A2 AI 管线示例**（小而亮眼，验证"AI 原生"叙事）
3. **B1 Paged.js/CDP 出版**（补齐出版品质）
4. **A1 Aludel 编辑器**（最大的产品价值，附带 C1/C5 的自然动力）
5. **B2 原生 OOXML**（长期依赖瘦身）

> 本文由 2026-09-15 的工作会话存档。重启任一项时：以落地方案 §9 的验收风格
> 为每项立"可执行验收标准"，沿用 §10 测试体系（语料/恒等/夹具/友好失败）。
