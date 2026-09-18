# Athanor Studio：后续增强项实施计划

> Task: 在现有 Word 式编辑器基础上完成资产、表格、富文本粘贴、分页预览、表现层历史、DOCX 能力说明与桌面质量增强。
> Evidence verified at commit `53019f0d48bb860960bca9a99068a9677e004efb`; GitNexus index refreshed at this commit with `analyze --index-only --pdg` and `detect-changes --scope all` clean.
> Claim tags: `[verified]` 源码或已执行验证；`[graph]` GitNexus/PDG；`[inferred]` 基于证据的设计判断；`[assumed]` 待实现阶段确认。

## 1. Objective

本计划只处理首轮 GUI 改造后仍未达到日常 Word 使用体验的增强项，目标是让“编辑→保存→重开→预览/导出”在资源、表格、分页和格式损失上形成可解释、可回归的闭环。保留 ProseMirror 作为编辑状态真源，保留 Prima/Azodoc 包格式与向后兼容；协作编辑、VBA、邮件合并、复杂浮动对象和任意 OOXML 像素级往返继续延期。

验收主线：插入离线图片和表格后保存重开；从 Word/网页粘贴后格式安全可控；预览与 PDF 由同一快照生成；主题修改可追溯；DOCX 导出显示能力与损失；Windows WebView2、键盘和高 DPI 下无阻断问题。

## 2. Current Behaviour

- `[verified]` 图片粘贴在 `DocumentController.createEditor` 中读成 data URI 并直接写入 `image.attrs.asset`（`engine/crates/aludel/editor/src/app/document-controller.ts:128-147`）；文件插入在 `main.ts:219-239` 走同样路径。当前没有 session 级暂存资产、保存时写入 registry/二进制、关闭清理或资产失败回滚。
- `[verified]` `schema.ts:86-91` 仅把 http(s)、blob、data:image 当作可显示 URL；`asset://` 进入占位框（`149-163`）。规范要求有引用时存在 `assets/registry.json` 和嵌入文件，并校验引用集合、hash、external URL（`spec/azodoc-package.md:36-38,136-143`）。
- `[verified]` 表格由自定义 `table-commands.ts` 重建行列，schema 只输出 `colSpan/rowSpan`（`schema.ts:139-147`），生成映射只声明 `column/colSpan/rowSpan`（`mapping.rs:274-298`）；没有 cell selection、colwidth、tableRole/header_cell、垂直合并和标准插件维护。
- `[verified]` `App.open` 读取内容、主题、历史并组装 `DocResponse`（`api.rs:110-173`）；`App.save` 经 PM→Prima、校验、容器内存写入、主题写入后走 `atomic_write`，GitNexus 将其标为 HIGH，直接影响 route、`DocumentSession.save`、主入口和保存 e2e（`App.save` impact，upstream，3 symbols/3 processes）。
- `[verified]` PDF 出版已有 `augment_print_html` 与 CDP/Paged.js 管线；`print_html_to_pdf_paged` 等待 `window.__azodocPagedDone`、统计 `.pagedjs_page` 后 `Page.printToPDF`（`paged.rs:52-177`）。当前缺少从版本化快照生成的独立预览、LayoutIndex、手动分页节点和系统打印入口。
- `[verified]` DOCX 已有 `azodoc_docx::export`、资产解析、表格/脚注/主题映射和 `LossLog`/`build_report`，但编辑器没有能力矩阵和逐项损失说明的用户出口。
- `[verified]` CI 已执行 editor build、单元/e2e 浏览器测试及 Rust workspace 检查；没有 Windows WebView2/高 DPI/辅助功能 smoke。当前 HEAD 的既有检查基线已通过，后续增强须保持同一门槛。

## 3. Relevant Architecture

编辑器边界保持为 `DocumentController`/`SessionStore` → `DocumentGateway`（Tauri/HTTP）→ `aludel::api::App`/`DocumentSession` → `azodoc-pm` → `azodoc-container`/转换器。资源注册、表现层快照和预览快照必须在 gateway 契约中显式传递，不能由 UI 拼 ZIP 或读取任意本地路径。

表格的持久化真源仍是 Prima `table.rows[].cells[]`；前端若采用 `prosemirror-tables`，应增加适配层，不替换生成 schema。分页采用“连续编辑 PM 文档 + 独立布局索引 + 快照预览”，不把视觉 page wrapper 写入 `content.json`。HTML、PDF、DOCX 从相同的 `ExportDoc`/`RenderSettings` 生成，所有异步操作以 session、revision/fingerprint 和请求 epoch 绑定。

## 4. GitNexus Findings

- `[graph]` `query -r Athanor_Studio "asset registry image paste embedded binary assets session registry"` 命中 `DocumentSession`、`DocResponse`、`schema.ts` 图片节点和 DOCX 资产测试；说明资产路径横跨编辑器、会话和转换器，不能只改前端。
- `[graph]` `query ... "table schema mapping colSpan rowSpan colwidth table selection"` 命中 `azodoc-pm::schema_spec/schema_mjs`、`build_table_cell`、DOCX `table_from/map_table` 和表格测试；schema 生成、PM 往返、DOCX 必须作为一个变更集。
- `[graph]` `query ... "paged preview publication layout index print PDF"` 命中 `print_html_to_pdf_paged`、`cmd_publish`、`studio::jobs::execute` 和 `paged_e2e`；出版管线是跨 CLI、任务和 PDF 的共享高风险边界。
- `[graph]` `query ... "DOCX export capability loss report styles theme"` 命中 `azodoc_docx::export`、`ast_out::block_ast`、`LossLog::build_report`、主题样式测试和 report schema 测试；能力说明应复用现有损失报告。
- `[graph]` `context/impact App.save`：`App.save` upstream risk **HIGH**，直接 callers 为 `route` 与 `DocumentSession.save`，并影响 `main`/保存 e2e；`context App.open` 为 lower-bound HIGH，少一个 receiver 无法解析，不能按“无 caller”缩小范围。
- `[graph]` `impact DocumentController`：upstream risk LOW，直接 caller 为 `main.ts`；`impact build_table_cell`：LOW，影响 `build_table_row`、`build_block` 及一个转换流程。
- `[graph]` `impact print_html_to_pdf_paged`：upstream risk **HIGH**，影响 `cmd_publish` 以及 studio jobs 的取消/提交边界和导出报告流程；任何预览复用必须保留这些错误和清理分支。

