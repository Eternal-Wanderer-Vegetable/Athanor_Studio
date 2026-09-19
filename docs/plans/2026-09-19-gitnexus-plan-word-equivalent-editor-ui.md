
# Athanor Word 等效编辑器界面深化计划

- 计划状态：待执行（本文件只记录方案，不包含实现改动）
- 生成日期：2026-09-19
- 目标分支：codex/opendoc-gui-build
- 基线提交：55e6e9c03c41a424212ea0567cd0f666d5d6fca5
- 计划深度：Deep；按可直接交给实现者执行的粒度编写
- 实现边界：保留 Athanor 的 PM/Rust 文档模型、修订/损失盘点/恢复机制和现有启动方式；只重构编辑器 chrome、命令投影、交互状态与可见性
- 证据状态：GitNexus 索引已在 Docker runner 下刷新到上述提交；源文件与测试基线按第 2 节核对。工作树存在与本计划无关的 .gitignore 未提交改动，不应在执行本计划时回滚。

## 1. 目标与完成定义

### 1.1 用户目标

让使用过 Word 的用户在 Athanor 中可以依照熟悉的空间位置和操作习惯完成日常工作：新建/打开/保存、撤销重做、剪贴板、字体与段落格式、样式、查找替换、插入、页面设置、视图缩放、表格编辑、预览和导出。界面参考 OpenDoc 的分层方式和“所有入口都必须真实可用”的原则，同时保留 Athanor 的中文标签、修订、损失盘点和能力矩阵语义。

### 1.2 完成定义

1. 行为等效：常用 Word 快捷键和入口位置保持稳定；Ctrl/Cmd+S/Z/Y/C/X/V/B/I/U/F/H/P 的语义不因换皮改变。菜单、功能区、命令面板和浮动工具栏都投影同一个命令，不允许同一行为存在多份实现。
2. 布局等效：桌面宽屏出现顶栏、应用菜单、功能区页签、分组工具、左导航轨/面板、中央页面画布、右上下文面板和状态栏；窄窗口采用溢出和折叠，不出现横向滚动条或被裁切的核心操作。
3. 能力诚实：没有对应 Athanor 文档模型或网关能力的 Word 功能不显示为可点击的假入口；需要模型的功能以 disabled reason 或 capability gated 的方式表达，命令面板和菜单的可见性一致。
4. 编辑稳定：光标、选区、IME、表格 CellSelection、图片资产、恢复草稿、保存排队和预览快照不因界面刷新而丢失。
5. 平台一致：浏览器 HTTP 预览模式继续可用于编辑和前端测试；Tauri Windows 模式提供文件对话框、任务、恢复和真实 WebView2 启动；两者仅在 capability 投影上有差异。
6. 可验收：新增的单元、Playwright、桌面 smoke 和视觉断点测试全部通过；现有 E4/E5/E6 工作流不回归；构建仍生成单文件 dist/index.html。

## 2. 已核对的现状和证据

### 2.1 现有实现骨架

- engine/crates/aludel/editor/index.html:8-88 已有顶栏、应用菜单占位、功能区页签/面板、查找栏、左 rail/大纲、中央 .page/#editor、右检查面板、消息条和状态栏。
- engine/crates/aludel/editor/src/app/command-registry.ts:17-192 是菜单、功能区、命令面板、浮动条、状态栏、顶栏的唯一命令元数据源。当前 MENU_GROUPS 为文件/编辑/插入/视图/文档检查，RIBBON_TABS 为开始/插入/视图/文档检查/表格。
- engine/crates/aludel/editor/src/main.ts:65-190 建立 DocumentController、网关、命令上下文和注册辅助函数；main.ts:650-760 的 refreshUI() 将同一命令的 visible/enabled/active 状态投影到所有 data-cmd 控件，main.ts:820-915 统一启动、快捷键、overlay 和文档挂载。
- engine/crates/aludel/editor/src/ui/ribbon.ts:44-286 已有页签、上下文 Table 页签、折叠偏好、组溢出和 ResizeObserver；应增强为 Word 风格的分组标题、优先级和可访问键盘导航，而不是重写投影逻辑。
- engine/crates/aludel/editor/src/ui/menus.ts、palette.ts、overlay.ts 已提供菜单、命令面板和覆盖层焦点恢复的基础。
- engine/crates/aludel/editor/src/ui/shell.ts:20-165 负责主题、左右面板偏好和字体/字号/颜色/高亮 picker；shell.css/tokens.css 负责视觉令牌与布局。
- engine/crates/aludel/editor/src/app/document-controller.ts:55-315 创建 ProseMirror、在 dispatchTransaction 后刷新派生 UI、维护 IME/粘贴/资产和恢复；后续保存、修订、任务和预览流程不应被 UI chrome 侵入。
- engine/crates/aludel/editor/src/app/format.ts 已有混合选区的字符/段落格式读取和事务写入，可作为 Home/浮动条 picker 的真实行为来源。
- engine/crates/aludel/editor/src/platform/gateway.ts:104-166 的 DocumentGateway 已覆盖文件、保存、任务、恢复和预览；当前平台差异主要由 desktop: boolean 表达，HTTP/Tauri 实现必须继续满足同一接口。
- engine/crates/aludel/editor/src/app/preview.ts:16-180 通过 Paged.js HTML 生成真实页数和 LayoutIndex，分页失败回退未分页预览；它是页面视图和 Pages 面板未来的唯一分页事实源。

### 2.2 OpenDoc 参考要点

OpenDoc 的编辑器设计文档把 shell 分成顶栏、菜单/功能区、导航轨/左面板、右上下文面板、状态栏和 overlay；同一套命令注册表服务菜单、快捷键、功能区、命令面板和浮动工具条，并要求窄窗口使用本地溢出、对话框焦点围栏、可用性优先于“摆出一个按钮”。本计划采用这些交互规则，但保留 Athanor 的中文标签、修订和能力矩阵。

参考：
- https://github.com/CasualOffice/opendoc/blob/26b4298fa39a176fcad18f45e336cec80ee5a5f8/docs/63-EDITOR-UI-UX-DESIGN-SYSTEM.md
- https://github.com/CasualOffice/opendoc/blob/26b4298fa39a176fcad18f45e336cec80ee5a5f8/docs/64-EDITOR-TOOLBAR-RIBBON-DESIGN.md
- https://github.com/CasualOffice/opendoc/blob/26b4298fa39a176fcad18f45e336cec80ee5a5f8/docs/56-EDITOR-SHELL-AND-RENDER-ARCHITECTURE.md

### 2.3 GitNexus 影响结论

- CommandRegistry 上游影响为 CRITICAL（约 41 个受影响符号、25 个执行流程），直接依赖包括 main.ts、floating.ts、menus.ts、palette.ts、ribbon.ts；任何类型或投影字段变更都必须先更新这些消费者和测试。
- DocumentController 的图调用方主要是 main.ts，风险为 LOW，但它承载事务、IME、保存和恢复；低图风险不等于可以把界面状态塞进控制器。
- Shell、PreviewSurface 的图风险为 LOW，但它们分别控制用户偏好/焦点和分页/打印；改动必须保持现有 localStorage 与预览 fallback 语义。
- refreshUI/dispatchTransaction 的 PDG 查询没有返回可用的语句级控制块或唯一上游保护条件；因此本计划不宣称存在细粒度 PDG 边，执行时以源码和上述调用图为准，并在每个 symbol 编辑前重新跑 impact。
- Docker 索引刷新时报告了少数跨语言字段解析、候选上限和不完整 flow 警告；这些警告只影响图的完整性，不能被当成“没有调用方”。动态/跨语言能力仍以源码、测试和网关实现复核。

## 3. 设计原则与不变的用户习惯

### 3.1 Word 习惯兼容合同

