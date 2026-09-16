# M6 — Aludel 编辑器原型：RFC 与执行计划

> **Status:** **M6.1 + M6.2 已交付**（2026-09-16；`azodoc-pm` 19 项测试、`aludel` 7 项测试，
> 全仓 114 测试绿）。本文存档自 Future Work §A1 的重启规划，含完整 RFC、
> Prima ↔ ProseMirror 映射表初版与分阶段计划；**后续修订直接改本文**。
> **下一阶段：** M6.3（ProseMirror 正式前端）。
> **上游依据:** 《Azodoc v0.1 — 落地方案》§9 M6 行 + 《Future Work — 可选项与后续计划》§A1。
> **基线:** M0–M5 + C7/A2/B1 已交付，CI 双平台，全仓 88 测试绿。

---

## 0. TL;DR

新建两个纯增量 crate：`azodoc-pm`（Prima ↔ ProseMirror JSON 双向转换 + schema 生成，
纯 Rust 库）与 `aludel`（本地 server + 内嵌 Web 前端的编辑器原型二进制）。引擎侧
"落链 / 标注重定位 / verify"三大能力全部现成，编辑器只做壳与转换层。分四阶段
（M6.1 转换库 → M6.2 server → M6.3 前端 → M6.4 验收打磨），总量 L（单人约 4–5 周）。

---

## 1. 验收标准（可执行）

落地方案 §9 M6 行 + Future Work §A1 合并，共四条：

| # | 验收 | 落点 |
|---|---|---|
| ① | 编辑器状态与 Prima 双向无损（以 M2 语料为准） | M6.1 roundtrip 恒等测试 |
| ② | M2 语料在编辑器中 打开→编辑→保存→`athanor verify` 通过 | M6.2 e2e + M6.3 人工脚本 |
| ③ | 编辑会话产生 `author: human` 修订 | M6.2 保存管线第 5 步硬编码 |
| ④ | 标注随编辑自动重定位 | M6.2 复用 `relocate_annotations`，UI 呈现统计 |

沿用落地方案 §10 测试体系：语料恒等（含幂等）、前向兼容夹具（R1/R2）、友好失败。

---

## 2. 调研结论：引擎侧现有能力（全部直接复用，零改动）

| A1 需要的能力 | 现状 | 位置 |
|---|---|---|
| 内容模型 | 16 块节点 + 14 行内 span，serde tagged enum，`id` + `ExtraMap`（R2） | `azodoc-model/src/content.rs` |
| 容器读写 | `open`/`set_entry`（自动同步层 sha256）/`write`（未修改零重写、R1 raw_copy） | `azodoc-container/src/lib.rs` |
| 人工修订落链 | `Container::commit(CommitInfo)`，`author_type="human"` 合法；快照模式自动块 ID diff | `azodoc-container/src/revisions.rs` |
| 标注自动重定位 | `relocate_annotations`：unchanged/reanchor/moved/detached 四级，checkout 已在用 | `azodoc-convert/src/semantics.rs` |
| 校验 | `athanor verify` 覆盖 spec §9 全部检查 | `athanor-cli/src/verify_cmd.rs` |
| 客户端驱动范式 | A2 演示：open → 改 content.json → set_entry → write → commit | `athanor-cli/src/ai_demo.rs` |
| Schema 生成基架 | schemars `schema_for!(ContentFile)` + 黄金比对测试模式 | `azodoc-model/src/schema_gen.rs` |
| 新块 ID | `AzodocId::generate(IdKind)`（`<前缀>_<Crockford ULID>`） | `azodoc-model/src/id.rs` |

**结论：A1 的新增量只有两块——① Prima ↔ ProseMirror JSON 转换层；② 薄壳（server + 前端）。**

---

## 3. 核心架构决策（RFC）

### 决策 1：转换逻辑放 Rust 侧，前端是"哑终端"

ProseMirror 文档模型是稳定且公开的 JSON 格式（`{type, attrs, content, marks, text}`）。
**Rust 侧承担 Prima ↔ PM-JSON 全部转换**；前端只做 `Schema.fromJSON(生成物)` + PM 编辑 +
把 PM-JSON 原样 POST 回来。