## 5. Statement-Level PDG Findings

本次 `--pdg` 索引可用，但针对 `build_table_cell:492`、`print_html_to_pdf_paged:120` 和 `DocumentController:350` 的行锚点返回 `pdg-no-block-at-line`；因此没有可引用的语句级控制/数据边，不能把 callgraph bridge 当作纯 PDG 结论。可用的 interprocedural bridge 仍显示：表格单元格变更沿 `build_table_row → build_block → pm_to_content_file` 传播；分页变更沿 `print_paged → cmd_publish → main/studio jobs` 传播。

由源码确认的时序约束：

1. `[verified]` 保存必须先转换/校验，再更新容器内存层和主题，最后 `atomic_write`；资产 registry 校验必须插入 PM→Prima 后、正式写盘前。
2. `[verified]` 前端保存捕获 `snapshot`、`epoch`、`editGeneration`，响应只确认同一快照（`document-controller.ts:317-377`）；资产上传/解析不能破坏这条 generation 语义。
3. `[verified]` `App.open` 的主题/历史读取与 `content_file_to_pm` 同属于打开快照；表现层历史扩展必须对无主题旧修订提供兼容 fallback。
4. `[verified]` Paged.js 必须等待分页完成且页数大于零；预览复用时仍需 timeout、浏览器退出、临时目录清理和 plain-print fallback。
5. `[inferred]` 表格 schema 改造必须先完成映射/往返和旧 irregular table 检测，再开启高级交互，避免插件在内存中静默修复持久化数据。

## 6. Proposed Changes

### E0 — 契约和夹具冻结

- 文件：`spec/azodoc-package.md`、`spec/json-schema/theme.schema.json`、`azodoc-pm` 生成产物、测试 fixtures。
- 明确资产 `asset://`、表格 `colspan/rowspan/colwidth/header_cell` 到 Prima 的字段、主题快照版本、预览快照 hash 和损失报告版本；为旧文档定义“读入不改写、用户显式修复”的规则。
- 建立包含离线图、外链图、孤儿资产、rowspan/colspan/列宽、不规则表、主题仅变更、分页/脚注的最小 fixture；冻结前不改生成 golden。
- 门槛：schema 校验、旧包 verify、PM↔Prima canonical round-trip 和现有全量测试均保持通过。

### E1 — 资产注册表和生命周期

- 文件/符号：`DocumentController.createEditor`、`main.ts` 图片插入、`DocumentGateway`、`studio/src/commands.rs`、`aludel/src/api.rs`/`doc_store.rs`、`azodoc-container`，新增 session-scoped asset service 与前端资源解析模块。
- 行为：图片进入 session 暂存区后用 `asset://id/filename` 写 PM；save/save-as 将可达资源写入 `assets/registry.json` 和 `assets/<id>/<filename>`，登记 mime、bytes、sha256、storage、来源；external 资源只保留 url；旧 data URI 在打开后可显示并在下一次保存迁移。
- 约束：大小/解码像素/MIME 白名单；文件名和路径拒绝 `..`/绝对路径；引用集合与 registry 一致；失败不产生悬空引用，关闭/取消/重开清理未提交暂存；孤儿资产不自动删除。
- UX：图片属性面板编辑 alt/题注/尺寸/对齐，受控 blob URL 预览并在 session 关闭时 revoke；插入、粘贴、拖入共用同一入口。
- 依赖/门槛：先于富文本粘贴和图片高级 UX；完成“插入→保存→重开离线显示→另存为→失败恢复”以及 registry hash 验证。

### E2 — 专业表格适配

- 文件/符号：`schema.ts`、`table-commands.ts`、`azodoc-pm/src/mapping.rs`、`to_prima.rs`、`from_prima.rs`、`gen.rs`/`gen/aludel-schema.json`、DOCX table mapping/tests。
- 行为：引入经过验证的 table node/cell selection/列宽命令；支持行列选择、插入/删除、水平/垂直合并拆分、表头单元格、拖动列宽、复制表格。标准 PM attrs 适配 Prima `column/colSpan/rowSpan/columns/header_row`，并显式记录 `colwidth`/角色。
- 约束：生成 schema 是唯一契约；默认 span=1；稳定 row/cell ID 不因操作漂移；不规则旧表只显示诊断和可撤销修复；删除列同步 `columns`；保留未知 `extra`。
- 门槛：2×2、跨列、跨行、混合表头、嵌套表、删列/撤销、保存重开、DOCX round-trip 及 schema golden 全部通过。

### E3 — 富文本剪贴板、公式和脚注可用性

