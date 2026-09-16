# A3 Athanor Studio GUI RFC

状态：提案，作为实现前的范围冻结文档。

## 1. 目标与首期范围

Athanor Studio 是基于 Tauri 的桌面应用。首期必须支持：

- 打开、新建、保存 `.azodoc`；
- 复用 Aludel 的 ProseMirror 编辑器；
- `verify`、损失摘要、修订历史；
- DOCX/Markdown/HTML 导入，以及现有出版入口；
- 保存产生 `author: human` 修订，失败时不破坏原文件。

协作编辑、远程同步、完整复杂块可视编辑、资产管理和 PDF 字节级确定性不属于首期。

## 2. 平台与发布

首期目标平台为 Windows 10/11、macOS 13+ 和主流 Linux x64。发布包采用 Tauri bundler；签名和自动更新在实现阶段分别确认平台凭据后启用，未签名构建仍可用于 CI smoke。应用不要求用户安装 Rust、Node 或 Pandoc；DOCX 原生读取为默认路径，Pandoc 仅作为兼容 fallback。

## 3. 分层架构

`studio` 是新的 Tauri crate，负责窗口、菜单、文件对话框、权限、任务和 IPC。`aludel` 提供可复用的文档会话服务；现有 HTTP server 作为兼容适配器保留。前端继续复用 `aludel/editor` 的 schema 与渲染组件，通过 Tauri adapter 调用 command。

文档会话按窗口隔离，状态包含规范化路径、当前 revision、脏标记和正在运行的任务。单实例策略负责把同一路径请求转发到已有窗口；不同文档可以在不同窗口打开。

## 4. Command 契约

首批 command：

| Command | 输入 | 输出 |
| --- | --- | --- |
| `open_document` | 路径或文件选择结果 | PM 文档、annotations、history、loss summary、warnings |
| `new_document` | 可选模板 | 新文档会话与初始路径状态 |
| `save_document` | PM 文档、message、author id | revision、ID 统计、重定位统计、warnings |
| `verify_document` | 会话 id | `ok`、退出码、结构化诊断 |
| `get_history` | 会话 id | 修订列表与当前 head |
| `run_job` | 导入/出版任务参数 | job id；通过事件流返回进度、LossLog、结果或错误 |
| `cancel_job` | job id | 是否成功取消 |

错误统一为 `{ code, message, path?, details? }`。不把堆栈、密钥或原始文档内容写入 UI 日志。

## 5. 保存与并发规则

服务层必须保持 Aludel 当前七步顺序：ID 补发 → 规范校验 → 写 content → 标注重定位 → `commit(author_type="human")` → 回写 → 返回统计。校验 Error 时不得写盘。每个会话串行化保存；任务取消只能发生在安全边界，不能留下半写容器。关闭脏窗口时必须提供保存、放弃或取消选项。

## 6. 安全边界

默认只允许用户选择的文件路径；禁止任意路径拼接、目录穿越和未授权覆盖。Tauri allowlist 仅开放本 RFC 定义的 command。窗口启用 CSP，生产构建禁止远程脚本。导入文件大小和任务资源使用受限，异常通过结构化错误返回。

## 7. 顺手修复分级

### Must（与 A3 同期）

1. 修正 DOCX/Pandoc converter 审计名称，区分 `athanor-docx` 与 `athanor-docx-pandoc`。
2. 为 Tauri command 和导入/出版任务补充路径安全、校验失败不落盘、取消和错误回归测试。

### Should（首期有余力时完成）

1. 将 C1 schema 生成物与手写 spec 的结构性 diff 纳入 CI。
2. 在 GUI 中暴露 C4 `--preserve-raw-active`，明确“隔离保存、不渲染”的提示。
3. 增强 `athanor info`，显示损失汇总和出版历史。

### Deferred

C5 delta 快照与裁剪、C6 资产垃圾回收、B3 KaTeX 呈现、协作同步、完整 figure/table 编辑、自动更新服务。

## 8. 验收门槛

M2 语料完成“打开 → 编辑 → 保存 → verify”，生成 human 修订且标注重定位统计正确；未知节点和悬空引用行为可解释且原文件无损；现有 `aludel` HTTP/e2e、Rust 测试、fmt/clippy 和 editor 构建通过；Windows/macOS/Linux 至少完成启动、打开、保存、退出 smoke；GitNexus impact 已复核，提交前 `detect-changes --scope all` 无未解释变更。

## 9. 未决事项

- 各平台签名证书、发布渠道和自动更新服务；
- 是否在首期开放 checkout UI；
- Should 项的最终排期与维护者；
- 大文档性能预算及后台任务并发上限。