- 理由：全部逻辑落入现有 `cargo test` / CI 体系（恒等/夹具/友好失败沿用 §10），
  不引入 JS 测试基建；单一事实源，杜绝 TS 侧模型副本漂移——"schema 从模型生成
  而非反之"（落地方案 §13 风险表）由此落地。
- 否决备选：转换放 TS 编辑器侧（需重建模型知识，正是要避免的漂移源）。

### 决策 2：薄壳用本地 HTTP server，而非 WASM / Tauri

新增二进制 `aludel <doc.azodoc>`：仅监听 `127.0.0.1`、仅允许启动时传入的文档路径，
serve 静态前端 + 三个 API（open / save / verify）。HTTP 层最小自实现
（`std::net` + 线程，约 200 行；先例：`azodoc-pdf/src/cdp.rs`），不扩大依赖面。

- 否决备选：WASM 编译容器（`Container` 进出都是 `Vec<u8>`，理论可行，但 zip/flate2
  wasm 特性 + 本地文件保存是新增风险面）；Tauri（属 A3）。
- **A3 复用性**：本决策产出的 Rust API 面（open/save/verify handler）将来直接平移为
  Tauri command，Web 前端原样复用——正是 A3 声明的前置依赖形态。

### 决策 3：schema 生成 = "手写映射表 + 类型一致性锁"

嵌套 span → PM flat marks 无法 100% 机械推导（需人工判定哪些 span 是 mark）。
务实解法：

```
azodoc-model 类型（唯一事实源）
  → schemars JSON Schema（现成：schema_gen::content_schema）
  → 手写映射表（唯一人工输入点：engine/crates/azodoc-pm/src/mapping.rs）
  → 生成 gen/aludel-schema.json（PM Schema.fromJSON 兼容）
  → 生成 gen/schema.mjs（ES module，供前端 import）
```

锁死机制（三道测试）：

1. **一致性测试**：映射表 `prima_fields` ↔ schemars 枚举变体字段逐一比对
   （每个 KNOWN 节点/span 都有映射；映射表跟不上类型改动 → CI 红）；
2. **黄金测试**：`gen/` 产物入库，测试重新生成并逐字节比对（复刻 `golden_schema.rs` 模式）；
   重新生成命令：`cargo run -p azodoc-pm --bin generate`；
3. **恒等测试**：M2 语料与 spec 黄金样例 Prima→PM→Prima 往返恒等（§5）。

C1（schemars 权威化）**不作为硬前置**：一致性测试就地提供"生成物与类型不漂移"的保证；
C1 剩余部分（生成 Schema vs 手写 spec 的结构性 diff）仍留独立项。

### 保存管线（A1 的心脏，M6.2 实现）

```
POST /api/save {pm_doc, message?, author_id?}
 1. pm_to_prima：PM-JSON → content.json（缺失/重复/非法 ID 按 IdKind 补发）
 2. azodoc_model::validate 值级校验，不合规拒绝并返回定位
 3. c.set_entry("document/content.json", pretty_bytes)
 4. 复用 relocate_annotations_layer → RelocateStats          ← 验收④
 5. c.commit(CommitInfo { author_type: "human", .. })        ← 验收③硬编码
 6. c.write() → 回写文件
 7. 返回 {revision, relocate_stats, verify_warnings}         ← 验收②的 UI 入口
```

`/api/open` 返回 PM 文档 + 标注列表（detached 高亮）+ 修订历史（author 徽标）+
unknown 块损失摘要——修订层/语义层/损失报告第一次面向真人的出口。

### ID 稳定规则（冻结于 M6.1，spec 级约定）

- 每个 Prima 节点的 `id` 成为 PM node attr，随编辑保持；
- 保存时发现 缺失/空/非法（非 `<前缀>_<26 位 Crockford>`）/重复 ID → 按 IdKind 补发：
  section=sec，list_item=li，table_row=row，table_cell=cel，其余块=blk；
- **段落拆分约定：前半保留原 ID，后半补发新 ID**（块级标注随前半）；
- ID 全文档唯一，扫描顺序 = 文档顺序；footnote_ref.id 是引用而非身份，不参与补发。

---

## 4. 映射表初版（M6.1 已按此实现；修订先改 `mapping.rs` 再同步本文）

### 4.1 约定（全部双向转换的实现规则）