- 文件/符号：`DocumentController.createEditor` paste handler、`schema.ts` parseDOM/marks、`main.ts` 插入命令、gateway 消息接口；新增 clipboard sanitizer、formula renderer、footnote navigation modules。
- 行为：粘贴模式为“保留支持格式/匹配目标格式/纯文本”；HTML 白名单只允许已建模节点和安全 URL，移除 script/style event handler；粘贴 Word/网页表格和图片走 E1/E2；丢失格式给一次性可读提示。
- 公式：保留 LaTeX 为真源，离线打包 KaTeX/MathJax（若性能 spike 不达标则只显示明确源码态）；脚注引用↔定义可跳转，删除前检查引用并可撤销。
- 门槛：恶意 HTML、危险 URL、Word 粘贴、中文 IME、公式重开、脚注跳转/删除、一次撤销覆盖粘贴事务等场景通过。

### E4 — 连续编辑与打印预览

- 文件/符号：`DocumentController`/gateway 快照接口、`athanor-cli::commands_m5::cmd_publish/print_css`、`azodoc-pdf::{augment_print_html,print_html_to_pdf_paged}`、新增 LayoutIndex/preview surface。
- 行为：从 PM+theme+asset manifest+revision/fingerprint 生成不可变 preview snapshot；连续编辑保持单一 PM 文档；预览使用独立 HTML 和 LayoutIndex（blockId/文本范围→页/框）；增加手动分页节点或等价布局 hint、页码、简单页眉页脚、系统打印入口。
- 约束：预览、PDF、打印共用 RenderSettings；不把 page wrapper 写入 content；未完成分页不显示伪造页数；Paged.js 失败仍可 plain-print，错误不改变文档。
- 门槛：跨页段落/选择、表格、超高图、脚注、字体加载变化、取消打印、预览/PDF snapshot hash 一致；连续编辑输入延迟记录基线后再决定是否默认启用分页编辑。

### E5 — 表现层历史与 DOCX 能力矩阵

- 文件/符号：`api.rs::App.open/save`、revision/container manifest、`theme.ts`/page settings、`azodoc-convert::build_report`、`azodoc-docx::{export,block_ast,build_theme}`、报告 schema 与 e2e。
- 行为：修订可选记录 presentation/theme hash 或快照；checkout 旧修订时无主题快照则提示“仅正文历史”；主题 dirty 与正文保存/恢复保持同一 generation。导出前生成能力矩阵：支持、降级、保留原始部件、未支持；在 UI/任务结果展示 loss summary 与输出 revision。
- 约束：旧 `revisions/<id>/content.json` 可读；不伪造旧主题；LossLog 仍是唯一事实来源；能力矩阵版本化，新增字段可选。
- 门槛：theme-only save/undo/reopen/checkout、旧包打开、DOCX 基础样式/表格/图片/脚注/页设置与报告 schema 测试。

### E6 — 可访问性、性能和 Windows 桌面验收

- 文件：editor UI/HTML、Playwright 配置、`.github/workflows/ci.yml`、`studio/tauri.conf.json`、新增 Windows smoke harness。
- 行为：toolbar/menu/tab/dialog/表格选择全键盘可达，按钮有可见焦点和 aria 状态；100%/150%/200% DPI 不裁剪；WebView2 缺失、浏览器缺失、字体缺失都有可读提示；长文档测量采用可见区域优先和取消旧布局任务。
- 门槛：Playwright keyboard/IME/paste/preview；Windows 真 Tauri 启动→新建→保存→重开→预览→关闭；固定夹具记录打开、输入、布局、保存 p95，超阈值只阻止发布不改变文档语义。

### E7 — 发布和文档

- 更新用户文档、迁移说明、能力矩阵、故障恢复说明和 debug bundle；在 CI 中加入 schema/golden、asset integrity、preview snapshot、Windows smoke 的明确 job 或明确跳过原因。
- 采用 feature flags：`assetRegistryV2`、`tableSelectionV2`、`printPreviewV1`；旧包默认安全读取，任何自动迁移只在用户保存时发生且可回滚。

## 7. Implementation Sequence

1. E0 冻结契约和 fixtures；先跑现有 build/test，生成一次新的 schema/fixture baseline。
2. E1 资产 service 和保存/打开契约；完成失败注入、session 清理、离线重开后再接 UI 属性。
3. E2 映射和生成 schema；先做 PM↔Prima/DOCX golden，再替换前端表格命令；保留 irregular table fallback。
4. E3 sanitizer、模式选择、公式/脚注导航；E1/E2 未达门槛不得启用图片/表格富粘贴。
5. E4 先做 print-preview spike 和 snapshot hash，再接 UI、系统打印和 PDF；每一步都保留 plain fallback。
6. E5 主题快照/历史兼容，再接 DOCX 能力矩阵和报告面板。
7. E6 可访问性、性能和 Windows smoke；最后执行 E7 发布收口。

每个阶段单独提交，提交前对修改符号运行 `impact`，完成阶段后运行 `detect-changes --scope all`；只在最终阶段更新 golden/快照，避免中间提交反复漂移。

## 8. Test Strategy

