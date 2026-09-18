# GitNexus Engineering Plan

> Task: 参考 OpenDoc 的成熟编辑器壳层与交互组织方式，构建 Athanor Studio 的下一代前端 GUI。
> Evidence verified at commit `190c0b673043c377e5fb0c9ebf3082aaf3d1c408`; GitNexus index refreshed this session with `analyze --index-only --pdg` (GitNexus 1.6.12, Docker runner; source-weighted where receiver resolution is lower-bound).
> Evidence provenance schema 2; global dirty digest and cited-path manifest are recorded in §11; exact generated plan path is excluded.

## 1. Objective

把当前可编辑但命令平铺、布局响应弱、状态反馈分散的 Aludel 前端，升级为可持续扩展的 Word-like GUI：顶部应用栏、标签式功能区、上下文工具、左右导航/检查面板、命令面板、状态栏、主题与响应式布局共用一套命令和状态契约。第一阶段保持 ProseMirror 作为编辑真源与现有 Rust/HTTP/Tauri gateway；不把 OpenDoc 的 DOCX/WASM 引擎直接移植进 Athanor。

## 2. Current Behaviour

- [verified] `index.html:23-56,124-205` 把菜单、单层 ribbon、左右面板和页面样式写在一个内嵌 HTML/CSS 中；工具栏分组会换行，窄屏直接隐藏导航与侧栏。
- [verified] `main.ts:65-110` 已有 `DocumentController`、`CommandRegistry`、HTTP/Tauri gateway 和集中 `refreshUI`，但 `main.ts` 仍负责大量具体 DOM 绑定。
- [verified] `command-registry.ts:16-73` 的命令目前只有 id、label、shortcut、active、enabled、run，缺少 tab/group/surface/icon/disabled-reason 等投影元数据。
- [verified] `panels.ts:28-156` 能渲染大纲、修订、标注、损失和状态，但没有统一的可折叠面板/页面缩略图/上下文检查器模型。
- [verified] `preview.ts:90-182` 已有独立预览浮层、Paged.js 等待、未分页回退、页数和焦点恢复；它是现有预览边界，不应被 ribbon 重构破坏。
- [verified] `gateway.ts:123-171` 已把文件、保存、预览、任务、恢复和最近文件能力抽象为 `DocumentGateway`；`tauri.conf.json:6-21` 让桌面直接使用 `editor/dist`。

## 3. Relevant Architecture

Athanor 当前是“TypeScript 交互壳 + ProseMirror 编辑模型 + Rust 文档服务”的结构：`main.ts` 装配命令与控制器，`DocumentController` 处理挂载、输入、保存、任务和预览，gateway 隔离桌面与 HTTP，Vite 单文件构建产物供 Tauri 和浏览器使用。

OpenDoc 的可借鉴部分是产品壳与交互契约，而不是其底层文档引擎：它的 README 将规范化可编辑模型、确定性渲染、可嵌入宿主和浏览器 WASM 编辑器作为边界；在线编辑器实际展示了 File/Edit/View/Insert/Format/Review/Tools/Help 菜单、Home/Insert/Table/View/Review 标签、上下文表格工具、Outline/Pages/Comments 侧栏、模式切换、查找替换和状态栏。

OpenDoc 的编辑器架构文档还强调三层边界：核心负责模型/布局/命中测试，前端负责输入/滚动/缩放/工具栏，宿主负责文件和字体。对 Athanor 的对应策略是先保持 ProseMirror/Rust gateway，建立相同的 UI 边界；只有在 DOM 编辑无法满足真正分页编辑时，才另立渲染 surface 评估 WASM/Canvas。

### OpenDoc 参考基线与已观察界面