1. **attrs 全量显式**：PM 节点/标记声明了 attrs 时，JSON 永远写全所有 attr
   （含默认值），键序 = 声明序——与 PM `doc.toJSON()` 行为一致，前端产物可直接比对；
   读取侧缺失 attr = 取默认值。
2. **`extra` 透传**：除 `doc`/`text` 外所有节点、全部 mark 声明 `extra` attr
   （ExtraMap JSON，默认 null）——R2 未知字段保真的载体；编辑器不理解、不修改。
3. **doc 层**：`schema_version` 与 `ContentFile.extra` 存于 `doc` 节点 attrs。
4. **marks 数组**：按 rank 升序、再按名称排序；`content/marks` 键序沿用 PM
   `toJSON` 顺序（type → attrs → content → text → marks）。
5. **嵌套 span ↔ flat marks**：load 展开；save 按 rank 重建嵌套（见 4.3）。
6. **text 合并**：相邻同 marks 文本合并为单节点/单 span（两侧同规则，规范形）。
7. **未知内容**：`unknown` 块/span → 只读 PM 节点（UI 层禁编辑），payload_ref 等原样。

### 4.2 块节点映射（PM 名 ← Prima type）

| PM 节点 | group / content | attrs（默认值） | Prima 结构性字段→转换 |
|---|---|---|---|
| `doc` | — / `block*` | schema_version("1.0"), extra | ContentFile 顶层 |
| `section` | block / `block*` | id, extra | children→content |
| `paragraph` | block / `inline*` | id, extra | content→inline |
| `heading` | block / `inline*` | id, level(1), extra | content→inline |
| `quote` | block / `block*` | id, extra | children→content |
| `list` | block / `list_item*` | id, style("bullet"), start(null), extra | items→content |
| `list_item` | block / `block*` | id, checked(null), extra | children→content |
| `code_block` | block / `text*` · code | id, language(null), extra | text→text 子节点 |
| `table` | block / `table_row*` | id, header_row(null), columns(null=JSON 数组), extra | rows→content；columns→attr |
| `table_row` | — / `table_cell*` | id, extra | cells→content |
| `table_cell` | — / `block*` | id, column(0), colSpan(null), rowSpan(null), extra | children→content |
| `figure` | block / 原子 | id, asset(""), alt(""), caption(null=span JSON 数组), extra | caption→attr（v1 不可视编辑） |
| `image` | block / 原子 | id, asset, alt, extra | — |
| `horizontal_rule` | block / 原子 | id, extra | — |
| `math_block` | block / 原子 | id, latex(""), extra | — |
| `callout` | block / `block*` | id, variant("info"), extra | children→content |
| `embed` | block / 原子 | id, asset, extra | — |
| `footnote` | block / `block*` | id, extra | children→content |
| `unknown_block` | block / `inline*` | id, origin(""), loss_class(""), summary(""), payload_ref(null), extra | content→inline |

容器一律 `*`（零或多）而非 `+`：Prima 允许空 children/items/rows，转换层必须无损
（编辑交互的约束属 M6.3 关注点）。`table_row`/`table_cell` 不入 block 组，防止 cell
出现在任意块位。`figure.caption` 与 `table.columns` 以 JSON attr 透传（v1 只读）。

### 4.3 行内映射：marks（rank = 重建嵌套序，小者在外层）

| PM mark | rank | attrs | Prima span |
|---|---|---|---|
| `link` | 0 | url(""), title(null), extra | Link |
| `strong` | 1 | extra | Strong |
| `em` | 2 | extra | Em |
| `underline` | 3 | extra | Underline |
| `strike` | 4 | extra | Strike |
| `code` | 5（最内/叶） | extra | Code（text 叶，不可包裹子 span） |
| `span_extra` | 99（隐形） | data(null=JSON) | **合成**：Text.extra 的载体（内部用） |

重建算法：文本片断按 rank 升序排序 marks → 最内层为 `Code{text}`（有 code 时）否则
`Text{text}`（extra=span_extra.data）→ 从最内 rank 向外逐层包裹容器 span
（Strong/Em/Underline/Strike 带 content；Link 带 url/title/content）。
例：`[link, strong, code]` → `Link[Strong[Code[text]]]`。

### 4.4 行内原子节点