| 层 | 输入 → 操作 → 预期 |
| --- | --- |
| 资产 | 离线 PNG → 粘贴/保存/重开 → `asset://`、registry、sha256 和预览一致；超限/坏 MIME → 拒绝且旧包不变；取消保存 → 暂存清理 |
| 表格 | rowspan/colspan/colwidth/header → 合并/拖宽/删列/撤销/重开 → Prima 与 DOCX 结构稳定；不规则旧表 → 只报告不静默重写 |
| 粘贴 | Word/HTML 含 script、事件、危险 URL → 三种模式 → 安全节点、一次 loss 提示、无脚本；中文 IME → 组合输入不被重挂载 |
| 预览 | 同一 PM/theme/asset snapshot → preview/PDF/print → snapshot hash 相同、真实页数、失败回退且文档不脏 |
| 历史/DOCX | 仅改主题 → save/undo/checkout/export → theme hash 可追踪；复杂 DOCX → report schema、LossLog 和 preserved 部件可见 |
| 桌面 | Windows WebView2 + 100/150/200% DPI → 键盘全流程 → 无裁剪、无焦点陷阱；缺依赖 → 清晰提示 |

现有文件优先更新：`engine/crates/aludel/tests/m6_2_e2e.rs`、`engine/crates/azodoc-docx/tests/b2_ooxml.rs`、`engine/crates/azodoc-convert/tests/report_schema.rs`、`engine/crates/azodoc-pdf/tests/paged_e2e.rs`、`engine/crates/aludel/editor/tests/e2e/workspace.spec.ts`。新增 editor 单测覆盖 sanitizer/session/commands；新增 Tauri smoke 只验证真实 IPC，不把浏览器 e2e 当桌面证明。

