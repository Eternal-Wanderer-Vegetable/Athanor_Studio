# A3 Athanor Studio GUI 计划

## 目标

以 A1 已完成的 Aludel 编辑器为核心，交付 Tauri 桌面应用 Athanor Studio。用户应能通过文件选择器打开或新建 `.azodoc`，编辑后保存并产生 `author: human` 修订，执行验证，并查看损失摘要与修订历史。

## 分阶段实施

1. **基础复核与 RFC**：在 Linux/Docker 中刷新 GitNexus（含 PDG），对 `App::open/save/verify` 和标注重定位做影响分析；冻结平台、窗口模型、命令 DTO、错误/进度协议、安全边界，以及本期“顺手修复”范围。
2. **服务层抽取**：从 `engine/crates/aludel/src/api.rs` 抽取不依赖 HTTP 的文档会话服务，保留现有 HTTP 适配器。严格保持保存顺序：ID 补发、规范校验、写 content、标注重定位、human commit、回写。
3. **Tauri 桌面壳**：新增独立 `studio` crate，实现单实例、窗口与文档会话隔离、文件对话框、最近文件、CSP、路径检查，以及 open/save/verify/history 命令。
4. **前端接入**：复用 Aludel 的 ProseMirror schema 和组件，加入菜单/快捷键、脏状态、保存冲突与关闭确认、验证/损失面板、历史侧栏、结构化错误和任务进度展示。
5. **导入与出版**：接入 DOCX 原生读取及 Pandoc fallback，并连接现有 HTML/Markdown/PDF 出版能力；后台任务支持取消，向前端返回 LossLog 和进度。
6. **顺手修复**：修正 DOCX/Pandoc converter 审计名称；将 schema 结构性 diff 纳入 CI；评估 `--preserve-raw-active` GUI 选项；增强 `athanor info` 的损失与出版汇总。C5 delta、C6 资产 GC、KaTeX 默认后置。
7. **验证与发布**：补齐路径越界、过大输入、校验失败不落盘、并发保存、取消/关闭恢复、多窗口隔离和两条 DOCX 读取路径的回归测试；完成 Windows/macOS/Linux 打包 smoke、签名策略、README 和设计记录。

## 验收标准

- 三平台应用可安装、启动、打开 M2 语料、编辑、保存并通过 `athanor verify`。
- 保存产生可审计的 human 修订，标注重定位统计正确，未知内容保持无损。
- 现有 `aludel` HTTP/e2e、Rust 全仓测试、fmt/clippy 和 editor 构建继续通过。
- 所有纳入本期的顺手修复均有自动化回归测试和文档记录。
- 提交前完成 GitNexus impact 复核和 `detect-changes --scope all`。

## 需要在 RFC 阶段决定

- 首版支持的平台、签名和自动更新策略。
- 是否把 checkout UI、资产上传、KaTeX、C5 delta、C6 GC 纳入 A3 首期。
- 四项顺手修复分别属于 Must、Should 还是 Deferred。

