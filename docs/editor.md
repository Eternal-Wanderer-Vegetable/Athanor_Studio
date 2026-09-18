# Aludel 编辑器指南（E0–E7 增强路线图收口）

面向使用者与维护者：编辑器能力清单、功能开关、迁移与兼容性、
转换能力矩阵、故障恢复与诊断收集。

## 1. 能力清单

| 能力 | 说明 |
| --- | --- |
| 资产管线（E1） | 粘贴/拖入/插入图片 → 服务端暂存 → 正文写 `asset://<id>/<name>`，字节随保存落库；MIME/字节上限预检（32MB），超限可读拒绝；会话结束清理暂存；离线重开后 `asset://` 经 `/api/asset` 或 `azodoc-asset://` 协议还原显示 |
| 专业表格（E2） | rowspan/colspan、列宽拖拽（colwidth→`columns[].width`）、表头行切换、合并/拆分、加删行列、不规则表只诊断+显式 `table.fix` 修复（可撤销） |
| 富文本粘贴（E3） | Ctrl+V 保留格式（白名单净化）/ Ctrl+Alt+V 匹配目标 / Ctrl+Shift+V 纯文本；KaTeX 公式 NodeView；脚注双向跳转与悬空引用提示 |
| 打印预览（E4） | Ctrl+P / 菜单：真实 Paged.js 分页、真实页数、LayoutIndex 块→页映射、超时自动回退未分页呈现、系统打印入口；`page_break` 手动分页全链路往返（PM/DOCX/HTML） |
| 表现层历史（E5） | 主题与正文同代际：每次修订落 `revisions/<rev>/theme.json` 快照；历史侧栏 ◈ 标记含主题快照的条目，非当前条目可"还原"（checkout 原子写回+标注重定位）；无快照条目还原时提示"仅正文历史" |
| DOCX 能力矩阵（E5） | `cmd_transmute` DOCX 报告携带 `capabilities` 成员（matrix_version 1.0）：supported/degraded/preserved/unsupported 按块/行内类型归类；LossLog 仍是唯一事实来源 |
| 可访问性（E6） | 全键盘可达（Tab 焦点链+焦点环+aria-pressed）、对话框焦点管理、aria-live 状态播报、IME 组合输入不丢字、100/150/200% DPI 不裁剪 |
| 性能门控（E6） | 字数统计 200ms 防抖归并（O(doc) 不阻塞击键）、预览代际计数器取消过期分页任务；500 块夹具基线：挂载 ~180ms、输入派发 ~0.2ms |
| 依赖预检（E6） | WebView2 缺失：中文指引+exit(2)；Pandoc/浏览器缺失沿用既有可读提示 |

## 2. 功能开关（feature flags）

三条发布旗标，**默认全部开启**；关闭只停用交互入口，不改变容器
读写语义——旧包始终安全读取。回归/测试用 URL 参数覆盖：

```
?flags=assetRegistryV2:0,tableSelectionV2:0,printPreviewV1:0
```

| 旗标 | 关闭后的行为 |
| --- | --- |
| `assetRegistryV2` | 图片粘贴/拖入/插入命令停用并提示；既有 `asset://` 引用照常解析显示 |
| `tableSelectionV2` | 卸载 `tableEditing`/`columnResizing`：无单元格选区、无列宽拖拽、无 Tab 格间导航；表格命令退化为文本选区语义 |
| `printPreviewV1` | 预览入口停用并提示；PDF 出版/系统打印不受影响 |

旗标在 `src/flags.ts` 集中定义（`FEATURE_FLAGS`），新增旗标只需
加入数组并在消费点 `flag(name)` 判定。

## 3. 迁移与兼容性

- **旧包安全读取**：E1–E6 引入的所有容器成员（`assets/`、主题快照
  `revisions/<rev>/theme.json`、链条目 `theme_path`/`theme_sha256`、
  报告 `capabilities`）均为可选；缺失成员按旧形态处理，不报错。
- **自动迁移只在保存时发生**：打开旧文档后首次保存即把暂存资产
  字节落库、为主题落快照——不单独跑迁移管线；保存失败保留原文件。
- **可回滚**：编辑器内所有结构修改（表格修复、主题变更、分页符）
  走标准 undo 栈；修订级回滚走历史还原（checkout 原子写回）。
- **旧形态修订**：importer/遗留文档的修订条目可能无 theme 成员
  ——属正常形态，历史侧栏不标 ◈；还原时提示"仅正文历史"。

## 4. DOCX 能力矩阵

`athanor transmute ... --report out.json` 的 DOCX 报告含
`capabilities` 对象：

```json
{
  "matrix_version": "1.0",
  "supported":   ["paragraph", "text", "strong", ...],
  "degraded":    ["footnote", ...],
  "preserved":   ["unknown_block", ...],
  "unsupported": ["..."]
}
```