| PM 节点 | attrs | Prima span |
|---|---|---|
| `text` | —（PM text 无 attrs） | Text（extra 经 `span_extra` mark） |
| `hard_break` | extra | HardBreak |
| `footnote_ref` | id（引用，不补发）, extra | FootnoteRef |
| `inline_math` | latex, extra | InlineMath |
| `inline_image` | asset, alt, extra | InlineImage |
| `mention` | target, extra | Mention |
| `cite` | key, extra | Cite |
| `unknown_span` | origin, loss_class, summary, payload_ref(null), extra；content `inline*` | Unknown |

原子节点可携带 marks（忠实往返 `Strong[InlineMath]` 类结构）；save 侧发现
`code` mark 施加于原子节点 → 结构性错误（Code 是 text 叶，PM 端 schema/编辑器不应产生）。

### 4.5 JSON 形态示例

Prima：

```json
{"type": "paragraph", "id": "blk_01J…",
 "content": [{"type": "text", "text": "重要"},
             {"type": "em", "content": [{"type": "text", "text": "提示"}]}]}
```

PM（等价物）：

```json
{"type": "paragraph", "attrs": {"id": "blk_01J…", "extra": null},
 "content": [{"type": "text", "text": "重要", "marks": [{"type": "strong", "attrs": {"extra": null}}]},
             {"type": "text", "text": "提示",
              "marks": [{"type": "strong", "attrs": {"extra": null}},
                        {"type": "em", "attrs": {"extra": null}}]}]}
```

---

## 5. 分阶段计划

### M6.1 — `azodoc-pm` 转换库（约 1 周）【本次执行】

```
engine/crates/azodoc-pm/
  src/mapping.rs        # 唯一手写映射表（§4 的机器可读形态）
  src/gen.rs            # 映射表 → gen/aludel-schema.json + gen/schema.mjs
  src/from_prima.rs     # Prima → PM-JSON（load）
  src/to_prima.rs       # PM-JSON → Prima（save；ID 补发；嵌套重建）
  src/bin/generate.rs   # 重新生成 gen/ 黄金文件
  tests/mapping_consistency.rs  # 映射表 ↔ schemars 变体逐一锁死
  tests/golden.rs               # gen/ 产物逐字节黄金比对
  tests/roundtrip.rs            # 全类型夹具 / spec 黄金样例 / M2 语料 / R2 / ID 规则
```

阶段验收：全类型夹具与 spec 三样例（minimal/rich/forward-compat）往返 Value 恒等；
M2 语料（md/html × basic/lossy）往返恒等（或幂等 + 归一化说明）；R2 夹具 extra
字节级保真；ID 补发规则用例（缺失/重复/非法）。

> **实施记录（2026-09-16，全部达成）**
> - 交付物：`engine/crates/azodoc-pm`（mapping/gen/from_prima/to_prima + `generate` bin
>   + `gen/` 黄金工件）+ 三套集成测试；`engine/Cargo.toml` members 注册（唯一存量改动）。
> - 测试 19 项全绿：mapping 单测 3、黄金 2、schemars 一致性锁 4、roundtrip 10。
>   **M2 语料四文件（md/html × basic/lossy）全部严格恒等**（导入器产物天然规范形，
>   未动用幂等兜底）；全仓 107 测试绿，clippy/fmt 清零。
> - 实施中固化的两条规则（已体现在 §4 与代码）：
>   1. **无标签结构的 `type` 是数据不是标签**：rich.azodoc 的 list_item 实际携带
>      `type: "list_item"`（serde 落入 ExtraMap 的 R2 数据）。extra 收集只对有标签
>      类型排除 `type`；无标签结构的该字段经 `extra` attr 原样往返。
>   2. **PM 键序沿用 `doc.toJSON()`**（type → attrs → content → text → marks），
>      前端 `doc.toJSON()` 产物与本转换器输出可直接比对。
> - 验收对照（RFC §1 之①）：M2 语料往返恒等 ✅；R2 夹具（forward-compat 的
>   unknown 块/span、payload_ref、条目级 type）保真 ✅；ID 补发（缺失/重复/非法）✅。

### M6.2 — `aludel` server 与保存管线（约 1 周）【已交付】

新 crate `engine/crates/aludel`：`main.rs`（启动 + 开浏览器）、`http.rs`（最小 HTTP）、
`api.rs`（open/save/verify，直调 azodoc-container/-convert/-pm 与
`athanor_cli::relocate_annotations_layer`）。e2e：open → 程序化改 PM-JSON → save →
`athanor verify` 绿 → history 出现 `human:<id>` → 重定位统计正确。