参考源码固定到 `26b4298fa39a176fcad18f45e336cec80ee5a5f8`。OpenDoc 官方说明编辑器为 pre-release developer surface；这里的“成熟”指可借鉴的交互组织，不代表稳定 SDK 或完整 Word 替代品。[官方 README](https://github.com/CasualOffice/opendoc/blob/26b4298fa39a176fcad18f45e336cec80ee5a5f8/README.md)

主要参考：[壳层设计](https://github.com/CasualOffice/opendoc/blob/26b4298fa39a176fcad18f45e336cec80ee5a5f8/docs/63-EDITOR-UI-UX-DESIGN-SYSTEM.md)、[功能区与交互](https://github.com/CasualOffice/opendoc/blob/26b4298fa39a176fcad18f45e336cec80ee5a5f8/docs/64-EDITOR-TOOLBAR-RIBBON-DESIGN.md)、[编辑器与渲染边界](https://github.com/CasualOffice/opendoc/blob/26b4298fa39a176fcad18f45e336cec80ee5a5f8/docs/56-EDITOR-SHELL-AND-RENDER-ARCHITECTURE.md)、[HTML 实现](https://github.com/CasualOffice/opendoc/blob/26b4298fa39a176fcad18f45e336cec80ee5a5f8/webapp/editor.html)。文档与源码存在时序差异（例如文档声称暂未实现的格式刷，在当前 HTML 已出现），实现者应以固定提交源码和实际行为为准，不复制旧文档能力清单。

本轮浏览器观察在线演示：深色壳、白色纸张、分组两行工具、标尺、左侧窄导航轨、底部统计和缩放；页面已载入 sample.docx，显示 15 页。在线部署还有 Layout/References 标签，可能与固定提交不同；观察不等于所有按钮已功能验收。方案借鉴区域层级与视觉密度，不要求颜色、图标或品牌完全一致。

## 4. GitNexus Findings

- [graph] `CommandRegistry` 的直接上游是 `main.ts`，风险为 LOW；因此可先扩展命令描述和 UI 投影，不必重写 gateway。
- [graph] `DocumentController` 的直接上游是 `main.ts`，风险为 LOW；编辑器输入、保存、任务、恢复和关闭逻辑仍应由 controller 维护，UI 组件只调用命令。
- [graph] `PreviewSurface.open` 由 `main.ts:openPreview` 调用，影响预览关闭、状态、渲染和 gateway `renderPreview` 流程；图谱报告一个 receiver 未解析，实际影响按下界处理并以源码验证为准。
- [graph] 查询命中 `Boot → NewDocument`、`OpenPreview → RenderPreview`、`Save → Manifest_value` 等流程；变更 ribbon/面板时必须保留启动、保存、预览和关闭的顺序。
- [verified] 现有前端测试脚本为 `npm test`、`npm run test:e2e`，Playwright 由 Vite dev server 提供 `127.0.0.1:4173`；这些是 GUI 方案的固定验证入口。

## 5. Statement-Level PDG Findings

本轮没有可用的独立 `pdg_query` MCP 调用面，因此不伪造语句级边；`analyze --index-only --pdg` 已完成，以下顺序约束由当前源码核实：

- [verified] `main.ts` 在 `boot()` 中先绑定命令/键盘，再创建或打开文档，最后 `refreshUI()`；新 shell 必须保持“注册 → boot → mount → 状态刷新”的顺序。
- [verified] `DocumentController.createEditor` 将输入、paste、IME、drop、mousedown 和 dispatchTransaction 汇入 controller；新 UI 的格式动作仍经 PM transaction，文件与会话动作经 controller；不得直接改 DOM 代替模型事务。
- [verified] `PreviewSurface.open` 先 `close()` 使旧序列失效，再创建 modal iframe，分页成功后才启用打印按钮，失败走 `print_html` 回退；View tab/command palette 只能调用这一入口。
- [verified] `renderSidebar` 将历史、标注、损失和警告从 `DocResponse` 派生到 DOM；右侧检查器必须继续是派生视图，不能把 UI 状态写回文档。
- [inferred] ribbon overflow、菜单、快捷键、命令面板和上下文 tab 若各自维护动作，会重新引入行为漂移；因此所有 surfaces 必须投影同一 registry 描述。

## 6. Proposed Changes

### 6.1 UI shell and design tokens

- 文件：`engine/crates/aludel/editor/index.html`、新增 `src/ui/shell.ts`、`src/ui/tokens.css`、`src/ui/shell.css`。
- 将内嵌页面拆为语义化 shell：top bar、application menu、tab strip、ribbon panel、left rail/panel、document viewport、right context panel、status bar、modal/command overlay。
- 借鉴 OpenDoc 的“功能真实才显示”规则：没有可执行命令、状态或数据的按钮/面板不进入默认布局。
- 设计 token 覆盖浅色/深色主题、accent、间距、控件尺寸、边框、焦点环；保存主题偏好到 localStorage，文档主题仍由现有 page theme 控制。
- ribbon 默认无横向滚动；按容器宽度把低优先级 group 收入 overflow 菜单，Clipboard/Editing/Mode 等关键组优先保留；支持折叠 ribbon 并记忆选择。

### 6.2 Command registry as the single UI contract

- 文件：`src/app/command-registry.ts`、`src/main.ts`，必要时新增 `src/app/command-surfaces.ts`。
- 扩展 `Command`：`tab`、`group`、`order`、`icon`、`surfaces`、`requiresSelection`、`isVisible`、`disabledReason`、`context`，保留现有 `enabled/active/run`。
- 由 registry 生成 application menu、ribbon、overflow、command palette、快捷键和 context menu；禁止在每个 surface 重新定义同一动作。
- 将 `main.ts` 中的 `bindButton`/静态 bindings 降为装配调用；按命令的 focusPolicy 恢复焦点：即时格式命令返回正文，查找/弹窗/预览保留自身焦点，关闭后回到触发控件；统一刷新所有同 ID 控件的 active/enabled/dirty 状态。
- 为菜单键盘导航、Escape 关闭、外部 pointer-down、焦点恢复和快捷键冲突建立统一 overlay controller。

### 6.3 OpenDoc-inspired ribbon and contextual tools

- Home：撤销/重做、剪贴板、字体/字号/颜色、段落对齐/列表/缩进、样式、查找替换。编辑/阅读模式需要明确实现 PM editable 与命令限制后才显示；建议模式暂不显示。
- Insert：表格、图片、链接、脚注、数学、分隔线、分页符和导入等当前已有命令；字段插入是后续能力；未实现的 Shapes/Dictate 等不显示。
- View：大纲/页面/检查面板开关、缩放、打印预览、ribbon 折叠、主题。
- Review（界面中文名“文档检查”）：修订历史、损耗/警告、标注、verify、恢复入口；只有已有 gateway 能力时才启用。
- Table：仅在光标位于表格时出现，包含加删行列、合并/拆分、表头、表格属性；没有表格选择时不显示或禁用。
- 选区存在时显示轻量 floating formatting toolbar，桌面和浏览器走同一命令；不要把右侧 panel 变成常驻空白占位。

### 6.4 Navigation, context panels, and status

- `panels.ts` 拆分为 `outline-panel.ts`、`history-panel.ts`、`inspector-panel.ts`、`status-bar.ts`，保留纯派生渲染原则。
- 左侧 rail 首先提供 Outline；Pages 只有在能展示真实预览页缩略图时加入；Search 复用现有 find/replace。
- 右侧 context panel 根据 selection context 显示段落/字符格式、图片 alt/尺寸、表格属性、修订与损耗；没有上下文时折叠而不是显示空状态。
- 状态栏统一显示保存状态、字数/段落数、当前页/总页数（仅对应未失效的预览快照；正文变化即失效）、编辑模式、语言和缩放；不要声称云同步。
- 通过 `aria-label`、`role=tablist/tab/tabpanel`、`aria-pressed`、live region 和焦点环保持现有 E6 可访问性约束。

### 6.5 Editor interaction boundary

- `DocumentController` 保持 PM 编辑真源、IME、paste/drop、asset staging、dirty/save/epoch 逻辑；新增 selection-context 只读事件供 ribbon/inspector 使用。
- 扩展 `format.ts` 的 active format 与新增选区归并 selector 作为 toolbar 反射来源：混合选区显示 Mixed/空值，不能仅读取最后一个 run；格式命令仍通过 transaction。
- 当前正文继续使用可编辑 DOM；先约定 surface 边界；首期只抽取实际需要的 focus/selection/preview 接口，避免创建无实现的 PagedSurface 框架。首期不引入 Canvas/WASM 以免同时重写模型与分页。
- 页码与分页仍由 `PreviewSurface`/Rust preview 产生；正文显示“分页符”节点和预览入口，避免把浏览器连续布局误报为 Word 真实分页。

### 6.6 Host/build boundary

- 保留 `editor/dist` 单文件产物和 `tauri.conf.json:6-8` 的 frontendDist 约定；构建前端后再构建 Tauri。
- 通过 `DocumentGateway` 暴露能力矩阵，让 HTTP 模式隐藏桌面文件对话框而不是显示死按钮。
- 未来若引入 OpenDoc 类 WASM/Canvas 渲染，只能作为独立 `PagedSurface`/worker feature flag，CPU/DOM preview 仍是回退和验收基线。

### 6.7 建议视觉规格与界面草图

以下尺寸为设计建议，G0 用真实 1280×820 桌面和 900×600 最小窗口确认：应用栏约 48–56px，标签栏 32px，展开功能区约 88–100px，状态栏 28px；导航轨 44px，大纲面板 220–260px，属性面板 280–320px。控件至少 30–32px 高，使用统一 SVG 图标与中文 tooltip。页面默认白色；深色主题只调整应用界面。字体优先系统中文栈，图标/资源本地打包，不增加运行时 CDN 依赖。

```text
Athanor   文档名称 · 未保存             命令搜索     保存   设置
文件  编辑  插入  视图  文档检查
开始 | 插入 | 视图 | 文档检查 | 表格（按选区出现）
撤销重做 | 字体/字号/颜色 | 段落 | 样式卡片 | 查找替换 | 更多
大纲/查找轨 | 可折叠大纲 |       白色文档编辑区       | 按需属性面板
字数 · 保存状态/任务提示                      连续编辑/预览 · 缩放
```

窄于 1100px 时面板变为可打开的覆盖抽屉，不直接消失；窄于 900px 时应用菜单与低优先级功能组进入“更多”。桌面最小宽度仍为 900px；768px 仅作为浏览器窄窗目标。长文档允许编辑画布自身滚动，不让功能区出现横向滚动。未实现的标尺拖拽、页面缩略图和建议模式不做装饰占位。

### 6.8 UI 数据模型与典型流程

新增状态（建议类型，尚未实现）：

- `ShellState`：activeTab、ribbonCollapsed、leftPanel、rightPanel、theme、zoom、openOverlay；仅偏好持久化，不写进 azodoc、dirty 或 undo。
- `SelectionContext`：sessionEpoch、selectionKind(text/table/image)、bookmark、paragraph/character 值和 mixed 标记；由 PM state 派生，UI 不持有第二份正文。
- `CommandState`：visible、enabled、active/mixed、disabledReason、label；`CommandSpec` 另加参数、keywords、icon 与 focusPolicy。菜单和 palette 触发参数化字体命令时打开 picker，不再执行现有空 run。
- `PreviewState`：idle/loading/ready/fallback/error、requestSeq、content/theme fingerprint；模型变化后清除过期页数，保持连续编辑状态与分页预览状态分开。

选文字→打开字体 picker：保存带 epoch 的 PM bookmark；输入字体时焦点留在 picker；确认时映射到当前事务并校验同一会话；应用一次 transaction；取消不写入，完成后恢复正文。期间文档关闭或换文档则丢弃旧 bookmark。

插入表格→单元格选择：上下文 Table tab 出现，命令用 PM dry-run 判定能否合并/拆分；离开表格恢复上一普通标签。不能只依据“有文档”启用表格命令。图片面板同样只提供已支持的尺寸、替代文字能力。

关闭与异步：保留已修复的 epoch 失效规则。弹窗、预览、资产暂存和后台任务必须丢弃旧会话响应；切换 ribbon/theme/panel 不销毁 EditorView，不清空 undo 或中断 IME。

### 6.9 构建与启动交付

保留 TypeScript + Vite + ProseMirror，首期不引入 React/Vue 或新文档引擎。把 CSS/TS 组织成模块，仍由 singlefile 构建到 dist。新增一个统一 Windows 启动脚本（建议 `scripts/start-studio.ps1`）：检查 Node/Rust/WebView2→按锁文件安装前端依赖→构建前端→运行 studio；任何子进程失败即停止，不能启动旧 dist。

现有入口已经可以用于实施期间的完整桌面验收：

```powershell
Set-Location E:\Athanor_Studio\Athanor_Studio\engine\crates\aludel\editor
npm ci
npm run build
Set-Location E:\Athanor_Studio\Athanor_Studio\engine
cargo run -p studio
```

`cargo run -p aludel -- <doc.azodoc>` 是已有单文档 HTTP 模式。`npm run dev` 当前没有后端代理；若增加热更新开发入口，需同时配置本地 API proxy/真实 aludel 进程，或 Tauri devUrl，不能将单独 Vite 页面宣称为完整可用编辑器。CI 增加“构建后 dist 与来源一致”校验；发布验收必须使用刚构建的 studio，不只跑 mock API 的 Playwright。无需更改现有文档格式。

## 7. Implementation Sequence

1. 冻结命令目录和状态矩阵：列出每个命令的 tab/group/surface/shortcut/能力条件/disabled reason，标出当前真实可用与明确延后的项。
2. 建立 tokens、shell layout、theme、focus/overlay controller；先接现有命令，不改变文档语义。
3. 扩展 CommandRegistry 并由它生成 application menu、Home/Insert/View/Review ribbon、overflow 和 command palette；删除重复 DOM 绑定。
4. 拆分左 rail、Outline、History/Review、右 context inspector、status bar；接入现有 `renderSidebar` 数据和 selection context。
5. 加入 Table contextual tab、floating toolbar、格式反射和响应式断点；保持 table-commands/format/controller 事务路径不变。
6. 将 PreviewSurface 接入 View tab、状态栏页数和预览 modal；验证分页失败仍走原回退路径。
7. 建立浏览器和 Tauri 两个 host 的能力投影，刷新 `dist/index.html`，运行构建和 Windows smoke。
8. 最后再评估 `PagedSurface`/WASM；只有 DOM surface 的性能或分页一致性验收失败时才启动该分支，不把它与本轮 shell 改造混合。

### 阶段交付与退出条件

| 阶段 | 交付 | 退出条件 |
| --- | --- | --- |
| G0 | 命令清单、桌面/HTTP 能力矩阵、浅/深色与窄窗可点击壳原型 | 常用任务入口可发现；区分已实现、需连接、延期能力 |
| G1 | tokens、shell、统一命令/焦点契约、菜单、开始功能区 | 多入口执行一致；打开字体/查找不丢选区或抢焦点 |
| G2 | 插入/视图/文档检查、overflow、命令搜索、上下文表格 | 键盘可完整操作；窄屏所有现有命令仍可达 |
| G3 | 大纲/查找抽屉、属性/历史面板、状态与预览整合 | 切换面板不影响编辑历史；页数失效和回退表达正确 |
| G4 | 构建启动脚本、真实桌面验收、视觉与性能回归 | 从前端源码可重复构建并启动同版桌面；无保存/导出回归 |

每阶段独立提交；修改现有符号前重新执行 impact，提交前运行 detect_changes。最终阶段统一更新视觉基线，构建产物每个可运行交付点同步。

## 8. Test Strategy

- 单元测试：命令描述到 menu/ribbon/palette 的投影一致性；enabled/active/disabled reason；tab/context 条件；overflow 优先级；折叠状态持久化；主题 token；焦点恢复；混合选区格式反射。
- Playwright：Home/Insert/View/Review/Table tab 切换；表格选中时 Table tab 出现且离开表格消失；命令面板搜索、执行、Escape；ribbon overflow 在 1280/900/768/窄视口下无横向滚动；左大纲跳转；右 inspector 根据选区变化；暗色主题；Ctrl/Cmd+S/B/F/P；IME、paste、拖图、预览关闭焦点恢复。
- 回归：`workspace.spec.ts`、`e3.spec.ts`、`e4.spec.ts`、`e5.spec.ts`、`e6.spec.ts` 保留现有保存、预览、表格、粘贴、可访问性、DPI 和性能场景；新增 `ribbon.spec.ts`、`command-palette.spec.ts`、`responsive-shell.spec.ts`。
- 桌面：`scripts/windows-smoke.ps1` 验证真实 Tauri 启动；人工检查打开→编辑→保存→关闭、文件菜单和系统打印入口。
- 命令：`cd engine/crates/aludel/editor && npm ci && npm run build && npm test -- --reporter=dot && npm run test:e2e`；`cd engine && cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`；Windows 运行 `scripts/windows-smoke.ps1`。

补充验收：同一命令在三个入口都正确更新；混合字体/颜色选区反射不写回正文；弹窗未提交时关闭文档不会复活旧状态；900×600、1280×820、1920×1080 与系统 150%/200% DPI 无裁切。500 段输入/打开测试在固定硬件、固定文档和字体下比较 G0 基线，p95 退化超过 20% 时必须解释和修正；合成 IME 测试外人工使用中文输入法检查候选、回车与撤销。视觉基线只约束应用界面和受控字体，不声称与 Word 像素一致。

## 9. Risk and Impact Analysis

- 高风险边界：`main.ts` 是 UI 装配中心；拆 shell 时必须保留 boot/mount/save/preview 顺序。
- 中风险：`DocumentController` 同时处理输入和生命周期；只增加只读 selection events，不把 UI 状态搬入 controller。
- 中风险：`PreviewSurface.open` receiver 图谱为 lower-bound；任何预览 surface 变化都要跑 e4、paged preview 和 PDF 出版回归。
- 中风险：响应式 overflow 容易造成隐藏命令不可发现；所有 overflow 命令必须仍可由 palette 和快捷键访问。
- 兼容性：HTTP gateway 缺少桌面对话框能力；按钮必须由 capability/desktop 条件隐藏或解释禁用。
- 性能：避免每次 selection 变化重建整棵 DOM；只更新 active/enabled/context panel，长文档输入继续由现有 debounce/PM 机制承载。
- 可访问性：tab、menu、dialog、palette 的焦点管理和 Esc 行为必须有自动化测试；不能只凭视觉验收。
- 迁移：保留现有 `index.html`/dist 的单文件构建方式，不在本计划中改变文档格式、gateway contract 或 Rust 内容模型。

## 10. Files Expected to Change

| File | Symbols / surface | Reason |
| --- | --- | --- |
| `engine/crates/aludel/editor/index.html` | shell mount points | 从平铺 HTML 过渡到语义 shell；保留 dist 生成入口 |
| `engine/crates/aludel/editor/src/ui/{shell,tokens,menus,ribbon,panels,status}.ts/css` | new UI modules | OpenDoc-style shell、主题、tab、overflow、面板和状态栏 |
| `engine/crates/aludel/editor/src/app/command-registry.ts` | `CommandRegistry`, `Command` | 单一命令描述与多 surface 投影 |
| `engine/crates/aludel/editor/src/main.ts` | boot/refresh/bindings | 只负责装配，去除重复 UI 行为 |
| `engine/crates/aludel/editor/src/app/document-controller.ts` | selection/context hooks | 只读选区上下文，保留输入/保存/epoch 边界 |
| `engine/crates/aludel/editor/src/app/preview.ts` | `PreviewSurface` | 接入 View tab 与统一 overlay/focus |
| `engine/crates/aludel/editor/src/ui/panels.ts` | current render helpers | 拆成具体 panel/inspector/status 模块 |
| `engine/crates/aludel/editor/src/platform/gateway.ts` | capability descriptors | 让 UI 明确投影 HTTP/Tauri 能力 |
| `engine/crates/aludel/editor/tests/e2e/*.spec.ts` | new shell tests | ribbon/palette/responsive/contextual behavior |
| `engine/crates/aludel/editor/src/**/*.test.ts` | command/surface tests | 不依赖浏览器验证纯状态和投影规则 |
| `engine/crates/aludel/editor/dist/index.html` | generated artifact | `npm run build` 后同步 Tauri/HTTP 嵌入产物 |
| `engine/crates/studio/tauri.conf.json` | window/CSP only if needed | 仅在桌面壳需要主题字体或窗口行为时调整 |

## 11. Reusable Implementation Context

```yaml
implementation_context:
  task_summary: Build an OpenDoc-inspired, Word-like Athanor frontend shell while preserving ProseMirror/Rust/gateway boundaries.
  acceptance_criteria:
  - Every visible command is functional, capability-aware, keyboard reachable, and represented consistently across menu/ribbon/palette/shortcut.
  - Home/Insert/View/Review and contextual Table workflows work at desktop and narrow responsive widths without horizontal
    ribbon scrolling.
  - Existing document editing, save/history/assets, preview fallback, accessibility, and Tauri packaging tests remain green.
  - The plan does not silently replace the current PM source of truth or claim continuous DOM layout is true Word pagination.
  evidence_provenance:
    schema_version: 2
    head_commit: 190c0b673043c377e5fb0c9ebf3082aaf3d1c408
    generated_plan_path: docs/plans/2026-09-18-gitnexus-plan-opendoc-gui-build.md
    global_dirty_digest:
      algorithm: sha256
      canonicalization: gitnexus-evidence-provenance-v2 NUL-framed UTF-8 records
      value: 0a9c85780067d9afcd0764f307b60891e3cee927ee11eaeb5ec7826d10fd82cd
    cited_path_manifest:
    - path: docs/editor.md
      object_kind:
        head: regular
        index: regular
        worktree: regular
        untracked: absent
      state: clean
      rename_from: null
      rename_to: null
      head_digest: sha256:47d4638d3322f3432374a31208d39259a90041767186e08fcc6d0426b88436ca
      index_digest: sha256:47d4638d3322f3432374a31208d39259a90041767186e08fcc6d0426b88436ca
      worktree_digest: sha256:47d4638d3322f3432374a31208d39259a90041767186e08fcc6d0426b88436ca
      untracked_digest: absent
    - path: engine/crates/aludel/editor/index.html
      object_kind:
        head: regular
        index: regular
        worktree: regular
        untracked: absent
      state: clean
      rename_from: null
      rename_to: null
      head_digest: sha256:a33ffb35d3ef2880e201cc3be8c4adbea20d35a360f4a38a9dc7bc8b41c5483c
      index_digest: sha256:a33ffb35d3ef2880e201cc3be8c4adbea20d35a360f4a38a9dc7bc8b41c5483c
      worktree_digest: sha256:a33ffb35d3ef2880e201cc3be8c4adbea20d35a360f4a38a9dc7bc8b41c5483c
      untracked_digest: absent
    - path: engine/crates/aludel/editor/package.json
      object_kind:
        head: regular
        index: regular
        worktree: regular
        untracked: absent
      state: clean
      rename_from: null
      rename_to: null
      head_digest: sha256:b87e7d294a5a3e14b6e1e03ba52895ad0203fc49da8e1f1291488d1b5c09dfda
      index_digest: sha256:b87e7d294a5a3e14b6e1e03ba52895ad0203fc49da8e1f1291488d1b5c09dfda
      worktree_digest: sha256:b87e7d294a5a3e14b6e1e03ba52895ad0203fc49da8e1f1291488d1b5c09dfda
      untracked_digest: absent
    - path: engine/crates/aludel/editor/playwright.config.ts
      object_kind:
        head: regular
        index: regular
        worktree: regular
        untracked: absent
      state: clean
      rename_from: null
      rename_to: null
      head_digest: sha256:c29c8100a78cf9350cf4c3614894930d77f7f73d83af9a0f6fec6d55493dda53
      index_digest: sha256:c29c8100a78cf9350cf4c3614894930d77f7f73d83af9a0f6fec6d55493dda53
      worktree_digest: sha256:c29c8100a78cf9350cf4c3614894930d77f7f73d83af9a0f6fec6d55493dda53
      untracked_digest: absent
    - path: engine/crates/aludel/editor/src/app/command-registry.ts
      object_kind:
        head: regular
        index: regular
        worktree: regular
        untracked: absent
      state: clean
      rename_from: null
      rename_to: null
      head_digest: sha256:910e5a22aa7de852e4d80d2dbe06eac75670bdfde8e00b7f2efde3bafc023876
      index_digest: sha256:910e5a22aa7de852e4d80d2dbe06eac75670bdfde8e00b7f2efde3bafc023876
      worktree_digest: sha256:910e5a22aa7de852e4d80d2dbe06eac75670bdfde8e00b7f2efde3bafc023876
      untracked_digest: absent
    - path: engine/crates/aludel/editor/src/app/document-controller.ts
      object_kind:
        head: regular
        index: regular
        worktree: regular
        untracked: absent
      state: clean
      rename_from: null
      rename_to: null
      head_digest: sha256:ad31d63e782572343071c7c65cf6853b86c418af05816b49bf87f5c42bbde954
      index_digest: sha256:ad31d63e782572343071c7c65cf6853b86c418af05816b49bf87f5c42bbde954
      worktree_digest: sha256:ad31d63e782572343071c7c65cf6853b86c418af05816b49bf87f5c42bbde954
      untracked_digest: absent
    - path: engine/crates/aludel/editor/src/app/format.ts
      object_kind:
        head: regular
        index: regular
        worktree: regular
        untracked: absent
      state: clean
      rename_from: null
      rename_to: null
      head_digest: sha256:3caadb8d49881e7da2ba045649e72c34f82792d2920ebf6b2faf945992754b62
      index_digest: sha256:3caadb8d49881e7da2ba045649e72c34f82792d2920ebf6b2faf945992754b62
      worktree_digest: sha256:3caadb8d49881e7da2ba045649e72c34f82792d2920ebf6b2faf945992754b62
      untracked_digest: absent
    - path: engine/crates/aludel/editor/src/app/preview.ts
      object_kind:
        head: regular
        index: regular
        worktree: regular
        untracked: absent
      state: clean
      rename_from: null
      rename_to: null
      head_digest: sha256:24764387f6dc5018ac61fb36171b49ce45db4d3319966b0c50344c4b24e329ec
      index_digest: sha256:24764387f6dc5018ac61fb36171b49ce45db4d3319966b0c50344c4b24e329ec
      worktree_digest: sha256:24764387f6dc5018ac61fb36171b49ce45db4d3319966b0c50344c4b24e329ec
      untracked_digest: absent
    - path: engine/crates/aludel/editor/src/main.ts
      object_kind:
        head: regular
        index: regular
        worktree: regular
        untracked: absent
      state: clean
      rename_from: null
      rename_to: null
      head_digest: sha256:e4acfc153d0c645e293b6652e6895762f5517e35586fcc7e5c81e53010941b7e
      index_digest: sha256:e4acfc153d0c645e293b6652e6895762f5517e35586fcc7e5c81e53010941b7e
      worktree_digest: sha256:e4acfc153d0c645e293b6652e6895762f5517e35586fcc7e5c81e53010941b7e
      untracked_digest: absent
    - path: engine/crates/aludel/editor/src/platform/gateway.ts
      object_kind:
        head: regular
        index: regular
        worktree: regular
        untracked: absent
      state: clean
      rename_from: null
      rename_to: null
      head_digest: sha256:5ac8b0c122c0e8deb5ad0008538dcfed081ba7ed01ddefaa45ceffa219ba5adc
      index_digest: sha256:5ac8b0c122c0e8deb5ad0008538dcfed081ba7ed01ddefaa45ceffa219ba5adc
      worktree_digest: sha256:5ac8b0c122c0e8deb5ad0008538dcfed081ba7ed01ddefaa45ceffa219ba5adc
      untracked_digest: absent
    - path: engine/crates/aludel/editor/src/ui/panels.ts
      object_kind:
        head: regular
        index: regular
        worktree: regular
        untracked: absent
      state: clean
      rename_from: null
      rename_to: null
      head_digest: sha256:49014926c79b8cd3dbc4edf851d9e09a6a4a66feeed69d6c26f8bf6161310d10
      index_digest: sha256:49014926c79b8cd3dbc4edf851d9e09a6a4a66feeed69d6c26f8bf6161310d10
      worktree_digest: sha256:49014926c79b8cd3dbc4edf851d9e09a6a4a66feeed69d6c26f8bf6161310d10
      untracked_digest: absent
    - path: engine/crates/aludel/editor/tests/e2e/e6.spec.ts
      object_kind:
        head: regular
        index: regular
        worktree: regular
        untracked: absent
      state: clean
      rename_from: null
      rename_to: null
      head_digest: sha256:c5df2ee764c76cdb0dbf30d1233e6e7d0d0dacbd8d0278067777b338b068e03b
      index_digest: sha256:c5df2ee764c76cdb0dbf30d1233e6e7d0d0dacbd8d0278067777b338b068e03b
      worktree_digest: sha256:c5df2ee764c76cdb0dbf30d1233e6e7d0d0dacbd8d0278067777b338b068e03b
      untracked_digest: absent
    - path: engine/crates/aludel/editor/vite.config.ts
      object_kind:
        head: regular
        index: regular
        worktree: regular
        untracked: absent
      state: clean
      rename_from: null
      rename_to: null
      head_digest: sha256:54e18518c03bcaee4a2f7533aa05f6228c70eab04b8cf7f95ca1140a8bffe3ee
      index_digest: sha256:54e18518c03bcaee4a2f7533aa05f6228c70eab04b8cf7f95ca1140a8bffe3ee
      worktree_digest: sha256:54e18518c03bcaee4a2f7533aa05f6228c70eab04b8cf7f95ca1140a8bffe3ee
      untracked_digest: absent
    - path: engine/crates/studio/Cargo.toml
      object_kind:
        head: regular
        index: regular
        worktree: regular
        untracked: absent
      state: clean
      rename_from: null
      rename_to: null
      head_digest: sha256:0db060dd2221945c39944b6a630677d4411e905d943c9e7b9249ddc7f90ed791
      index_digest: sha256:0db060dd2221945c39944b6a630677d4411e905d943c9e7b9249ddc7f90ed791
      worktree_digest: sha256:0db060dd2221945c39944b6a630677d4411e905d943c9e7b9249ddc7f90ed791
      untracked_digest: absent
    - path: engine/crates/studio/build.rs
      object_kind:
        head: regular
        index: regular
        worktree: regular
        untracked: absent
      state: clean
      rename_from: null
      rename_to: null
      head_digest: sha256:76fd149d33ec7fc2bba2b34aaa54beb0e2eccf891fefdec38db25b5f3d56ab93
      index_digest: sha256:76fd149d33ec7fc2bba2b34aaa54beb0e2eccf891fefdec38db25b5f3d56ab93
      worktree_digest: sha256:76fd149d33ec7fc2bba2b34aaa54beb0e2eccf891fefdec38db25b5f3d56ab93
      untracked_digest: absent
    - path: engine/crates/studio/tauri.conf.json
      object_kind:
        head: regular
        index: regular
        worktree: regular
        untracked: absent
      state: clean
      rename_from: null
      rename_to: null
      head_digest: sha256:c4454df021a9075d4f7977d6e936078763896e602e1040d172b8d3c222598365
      index_digest: sha256:c4454df021a9075d4f7977d6e936078763896e602e1040d172b8d3c222598365
      worktree_digest: sha256:c4454df021a9075d4f7977d6e936078763896e602e1040d172b8d3c222598365
      untracked_digest: absent
    - path: scripts/windows-smoke.ps1
      object_kind:
        head: regular
        index: regular
        worktree: regular
        untracked: absent
      state: clean
      rename_from: null
      rename_to: null
      head_digest: sha256:1b5e9535ac2fa0f7b9d8815d0e5fc733c827c54035ce4766da9c0c0df070f5b8
      index_digest: sha256:1b5e9535ac2fa0f7b9d8815d0e5fc733c827c54035ce4766da9c0c0df070f5b8
      worktree_digest: sha256:1b5e9535ac2fa0f7b9d8815d0e5fc733c827c54035ce4766da9c0c0df070f5b8
      untracked_digest: absent
  primary_symbols:
  - symbol: CommandRegistry
    file: engine/crates/aludel/editor/src/app/command-registry.ts
    lines: 16-73
    role: single command metadata and execution registry
  - symbol: DocumentController
    file: engine/crates/aludel/editor/src/app/document-controller.ts
    lines: 55-820
    role: PM editor lifecycle, input, save, job, preview and close boundary
  - symbol: PreviewSurface.open
    file: engine/crates/aludel/editor/src/app/preview.ts
    lines: 90-182
    role: paged preview modal, fallback, print and focus restoration
  - symbol: boot / refreshUI / openPreview
    file: engine/crates/aludel/editor/src/main.ts
    lines: 65-110, 420-760
    role: frontend assembly and command/surface wiring
  related_symbols:
  - relationship: renders
    symbol: renderSidebar / renderOutline / renderStatusBar
    file: engine/crates/aludel/editor/src/ui/panels.ts
  - relationship: platform boundary
    symbol: DocumentGateway
    file: engine/crates/aludel/editor/src/platform/gateway.ts
  - relationship: desktop frontend host
    symbol: frontendDist
    file: engine/crates/studio/tauri.conf.json
  execution_path:
  - boot registers command handlers and host gateway
  - DocumentController mounts ProseMirror and emits state/outline/stats
  - registry projections update menu/ribbon/palette/status enabled and active state
  - selection context updates floating toolbar and right inspector
  - preview command calls gateway renderPreview then PreviewSurface.open
  - save/verify/history/assets remain on existing gateway/controller paths
  pdg_constraints: []
  architectural_patterns:
  - pattern: single command registry with enabled/active state
    example_location: engine/crates/aludel/editor/src/app/command-registry.ts
    usage_guidance: Extend metadata and project it; do not create per-surface command implementations.
  - pattern: gateway host abstraction
    example_location: engine/crates/aludel/editor/src/platform/gateway.ts
    usage_guidance: Keep browser and Tauri capability differences explicit.
  - pattern: derived UI rendering
    example_location: engine/crates/aludel/editor/src/ui/panels.ts
    usage_guidance: Panels consume DocResponse/state and never mutate the document directly.
  files_to_modify:
  - file: engine/crates/aludel/editor/index.html and new src/ui modules
    symbols:
    - shell
    - ribbon
    - menus
    - panels
    - status
    intended_change: semantic responsive shell and OpenDoc-inspired visual system
  - file: engine/crates/aludel/editor/src/app/command-registry.ts
    symbols:
    - Command
    - CommandRegistry
    intended_change: metadata-rich single source for every UI surface
  - file: engine/crates/aludel/editor/src/main.ts
    symbols:
    - boot
    - refreshUI
    - openPreview
    intended_change: thin assembly and projection wiring
  - file: engine/crates/aludel/editor/src/app/document-controller.ts
    symbols:
    - DocumentController
    intended_change: read-only selection/context events only
  tests:
  - file: engine/crates/aludel/editor/src/app/*.test.ts
    scenarios:
    - command projection → all surfaces expose same id/shortcut
    - context change → enabled/active/disabled reason updates
    - theme/overflow preference → persisted and restored
  - file: engine/crates/aludel/editor/tests/e2e/ribbon.spec.ts
    scenarios:
    - Home/Insert/View/Review switch
    - Table tab appears only in table
    - overflow/palette execution
  - file: engine/crates/aludel/editor/tests/e2e/responsive-shell.spec.ts
    scenarios:
    - 1280/900/768 widths
    - no horizontal scroll
    - focus and Escape restoration
  - file: engine/crates/aludel/editor/tests/e2e/workspace.spec.ts and e6.spec.ts
    scenarios:
    - save/preview/IME/accessibility/DPI regressions remain green
  verification_commands:
  - cd engine/crates/aludel/editor && npm ci && npm run build && npm test -- --reporter=dot && npm run test:e2e
  - cd engine && cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
  - powershell -NoProfile -ExecutionPolicy Bypass -File scripts/windows-smoke.ps1
  risks:
  - main.ts and index.html are high-churn UI assembly points; preserve controller/gateway ordering.
  - PreviewSurface impact is lower-bound because one receiver was unresolved in the graph.
  - A paginated Canvas/WASM rewrite is intentionally deferred and must not be smuggled into shell work.
  assumptions:
  - 第一版包含开始/插入/视图/文档检查与上下文表格；文档检查只包含现有历史、标注、损耗与校验能力。
  - 页面缩略图限预览模式；不在连续编辑模式显示过期页码。
  - 首版不添加建议修订、线程批注或字段编辑模型。
  open_questions:
  - 后续独立分页编辑路线尚未选择；主线保留 DOM 编辑和分页预览。
  avoid:
  - Do not copy OpenDoc's DOCX model or WASM renderer into Athanor as part of this GUI plan.
  - Do not expose nonfunctional placeholder buttons or empty side panels.
  - Do not let menus, ribbon, palette and shortcuts implement separate command logic.
  - Do not bypass DocumentController, gateway capabilities, or preview stale-result guards.
  source_constraints:
  - description: 命令经 PM transaction 或 controller 执行；菜单/面板管理焦点，不统一抢回正文；异步 dialog/preview 以 epoch/generation 检查有效性。
  external_references:
    opendoc_commit: 26b4298fa39a176fcad18f45e336cec80ee5a5f8
    demo_url: https://opendoc.casualoffice.org/editor.html?demo=1
    demo_note: 在线部署与仓库固定提交可能不一致；本轮只核实 UI 与来源，未进行 OpenDoc 全功能测试。
```

## 12. Assumptions and Open Questions

### Assumptions

- [assumed] The first GUI milestone remains PM/DOM-based; this must be checked against measured typing, long-document scroll and preview fidelity before any canvas/WASM branch.
- [assumed] OpenDoc's public repository and editor page are reference material only; implementation will be original and license-reviewed.
- [assumed] Existing Tauri window size and single-file dist remain acceptable while the shell is rebuilt.

### 默认决策与后续问题

默认交付开始/插入/视图/文档检查和上下文表格。“文档检查”复用历史、语义标注与损耗，不把版本快照包装成 Word 修订建议，不把语义标注包装成完整评论线程。页面缩略图限预览能力可支撑的阶段。字段、自由形状、协作和建议模式另行规划。

真正分页编辑应作为独立技术决策：保留 PM 模型并研究分页 surface，与引入 OpenDoc 引擎加双向模型映射两条路线，用中文 IME、跨页选择、表格跨页、undo、Azodoc 未知节点与保存往返夹具比较；GUI 主线不承诺其实现。

Deferred follow-ups: native/WASM paginated editing, worker/SAB rendering, GPU backend, collaboration, and new document-model work. These are deliberately outside the shell build until the current GUI has measurable gaps that justify them.

## 13. Definition of Done

- A single metadata-rich command registry drives application menus, ribbon, overflow, palette, context menus and shortcuts.
- Home/Insert/View/Review and contextual Table workflows are usable at desktop and narrow widths; no horizontal ribbon scrollbar is needed.
- All visible controls are functional or capability-aware; no placeholder buttons or empty panels are shipped.
- Outline, status, history/review, selection context and preview are discoverable, keyboard accessible and focus-safe.
- Existing ProseMirror editing, IME/paste/assets, save/revision/verify, preview fallback and Tauri/HTTP behavior remain intact.
- Unit, Playwright, Rust and Windows smoke checks pass; generated `editor/dist/index.html` is synchronized.
- Documentation records the shell contract, command matrix, responsive breakpoints and the explicit boundary between current DOM editing and any future paged/WASM surface.