- 顶部固定顺序：应用入口/品牌 → 文档名和脏状态 → 命令搜索 → 保存/窗口级操作。
- 应用菜单固定为 文件、编辑、视图、插入、格式、工具、帮助；Athanor 特有的文档检查可作为“工具”下的子组或 Review 页签，不再占用一个陌生的顶层菜单。
- 功能区固定为 开始、插入、布局、引用、视图、审阅，表格上下文页签仅在表格内出现。当前“文档检查”命令仍可通过审阅/工具/命令面板到达。
- 常用 Home 分组固定为剪贴板、字体、段落、样式、编辑；各组保留可读的中文标题、快捷键提示和更多溢出。
- 状态栏继续显示字数/保存状态/页数/编辑模式/语言/缩放；新增页码、节和视图模式时沿用现有顺序，不挪走已有信息。
- localStorage 中现有主题、左右面板、功能区页签/折叠键名保持兼容；新偏好使用版本化前缀，不能把 shell 偏好写进 azodoc、dirty 或 undo。
- 菜单、Ribbon、命令面板和快捷键的命令 ID 不能为视觉改版而改名；如需别名只在元数据层增加 alias，并保留旧 ID 的测试选择器。

### 3.2 能力诚实合同

- “有按钮”必须意味着 run() 有真实效果；没有文档模型的批注线程、实时协作、修订追踪、字段/目录刷新等不在本期伪造。
- visible 表示入口是否应该出现；enabled 表示入口存在但当前上下文不能执行；disabledReason 必须能向用户解释原因。
- 对 HTTP 模式，文件对话框、后台任务等能力继续隐藏或明确提示；不能由 UI 自己猜测 Tauri。
- 所有用户可见文本使用 textContent/安全 DOM 构造，文档元数据和文件名不能通过不受信任 innerHTML 进入 chrome。

## 4. 目标信息架构

### 4.1 Shell 层级

1. appbar：Athanor 图标/名称、文档显示名、dirty/保存状态、命令搜索、窗口级保存。
2. menubar：文件、编辑、视图、插入、格式、工具、帮助；窄窗口菜单内部滚动，菜单外点关闭。
3. ribbon-tabs 与 ribbon-panel：开始、插入、布局、引用、视图、审阅；表格上下文页签；每个组包含图标按钮、选择器、组标题和更多入口。
4. findbar/overlay：查找替换、页面设置、字体/段落对话框、打印预览均由共享 overlay 管理焦点和恢复。
5. main：左 rail（大纲/页面/搜索/评论按能力显示）、左导航面板、中央页面画布、右侧上下文面板（属性/样式/导航/修订/损失盘点）。
6. statusbar：页码/总页数、字数、语言、编辑模式、保存状态、缩放和视图按钮。

### 4.2 功能区映射

| 页签 | 组 | 本期真实命令 |
| --- | --- | --- |
| 开始 | 剪贴板 | 粘贴、匹配格式、纯文本粘贴、剪切、复制 |
| 开始 | 字体 | 字体、字号、加粗、斜体、下划线、删除线、上标/下标、文字颜色、高亮、清除格式 |
| 开始 | 段落 | 项目符号/编号、缩进、对齐、行距、段前后距、边框/底纹（只有已有事务时显示） |
| 开始 | 样式 | 正文、标题级别、引用等已有 schema 支持的块样式；没有 schema 支持的样式不显示 |
| 开始 | 编辑 | 查找、替换、选择、撤销、重做 |
| 插入 | 文本/媒体/表格/数学 | 只投影现有图片、表格、公式、脚注、分页符等 run()；新增功能先进入 capability 清单 |
| 布局 | 页面设置/段落/视图 | 复用 DocumentController.setPageTheme、页面大小、方向、页边距、缩放和视图模式 |
| 引用 | 脚注/交叉引用 | 只显示当前 schema 和网关已有的脚注/引用命令；引用目录等模型未就绪时隐藏 |
| 视图 | 显示/面板/缩放/预览 | 大纲、导航面板、检查器、连续/页面/宽度视图、打印预览、缩放 |
| 审阅 | 修订/检查/批注 | 复用历史、未知内容、损失、警告和已有审阅命令；批注线程未有模型时显示诊断入口而不是空列表 |
| 表格 | 表格/行列/属性 | 仅在 inTable 时显示；保持现有 table 命令和 CellSelection 行为 |

## 5. 目标架构与运行流程

### 5.1 单一命令投影

保留 CommandRegistry 为唯一源头，在 command-registry.ts 增加：

- RibbonTab、MenuGroup 的新固定枚举；
- CapabilityKey、EditorMode、ViewMode 类型；
- CommandContext.capabilities、mode、viewMode、pageCount 等结构化上下文；暂时保留 desktop 字段作为兼容别名；
- requiredCapabilities、mode、surfacePriority 和 disabledReason 元数据；
- 统一的 group label、shortcut、ARIA label 和 menu path。

main.ts 的 ctx() 只从 DocumentController.store、sel()、网关 capability 和 shell 状态构造上下文；注册 helper 继续负责 enabled/visible/reason，不在每个 UI 组件中重复判断。Ribbon/AppMenus/CommandPalette/FloatingBar 只消费投影结果。

### 5.2 UI 刷新节流

保留现有 refreshUI() 入口，但拆出纯 UI view model：

1. DocumentController.dispatchTransaction 更新 PM state/dirty/recovery；
2. refreshDerivedUI 更新 outline/stats，再触发一次 onStateChange；
3. main.refreshUI 计算带版本号的 EditorChromeState；
4. Ribbon 只有 command signature、tab、capability 或折叠状态变化时重建 DOM；selection-only 变化只更新 enabled/active/picker；
5. 面板和状态栏按各自子签名更新；连续输入在一个 requestAnimationFrame 内合并；
6. 预览分页只由 PreviewSurface 更新 LayoutIndex，不重建正文编辑器。

### 5.3 网关/能力投影

在 DocumentGateway 上增加只读 capability 描述或由实现类提供 getCapabilities()，至少覆盖：

- desktopFileDialogs
- backgroundJobs
- recoveryDrafts
- previewPaged
- assetRegistry
- comments
- trackedChanges
- fields
- toc
- referenceCitations

HTTP/Tauri 都返回完整、稳定的 capability 对象；不支持项仍可由实现返回 false。旧 desktop 属性保留到所有调用方迁移完毕。命令 visible/enabled 的结果在两端必须可测。

### 5.4 页面与面板事实源

- 页面尺寸、方向、边距继续从 DocumentController.currentPageTheme 和 applyPageThemeToWorkspace() 反射。
- “页码/总页数”和左侧 Pages 缩略图只能来自最近一次成功的 PreviewSurface LayoutIndex；分页失败时显示“未分页预览”，不假造页数或缩略图。
- 大纲继续来自 collectHeadings；导航点击只改变 PM selection 和滚动。
- 右侧属性/审阅/损失盘点继续调用现有 renderInspector/renderReviewSections，新增 Styles/Comments 面板必须有对应的文档或会话数据源。

## 6. 分阶段实施计划

### G0 — 基线稳固与契约冻结

目的：先修复当前 UI 的两个已知回归并冻结可观察行为，避免在换壳后无法判断问题来自新设计还是旧控件。

实现内容：

- ui/floating.ts：浮动条保留“点击格式按钮不丢选区”，但对 select、input[type=color] 等原生控件放行默认 mousedown/focus；按钮仍执行统一 data-cmd。为 picker、普通按钮和选区清空分别定义 pointer/mouse 行为。
- ui/shell.ts：统一字体/字号/颜色/高亮在 mixed 选区时的回显；颜色 input 不能设置空字符串时，使用 data-mixed/CSS 混合态和标题，不能把上一次选区的颜色当作当前值；聚焦控件不被刷新覆盖。
- command-registry.test.ts、format-selection.test.ts 和新增 floating/shell 单测覆盖以上行为。
- 建立 docs/editor-word-compatibility.md：记录命令 ID、Word 习惯快捷键、入口 surface、能力要求、当前状态 implemented/gated/planned，作为后续验收表。

验收：

- 在选区上打开浮动字体/字号/颜色/高亮控件，原生控件可以聚焦和变更，选区仍保留。
- 混合颜色/高亮不会显示为旧值，用户选择新颜色只对当前选区写入一次事务。
- G0 不改变现有命令 ID、localStorage 键、PM schema、保存/预览结果。

### G1 — 命令注册表与能力矩阵

