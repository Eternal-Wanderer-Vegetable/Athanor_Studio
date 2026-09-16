# GitNexus Engineering Plan

> Task: A3 Athanor Studio GUI（Tauri 桌面壳，复用 Aludel 编辑核心）
> Evidence verified at commit c14432781c43baea846d7075442dd6d06706240c; GitNexus index not used (runner bootstrap failed on unavailable cmd.exe; fallback source verification).
> Evidence provenance schema 2; global dirty digest 4dc73696c224c500a45f5cd1b78bfb903a4a9d74e79d1a8e0fbca112e830aac9; cited-path manifest 10 entries.

## 1. Objective
交付可安装、跨平台的 Athanor Studio：打开/新建/保存 .azodoc，复用 Aludel 编辑器，提供 verify、损失提示、修订历史及导入/出版入口。

## 2. Current Behaviour
[verified] `aludel::api::App::{open,save,verify}` 位于 `engine/crates/aludel/src/api.rs`；save 固定执行 ID 补发、校验、写 content、标注重定位、human commit、回写七步。
[verified] Vite/TypeScript/ProseMirror 前端生成单文件 dist，由 `aludel/src/lib.rs` 嵌入；复杂块目前只读。
[verified] 原型仅回环地址、单文档、Mutex 串行化、64 MiB 请求上限；没有 Tauri 生命周期、多窗口和文件对话框。

## 3. Relevant Architecture
[verified] `aludel` 依赖 container/convert/model/pm/athanor-cli；M6 文档明确将其 API 作为未来 Tauri command 候选，前端可原样复用。
[inferred] 新增独立 `studio` crate/前端包，保留 aludel HTTP 适配器，隔离桌面生命周期与核心文档服务。

## 4. GitNexus Findings
[graph] GitNexus 不可用，未能取得 callers/impact/PDG；执行阶段必须先 `analyze --index-only --pdg`，再对 `App::open/save/verify` 与 `relocate_annotations_layer` 做 impact。
[source-derived] `api.rs` 直接连接容器、PM 转换、规范校验、标注重定位和 CLI verify；`aludel/tests/m6_2_e2e.rs` 覆盖现有行为。

## 5. Statement-Level PDG Findings
[assumed] 无可用 PDG。执行阶段确认 save 的“校验失败不写盘、重定位先于 commit、write 错误恢复”以及窗口关闭/自动保存取消路径。

## 6. Proposed Changes
- 新增 Tauri Rust 壳：单实例、窗口/会话状态、文件对话框、CSP、命令 DTO、错误与进度事件。
- 从 `aludel::api::App` 抽取无 HTTP 的 session/service；open/save/verify/history，并按窗口隔离文档路径。
- 前端复用 ProseMirror schema，补菜单/快捷键、脏状态、保存冲突、验证/损失面板、历史与错误页。
- 导入/出版任务接入 DOCX 原生读取及既有 HTML/Markdown/PDF 能力，支持取消并回传 LossLog。
- GUI 顺手修复候选：B2 converter 审计命名、C1 schema 结构 diff、C4 `--preserve-raw-active` 选项、C8 `athanor info` 汇总；C5/C6/KaTeX 默认后置。

## 7. Implementation Sequence
1. 修复 GitNexus runner、刷新索引并完成 impact/PDG。
2. 写 A3 RFC，冻结平台矩阵、命令协议、威胁模型及顺手修复 Must/Should/Deferred。
3. 抽取 session/service，保持 HTTP 与 M6 e2e 通过。
4. 建立 studio Tauri crate 与最小窗口，实现 open/save/verify/history 桥接。
5. 接入前端，加入脏状态、文件选择、并发保存/关闭确认、错误展示。
6. 接入导入/导出/出版、LossLog/进度/取消；逐项实现已批准顺手修复并补回归测试。
7. 完成跨平台打包/签名/诊断，更新 README 与设计文档；生成最终 dist。
8. 全量测试、人工 GUI 冒烟、`detect-changes --scope all` 后提交。

## 8. Test Strategy
运行 `cargo test -p aludel` 与 editor `npm run build`。新增命令测试覆盖路径越界、过大输入、校验失败不落盘、并发/取消/关闭恢复、历史一致性；集成覆盖 M2 语料编辑保存 verify、DOCX 两条读取路径命名、未知节点/悬空引用无损、多窗口隔离；CI 做 fmt/clippy/test 和三平台启动/打开/保存/退出 smoke。

## 9. Risk and Impact Analysis
[assumed] 风险集中在路径穿越、IPC DTO 漂移、窗口并发/自动保存、原生打包签名和大文档内存。必须保留现有 Mutex、请求上限、校验先行与原子回写顺序；影响清单待刷新后的 impact 补齐。

## 10. Files Expected to Change
`engine/crates/studio/*`（新壳）；`engine/crates/aludel/src/{api,lib,http}.rs`（服务抽取/兼容）；`engine/crates/aludel/editor/src/*`（桌面适配）；`engine/Cargo.toml`；`aludel/tests` 与新增集成测试；A3 RFC/README。

## 11. Reusable Implementation Context
{"primary_symbols":["aludel::api::App::open","aludel::api::App::save","aludel::api::App::verify"],"graph_status":"unavailable; rerun analyze --index-only --pdg","constraints":["preserve seven-step save order","preserve HTTP/e2e","impact before edits","detect-changes before commit"],"evidence_provenance":{"schema_version":2,"head_commit":"c14432781c43baea846d7075442dd6d06706240c","global_dirty_digest":"4dc73696c224c500a45f5cd1b78bfb903a4a9d74e79d1a8e0fbca112e830aac9","cited_path_manifest_count":10}}

## 12. Assumptions and Open Questions
- 首版目标平台、签名/自动更新尚未定；RFC 阶段确认。
- checkout UI、资产上传、KaTeX、C5 delta、C6 GC 默认后置。
- “顺手修复”四项需在 RFC 中逐项定 Must/Should/Deferred。
- 工作树已有未跟踪 `.zcode/plans/plan-sess_363feadc-36fc-4731-8570-9972d1d665c4.md`，不得覆盖。

## 13. Definition of Done
Tauri 应用可安装启动并完成打开→编辑→保存→verify，产生 human 修订；HTTP/e2e 与全仓构建保持通过；安全、并发、损失/进度和顺手修复有回归测试；A3 RFC/文档、跨平台包、impact 复核和提交前 detect-changes 完成。