分类是**内容静态扫描**（按节点类型映射），与逐条 LossLog 互补；
任何"矩阵说 supported 但 LossLog 记 degraded"的差异，以 LossLog 为准。

## 5. 故障恢复

| 场景 | 行为 |
| --- | --- |
| 崩溃/未保存退出 | 编辑产生恢复草稿（防抖 2s）；下次打开同文档提示恢复，草稿独立存于会话目录不污染原件 |
| 外部修改 | 保存/checkout 前做 external-change 门禁，冲突时拒绝并提示，不静默覆盖 |
| 保存失败/取消 | 原子写（暂存→rename）；失败保留原文件与当前编辑快照，暂存资产清理 |
| 分页失败/超时 | 预览自动回退未分页呈现（同一份印刷管线产物），文档不脏 |
| 主题快照缺失 | checkout 提示"仅正文历史"，正文照常还原，不伪造主题 |
| 脚注悬空引用 | 删定义后一次性提示（撤销可恢复），保存侧仍是硬门槛 |
| 资产超限/坏 MIME | 暂存前拒绝并给可读原因；旧包不变 |
| WebView2/Pandoc/浏览器缺失 | 启动/调用前预检，输出安装指引而非深层 panic |

## 6. 诊断收集（debug bundle）

问题上报时收集以下产物即可定位绝大多数故障：

```powershell
# 文档完整性/历史链
athanor verify  <doc>.azodoc
athanor history <doc>.azodoc
# 资产与主题快照
athanor inspect <doc>.azodoc            # 成员清单 + 哈希
# 转换损耗
athanor transmute <doc>.azodoc --to docx --report report.json
```

上报内容：上述命令输出、文档副本（可脱敏）、编辑器状态栏
`文档ID/修订/会话ID`、复现步骤、操作系统与 WebView2 版本
（`scripts/windows-smoke.ps1` 的 WebView2 预检行即版本号）。

## 7. Windows 桌面验收

```powershell
# 全量（含构建）：WebView2 探测 → editor dist → cargo build -p studio → 主窗口存活 → 干净退出
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/windows-smoke.ps1
# 跳过构建、只验证启动
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/windows-smoke.ps1 -SkipBuild -KeepSeconds 15
```

CI 中 `windows-smoke` job 为 `workflow_dispatch` 手动触发
（桌面冒烟昂贵且需真实窗口环境，不随每次 push 跑）。

## 8. GUI 壳层契约（OpenDoc 改造）

界面结构：顶栏（品牌/文档名/命令搜索/保存）→ 应用菜单（文件/编辑/插入/视图/
文档检查）→ 页签功能区（开始/插入/视图/文档检查 + 上下文“表格”）→ 查找栏 →
左导航轨 + 大纲面板 + 文档工作区 + 右侧检查面板 → 状态栏。

- **单一命令契约**：菜单、功能区、命令面板（Ctrl+K）、快捷键、浮动格式条、
  状态栏全部投影 `CommandRegistry` 的同一份 `Command` 元数据；任何入口不单独
  实现命令逻辑。命令目录与启用条件冻结于 `docs/editor-gui-command-matrix.md`。
- **能力投影**：桌面专属命令（新建/打开/另存/导入/导出/任务）经 `visible` 在
  HTTP 模式不渲染入口；条件不满足的命令禁用并给出 `disabledReason`。
- **上下文页签**：光标/选区进入表格自动选中“表格”页签（边沿触发，表格内
  手动切走不被拉回）；离开表格恢复之前页签。浮动格式条只在非空文本选区出现。
- **响应式**：功能区按容器宽度把低优先级组收进“更多”（无横向滚动）；
  <1100px 左右面板变覆盖抽屉；<900px 插入/视图/文档检查菜单组收进“更多”；
  页签/折叠/主题/面板开合为 localStorage 偏好，不进文档/undo。
- **状态栏页数**只显示最近一次成功预览且快照未失效的分页结果；正文一代际
  变化即失效——连续编辑布局不是 Word 分页。
- **混合格式反射**：选区归并字符/段落格式；不一致的键显示“混合”而不是
  最后一个 run 的值；反射只读，不写回正文。
- **边界**：分页/WASM 编辑不在本壳层内；预览仍是 Paged.js + 回退路径，
  正文继续走 ProseMirror 事务与 DocumentController 会话边界。

## 9. 一键启动

```powershell
# 依赖预检（Node/Rust/WebView2）→ npm ci → 前端构建 → cargo run -p studio
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/start-studio.ps1
# 跳过前端构建（沿用现有 dist）、或直接 dev 运行
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/start-studio.ps1 -SkipFrontendBuild
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/start-studio.ps1 -Dev
```