验证命令：`npm ci && npm run build && npm test -- --reporter=dot && npm run test:e2e`；`cargo fmt --all -- --check`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace`；生成 schema 时额外运行对应 `azodoc-pm` generator；Windows runner 执行 Tauri smoke。

## 9. Risk and Impact Analysis

- `[graph]` `App.open/save` 为 HIGH；所有资产/主题/历史字段必须可选、版本化并覆盖 HTTP、Tauri、session、route、e2e。任何失败都必须保留原文件和当前编辑快照。
- `[graph]` `print_html_to_pdf_paged` 为 HIGH；浏览器进程、CDP、Paged.js、临时目录和 studio job cancellation 是直接依赖，必须保留 timeout、cleanup、plain fallback。
- `[inferred]` table schema 是跨语言契约；若只改 TS 会导致生成器、校验器、DOCX 不一致，故 E2 先 Rust mapping/golden 后 UI。
- `[inferred]` 资产可能成为路径穿越、ZIP 炸弹或资源耗尽入口；沿用 container limits，加 MIME、字节、像素、路径和 URL allowlist，并记录被拒原因。
- `[assumed]` Windows CI 可安装 WebView2/Chromium；若不可用，必须保留人工/夜间 runner 的明确阻断记录，不可把浏览器 CI 标成桌面通过。
- `[inferred]` LayoutIndex 与真实跨页编辑是最大性能和交互风险；先以预览快照交付，连续编辑分页低于基线时保持连续模式。

## 10. Files Expected to Change

| File / new area | Responsibility |
| --- | --- |
| `engine/crates/aludel/editor/src/app/document-controller.ts`, `main.ts`, `schema.ts`, `src/platform/gateway.ts` | asset/session/paste/preview contract and UI commands |
| `engine/crates/aludel/editor/src/app/table-commands.ts` plus new table/clipboard/preview modules | table selection, sanitizer, LayoutIndex adapters |
| `engine/crates/aludel/src/api.rs`, `doc_store.rs`; `engine/crates/studio/src/commands.rs`, `sessions.rs`, `recovery.rs` | asset/theme/revision snapshot validation, IPC and cleanup |
| `engine/crates/azodoc-pm/src/{mapping,to_prima,from_prima,gen}.rs`, `gen/aludel-schema.json` | canonical table/schema mapping |
| `engine/crates/azodoc-container/src/lib.rs`, `spec/azodoc-package.md` | registry integrity and compatibility rules |
| `engine/crates/azodoc-pdf/src/paged.rs`, `engine/crates/athanor-cli/src/commands_m5.rs` | shared preview/PDF/render settings and fallbacks |
| `engine/crates/azodoc-convert/src/lib.rs`, `engine/crates/azodoc-docx/src/{lib,ast_out}.rs`, `ooxml/styles.rs` | capability matrix, loss reporting and DOCX styles |
| tests/CI paths listed in §8 | regression, golden, browser and Windows smoke |

## 11. Reusable Implementation Context

```yaml
implementation_context:
  task_summary: "完成后续增强：资产 registry、专业表格、富粘贴、打印预览、表现层历史、DOCX 能力矩阵和桌面质量。"
  acceptance_criteria:
    - "旧包可读且不因打开自动改写；保存失败不破坏原件。"
    - "离线资产保存为 asset:// + registry + binary，引用/hash/清理可验证。"
    - "表格标准交互与 Prima/DOCX 往返稳定。"
    - "预览/PDF/打印使用同一版本化快照，真实页数可证明。"
    - "主题历史和 DOCX LossLog 能力说明可追踪。"
    - "浏览器、Rust、Windows WebView2、高 DPI、键盘验收通过。"
  primary_symbols:
    - symbol: "DocumentController.createEditor"
      file: "engine/crates/aludel/editor/src/app/document-controller.ts"
      lines: "94-186"
      role: "编辑器事件、paste、transaction 与 dirty/recovery 边界"
    - symbol: "App.save"
      file: "engine/crates/aludel/src/api.rs"
      lines: "317-352"
      role: "高风险保存、校验、atomic write 入口"
    - symbol: "App.open"
      file: "engine/crates/aludel/src/api.rs"
      lines: "110-174"
      role: "内容、主题、历史和损失摘要读取入口"
    - symbol: "build_table_cell"
      file: "engine/crates/azodoc-pm/src/to_prima.rs"
      lines: "483-504"
      role: "表格单元格 PM→Prima 映射"
    - symbol: "print_html_to_pdf_paged"
      file: "engine/crates/azodoc-pdf/src/paged.rs"
      lines: "87-177"
      role: "Paged.js/CDP 输出和错误清理边界"
  related_symbols:
    - symbol: "DocumentController.mount"
      relationship: "CALLS/owns state"
      relevance: "asset/theme snapshot mount"
    - symbol: "build_table_row/build_block/pm_to_content_file"
      relationship: "CALLS"
      relevance: "table conversion propagation"
    - symbol: "print_paged/cmd_publish/studio::jobs::execute"
      relationship: "CALLS"
      relevance: "preview/PDF/job cancellation consumers"
    - symbol: "azodoc_convert::build_report/LossLog"
      relationship: "CALLS"
      relevance: "DOCX capability matrix facts"
  execution_path:
    - "Editor captures PM/theme/session snapshot and sends typed gateway request."
    - "App validates PM and assets, converts PM→Prima, updates container layers, and atomically commits."
    - "Open reverses container/Prima→PM while exposing theme/history/assets diagnostics."
    - "Preview/export pins the same snapshot, creates HTML, runs Paged.js/CDP or plain fallback, and records report."
  pdg_constraints:
    - description: "No statement block was available at the probed lines; retain callgraph bridge only."
      affected_statements: ["engine/crates/azodoc-pm/src/to_prima.rs:483-504", "engine/crates/azodoc-pdf/src/paged.rs:87-177"]
      implementation_consequence: "Re-run impact --mode pdg with executable line anchors before editing; do not infer missing data edges."
    - description: "Asset validation precedes container commit; preview waits for paged completion before PDF."
      affected_statements: ["engine/crates/aludel/src/api.rs:213-266", "engine/crates/azodoc-pdf/src/paged.rs:125-167"]
      implementation_consequence: "Preserve ordering and failure cleanup in every stage."
  architectural_patterns:
    - pattern: "ProseMirror single source of truth + typed gateway"
      example_location: "engine/crates/aludel/editor/src/app/document-controller.ts"
      usage_guidance: "Do not add a second mutable document JSON store."
    - pattern: "Prima mapping and generated schema"
      example_location: "engine/crates/azodoc-pm/src/mapping.rs"
      usage_guidance: "Change mappings/generator/goldens together."
    - pattern: "atomic container commit and loss report"
      example_location: "engine/crates/aludel/src/api.rs; engine/crates/azodoc-convert/src/lib.rs"
      usage_guidance: "Reuse existing commit/report paths."
  files_to_modify:
    - file: "engine/crates/aludel/editor/src/{app,platform,schema}.ts"
      symbols: ["DocumentController.createEditor", "DocumentGateway"]
      intended_change: "asset/paste/table/preview contracts"
    - file: "engine/crates/aludel/src/{api,doc_store}.rs; engine/crates/studio/src/{commands,sessions,recovery}.rs"
      symbols: ["App.open", "App.save"]
      intended_change: "asset/theme/revision snapshot lifecycle"
    - file: "engine/crates/azodoc-pm/src/{mapping,to_prima,from_prima,gen}.rs"
      symbols: ["build_table_cell"]
      intended_change: "canonical table attrs and generated schema"
    - file: "engine/crates/azodoc-pdf/src/paged.rs; engine/crates/athanor-cli/src/commands_m5.rs"
      symbols: ["print_html_to_pdf_paged"]
      intended_change: "snapshot-based preview and shared render settings"
    - file: "engine/crates/azodoc-convert/src/lib.rs; engine/crates/azodoc-docx/src/{lib,ast_out}.rs"
      symbols: ["build_report"]
      intended_change: "DOCX capability matrix and loss presentation"
  tests:
    - file: "engine/crates/aludel/tests/m6_2_e2e.rs"
      scenarios: ["asset/theme save and reopen; failure leaves original intact"]
    - file: "engine/crates/azodoc-docx/tests/b2_ooxml.rs"
      scenarios: ["table spans/headers, assets, footnotes, theme schema"]
    - file: "engine/crates/azodoc-pdf/tests/paged_e2e.rs"
      scenarios: ["paged footer/page count and preview snapshot"]
    - file: "engine/crates/aludel/editor/tests/e2e/workspace.spec.ts"
      scenarios: ["paste modes, table selection, preview, keyboard and DPI smoke"]
  verification_commands:
    - "npm ci && npm run build && npm test -- --reporter=dot && npm run test:e2e"
    - "cargo fmt --all -- --check"
    - "cargo clippy --workspace --all-targets -- -D warnings"
    - "cargo test --workspace"
    - "node .gitnexus/run.cjs detect-changes --scope all --repo ."
  risks:
    - "App.open/save and print_html_to_pdf_paged are HIGH impact; verify all direct dependants."
    - "Generated schema drift can corrupt old tables; require round-trip and golden gates."
    - "Asset paths and decode limits are security/resource boundaries."
    - "Paged layout and WebView2 availability are platform risks."
  assumptions:
    - "Windows runner can provide WebView2/Chromium; verify in E6 before gating release."
    - "KaTeX/MathJax licensing and offline bundle size are acceptable; otherwise keep source-only formula mode."
  open_questions:
    - "Embedded-by-default versus external assets and maximum decoded pixels."
    - "Theme snapshot granularity and revision storage size."
    - "Whether manual page break is a content node or presentation hint."
  avoid:
    - "Do not edit production code in this planning step."
    - "Do not add page wrappers to content.json."
    - "Do not silently normalize irregular tables or auto-delete orphan assets."
    - "Do not treat browser e2e as Windows desktop proof."
    - "Do not claim pure PDG statement edges from the no-block probes."