目的：为 Word 风格入口提供稳定的行为底座，解决当前 desktop 布尔值和五页签/五菜单的扩展瓶颈。

实现内容：

- 扩展 command-registry.ts 类型和投影 API；为每个命令补齐 menuPath、ribbonTab/group、shortcut、requiredCapabilities、mode、surfacePriority。
- 更新 main.ts 注册表：补齐文件/编辑/视图/插入/格式/工具/帮助；把已有文档检查命令迁入工具/审阅 projection；为 Layout/References 提供只有真实命令才出现的页签。
- 将大块 reg(...) 注册拆到按领域的模块（例如 app/commands/file.ts、format.ts、insert.ts、view.ts）；模块只返回定义，仍由 main.ts 统一绑定 controller/shell/gateway 依赖，避免建立第二个 dispatcher。
- 增加 EditorChromeState 或纯 buildCommandContext()，集中计算 mode/capability/selection/page state。
- 对不可用命令保留 disabled reason；对无 capability 的入口使用 visible=false；命令面板搜索结果与菜单/Ribbon 可见性保持一致。

验收：

- 每个现有命令至少有一个菜单或 Ribbon 入口，并可从命令面板找到；快捷键测试不依赖控件位置。
- HTTP 模式不渲染文件对话框/后台任务死按钮；Tauri 模式保持文件/任务入口。
- CommandRegistry projection 单测覆盖新菜单、页签、上下文 Table、mode 和 capability 交叉组合。

### G2 — Word/OpenDoc shell 和视觉系统

目的：把现有骨架变成稳定的 Word 风格桌面 shell，保持 Athanor 品牌和现有面板能力。

实现内容：

- index.html：将 placeholder 结构改为语义化 appbar/menubar/ribbon/rail/pane/page/status 区域；增加菜单导航焦点容器、页面/搜索/评论 rail slot、视图切换 slot 和文档状态 slot；保留已有稳定 id。
- shell.css/tokens.css：建立 ribbon group、组标题、icon button、split button、gallery、menu item、popover、dialog、focus ring、compact/overflow/主题 token；用本地 CSS/图标资源，不依赖运行时 CDN。
- ribbon.ts：支持新页签、组 label、优先级溢出、窄窗口本地滚动菜单、折叠态、aria-selected/roving tabindex；保留 Table 上下文页签边沿行为。
- menus.ts/overlay.ts：补齐七个顶层菜单、键盘左右切换、Home/End/Arrow 导航、Escape/外点关闭、焦点恢复和菜单内部滚动。
- shell.ts：统一主题、面板偏好、compact ribbon/状态栏密度和视图模式偏好；偏好版本化并向后兼容旧键。
- 使用真实命令 metadata 生成 icon/label/title；没有真实命令的 slot 不渲染。

验收：

- 1280×820 下无核心命令横向滚动；900×600 下 Ribbon 自动折叠/溢出，菜单可滚动且键盘可达。
- 主题、面板开合、页签和折叠状态重启后保持；这些变化不使文档变脏。
- 所有 overlay 关闭后焦点回到触发点或正文，Tab 不进入已隐藏面板。

### G3 — Word 日常编辑流

目的：完成用户每天使用最多的 Home、Insert、Layout、View 工作流，不改文档数据结构。

实现内容：

- Home：接入现有 format.ts 的字符/段落事务和 selectionContext，补齐清除格式、行距/段前后距、上下标、样式选择器和剪贴板模式；每个控件同时支持 Ribbon、命令面板和快捷键。
- Insert：复用现有图片资产、表格、数学、脚注、分页符等命令；拖放/粘贴继续经过 DocumentController 的资产与 sanitize 路径。
- Layout：使用 PageTheme 设置页边距/方向/纸张；页面设置对话框用 overlay，保存/撤销/恢复沿用 controller 的 dirty、recovery 和 revision 语义。
- View：连续编辑、页面/打印预览、页面宽度、导航面板、大纲、检查器、缩放均使用现有 PreviewSurface/shell 状态；页码只显示真实 LayoutIndex。
- 状态栏：增加当前页/总页、选区字数、语言/模式和 zoom，不移除现有保存状态和文档名信息。
- 任何尚无模型的“跟踪修订、字段、目录自动更新、批注线程、协作”保留为 gated/roadmap，不放置会误导用户的可点击按钮。

验收：

- 从打开文档到输入、格式化、撤销/重做、保存、关闭确认、重新打开的路径与当前行为一致。
- 页面设置修改只改变表现层主题并正确进入 dirty/recovery/save；PM undo 不因打开对话框被污染。
- 预览分页成功显示真实页数；分页失败显示明确 fallback，状态栏不出现伪页数。

### G4 — 面板、浮动条与上下文体验

目的：让导航和上下文操作接近成熟编辑器，同时保持现有诊断信息可见。

实现内容：

- 左 rail/panel：Outline、Pages、Search、Review/Comments 按 capability 显示；Pages 只有存在 LayoutIndex 时显示缩略图，否则提供“先打开预览”状态。
- 右 panel：属性、样式、修订/历史、损失盘点、容器警告使用 tab/section 组织；每个 section 由现有 render 函数或明确数据适配器驱动。
- FloatingBar：沿用 registry 投影和选区矩形定位，修复 G0 的原生控件焦点；选择变化、滚动、窗口 resize 时重定位并限制在 viewport 内。
- Overlay：页面设置、段落、字体、查找替换、预览、命令面板统一 OverlayController 的模态/非模态、焦点围栏、Escape 和 restoreFocus。
- 表格上下文：进入 CellSelection 时只显示 Table 页签和表格面板；离开表格恢复用户原页签，不抢走正文焦点。

验收：

- 面板开合不会丢选区或改变文档内容；隐藏面板不会留下不可达焦点。
- 浮动条在窄窗口、页面顶部和滚动后都不遮挡正文关键区域；其按钮/picker 与 Ribbon 写入同一事务。
- Pages/Comments/Styles 没有数据时显示可解释的空态或不显示入口，不出现假列表。

### G5 — 回归、性能、可访问性和发布收口

目的：把“接近 Word”变成可重复验收的产品合同。

实现内容：

- 增加视觉断点 fixture 和截图基线：1280×820、1440×900、1920×1080、900×600；light/dark、100%/150%/200% DPR；记录允许差异而不是像素盲比。
- 增加长文档 fixture（至少 500 个段落、表格、图片占位、标题层级），验证 selection-only refresh 不重建编辑器、Ribbon 重建不超过一次/状态变更、输入无明显卡顿。
- 完成 ARIA：tablist/tab/tabpanel、menubar/menu/menuitem、toolbar、dialog、listbox、status；键盘只移动焦点时不触发命令，Enter/Space 才执行。
- 更新启动与贡献文档，明确前端开发、浏览器预览和 Windows Tauri 启动命令；保持 scripts/start-studio.ps1/windows-smoke.ps1。
- 运行全套前端、Rust、桌面 smoke 和 GitNexus detect-changes；任何 HIGH/CRITICAL 风险或图索引不一致都阻断合入。

完成门槛：

- 功能矩阵中所有标为 implemented 的项目均有自动测试或桌面手测记录。
- 现有 E4/E5/E6 测试无回归；新视觉/键盘/能力测试通过。
- npm run build、npm test、npm run test:e2e、Rust fmt/clippy/test/build、Windows smoke 全通过。
- 发布前执行 node .gitnexus/run.cjs detect-changes --scope all --repo .，结果完整且无未审计的关键调用方。

## 7. 文件与符号变更矩阵