> **实施记录（2026-09-16，全部达成）**
> - 交付物：`engine/crates/aludel`（lib + bin；`http.rs` 约 190 行最小 HTTP/1.1、
>   `api.rs` 保存七步管线、`static/index.html` 冒烟页、`tests/m6_2_e2e.rs` 7 项 e2e）。
>   `engine/Cargo.toml` members 注册（存量改动）。
> - API 面（即未来 A3 Tauri command 层候选）：`GET /api/doc`（PM 状态 + 语义层 +
>   修订链 + unknown/失配损失盘点 + 容器警告）、`POST /api/save`（七步管线，见 §3）、
>   `POST /api/verify`（复用 `athanor verify`）。server 仅监听 127.0.0.1、绑定启动时
>   唯一文档；保存经进程内互斥锁串行化；请求体上限 64 MiB；handler panic 拦截为 500。
> - 七步管线落点：`pm_to_content_file`（ID 补发走 `AzodocId::generate`）→
>   `validate::check_content`（Error 级拒绝保存，含悬空 footnote_ref / payload_ref
>   存在性）→ `set_entry` → `relocate_annotations_layer`（复用 `athanor-cli`）→
>   `commit(author_type: "human")`（硬编码）→ `write()` 回写 → 响应统计。
> - e2e 断言（7 项全绿）：human 修订落链 + manifest 推进 + **保存后
>   `athanor verify` 通过**（验收②③④）；未知 PM 节点/悬空 footnote_ref 拒绝且
>   原文件无损；HTTP 栈全链路（静态页/open/save/404/413）；手动 curl 冒烟通过。
> - 全仓 114 测试绿，clippy/fmt 清零。

### M6.3 — ProseMirror 前端原型（约 1–1.5 周）

`aludel/editor/`（vite + TS + 裸 PM，不用 TipTap），dist 嵌入二进制。
v1 可编辑：段落/标题/列表（含任务）/引用/代码块/基础 marks/数学源码编辑。
v1 只读：unknown 卡片、figure/table/callout/footnote 最简展示。
UI：中央编辑器 + 右侧栏（修订历史 author 徽标、标注列表 detached 高亮）+
保存 toast（"未变 N · 重锚 N · 迁移 N · 失配 N"）+ 四段式错误页。

### M6.4 — 验收打磨与 CI（约 3–5 天）

原型期提交 `editor/dist` 入库（CI 只跑 Rust + schema 黄金 diff；Node 矩阵刻意推迟）；
design_docs 增补 M6 验收记录；README 中英状态更新；四条验收逐条演练。

---

## 6. 关键难点与对策

1. **嵌套 span ↔ flat marks 非结构双射**：`em[strong]` 与 `strong+em` 语义同构。
   恒等标准定义为**归一化恒等**（同 M2）：语料两侧均规范形时 Value 恒等；
   另以"二次往返不动点"（幂等）+ 语义流（text+marks 集合序列）相等兜底深嵌套。
2. **ID 稳定**：PM 增删改/粘贴产生无主 ID；统一在 save 侧 Rust 补发（§3 规则），
   规则唯一且 M6.1 冻结——块级标注锚定依赖它。
3. **R2/未知内容**：`extra` attr 透传 + unknown 只读节点；M1 前向兼容夹具在 PM
   往返下重跑锁死。
4. **文本 extra 无处安放**：PM text 无 attrs，以内部隐形 `span_extra` mark 承载
   （§4.3）；原子节点同理可用其 extra attr。

## 7. 影响面与工作流

纯增量：新增 `azodoc-pm`、`aludel` 两 crate；触到的现有文件仅 `engine/Cargo.toml`
（members 两行）与 README（M6.4）。复用不动：`Container::commit`、
`relocate_annotations_layer`。按 AGENTS.md：实施期每次符号编辑前跑 impact、
提交前跑 `detect-changes --scope all`。

## 8. v1 明确排除（防范围膨胀）

协作/增量 steps 同步（快照模式即全量）、checkout UI 化（API 留位）、资产上传编辑、
figure caption/table 单元格可视编辑、`--preserve-raw-active`（C4）、Tauri 壳（A3）。