```

Evidence provenance (schema 2; exact helper output):

```json
{
  "schema_version": 2,
  "head_commit": "53019f0d48bb860960bca9a99068a9677e004efb",
  "generated_plan_path": "docs/plans/2026-09-18-gitnexus-plan-editor-enhancement-roadmap.md",
  "global_dirty_digest": {
    "algorithm": "sha256",
    "canonicalization": "gitnexus-evidence-provenance-v2 NUL-framed UTF-8 records",
    "value": "0a9c85780067d9afcd0764f307b60891e3cee927ee11eaeb5ec7826d10fd82cd"
  },
  "cited_path_manifest": [
    {
      "path": ".github/workflows/ci.yml",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:392e51db53375665717b0ff5bb2c7d6b54c54e532063d90cc45965348a6ea0ba",
      "index_digest": "sha256:392e51db53375665717b0ff5bb2c7d6b54c54e532063d90cc45965348a6ea0ba",
      "worktree_digest": "sha256:392e51db53375665717b0ff5bb2c7d6b54c54e532063d90cc45965348a6ea0ba",
      "untracked_digest": "absent"
    },
    {
      "path": "AGENTS.md",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:491bd2d556d9033375befe356dccdbd736b7d193595d7b857d58988ef4271285",
      "index_digest": "sha256:491bd2d556d9033375befe356dccdbd736b7d193595d7b857d58988ef4271285",
      "worktree_digest": "sha256:491bd2d556d9033375befe356dccdbd736b7d193595d7b857d58988ef4271285",
      "untracked_digest": "absent"
    },
    {
      "path": "docs/plans/2026-09-17-gitnexus-plan-word-like-gui.md",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:a915c9bd0e0f53268167a5243bfe57159331fd285c4962eebe25100326158469",
      "index_digest": "sha256:a915c9bd0e0f53268167a5243bfe57159331fd285c4962eebe25100326158469",
      "worktree_digest": "sha256:bdeda6a703fe83f2bcf735e8b9a5dc996aedf0a8d5c16169ab1a1b0c12b1399d",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/aludel/editor/package.json",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:8c75bc14fbf9e7cb7b9627b6e1d471f013b4aac1f72e7346fdbb0ce0c435d346",
      "index_digest": "sha256:8c75bc14fbf9e7cb7b9627b6e1d471f013b4aac1f72e7346fdbb0ce0c435d346",
      "worktree_digest": "sha256:8c75bc14fbf9e7cb7b9627b6e1d471f013b4aac1f72e7346fdbb0ce0c435d346",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/aludel/editor/playwright.config.ts",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:c29c8100a78cf9350cf4c3614894930d77f7f73d83af9a0f6fec6d55493dda53",
      "index_digest": "sha256:c29c8100a78cf9350cf4c3614894930d77f7f73d83af9a0f6fec6d55493dda53",
      "worktree_digest": "sha256:c29c8100a78cf9350cf4c3614894930d77f7f73d83af9a0f6fec6d55493dda53",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/aludel/editor/src/app/document-controller.ts",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:95c7ffa450ffa7c205e27620e494c2791a3a723a84fab6ec6aa5b2522ee907da",
      "index_digest": "sha256:95c7ffa450ffa7c205e27620e494c2791a3a723a84fab6ec6aa5b2522ee907da",
      "worktree_digest": "sha256:95c7ffa450ffa7c205e27620e494c2791a3a723a84fab6ec6aa5b2522ee907da",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/aludel/editor/src/app/table-commands.ts",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:3993e59b506edbcd0c0f15d7cdebbac61464b28ed421afeed5c0a43d600890c4",
      "index_digest": "sha256:3993e59b506edbcd0c0f15d7cdebbac61464b28ed421afeed5c0a43d600890c4",
      "worktree_digest": "sha256:3993e59b506edbcd0c0f15d7cdebbac61464b28ed421afeed5c0a43d600890c4",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/aludel/editor/src/main.ts",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:76e259beee8fa6e535890acb6215f648de262c3f5c9537a0ddb60dbb50483df8",
      "index_digest": "sha256:76e259beee8fa6e535890acb6215f648de262c3f5c9537a0ddb60dbb50483df8",
      "worktree_digest": "sha256:76e259beee8fa6e535890acb6215f648de262c3f5c9537a0ddb60dbb50483df8",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/aludel/editor/src/platform/gateway.ts",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:3bdaffcfbbeada7281bd38c07387cbd90d4237ff955a25cbfab89cc0639a7c09",
      "index_digest": "sha256:3bdaffcfbbeada7281bd38c07387cbd90d4237ff955a25cbfab89cc0639a7c09",
      "worktree_digest": "sha256:3bdaffcfbbeada7281bd38c07387cbd90d4237ff955a25cbfab89cc0639a7c09",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/aludel/editor/src/schema.ts",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:e0017d71bbd1a49cf9631900ee8c45e6c53a2ffb947da4881285720ee548fad1",
      "index_digest": "sha256:e0017d71bbd1a49cf9631900ee8c45e6c53a2ffb947da4881285720ee548fad1",
      "worktree_digest": "sha256:e0017d71bbd1a49cf9631900ee8c45e6c53a2ffb947da4881285720ee548fad1",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/aludel/editor/tests/e2e/workspace.spec.ts",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:99fdde6a176071d56a6ca5a5bc63b6cc2c94734b1e1cb48485b007db4c38fdf5",
      "index_digest": "sha256:99fdde6a176071d56a6ca5a5bc63b6cc2c94734b1e1cb48485b007db4c38fdf5",
      "worktree_digest": "sha256:99fdde6a176071d56a6ca5a5bc63b6cc2c94734b1e1cb48485b007db4c38fdf5",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/aludel/src/api.rs",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:0345e13fb5ab745830a75434a9d26f08d3e0b56c2b3165547ea18f86c214ed89",
      "index_digest": "sha256:0345e13fb5ab745830a75434a9d26f08d3e0b56c2b3165547ea18f86c214ed89",
      "worktree_digest": "sha256:0345e13fb5ab745830a75434a9d26f08d3e0b56c2b3165547ea18f86c214ed89",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/aludel/src/doc_store.rs",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:72297886285f1b024a802f2f88993ff265528761162e07fcb7880df6e66c5310",
      "index_digest": "sha256:72297886285f1b024a802f2f88993ff265528761162e07fcb7880df6e66c5310",
      "worktree_digest": "sha256:72297886285f1b024a802f2f88993ff265528761162e07fcb7880df6e66c5310",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/athanor-cli/src/commands_m5.rs",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:0ac958c5660c0f5326da5e20cb77e83b4ae995d827581560fa0de14d92edc187",
      "index_digest": "sha256:0ac958c5660c0f5326da5e20cb77e83b4ae995d827581560fa0de14d92edc187",
      "worktree_digest": "sha256:0ac958c5660c0f5326da5e20cb77e83b4ae995d827581560fa0de14d92edc187",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/azodoc-convert/src/lib.rs",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:ad5febef1d31cd33a6a1db42fbdf5e513d63ce7d9046f7047357bd7d55c0b1f0",
      "index_digest": "sha256:ad5febef1d31cd33a6a1db42fbdf5e513d63ce7d9046f7047357bd7d55c0b1f0",
      "worktree_digest": "sha256:ad5febef1d31cd33a6a1db42fbdf5e513d63ce7d9046f7047357bd7d55c0b1f0",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/azodoc-convert/tests/report_schema.rs",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:f85919f59d16c5af008e81560a2c1d537a80a59e2b87d1ad4dea75c28176f8d6",
      "index_digest": "sha256:f85919f59d16c5af008e81560a2c1d537a80a59e2b87d1ad4dea75c28176f8d6",
      "worktree_digest": "sha256:f85919f59d16c5af008e81560a2c1d537a80a59e2b87d1ad4dea75c28176f8d6",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/azodoc-docx/src/ast_out.rs",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:1c570d6a6279ee9de2d39bd9e752c4ff36d8465f824adcc898192b6076333cf3",
      "index_digest": "sha256:1c570d6a6279ee9de2d39bd9e752c4ff36d8465f824adcc898192b6076333cf3",
      "worktree_digest": "sha256:1c570d6a6279ee9de2d39bd9e752c4ff36d8465f824adcc898192b6076333cf3",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/azodoc-docx/src/lib.rs",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:4c57b041e55e848c878b72c4b363923b4b41c14341dd658348a84c8a1b598586",
      "index_digest": "sha256:4c57b041e55e848c878b72c4b363923b4b41c14341dd658348a84c8a1b598586",
      "worktree_digest": "sha256:4c57b041e55e848c878b72c4b363923b4b41c14341dd658348a84c8a1b598586",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/azodoc-docx/src/ooxml/styles.rs",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:c2e9c9161ffeeb8ff95cd7a9a9e879c216fdb1edb5c7e505f67e4bacd8b88fa3",
      "index_digest": "sha256:c2e9c9161ffeeb8ff95cd7a9a9e879c216fdb1edb5c7e505f67e4bacd8b88fa3",
      "worktree_digest": "sha256:c2e9c9161ffeeb8ff95cd7a9a9e879c216fdb1edb5c7e505f67e4bacd8b88fa3",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/azodoc-docx/tests/b2_ooxml.rs",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:fab11b788f0ab9ff6590fc0aa0ef0f2814c5a24e4f9e22d93e6f18e62b60ce55",
      "index_digest": "sha256:fab11b788f0ab9ff6590fc0aa0ef0f2814c5a24e4f9e22d93e6f18e62b60ce55",
      "worktree_digest": "sha256:fab11b788f0ab9ff6590fc0aa0ef0f2814c5a24e4f9e22d93e6f18e62b60ce55",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/azodoc-pdf/src/paged.rs",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:f0c9dbea6c827e4eca921b2b543affa81b6b10c56fb9c62d37a37520f0f4da40",
      "index_digest": "sha256:f0c9dbea6c827e4eca921b2b543affa81b6b10c56fb9c62d37a37520f0f4da40",
      "worktree_digest": "sha256:f0c9dbea6c827e4eca921b2b543affa81b6b10c56fb9c62d37a37520f0f4da40",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/azodoc-pm/gen/aludel-schema.json",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:cb30d3b5001697521bbacd0d53f4f4394ba92e4b6f4d4efd6a21eb4ced4a983f",
      "index_digest": "sha256:cb30d3b5001697521bbacd0d53f4f4394ba92e4b6f4d4efd6a21eb4ced4a983f",
      "worktree_digest": "sha256:cb30d3b5001697521bbacd0d53f4f4394ba92e4b6f4d4efd6a21eb4ced4a983f",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/azodoc-pm/src/from_prima.rs",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:1e21415a4be524713a310a42b165f2b273b358c474a5fde7a799df22aa52c41e",
      "index_digest": "sha256:1e21415a4be524713a310a42b165f2b273b358c474a5fde7a799df22aa52c41e",
      "worktree_digest": "sha256:1e21415a4be524713a310a42b165f2b273b358c474a5fde7a799df22aa52c41e",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/azodoc-pm/src/mapping.rs",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:eb7638a4dffab0679a0ca36cbb2394f82ee1aa6e684244b62d567cdb07760c86",
      "index_digest": "sha256:eb7638a4dffab0679a0ca36cbb2394f82ee1aa6e684244b62d567cdb07760c86",
      "worktree_digest": "sha256:eb7638a4dffab0679a0ca36cbb2394f82ee1aa6e684244b62d567cdb07760c86",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/azodoc-pm/src/to_prima.rs",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:d4edcbd4ca55bceb5effbb2d81dbf14fa9a79529014093bccd9fb20f6dd7a54e",
      "index_digest": "sha256:d4edcbd4ca55bceb5effbb2d81dbf14fa9a79529014093bccd9fb20f6dd7a54e",
      "worktree_digest": "sha256:d4edcbd4ca55bceb5effbb2d81dbf14fa9a79529014093bccd9fb20f6dd7a54e",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/studio/src/commands.rs",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:09475ca2e2458f5bb973a90ef9290cf3b0838005aa2ed31c964ed97bef339e46",
      "index_digest": "sha256:09475ca2e2458f5bb973a90ef9290cf3b0838005aa2ed31c964ed97bef339e46",
      "worktree_digest": "sha256:09475ca2e2458f5bb973a90ef9290cf3b0838005aa2ed31c964ed97bef339e46",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/studio/src/recovery.rs",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:2ad073ebec60a1d212766b7af228819419504d9ef1f412f4c04d256b4a7fd569",
      "index_digest": "sha256:2ad073ebec60a1d212766b7af228819419504d9ef1f412f4c04d256b4a7fd569",
      "worktree_digest": "sha256:2ad073ebec60a1d212766b7af228819419504d9ef1f412f4c04d256b4a7fd569",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/studio/src/sessions.rs",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:c954042a1bd660432eac597fd8343b0f14c9341ac5cee9d274ad1de2b76dd197",
      "index_digest": "sha256:c954042a1bd660432eac597fd8343b0f14c9341ac5cee9d274ad1de2b76dd197",
      "worktree_digest": "sha256:c954042a1bd660432eac597fd8343b0f14c9341ac5cee9d274ad1de2b76dd197",
      "untracked_digest": "absent"
    },
    {
      "path": "spec/azodoc-package.md",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:88e0ac39da8baaa801897f69cd368b5cf03a872326004a61c498978cbe08e5c5",
      "index_digest": "sha256:88e0ac39da8baaa801897f69cd368b5cf03a872326004a61c498978cbe08e5c5",
      "worktree_digest": "sha256:88e0ac39da8baaa801897f69cd368b5cf03a872326004a61c498978cbe08e5c5",
      "untracked_digest": "absent"
    },
    {
      "path": "spec/json-schema/theme.schema.json",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:3fd3ad04d1d0259920eaec31ae873bf33ccac63a021757008cf8dafaa23b7c74",
      "index_digest": "sha256:3fd3ad04d1d0259920eaec31ae873bf33ccac63a021757008cf8dafaa23b7c74",
      "worktree_digest": "sha256:3fd3ad04d1d0259920eaec31ae873bf33ccac63a021757008cf8dafaa23b7c74",
      "untracked_digest": "absent"
    }
  ]
}
```

## 12. Assumptions and Open Questions

Confirmed facts are listed in §§2–5. The following must be decided in E0/E1 before implementation locks:

- `[assumed]` 默认嵌入资产的单文件大小、解码像素上限和外链是否允许离线发布；用真实语料和容器 limits 验证。
- `[assumed]` 主题快照采用完整 `presentation/theme.json` 还是 hash+按需快照；用历史体积和 checkout UX 评估。
- `[assumed]` 手动分页表示为显式 PM node 还是 presentation hint；用 DOCX/PDF/旧 reader 兼容 fixture 评估。
- `[assumed]` KaTeX/MathJax 离线包的许可、体积和渲染耗时；spike 不达标就保持源码态。
- 明确延期：协作/冲突合并、VBA、邮件合并、复杂浮动环绕、任意 OOXML 像素级保真、自动资产垃圾回收。

## 13. Definition of Done

1. E0–E7 每阶段有独立提交、验收记录和 `detect-changes` 结果；未启用的 feature flag 有明确原因。
2. 资产引用、registry、二进制、hash、external URL、孤儿和清理规则通过规范校验；旧包打开无自动改写。
3. 表格标准交互、PM↔Prima、DOCX 和旧 irregular table 行为有 golden/回归证据。
4. 三种粘贴模式、脚本/危险 URL 清理、公式/脚注 UX 和中文 IME 通过真实浏览器场景。
5. 预览/PDF/打印使用同一 snapshot；页数/页码有真实排版回执；失败不改变文档。
6. theme-only 历史与恢复可解释；DOCX 能力矩阵来自 LossLog/report schema，未支持内容有保留或提示。
7. `npm`、Rust 全量检查和 Windows WebView2/high DPI/键盘 smoke 通过，性能基线已记录且无未解释回归。