| 文件/目录 | 主要符号 | 计划变更 |
| --- | --- | --- |
| engine/crates/aludel/editor/index.html | shell markup | 语义化 appbar/menu/ribbon/rail/panels/status slots；保留稳定 id |
| src/app/command-registry.ts | CommandContext、Command、MENU_GROUPS、RIBBON_TABS | capability/mode/menu/tab/surface metadata |
| src/main.ts | ctx、reg、refreshUI、boot、command registrations | 构造 chrome state、领域命令注册、焦点/快捷键桥接 |
| src/app/commands/*.ts（新增） | domain command definitions | 拆分定义，不复制 dispatcher |
| src/platform/gateway.ts、platform/http.ts、platform/tauri.ts | DocumentGateway implementations | 增加稳定 capability 描述，保留旧 desktop |
| src/ui/ribbon.ts | Ribbon | Word tabs/groups/overflow/ARIA |
| src/ui/menus.ts、palette.ts、overlay.ts | surface/overlay controllers | 七菜单、键盘导航、焦点围栏和 capability projection |
| src/ui/shell.ts、shell.css、tokens.css | Shell、layout tokens | Word/OpenDoc shell、主题、密度、响应式和混合 picker 状态 |
| src/ui/floating.ts | FloatingBar | 原生 picker 交互和选区保持 |
| src/ui/status-bar.ts、outline-panel.ts、inspector-panel.ts、review-panel.ts | renderers | 页码/视图/面板分区和真实空态 |
| src/app/document-controller.ts、format.ts、preview.ts | controller/format/preview | 只增加 UI 所需只读 hook/页面状态，不改变 PM schema 或保存协议 |
| docs/editor-word-compatibility.md、docs/editor.md | docs | 命令兼容矩阵、入口说明、开发/验收流程 |
| src/app/*.test.ts、tests/e2e/*.spec.ts | test suites | 单测、键盘/响应式/能力/视觉和 Tauri smoke 场景 |

禁止的改动：为实现视觉效果修改 azodoc PM schema、修订哈希、保存请求结构、导入导出协议、Rust 分页算法或网关错误语义；如未来确需模型能力，另开数据模型计划并先做 impact。

## 8. 测试与验收场景

### 8.1 单元测试

- command-registry.test.ts：菜单/页签投影排序；visible/enabled/disabledReason；HTTP/Tauri capability；Table context；mode/viewMode；旧命令 ID 兼容。
- format-selection.test.ts：统一值、混合值、空选区、段落格式和清除格式。
- 新增 shell.test.ts：主题/面板/页签/折叠偏好；颜色/高亮 mixed state；picker 聚焦时不回写。
- 新增 floating.test.ts：按钮 mousedown 保留选区；select/color 可原生聚焦；Escape/selection empty 清除；定位边界。
- 新增纯函数测试：buildCommandContext、capability projection、status/page display、ribbon overflow ordering。

### 8.2 Playwright 浏览器场景

- word-shell.spec.ts：七菜单、Ribbon 页签/组、折叠/溢出、菜单键盘导航、命令搜索和 focus restore。
- word-editing.spec.ts：Ctrl/Cmd 保存/撤销/重做/剪贴板、字体/字号/颜色/高亮/段落、样式、查找替换、表格上下文。
- layout-view.spec.ts：页面设置、方向/页边距、连续/页面/宽度视图、真实分页页数、缩放和状态栏。
- panels-overlay.spec.ts：大纲导航、Pages 空态/真实缩略图、属性/审阅/损失盘点、浮动条、dialog focus trap。
- capability-matrix.spec.ts：HTTP 隐藏桌面专属入口；Tauri 能力 projection；没有模型的功能不出现假按钮。
- responsive-shell.spec.ts 扩充 900×600、1280×820、1920×1080 和 DPR 断点；断言没有横向滚动和遮挡。
- 保留并回归 ribbon.spec.ts、command-palette.spec.ts、e4.spec.ts、e5.spec.ts、e6.spec.ts、workspace.spec.ts。

### 8.3 桌面验收

- 使用真实 WebView2 运行 scripts/windows-smoke.ps1：启动、窗口出现、关闭。
- 手测/自动化：新建未命名文档→保存对话框→输入→另存→重开；打开带修订/损失盘点文档；运行导出/出版任务；恢复草稿；打印预览成功与 fallback。
- 检查 Tauri 与 HTTP 的命令可见性、保存状态、错误消息和焦点恢复相同。

### 8.4 性能与可访问性验收

- 输入 500 段落文档时连续输入没有每键重建 Ribbon/编辑器；用 Playwright trace 或 performance mark 记录。
- Tab/Shift+Tab/Arrow/Escape/Enter/Space 路径完整；屏幕阅读器可读出页签、菜单项、禁用原因、状态栏。
- light/dark、高 DPI、系统字体缩放 125% 下无溢出或不可见焦点。

## 9. 验证命令与执行顺序

从仓库根目录执行：

~~~
Set-Location engine/crates/aludel/editor
npm ci
npm run build
npm test -- --reporter=dot
npm run test:e2e

Set-Location ../../../..
Set-Location engine
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build -p studio

Set-Location ..
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\windows-smoke.ps1
node .gitnexus/run.cjs detect-changes --scope all --repo .
~~~

执行者应在每个阶段提交前：

1. 对即将修改的 symbol 运行 impact({target, direction:"upstream"})；UNKNOWN 必须用文本搜索和源码复核，HIGH/CRITICAL 必须先缩小变更面。
2. 跑该阶段最小单测和 Playwright 场景。
3. 阶段合并前重复完整验证，并记录浏览器/桌面矩阵。
4. detect-changes 出现 partial/truncated 或非零时不得宣称安全；修复/补充索引后重跑。

## 10. 推荐提交切分与回滚点

- ui: stabilize floating and mixed pickers（G0）：可单独回滚，不改变布局。
- ui: add capability-aware command projections（G1）：命令类型和测试先合入。
- ui: rebuild Word-style shell chrome（G2）：DOM/CSS/菜单/Ribbon。
- ui: complete everyday editing workflows（G3）：Home/Layout/View 和状态栏。
- ui: add contextual panels and overlays（G4）：rail/right panel/floating/dialog。
- test: add visual, keyboard, desktop acceptance matrix（G5）：测试、文档、启动说明。

每个提交都应保持可构建；不要把 G2 的纯视觉变化和 G3 的文档行为/数据协议混在一个提交中。

## 11. 可复用实现上下文（Implementation Context Pack）

以下 JSON 是给后续执行代理使用的稳定上下文；执行前必须按 evidence provenance receipt 重新确认计划路径和工作树是否漂移。

~~~json
{
  "implementation_context": {
    "task_summary": "参考 OpenDoc 的 shell 分层和命令投影，把 Athanor 编辑器深化为接近 Word 的日常编辑界面；保持现有 PM/Rust 文档模型、修订/损失/恢复/预览语义和用户快捷键习惯。",
    "acceptance_criteria": [
      "Word 常用快捷键、菜单/功能区位置和文档状态语义保持兼容",
      "顶栏、七菜单、Ribbon、左右面板、页面画布、状态栏和 overlay 在宽窄窗口都可用",
      "所有入口来自同一 CommandRegistry；能力不足时隐藏或给出 disabled reason",
      "不改变 PM schema、保存/修订协议、Rust 分页和 HTTP/Tauri 网关错误语义",
      "前端单测、Playwright、Rust、Windows WebView2 smoke 和 GitNexus detect-changes 通过"
    ],
    "evidence_provenance": {
  "schema_version": 2,
  "head_commit": "55e6e9c03c41a424212ea0567cd0f666d5d6fca5",
  "generated_plan_path": "docs/plans/2026-09-19-gitnexus-plan-word-equivalent-editor-ui.md",
  "global_dirty_digest": {
    "algorithm": "sha256",
    "canonicalization": "gitnexus-evidence-provenance-v2 NUL-framed UTF-8 records",
    "value": "27f34b399b8dcf75f030765382ac086a54ae396f2f5e5e54c40446ade2912afc"
  },
  "cited_path_manifest": [
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
      "path": "docs/editor-gui-command-matrix.md",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:2492ba0bcb44452e7cfd800951ab27c6c8ac60360921d14ab06f1a4f51ea7bd0",
      "index_digest": "sha256:2492ba0bcb44452e7cfd800951ab27c6c8ac60360921d14ab06f1a4f51ea7bd0",
      "worktree_digest": "sha256:2492ba0bcb44452e7cfd800951ab27c6c8ac60360921d14ab06f1a4f51ea7bd0",
      "untracked_digest": "absent"
    },
    {
      "path": "docs/editor.md",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:0c3aeeb5a6695f715915fbd4ebddf9bd3ddc6c8d15aa08f12a8207d9b2e48ed7",
      "index_digest": "sha256:0c3aeeb5a6695f715915fbd4ebddf9bd3ddc6c8d15aa08f12a8207d9b2e48ed7",
      "worktree_digest": "sha256:0c3aeeb5a6695f715915fbd4ebddf9bd3ddc6c8d15aa08f12a8207d9b2e48ed7",
      "untracked_digest": "absent"
    },
    {
      "path": "docs/plans/2026-09-18-gitnexus-plan-opendoc-gui-build.md",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:b55a7c409010d7c4125d2fa6aa44e5919295c82979401bfa438f711a1557d178",
      "index_digest": "sha256:b55a7c409010d7c4125d2fa6aa44e5919295c82979401bfa438f711a1557d178",
      "worktree_digest": "sha256:091ba219514e427d448479a2b0d4fd492f7352b996bf7db5d9833c304d18b8bb",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/aludel/editor/index.html",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:08e5471492b0b63a769fdc2a56c32667257aa8e524b6f31292348cb9999063b8",
      "index_digest": "sha256:08e5471492b0b63a769fdc2a56c32667257aa8e524b6f31292348cb9999063b8",
      "worktree_digest": "sha256:08e5471492b0b63a769fdc2a56c32667257aa8e524b6f31292348cb9999063b8",
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
      "head_digest": "sha256:b87e7d294a5a3e14b6e1e03ba52895ad0203fc49da8e1f1291488d1b5c09dfda",
      "index_digest": "sha256:b87e7d294a5a3e14b6e1e03ba52895ad0203fc49da8e1f1291488d1b5c09dfda",
      "worktree_digest": "sha256:b87e7d294a5a3e14b6e1e03ba52895ad0203fc49da8e1f1291488d1b5c09dfda",
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
      "path": "engine/crates/aludel/editor/src/app/command-registry.test.ts",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:62dc3107a7eda69da17ab499d359db3f7fd8149f8e288a73bcfd7efcd31b1495",
      "index_digest": "sha256:62dc3107a7eda69da17ab499d359db3f7fd8149f8e288a73bcfd7efcd31b1495",
      "worktree_digest": "sha256:62dc3107a7eda69da17ab499d359db3f7fd8149f8e288a73bcfd7efcd31b1495",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/aludel/editor/src/app/command-registry.ts",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:36d36091836abfed9955d8a5cba26cef537a6259f0b71bd493184bdd0fe60078",
      "index_digest": "sha256:36d36091836abfed9955d8a5cba26cef537a6259f0b71bd493184bdd0fe60078",
      "worktree_digest": "sha256:36d36091836abfed9955d8a5cba26cef537a6259f0b71bd493184bdd0fe60078",
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
      "head_digest": "sha256:ad31d63e782572343071c7c65cf6853b86c418af05816b49bf87f5c42bbde954",
      "index_digest": "sha256:ad31d63e782572343071c7c65cf6853b86c418af05816b49bf87f5c42bbde954",
      "worktree_digest": "sha256:ad31d63e782572343071c7c65cf6853b86c418af05816b49bf87f5c42bbde954",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/aludel/editor/src/app/format-selection.test.ts",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:deea55f78056be5db8b4eea795c94c1b969996b8330b7a8c5f937cee117928ad",
      "index_digest": "sha256:deea55f78056be5db8b4eea795c94c1b969996b8330b7a8c5f937cee117928ad",
      "worktree_digest": "sha256:deea55f78056be5db8b4eea795c94c1b969996b8330b7a8c5f937cee117928ad",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/aludel/editor/src/app/format.ts",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:4e2dd2b094ebf06093ba31b3baa9d1e7b909b15763bdb0f97134a2603e2cb0f7",
      "index_digest": "sha256:4e2dd2b094ebf06093ba31b3baa9d1e7b909b15763bdb0f97134a2603e2cb0f7",
      "worktree_digest": "sha256:4e2dd2b094ebf06093ba31b3baa9d1e7b909b15763bdb0f97134a2603e2cb0f7",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/aludel/editor/src/app/preview.ts",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:24764387f6dc5018ac61fb36171b49ce45db4d3319966b0c50344c4b24e329ec",
      "index_digest": "sha256:24764387f6dc5018ac61fb36171b49ce45db4d3319966b0c50344c4b24e329ec",
      "worktree_digest": "sha256:24764387f6dc5018ac61fb36171b49ce45db4d3319966b0c50344c4b24e329ec",
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
      "head_digest": "sha256:09f5992ce62941ef2c29748c959b06a761f196507495d34df8269fc6d69dcf84",
      "index_digest": "sha256:09f5992ce62941ef2c29748c959b06a761f196507495d34df8269fc6d69dcf84",
      "worktree_digest": "sha256:09f5992ce62941ef2c29748c959b06a761f196507495d34df8269fc6d69dcf84",
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
      "head_digest": "sha256:5ac8b0c122c0e8deb5ad0008538dcfed081ba7ed01ddefaa45ceffa219ba5adc",
      "index_digest": "sha256:5ac8b0c122c0e8deb5ad0008538dcfed081ba7ed01ddefaa45ceffa219ba5adc",
      "worktree_digest": "sha256:5ac8b0c122c0e8deb5ad0008538dcfed081ba7ed01ddefaa45ceffa219ba5adc",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/aludel/editor/src/platform/http.ts",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:b54a6d8a44b13f2367fd0c2320aacdbe26e348e26fc425675791bb3084ab4b07",
      "index_digest": "sha256:b54a6d8a44b13f2367fd0c2320aacdbe26e348e26fc425675791bb3084ab4b07",
      "worktree_digest": "sha256:b54a6d8a44b13f2367fd0c2320aacdbe26e348e26fc425675791bb3084ab4b07",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/aludel/editor/src/platform/tauri.ts",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:8332e32d1d2b327413d81ae66b8b96469c91dfff15884fa34044cba7b53a8f0d",
      "index_digest": "sha256:8332e32d1d2b327413d81ae66b8b96469c91dfff15884fa34044cba7b53a8f0d",
      "worktree_digest": "sha256:8332e32d1d2b327413d81ae66b8b96469c91dfff15884fa34044cba7b53a8f0d",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/aludel/editor/src/ui/floating.ts",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:2dd80694ffbda30ed369d2734445d67831b11d15bc940342ddeb47b9b8f6c898",
      "index_digest": "sha256:2dd80694ffbda30ed369d2734445d67831b11d15bc940342ddeb47b9b8f6c898",
      "worktree_digest": "sha256:2dd80694ffbda30ed369d2734445d67831b11d15bc940342ddeb47b9b8f6c898",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/aludel/editor/src/ui/inspector-panel.ts",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:fd94c32a5a14f728c984f7ff9240f0219e61b4380f255c4ecd530c33e5a8bafa",
      "index_digest": "sha256:fd94c32a5a14f728c984f7ff9240f0219e61b4380f255c4ecd530c33e5a8bafa",
      "worktree_digest": "sha256:fd94c32a5a14f728c984f7ff9240f0219e61b4380f255c4ecd530c33e5a8bafa",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/aludel/editor/src/ui/menus.ts",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:da9ea911095512cba871b0a448216fe648ee8b4ce2f4c7b8513718ab64683f9a",
      "index_digest": "sha256:da9ea911095512cba871b0a448216fe648ee8b4ce2f4c7b8513718ab64683f9a",
      "worktree_digest": "sha256:da9ea911095512cba871b0a448216fe648ee8b4ce2f4c7b8513718ab64683f9a",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/aludel/editor/src/ui/outline-panel.ts",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:268e183176aca8b21e4cadf53f60b10f25d5679a0dfee1df33625e7d05e3270d",
      "index_digest": "sha256:268e183176aca8b21e4cadf53f60b10f25d5679a0dfee1df33625e7d05e3270d",
      "worktree_digest": "sha256:268e183176aca8b21e4cadf53f60b10f25d5679a0dfee1df33625e7d05e3270d",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/aludel/editor/src/ui/overlay.ts",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:a4c8563f04a10b65fcd87cc0a33e3385b8a258a7ac55a4fa671e4229deb16357",
      "index_digest": "sha256:a4c8563f04a10b65fcd87cc0a33e3385b8a258a7ac55a4fa671e4229deb16357",
      "worktree_digest": "sha256:a4c8563f04a10b65fcd87cc0a33e3385b8a258a7ac55a4fa671e4229deb16357",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/aludel/editor/src/ui/palette.ts",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:12d40fd9cd5e21eef11760f15c0a39079a386d6d1879797a0574905f5988aaf9",
      "index_digest": "sha256:12d40fd9cd5e21eef11760f15c0a39079a386d6d1879797a0574905f5988aaf9",
      "worktree_digest": "sha256:12d40fd9cd5e21eef11760f15c0a39079a386d6d1879797a0574905f5988aaf9",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/aludel/editor/src/ui/review-panel.ts",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:81cfb72db855ef92810cec4f89d6dc5665d6822f47212d9679d60db7ff11f685",
      "index_digest": "sha256:81cfb72db855ef92810cec4f89d6dc5665d6822f47212d9679d60db7ff11f685",
      "worktree_digest": "sha256:81cfb72db855ef92810cec4f89d6dc5665d6822f47212d9679d60db7ff11f685",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/aludel/editor/src/ui/ribbon.ts",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:4ff31b6f8193272d7eb66a7bbbf13c61e55bb7be280c4d96d4b4a4a331c9440d",
      "index_digest": "sha256:4ff31b6f8193272d7eb66a7bbbf13c61e55bb7be280c4d96d4b4a4a331c9440d",
      "worktree_digest": "sha256:4ff31b6f8193272d7eb66a7bbbf13c61e55bb7be280c4d96d4b4a4a331c9440d",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/aludel/editor/src/ui/shell.css",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:8d9d790af92613fb6620917c78ca96249815201c8010791be42eaa940260ce4c",
      "index_digest": "sha256:8d9d790af92613fb6620917c78ca96249815201c8010791be42eaa940260ce4c",
      "worktree_digest": "sha256:8d9d790af92613fb6620917c78ca96249815201c8010791be42eaa940260ce4c",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/aludel/editor/src/ui/shell.ts",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:daa862ea3576d9c4c4d24868836b1e97dec92a52a43944f1a45253a10e284b42",
      "index_digest": "sha256:daa862ea3576d9c4c4d24868836b1e97dec92a52a43944f1a45253a10e284b42",
      "worktree_digest": "sha256:daa862ea3576d9c4c4d24868836b1e97dec92a52a43944f1a45253a10e284b42",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/aludel/editor/src/ui/status-bar.ts",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:a4c631511049eff1ae5a3070310d2795f9bd4a1cfdc645df01fbe2aa0d601fdf",
      "index_digest": "sha256:a4c631511049eff1ae5a3070310d2795f9bd4a1cfdc645df01fbe2aa0d601fdf",
      "worktree_digest": "sha256:a4c631511049eff1ae5a3070310d2795f9bd4a1cfdc645df01fbe2aa0d601fdf",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/aludel/editor/src/ui/tokens.css",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:c9a04c1b901350e9e69e5a20d4398f0375870dba6d4d2e41908401dc7c3e42b1",
      "index_digest": "sha256:c9a04c1b901350e9e69e5a20d4398f0375870dba6d4d2e41908401dc7c3e42b1",
      "worktree_digest": "sha256:c9a04c1b901350e9e69e5a20d4398f0375870dba6d4d2e41908401dc7c3e42b1",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/aludel/editor/tests/e2e/command-palette.spec.ts",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:c00c286c87e1ed98712f990dd5a836ef9615adc66f2fcbca470b2da2419d7f20",
      "index_digest": "sha256:c00c286c87e1ed98712f990dd5a836ef9615adc66f2fcbca470b2da2419d7f20",
      "worktree_digest": "sha256:c00c286c87e1ed98712f990dd5a836ef9615adc66f2fcbca470b2da2419d7f20",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/aludel/editor/tests/e2e/e4.spec.ts",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:5287bbbc56bc652d9d002e52eecce9e0cb7b7b66c034de9dbe2210e7f7e2a185",
      "index_digest": "sha256:5287bbbc56bc652d9d002e52eecce9e0cb7b7b66c034de9dbe2210e7f7e2a185",
      "worktree_digest": "sha256:f8f372fc0b49d11a91b04e296901af54395ffcf6bace0d8efc1f2c7c772649fb",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/aludel/editor/tests/e2e/e5.spec.ts",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:705c0f7e92282ec6fcf4b1db1cb5b3fa4feeecfecf22e6bcbca679c6ed4b0242",
      "index_digest": "sha256:705c0f7e92282ec6fcf4b1db1cb5b3fa4feeecfecf22e6bcbca679c6ed4b0242",
      "worktree_digest": "sha256:5ceab99e8e90f92d02be3f9d93a640f0b5cd7ab8e53933007a37cf5c6f1c15e7",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/aludel/editor/tests/e2e/e6.spec.ts",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:7f58ebe96bce0b1b247194b018aca7181c97dbc629f0572571c4b571d72c15e4",
      "index_digest": "sha256:7f58ebe96bce0b1b247194b018aca7181c97dbc629f0572571c4b571d72c15e4",
      "worktree_digest": "sha256:269dd9064f8bd3d6bf7b2a0b84087abbc3d4ebe160b55398d1241ee20dca17e3",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/aludel/editor/tests/e2e/responsive-shell.spec.ts",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:d19adb080b323340dd282ab91546a987bb20fddb70dffd67179c62e6e0c56d72",
      "index_digest": "sha256:d19adb080b323340dd282ab91546a987bb20fddb70dffd67179c62e6e0c56d72",
      "worktree_digest": "sha256:d19adb080b323340dd282ab91546a987bb20fddb70dffd67179c62e6e0c56d72",
      "untracked_digest": "absent"
    },
    {
      "path": "engine/crates/aludel/editor/tests/e2e/ribbon.spec.ts",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:850adbf5aa7b61be8512d0eca426d46292cd3b67bc29bbf2e8e9b4f0861f4320",
      "index_digest": "sha256:850adbf5aa7b61be8512d0eca426d46292cd3b67bc29bbf2e8e9b4f0861f4320",
      "worktree_digest": "sha256:850adbf5aa7b61be8512d0eca426d46292cd3b67bc29bbf2e8e9b4f0861f4320",
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
      "head_digest": "sha256:25bc269d860848612aba977416a862a241f23bb4b911f4bb70ab416ec2b04dff",
      "index_digest": "sha256:25bc269d860848612aba977416a862a241f23bb4b911f4bb70ab416ec2b04dff",
      "worktree_digest": "sha256:25bc269d860848612aba977416a862a241f23bb4b911f4bb70ab416ec2b04dff",
      "untracked_digest": "absent"
    },
    {
      "path": "scripts/start-studio.ps1",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:5351e1b2e7e287d79fc658640aab331bebfe210d03249ad822cd942b418e64d3",
      "index_digest": "sha256:5351e1b2e7e287d79fc658640aab331bebfe210d03249ad822cd942b418e64d3",
      "worktree_digest": "sha256:5351e1b2e7e287d79fc658640aab331bebfe210d03249ad822cd942b418e64d3",
      "untracked_digest": "absent"
    },
    {
      "path": "scripts/windows-smoke.ps1",
      "object_kind": {
        "head": "regular",
        "index": "regular",
        "worktree": "regular",
        "untracked": "absent"
      },
      "state": "clean",
      "rename_from": null,
      "rename_to": null,
      "head_digest": "sha256:1b5e9535ac2fa0f7b9d8815d0e5fc733c827c54035ce4766da9c0c0df070f5b8",
      "index_digest": "sha256:1b5e9535ac2fa0f7b9d8815d0e5fc733c827c54035ce4766da9c0c0df070f5b8",
      "worktree_digest": "sha256:1b5e9535ac2fa0f7b9d8815d0e5fc733c827c54035ce4766da9c0c0df070f5b8",
      "untracked_digest": "absent"
    }
  ]
},
    "primary_symbols": [
      {"symbol":"CommandRegistry","file":"engine/crates/aludel/editor/src/app/command-registry.ts","lines":"17-192","role":"唯一命令元数据和 surface projection 源"},
      {"symbol":"ctx/reg/refreshUI/boot","file":"engine/crates/aludel/editor/src/main.ts","lines":"65-190; 650-915","role":"构造上下文、绑定命令、刷新 chrome、启动应用"},
      {"symbol":"Ribbon","file":"engine/crates/aludel/editor/src/ui/ribbon.ts","lines":"44-286","role":"页签、分组、上下文表格页签和溢出"},
      {"symbol":"Shell","file":"engine/crates/aludel/editor/src/ui/shell.ts","lines":"20-165","role":"主题/面板偏好和 picker"},
      {"symbol":"FloatingBar","file":"engine/crates/aludel/editor/src/ui/floating.ts","lines":"28-126","role":"选区浮动格式条"},
      {"symbol":"DocumentController.createEditor.dispatchTransaction","file":"engine/crates/aludel/editor/src/app/document-controller.ts","lines":"120-315","role":"PM 事务、dirty、IME、恢复和派生 UI 边界"},
      {"symbol":"PreviewSurface","file":"engine/crates/aludel/editor/src/app/preview.ts","lines":"16-180","role":"Paged.js 页数/LayoutIndex/fallback"},
      {"symbol":"DocumentGateway","file":"engine/crates/aludel/editor/src/platform/gateway.ts","lines":"104-166","role":"HTTP/Tauri 能力和文件/任务/预览协议"}
    ],
    "related_symbols": [
      {"symbol":"AppMenus","relationship":"IMPORTS/CALLS CommandRegistry","relevance":"七菜单和 overflow projection"},
      {"symbol":"CommandPalette","relationship":"IMPORTS/CALLS CommandRegistry","relevance":"命令搜索必须与 surface 可见性一致"},
      {"symbol":"OverlayController","relationship":"CALLED BY surfaces","relevance":"焦点围栏、Escape、restoreFocus"},
      {"symbol":"setCharacterFormat/setParagraphFormat","relationship":"CALLED BY Shell/main","relevance":"Home/浮动条真实格式事务"},
      {"symbol":"renderOutline/renderInspector/renderReviewSections/renderStatusBar","relationship":"CALLED BY main.refreshUI","relevance":"面板和状态栏投影"},
      {"symbol":"HttpGateway/TauriGateway","relationship":"IMPLEMENTS DocumentGateway","relevance":"能力矩阵和平台一致性"}
    ],
    "execution_path": [
      "DocumentController mounts PM document and installs dispatchTransaction.",
      "Transaction updates dirty/recovery/derived outline/stats, then invokes onStateChange.",
      "main.refreshUI builds CommandContext and projects visible/enabled/active state to Ribbon, menus, palette, floating, panels, and status bar.",
      "Commands call format/controller/gateway operations; focus policy restores editor or keeps dialog/picker focus.",
      "Preview commands call DocumentController.requestPreview and PreviewSurface; LayoutIndex is the only source for real page counts.",
      "Tauri/HTTP implementations expose different capabilities while preserving DocumentGateway method contracts."
    ],
    "pdg_constraints": [
      {
        "description":"PDG queries for main.refreshUI and the nested dispatchTransaction did not return distinct reachable blocks or statement-precise upstream guards; use callgraph plus source/tests and re-run impact before each edit.",
        "affected_statements":["engine/crates/aludel/editor/src/main.ts:732","engine/crates/aludel/editor/src/app/document-controller.ts:283"],
        "implementation_consequence":"Keep refresh and transaction boundaries small; do not infer safety from an empty PDG result; add focused unit/e2e coverage for every new state projection."
      }
    ],
    "architectural_patterns": [
      {"pattern":"single command registry with surface projections","example_location":"engine/crates/aludel/editor/src/app/command-registry.ts:17-192","usage_guidance":"Add metadata and projections here; do not duplicate handlers in menus/Ribbon."},
      {"pattern":"signature-based DOM rebuild with overflow","example_location":"engine/crates/aludel/editor/src/ui/ribbon.ts:151-286","usage_guidance":"Rebuild only when tab/capability command set changes; update selection state in place."},
      {"pattern":"controller owns document transaction and persistence","example_location":"engine/crates/aludel/editor/src/app/document-controller.ts:120-315","usage_guidance":"UI chrome may read state/call public methods, but must not own PM or save state."},
      {"pattern":"shared overlay with focus restoration","example_location":"engine/crates/aludel/editor/src/ui/overlay.ts","usage_guidance":"All menus/dialogs/previews must use one focus/close contract."},
      {"pattern":"Paged preview with explicit fallback","example_location":"engine/crates/aludel/editor/src/app/preview.ts:16-180","usage_guidance":"Never invent page counts or thumbnails when pagination fails."}
    ],
    "files_to_modify": [
      {"file":"engine/crates/aludel/editor/index.html","symbols":["shell markup"],"intended_change":"Word/OpenDoc semantic shell slots; keep stable selectors."},
      {"file":"engine/crates/aludel/editor/src/app/command-registry.ts","symbols":["CommandContext","Command","MENU_GROUPS","RIBBON_TABS"],"intended_change":"Capability-aware menu/tab/surface metadata."},
      {"file":"engine/crates/aludel/editor/src/main.ts","symbols":["ctx","reg","refreshUI","boot"],"intended_change":"Chrome state, domain command modules, keyboard/focus bridge."},
      {"file":"engine/crates/aludel/editor/src/platform/gateway.ts","symbols":["DocumentGateway"],"intended_change":"Read-only capability descriptor with desktop compatibility."},
      {"file":"engine/crates/aludel/editor/src/ui/ribbon.ts","symbols":["Ribbon"],"intended_change":"Word tabs/groups/overflow/ARIA."},
      {"file":"engine/crates/aludel/editor/src/ui/menus.ts","symbols":["AppMenus"],"intended_change":"Seven menus and keyboard navigation."},
      {"file":"engine/crates/aludel/editor/src/ui/shell.ts","symbols":["Shell"],"intended_change":"Preferences, density, picker mixed state."},
      {"file":"engine/crates/aludel/editor/src/ui/floating.ts","symbols":["FloatingBar"],"intended_change":"Native picker interaction and selection preservation."},
      {"file":"engine/crates/aludel/editor/src/ui/shell.css","symbols":["layout/theme rules"],"intended_change":"Ribbon/panel/page/status design tokens and responsive rules."},
      {"file":"engine/crates/aludel/editor/src/ui/tokens.css","symbols":["tokens"],"intended_change":"Athanor/OpenDoc-inspired local token set."},
      {"file":"engine/crates/aludel/editor/src/app/document-controller.ts","symbols":["public read-only hooks"],"intended_change":"Only UI view-state hooks; no schema/protocol changes."},
      {"file":"engine/crates/aludel/editor/src/app/preview.ts","symbols":["PreviewSurface/LayoutIndex"],"intended_change":"Expose real layout state to Pages/status view."},
      {"file":"docs/editor-word-compatibility.md","symbols":["new compatibility matrix"],"intended_change":"Document behavior/shortcut/capability contract."}
    ],
    "tests": [
      {"file":"engine/crates/aludel/editor/src/app/command-registry.test.ts","scenarios":["capability/mode/menu/tab projection → expected visible/enabled/reason and order"]},
      {"file":"engine/crates/aludel/editor/src/app/format-selection.test.ts","scenarios":["mixed/unified character and paragraph selection → correct picker/format state"]},
      {"file":"engine/crates/aludel/editor/src/ui/shell.test.ts","scenarios":["theme/panel/picker preference and mixed color/highlight → no stale value"]},
      {"file":"engine/crates/aludel/editor/src/ui/floating.test.ts","scenarios":["mousedown on button vs select/color → selection preserved and native control works"]},
      {"file":"engine/crates/aludel/editor/tests/e2e/word-shell.spec.ts","scenarios":["menus/Ribbon/overflow/collapse/focus at wide and narrow viewports"]},
      {"file":"engine/crates/aludel/editor/tests/e2e/word-editing.spec.ts","scenarios":["shortcuts, formatting, styles, search/replace, table context, save/undo"]},
      {"file":"engine/crates/aludel/editor/tests/e2e/layout-view.spec.ts","scenarios":["page settings, zoom/view modes, preview page count/fallback"]},
      {"file":"engine/crates/aludel/editor/tests/e2e/panels-overlay.spec.ts","scenarios":["outline/Pages/right panels/floating/dialog focus"]},
      {"file":"engine/crates/aludel/editor/tests/e2e/capability-matrix.spec.ts","scenarios":["HTTP vs Tauri visible capability surface; no dead buttons"]},
      {"file":"engine/crates/aludel/editor/tests/e2e/ribbon.spec.ts","scenarios":["existing tabs/context/overflow regression"]},
      {"file":"engine/crates/aludel/editor/tests/e2e/command-palette.spec.ts","scenarios":["command search follows registry visibility and keyboard execution"]},
      {"file":"engine/crates/aludel/editor/tests/e2e/responsive-shell.spec.ts","scenarios":["900x600/1280x820/1920x1080/DPR no horizontal scroll"]},
      {"file":"engine/crates/aludel/editor/tests/e2e/e4.spec.ts","scenarios":["preview/fallback regression"]},
      {"file":"engine/crates/aludel/editor/tests/e2e/e5.spec.ts","scenarios":["save/revision/capability report regression"]},
      {"file":"engine/crates/aludel/editor/tests/e2e/e6.spec.ts","scenarios":["desktop startup/close regression"]}
    ],
    "verification_commands":[
      "Set-Location engine/crates/aludel/editor; npm ci; npm run build; npm test -- --reporter=dot; npm run test:e2e",
      "Set-Location engine; cargo fmt --all -- --check; cargo clippy --workspace --all-targets -- -D warnings; cargo test --workspace; cargo build -p studio",
      "Set-Location ..; powershell -NoProfile -ExecutionPolicy Bypass -File .\\scripts\\windows-smoke.ps1",
      "node .gitnexus/run.cjs detect-changes --scope all --repo ."
    ],
    "risks":[
      "CommandRegistry has CRITICAL upstream blast radius; update all projections/tests atomically.",
      "main.ts is an orchestration hotspot; split definitions without creating a second dispatcher.",
      "PDG result is non-specific; empty reachable blocks do not prove safety.",
      "Preview fallback has no real page count; Pages must be capability/state gated.",
      "Gateway capability expansion can desynchronize HTTP and Tauri.",
      "Responsive CSS and DOM rebuilds can lose focus/selection or hurt long-document typing.",
      "External fonts/icons/CDN would break singlefile/offline Windows packaging."
    ],
    "assumptions":[
      "Existing PM schema and PageTheme APIs are the source of truth for current editing/layout scope; verify before G3.",
      "No comment thread/tracked changes/fields/TOC model is available for this UI scope; verify capabilities before adding any entry.",
      "Current build scripts and Windows smoke remain runnable; verify after each phase.",
      "OpenDoc references are interaction/design guidance, not code to copy or a license grant for assets."
    ],
    "open_questions":[
      "Which exact new PageTheme fields are accepted by the Rust/backend contract for section breaks and headers/footers?",
      "Should a future Pages thumbnail panel use preview iframe screenshots or a dedicated low-cost layout endpoint?",
      "Which Athanor capabilities should be exposed in the first public compatibility matrix as planned versus gated?"
    ],
    "avoid":[
      "Do not repeat full repository discovery; start from this context and re-run targeted impact.",
      "Do not duplicate command handlers in menu/Ribbon/palette/floating components.",
      "Do not change PM schema, revision hashes, save/import/export protocols, Rust pagination, or gateway error semantics for visual work.",
      "Do not render fake Word controls, fake page counts, fake comments, or fake collaboration.",
      "Do not treat UNKNOWN/empty PDG or caller results as an all-clear.",
      "Do not add runtime CDN fonts/icons or unlicensed OpenDoc assets."
    ]
  }
}
~~~

## 12. 风险、假设与开放问题

### 风险

- 命令中心高风险：CommandRegistry 的 CRITICAL fan-out 使“顺手改个类型”可能影响 25 个流程；每次字段变更必须先跑 impact，再跑 projection 单测和全部 surface E2E。
- 刷新与焦点回归：refreshUI 在事务后触发多个 surface；若把 selection-only 更新变成 DOM 重建，会丢失 picker focus、IME 或选区。必须采用签名和 requestAnimationFrame 合并。
- 能力漂移：HTTP/Tauri 实现可能返回不同 capability，造成一个平台有入口、另一个平台死按钮；能力对象必须由两端测试锁定。
- 分页事实错误：Paged.js 失败时没有可靠页数；禁止用编辑器高度估算总页数，Pages 只接受 LayoutIndex。
- 视觉资产与打包：外部字体、图标或 CDN 会破坏 singlefile/offline；所有视觉资源必须本地或系统字体 fallback。
- 模型边界：为了看起来像 Word 而添加批注/修订/字段入口会制造用户错误预期；模型能力另开计划。

### 假设

- 当前 PageTheme、ProseMirror schema、修订/损失/恢复流程是本期稳定边界。
- OpenDoc 文档可作为公开交互参考；不复制其代码或受许可限制的视觉资产。
- 现有 scripts/start-studio.ps1、windows-smoke.ps1 仍是唯一推荐的 Windows 启动/验收入口。
- 工作树的 .gitignore 改动与本计划无关，执行者不应回滚它。

### 开放问题

1. 页面分节、页眉页脚、目录和引用的后端能力是否已稳定？在 G3/G1 前用 capability/网关源码确认。
2. Pages 缩略图是否采用 Paged.js iframe 截图，还是需要新的后端 layout endpoint？在 G4 前决定性能预算。
3. 是否要把“文档检查”保留为中文顶层菜单别名，还是完全归并到“工具/审阅”？先以兼容 alias 过渡，待用户验收后删除旧入口。

## 13. 参考与证据索引

### 仓库证据

- engine/crates/aludel/editor/index.html:8-88
- engine/crates/aludel/editor/src/app/command-registry.ts:17-192
- engine/crates/aludel/editor/src/main.ts:65-190,650-915
- engine/crates/aludel/editor/src/ui/ribbon.ts:44-286
- engine/crates/aludel/editor/src/ui/shell.ts:20-165
- engine/crates/aludel/editor/src/ui/floating.ts:28-126
- engine/crates/aludel/editor/src/app/document-controller.ts:55-315
- engine/crates/aludel/editor/src/app/format.ts
- engine/crates/aludel/editor/src/app/preview.ts:16-180
- engine/crates/aludel/editor/src/platform/gateway.ts:104-166
- engine/crates/aludel/editor/src/ui/menus.ts
- engine/crates/aludel/editor/src/ui/palette.ts
- engine/crates/aludel/editor/src/ui/overlay.ts
- engine/crates/aludel/editor/src/ui/shell.css
- engine/crates/aludel/editor/src/ui/tokens.css
- engine/crates/aludel/editor/package.json
- engine/crates/aludel/editor/playwright.config.ts
- scripts/start-studio.ps1
- scripts/windows-smoke.ps1
- docs/editor-gui-command-matrix.md
- docs/editor.md
- docs/plans/2026-09-18-gitnexus-plan-opendoc-gui-build.md

### 图证据

- CommandRegistry upstream depth 3：CRITICAL，直接调用方为 main/surface，影响菜单、Ribbon、palette、floating、shortcut、save/jobs/theme 等流程。
- DocumentController upstream：LOW，main 为主要调用方；事务/保存语义仍属高敏感边界。
- Shell upstream：LOW；偏好与 picker 行为由 main/Ribbon/Floating 组合使用。
- PreviewSurface upstream：LOW；main.openPreview 和 preview/status 流程依赖真实分页/fallback。
- refreshUI 与嵌套 dispatchTransaction 的 PDG slice：未形成可用的语句级块/上游边，执行阶段不得从空结果推断无影响。

### 外部参考

- OpenDoc UI/UX design system、Ribbon design、shell/render architecture（第 2.2 节链接）。
