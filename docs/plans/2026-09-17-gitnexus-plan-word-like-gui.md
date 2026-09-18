# Athanor Studio：Word 式编辑器改造方案

> 本轮只规划，不修改生产代码、测试或配置；仅新增本方案文档。深度：Deep。
> 证据基线：a211033dc05c04089af8a879f47acb3b9aef2c53。
> GitNexus：索引与 HEAD、149 个覆盖文件匹配；Docker 中 CLI 1.6.12 的 runner/build/dependency identity 与索引一致。未刷新索引。
> 证据标记：[verified] 当前源码核实；[graph] 图谱输出；[inferred] 根据证据作出的设计判断；[assumed] 尚需验证的前提。设计中的新文件、类型和字段均为拟新增，不代表已经存在。
> provenance schema 2、全局 dirty digest 和完整引用文件 manifest 见 §11。仅排除本方案路径。

## 1. 目标与产品边界

目标是让用户能够完成“新建一篇中文报告 → 连续写作 → 设置字体和段落 → 插图、制表 → 保存并重新打开 → 分页预览 → 输出 DOCX/PDF”的完整工作，而不需要输入文件路径或理解容器、修订 ID、CLI 参数。

推荐继续使用现有 Tauri + TypeScript + ProseMirror + Rust 文档引擎，不更换编辑核心；前端采用模块化 TypeScript 和组件职责分离，暂不引入新的 UI 框架迁移。用户界面以中文为默认，保留跨平台快捷键映射。

“Word 式可用版”必须具备：
- 文件：新建、打开、保存、另存为、最近文件、关闭保护、恢复草稿。
- 编辑：中文 IME、选择/复制/剪切/粘贴、撤销重做、查找替换、常用快捷键。
- 排版：正文/标题样式、字体字号、粗斜下划线、颜色/高亮、段落对齐、首行/悬挂缩进、行距/段前段后、列表。
- 对象：表格行列/合并拆分/宽度、图片插入/粘贴/缩放、超链接、基础公式与脚注。
- 页面：纸张/方向/页边距、手动分页、页码/简单页眉页脚、打印预览、PDF 和系统打印入口。
- 导航：大纲、查找面板、字数/保存状态/缩放；常用操作不显示工程术语。
- 可靠性：保存和导出对应明确内容版本；格式重开不丢；失败不破坏原文件；未知内容保持。

本计划建议将完整 Word 功能等价作为长期目标。VBA、复杂浮动对象、邮件合并、多人协作、完整 OOXML 排版等价不作为首个可用版本的完成条件。以上基础能力均在本计划内，不因“GUI”名义只做外观。

## 2. 现状与具体缺口

| 领域 | 已确认现状 | 对用户的影响 |
| --- | --- | --- |
| 核心编辑 | [verified] main.ts:68–111 已使用 PM state/view/history/baseKeymap；并非没有撤销重做 | 可以延续现有核心，但缺少完整命令、选中态、列表/表格操作 |
| 文档生命周期 | [verified] main.ts:382–391 用 prompt 输入路径；commands.rs:63–124 仅 open/save/verify/history | 无新建/另存为/原生选择器/最近文件闭环 |
| dirty | [verified] main.ts:106–110 每个 transaction 都设 dirty；267–295 保存完成无条件清 dirty | 移动光标可能被算修改；保存中继续输入可能被误标已保存 |
| 打开与异步 | [verified] openDocument、renderJob 在成功时挂载文档，未先处理 dirty；save 在 await 后仍读全局 currentDoc | 切换或导入可能丢当前输入；旧请求结果可能污染新文档状态 |
| 事件监听 | [verified] mountDocument:125–131 每次绑定按钮；toolbar:322–358 追加 listener | 多次打开后一次按钮操作可能执行多次 |
| 保存状态 | [verified] save:288–295 只刷新侧栏，不重建 view | 已有保护光标的措施，需保留；不是“保存必然重建编辑器” |
| ID 回填 | [verified] api.rs:217–229 只回统计，无规范化 PM/ID 映射；to_prima.rs:71–87 补发缺失/重复 ID | [inferred] 新节点的服务端 ID 未同步到活跃 PM state，连续保存可能重复补发，影响锚点 |
| 落盘 | [verified] api.rs:211–214 使用 std::fs::write 原路径 | 校验失败前未写盘，但不能据此保证磁盘写入中断时原文件安全 |
| 导出 | [verified] runJob:237–249 使用 currentDoc.path、固定 Markdown；jobs.rs:323–360 从磁盘副本转换 | 尚未保存内容不在导出/PDF 中；其他导出能力未完整暴露 |
| 格式数据 | [verified] content.rs:86–98、249–296 中无专用字号/字体/颜色/段落格式字段；有 ExtraMap | 不能只给 toolbar 加控件；需要明确持久化和降级协议 |
| 资源显示 | [verified] schema.ts:33–35 仅接受 http(s)，asset:// 显示占位 | 导入图像不能在编辑区正常预览 |
| 表格 | [verified] schema.ts:78–86 可显示 spans，parseDOM:202–204 未解析跨度；mapping.rs:275–297 使用 colSpan/rowSpan | 不能直接接标准表格插件并宣称兼容 |
| 元数据渲染 | [verified] schema.ts:160–165 把 span_extra 内容包在 display:none 中 | [inferred] 携带未知文本字段的正文可能不可见；需回归而非删元数据 |
| 主题与输出 | [verified] theme.schema.json 有默认字体/页设置/样式；ExportDoc:253–260 不带 theme；export_html:73–75 使用固定 CSS；PRINT_CSS 固定 A4 | 已有主题文件不等于编辑、HTML、PDF 已共享排版规则 |
| 历史范围 | [verified] revisions.rs:109–140 的快照指向 content.json | 不能把 theme 修改的历史恢复宣称为已支持 |
| 构建 | [verified] studio/ui/index.html 内嵌脚本，与 editor/dist/index.html SHA256 相同；Tauri frontendDist=ui | 桌面不是空壳；问题是重复产物的同步和自动构建契约 |
| 测试 | [verified] editor/package.json 无测试脚本，CI 当前执行 Rust 检查；m6_2_e2e 覆盖保存/校验/重定位 | Rust 测试不能替代真实输入、焦点、快捷键和分页验收 |

以上是静态源码和图谱调查，本轮未启动桌面做实际操作，未执行构建/测试；表中用户影响的推断需要在第一阶段建立可复现用例。

## 3. 架构与职责

建议保留三层，但将当前 main.ts 的职责拆开：

~~~text
桌面工作区：菜单 / Ribbon / 大纲 / 属性与批注 / 状态栏
                       ↓ 统一 CommandRegistry
编辑会话：SessionStore + EditorController + ProseMirror plugins
                       ↓ 类型化 DocumentGateway
TauriGateway / HttpGateway（能力差异显式声明）
                       ↓
DocumentSession / 文档服务 → PM↔Prima → 校验 / 容器 / 修订 / 转换器
                       ↓

原子保存、恢复草稿、资源解析、固定版本的导出任务
~~~

- ProseMirror 是正在编辑正文的唯一状态源；不复制一份可变 JSON 与之双向同步。
- SessionStore 保存 sessionId、path?、displayName、baseRevision、fileFingerprint、editGeneration、savedGeneration、recoveryGeneration、requestEpoch、saveState、pendingJobIds。
- dirty 由正文/文档级设置与已保存基线比较得出，selection/zoom/sidebar 不计脏。generation 用于请求关联；撤销回保存点时通过基线等价判定恢复 clean。
- CommandRegistry 为每项操作统一 run、enabled、active/mixed、label、shortcut；菜单、工具栏、右键、快捷键共享它。
- 首期一个窗口一个活动文档；另开文档可选择当前窗口或新窗口。同一路径聚焦已有会话。先完成单窗口闭环，再加多窗口路由，避免同时引入标签页架构。
- UI 不直接调用文件系统、拼装 zip 或执行 CLI。HTTP 模式保留既有编辑/保存功能，桌面专有能力明确禁用并解释。

## 4. GitNexus 结果与影响范围

| 主目标 | 图谱结果 | 实施必须覆盖 |
| --- | --- | --- |
| main.ts::save | [graph] LOW，d1=mountDocument，d2=boot/openDocument；lower-bound，有 1 个未解析接收者调用 | 挂载、启动、打开、保存状态回归 |
| main.ts::mountDocument | [graph] LOW，d1=boot/openDocument | [verified] 源码另有 renderJob 的 Promise 回调；图谱即使标 exact 也不能覆盖所有回调 |
| api.rs::App::save | [graph] HIGH，6 个受影响符号、5 个直接依赖、3 个受影响流程条目；lower-bound | route、DocumentSession::save、save_pipeline_lands_human_revision_and_passes_verify、save_rejects_dangling_footnote_ref、save_rejects_unknown_pm_node_and_keeps_file_intact；另回归 Tauri IPC |
| commands.rs::open_document/save_document | [graph] UNKNOWN，无解析 caller | [verified] main.rs 的宏注册以及 main.ts 的 invoke 字符串已补充核实 |
| schema 常量、DocumentSession/JobManager 查询 | [graph] UNKNOWN 或有歧义，不能认定无影响 | 使用已读源码的 import、注册表、session facade、任务调用补足；修改时按具体方法再次 impact |

HIGH 警告已经在规划中明确：修改保存服务必须同步验证 HTTP、桌面、ID、标注和持久化行为，不能依据 riskSharedAxes=MEDIUM 降级。

使用了 context、query、impact 和 context/clusters/processes 资源。资源返回 top-N，不能声称枚举了全部模块。部分 query 输出出现不正常路径/旧行号，所有代码引用均以当前源码为准。未来实际编辑每个函数前仍需新的 impact；本轮未提交，因此未运行提交前 detect_changes。

## 5. PDG 与关键时序约束

[graph] 索引 metadata 声明 PDG 配置，但 main.ts 的 controls/flows 和 api.rs 的 controls 都返回空结果。空结果不是“不存在依赖”，也不足以证明没有 PDG 层。本轮没有可用的语句级边，以下为 source-derived 约束：

1. 保存必须保持 PM 转换/ID 校验 → 内容校验 → 写容器内存层 → 标注重定位 → human commit → 可靠落盘 → 成功响应。失败不可先清 dirty。
2. 保存请求捕获 sessionId、epoch、generation、snapshot 和 expected fingerprint。响应只更新同一 session；只确认此次 snapshot 已保存，之后的输入仍保持 dirty。
3. 前端维持单一在途 save；重复 Ctrl+S 合并为下一次保存最新状态。保存结果携带同一次提交的元数据，消除“save 后再 open”创建新后端 session 的做法。
4. 新增/复制/拆分节点在 PM transaction 中获得稳定 ID；服务端保留最终校验和纠错。若服务端纠正 ID，返回按提交文档位置定位的 patch，通过自请求起的 transaction mapping 应用，addToHistory=false，不覆盖后来输入。重复 ID 不能只用 oldId→newId 的一对一 map。
5. 打开、新建、导入结果接管、历史恢复、关闭统一经过 dirty guard。打开失败保留旧 view。慢请求用 epoch 丢弃过期结果。
6. jobs.rs 已有暂存和 commit gate；Committing 后不可承诺取消。任务启动立即设 starting 状态；回调先于 invoke 返回时，终态不能被迟到的 jobId 重置为运行中。

## 6. 改造设计

### 6.1 工作区与操作流程

目标工作区：

~~~text
文档名 / 保存状态          快速保存  撤销  重做
文件 | 开始 | 插入 | 布局 | 审阅 | 视图
按标签显示的命令分组（选区状态同步；表格/图片出现上下文工具）
大纲/查找（可收起） | 灰色工作区中的纸张与正文 | 属性/批注（按需）
字数 / 页信息 / 输入语言 / 保存状态                 缩放  适应宽度
~~~

开始页提供“空白文档、打开、最近文档、导入”；空白正文可直接输入。路径使用原生选择器；将“落链 author:human”“athanor verify”等移到历史/文档检查面板，主界面使用“保存”“检查文档”“导出 PDF”。错误给出原因和下一步操作，保留草稿；取消作为正常状态呈现。修订说明为可选高级项。

Ribbon 优先实现开始、插入、布局、视图；审阅先实现批注查看/定位和历史浏览。不能显示看似可点、实际无实现的控件。窄屏折叠分组；按钮有 label、tooltip、焦点环、aria-pressed；全键盘可达。右侧不常驻原始 ID 列表。

### 6.2 文件与可靠保存（P0）

拟新增模块：
- editor/src/app/session-store.ts、document-controller.ts、command-registry.ts。
- editor/src/platform/{gateway,tauri,http}.ts。
- studio/src/{dialogs,windows,recent,recovery}.rs；session 契约放在 aludel 可复用层。

采用 Tauri v2 官方 dialog 插件；在 Rust 注册并配置必要 capabilities，再由平台 adapter 封装。官方来源：[Tauri Dialog](https://v2.tauri.app/plugin/dialog/)。

建议接口契约（拟新增）：

| 接口 | 关键输入 | 关键输出/约束 |
| --- | --- | --- |
| new_document | title?, template? | 无正式 path 的 session + 合法空白正文；首次保存选路径 |
| open_document | 选择器得到的 path | op
aque sessionId、canonical path、revision、fingerprint、PM、theme、资源描述、diagnostics |
| save_document | sessionId、requestId、generation、expectedRevision/fingerprint、PM、documentSettings | 同一请求确认、revision、fingerprint、ID corrections、标注/历史摘要 |
| save_document_as | sessionId、target、snapshot、覆盖授权 | 成功后才切换 path/session binding；失败旧会话不变；不直接复制磁盘旧版 |
| close_document | sessionId | 由前端完成保存/放弃/取消后关闭；释放资源和锁 |
| write_recovery / list_recoveries | session、generation、草稿与设置 | 临时草稿完整写入确认，与正式 human 修订分离 |
| resolve/import_asset | sessionId、资源标识/选中文件 | 受控资源句柄及 asset 引用；拒绝路径越界 |
| run_job | sessionId、snapshot token、类型、目标、格式 | 固定输入版本、jobId、单调序号事件、结果/报告 |

保存层改成同目录临时文件 → flush/sync → 平台原子替换；实现 Windows/Unix 各自可靠语义，不使用“截断原文件再写”。原子替换前复核文件指纹；同进程按规范化路径锁。跨进程可配合文件锁并显式报告冲突，但不夸称能阻止不遵守锁的外部程序竞态。错误发生时保留原文件或可验证恢复副本，界面显示恢复位置。

草稿恢复建议：停止输入约 2 秒写恢复快照，持续输入约 10 秒做一次检查点；时间是待基准验证的默认值。恢复文件包含原路径/基线指纹/生成号/正文/设置/暂存资源。应用重启呈现恢复、另存、丢弃；成功正式保存后才清理对应 generation。不要每按键追加一条永久修订；首期不需要先重做 delta 历史。

导出固定为“此刻编辑快照”。正式出版先完成保存并 pin revision/fingerprint；保存失败或用户取消时不出版。任务运行期间可继续输入，完成提示标明输出版本；禁止自动用导入结果无提示替换脏文档。

### 6.3 文本、样式与数据模型（P1）

[verified] ExtraMap 与 span_extra 已能保留附加字段，但没有完整格式语义。推荐使用一个有规范、有校验的兼容扩展，避免只在 DOM 添加 style。

拟定存储约定：
- 段落/标题等 extra 中的 x-athanor-format：version、styleRef、paragraph{align, indentStartPt, indentEndPt, firstLinePt, lineHeight, spaceBeforePt, spaceAfterPt, keepWithNext, breakBefore}。
- 文本/行内节点 extra 中同名扩展：character{fontFamily,fontSizePt,color,highlight,verticalAlign}。
- 文档级 theme 保留默认字体、样式和 page 设置；直接格式优先于样式，样式优先于文档默认值。增加 typed Rust accessor/validator 和规范定义，保留其他未知键。
- 字号/缩进使用有界数值和明确单位，颜色标准化；不保存任意 CSS/script。缺字体提示并 fallback，不下载未知字体。
- PM 的格式 attrs/合成 mark 由 azodoc-pm 的 mapping.rs/gen.rs 生成，from_prima/to_prima 承担扩展转换。text.extra 的其余内容继续 span_extra 透传；去掉 display:none 对正文的隐藏。清除格式只能删受控格式键，不能删未知 extra。
- 旧文档缺扩展时按默认显示；旧程序可保留扩展但不承诺能正确展示。本仓库 MD/TXT 等降级时必须生成格式损失项。
- theme-only 修改纳入保存 dirty，并明确修订语义。当前历史只保存 content：在本阶段扩展“可选表现层快照/哈希”的规范及 checkout 恢复；旧修订缺表现层快照时显示“仅正文历史”，不能伪造历史样式。

高频命令：Ctrl/Cmd+N/O/S、另存为、B/I/U、Z/Shift+Z/Y、F/H、A，Enter/Shift+Enter、Tab/Shift+Tab 列表缩进。处理混合格式、无选区 storedMarks、焦点回正文。查找默认字面搜索，支持上一/下一、匹配计数、替换/全部替换（单组撤销），不跨 unknown/原子边界错误合并文本；正则后置。

中文输入必须使用 composition-aware 逻辑；组合输入期间不重挂载 view、不做破坏性结构规范化。测试拼音选词、回车确认、标点、中英文混排、emoji/代理对；复制/查找 offsets 区分 UTF-16 与内容模型文本锚点。

### 6.4 表格、列表、图片与剪贴板（P1）

- 列表接入成熟 PM 命令，但当前 list_item content 是 block*，必须验证 Enter 分项、空项退级、Tab/Shift+Tab 与嵌套段落的 schema 前提；不为使用插件静默改写旧内容。
- 表格优先使用 prosemirror-tables 的选区、行列、合并拆分、宽度能力；需要先完成 schema 适配。官方模块要求其表格 schema/插件约定，并提供 cell selection 和表格结构维护：[ProseMirror Tables](https://github.com/ProseMirror/prosemirror-tables)。
- PM 适配层明确映射标准 colspan/rowspan（默认 1）、colwidth、tableRole、header_cell 到 Prima colSpan/rowSpan、column、columns、header_row；生成器/往返/校验一起改，不能在前端另造一套 schema。
- 对旧空表/不规则表先展示并报告可修复项，修复为明确用户编辑且可撤销，不能仅打开即改变数据。拖宽、复制表格、行列删除后同步列元数据和稳定 ID。
- 图片从本地文件、拖入、剪贴板进入 session 暂存资产，再在保存中一并写入 registry+二进制；正文用 asset://。用受控协议或 blob URL 预览，关闭撤销时管理 URL 生命周期；限制解码尺寸，未知活动资源隔离。
- 图片第一版支持内联/独立块、等比拖动、宽高/对齐/alt/题注；Word 式自由浮动环绕后置。题注编辑映射回现有 caption，不能只改 DOM。
- 富文本粘贴提供“保留支持的格式/匹配目标格式/纯文本”；Word HTML、网页 HTML 经过白名单解析，过滤事件/脚本/危险 URL；丢失格式给简短提示，不承诺复原所有 Word 私有标记。
- 公式提供非 prompt 的编辑弹层，首期至少源码编辑与明确预览状态；若加入渲染引擎，字体/脚本离线打包并保持 latex 真源。脚注支持引用与定义双向跳转，删除有引用检查。

### 6.5 页面布局、分页、预览与输出（P1/P2）

这是独立技术任务，不将 CSS 固定纸高称为分页能力。

交付两种模式：
1. 连续编辑：纸宽/页边距/缩放一致，优先保证长文档和 IME 流畅；页数未知时不显示伪造页数。
2. 打印布局：手动分页、自动分页、页眉页脚/页码、打印预览；支持范围内编辑和预览/PDF 一致性达到门槛后再成为默认。

先做分页 spike（实现阶段，不是本轮）：
- 复用现有 Paged.js/CDP 作为打印预览与 PDF 的确定输出管线。预览从版本化快照生成，不对 live ProseMirror DOM 运行 Paged.js。
- 编辑分页保持一个逻辑 PM 文档、一个历史
栈，使用独立 LayoutIndex（blockId/文本范围 → page/bounding boxes），不把视觉 page wrapper 塞进 content.json。
- 先验证跨页段落选区、光标上下移动、复制粘贴、IME、跨页表格、超高图片、脚注和字体加载变化。不能通过强制拆段或多份独立编辑器“凑页”。
- 增量测量、可见页优先、延迟重排；无法满足测试时连续编辑仍可交付，但打印布局编辑保持试验状态，不能把最终 Word 式目标标为完成。

共享渲染契约：新增受校验 RenderSettings/Presentation 输入，连接 open/save → ExportDoc → HTML → PDF/preview。调整固定 PRINT_CSS 的覆盖优先级；尺寸、字体替代、表格宽度、break 规则与 UI 使用同一 token 语义。PDF 页数/页码来自真实排版完成回执。

简单页眉/页脚第一版支持文本、页码、总页数；更复杂分节/奇偶页规则后续再扩展。打印入口调用已生成的同版本 PDF/系统打印能力，打印取消不改变文档。

DOCX 先支持本计划的常用能力集；正文/标题/列表/基础表格/图片以及新增格式逐项对照夹具。已有 Pandoc 导出路径要识别能力与依赖可用性，字体/页设置不能只在应用里有效；必要的 writer 扩展列入实施。暂不保证任意复杂 Word 文档像素级往返，不支持项必须展示 LossLog 并保存原始部件。

### 6.6 交付工程

- 保留 aludel/editor 为单一前端源。优先让 Tauri frontendDist 直接指向 editor/dist；保留 HTTP include_str 同一产物。路径和打包能力在第一阶段做干净构建验证。
- 过渡期可继续入库 dist，但 CI 必须重新构建并验证产物与源码一致；移除手工复制依赖。开发/build 脚本明确 prerequisite。
- 增加 Vitest 用于状态/命令，Playwright 用于真实浏览器编辑器交互；Windows WebView2/Tauri 安装包另做 smoke，不将浏览器测试冒充桌面测试。版本在实现时固定并提交 lockfile。
- 任务首先保留现有全局单转换限制，增加 session/version 归属和完成记录；没有证据支持立即扩大后台并发或重写 JobManager。

## 7. 实施顺序、阶段门槛

| 阶段 | 工作包 | 必须通过才进入下一阶段 |
| --- | --- | --- |
| M1 可靠编辑基础 | 前端测试夹具/构建单源；事件绑定；dirty/请求 epoch；ID 稳定；保存期间继续输入；原子落盘；恢复草稿；new/open/save-as/close | 新建中文文档→保存→重开；取消/失败不丢输入；故障注入保护原件；同文档多次保存 ID 不漂移 |
| M2 Word 式工作区 | CommandRegistry、Ribbon/菜单、状态栏、快捷键、文件对话框、大纲、查找替换、焦点/IME、窗口与最近文件 | 无需输入路径即可工作；菜单状态正确；连续输入/撤销/重做/切换均稳定 |
| M3 格式数据闭环 | 格式扩展规范/typed validation、PM 生成与往返、主题/段落/字符设置、样式继承、样式历史、基础 HTML/PDF 支持 | 每项格式“编辑→保存→关闭→重开→导出”保持；老文件/未知字段保持 |
| M4 表格与对象 | 表格插件适配、列表行为、资源暂存/预览、图片粘贴/拖入/缩放、题注、脚注/公式编辑 | 中文图文报告可完成；合并单元格往返、复制/撤销/资源保存通过 |
| M5 页面与输出 | 分页 spike→布局索引；纸张/方向/页边距/页眉页脚/分页；同版本预览/PDF/打印；DOCX 能力矩阵 | 跨页编辑用例及输出一致性通过；仅有纸张外观不能验收 |
| M6 稳定性与发布验收 | 长文档、可访问性、高 DPI、依赖缺席、安装包、跨平台 smoke、用户文档 | 完成 §13 全流程；无未解释的保存/格式损失；回归通过 |

M1–M2 是“能放心写”的中间版本，M3–M4 是“日常办公可用”的中间版本，M5–M6 完成这里定义的 Word 式版本。阶段只反映依赖和验收，不给未经原型验证的固定工期。分页、DOCX 样式输出、表现层历史是主要估算不确定项。

## 8. 测试策略与验证命令

本轮只确认测试/命令存在，未运行测试；下述新增用例均待实现。

| 层级 | 关键场景 |
| --- | --- |
| 前端状态 | selection-only 不脏；撤销回保存点 clean；保存中的新增输入仍 dirty；旧 save/open/job 响应不污染新 session；Ctrl+S 重入；任务终态早到 |
| 编辑行为 | 连开三个文档再点击加粗只执行一次；快捷键/按钮等价；弹层返回选区；拼音 composition；跨段/跨页选择；查找替换一次撤销 |
| ID/标注 | 拆段、合并、复制、撤销、粘贴后连续保存 ID 稳定；duplicate correction 可映射到提交后的 state；标注保持/迁移/detached 可解释 |
| 模型与格式 | generated schema goldens；新格式 roundtrip/idempotence；未知 extra 不可见但文字可见；旧文件 defaults；颜色/字号无效值拒绝；只改 theme 的保存和历史 |
| 表格/资源 | rowspan/colspan/列宽/表头往返；删列/合并撤销；Word HTML 粘贴；图片离线打开；资产保存失败不产生悬空引用；超高图片/大图 |
| 持久化 | new/save-as 覆盖/取消；只读/磁盘满/rename 失败；保存中断；恢复草稿；外部文件变化；相同路径别名；同路径跨窗口 |
| 输出 | 未保存改动包含在导出固定快照；任务期间继续输入输出仍为明确版本；失败不伪报成功；PDF 非空/页数/页码；打印取消；DOCX/HTML/MD/TXT 降级报告 |
| 回归 | HTTP 编辑/保存；human revision；现有 M6.2 e2e；全部旧格式语料；unknown/preserved 逐字节/结构保真 |

现有测试证据：m6_2_e2e.rs:202–309（human 保存与拒绝路径）、426–434（session facade）；jobs.rs:432–490（转换/源文件保持、取消与提交边界）。现有 test 名中的“原件保持”不能替代磁盘故障注入。

验证路径：
- editor 目录：npm ci、npm run build（现有 scripts），新增 npm run test / test:e2e 后执行。
- engine 目录：cargo test -p aludel、cargo test -p azodoc-pm、cargo test -p studio；需要外部工具的测试必须记录实际执行或跳过。
- engine 目录：cargo fmt --all -- --check；cargo clippy --workspace --all-targets -- -D warnings；cargo test --workspace（CI 已存在）。
- generated schema 改动才运行 cargo run -p azodoc-pm --bin generate，并检查 golden/spec 一致性。
- 安装包：Windows 优先真实 WebView2 + 微软拼音；macOS/Linux 在声明支持前必须各完成打开、编辑、保存、打印/导出、关闭 smoke。

建议性能门槛（[assumed]，M1 记录硬件和基线后确认）：日常夹具 50 页/2 万汉字/10 张图/5 张表，键入到绘制 p95 <50ms、打开 <3s；200 页压力文档输入不出现 >200ms 持续卡顿、局部布局稳定后重排 <500ms。保存/PDF 用后台状态提示，不冻结正文。测试须固定字体、缩放、显示密度和渲染器，不能跨机器断言像素完全相等。

## 9. 风险与控制

- HIGH：App::save 跨
 HTTP/桌面/修订测试。按阶段兼容 DTO，新增字段先可选、客户端协商版本，再收紧校验。原子写入与业务校验分别测试。
- UNKNOWN：Tauri 宏/动态回调超出图谱完整度。用 IPC 契约测试、注册表和真实 app smoke 补足，实际编辑前逐符号 impact。
- 高工程风险：分页编辑和 IME。先原型/明确 fallback；不能把多个 EditorView 拆页当作稳定解决方案。
- 格式仅存不显或仅显不存：以全链路 fixture 作门槛；ExportDoc/theme 和 DOCX renderer 必须一起处理。
- 历史样式：当前 content-only 快照不等于完整文档恢复；扩展规范与读取兼容先行。
- 表格插件与生成 schema 不匹配：明确属性名、默认值、角色、header_row 和列元数据适配；不可直接替换 schema 导致无损回归失败。
- 稳定 ID 与异步修正：必须将 ID patch 映射到后续 transactions，而不是用保存响应整篇覆盖。
- 多窗口/任务：固定 session/request epoch/输入版本；首期不增加转换并发，避免无必要重构。
- 资源及恢复文件：采用 session scope 和明确清理策略，恢复资源不能早于可恢复引用被清理。

## 10. 预计修改文件与责任

以下均为未来实施范围，本轮未修改：

| 文件/拟新增目录 | 职责 |
| --- | --- |
| engine/crates/aludel/editor/src/main.ts | boot/mountDocument/createEditor/save/runJob 拆装配、修复状态与事件 |
| editor/src/app/*、platform/*、ui/*（拟新增，位于上述 editor 下） | 会话、统一命令、adapter、Ribbon/导航/状态/弹层 |
| editor/src/editor/*（拟新增）和现有 src/schema.ts | PM plugins、ID、查找、IME/clipboard、表格/图片 NodeView 和 DOM 映射 |
| engine/crates/aludel/editor/index.html、package.json、package-lock.json、vite.config.ts | 页面壳、主题样式、构建与测试 |
| engine/crates/studio/{Cargo.toml,tauri.conf.json,src/main.rs,src/commands.rs,src/jobs.rs} | 对话框/窗口事件/命令、session 归属和快照任务 |
| studio/src/{dialogs,windows,recent,recovery}.rs（拟新增） | 文件选择、路径去重、最近文件、恢复管理 |
| engine/crates/aludel/src/{api,session}.rs | Open/Save DTO、预期版本、原子保存、结构化校验、资源和新建 |
| engine/crates/azodoc-model/src/content.rs 及拟新增格式校验模块 | 受控扩展 typed API，与 ExtraMap 兼容 |
| engine/crates/azodoc-pm/src/{mapping,gen,from_prima,to_prima}.rs、gen/* | 格式/表格 schema、双向转换、ID correction receipt |
| spec/azodoc-model.md、spec/json-schema/{content,theme,revisions}.schema.json | 格式扩展/表现层快照的规范与兼容性 |
| engine/crates/azodoc-container/src/revisions.rs | 可选表现层快照及历史读取边界 |
| engine/crates/azodoc-convert/src/lib.rs | ExportDoc 渲染设置入口、损失分类 |
| engine/crates/azodoc-html/src/export.rs、azodoc-docx/src/ast_out.rs | 输出格式与样式支持；具体叶函数修改前需补 impact |
| engine/crates/athanor-cli/src/commands_m5.rs、azodoc-pdf/src/paged.rs | 排版设置、预览和出版路径共享；按需要修改 |
| 各 crate tests、editor/tests（拟新增）、.github/workflows/ci.yml | 状态/故障/格式/桌面测试，构建产物检查 |
| README 中英文、A3 设计文档 | 真实使用步骤、能力矩阵、已知限制 |

新的原子保存/格式工具模块归属在实施前的小型设计提交中固定；不要为了共用一个 helper 引入循环 crate 依赖。从本方案定位到的新叶函数，实际编辑时仍需源读和 impact。

## 11. 可复用实施上下文

```yaml
implementation_context:
  task_summary: "将 Aludel/Studio 原型升级为可持续使用的 Word 式 Tauri 文档编辑器，保留 Azodoc 七步保存和无损降级。"
  acceptance_criteria:
    - "Tauri 和 HTTP 原型加载同一份构建后的前端。"
    - "支持新建、打开、保存、另存为、关闭确认、最近文件和多会话隔离。"
    - "支持 Word 高频格式、快捷键、列表、表格、图片/数学/链接、查找替换、页面布局和主题默认值。"
    - "保存成功不丢 selection/history；冲突、校验失败、取消不破坏原文件。"
    - "human revision、标注重定位、unknown/loss 保真、verify 和任务进度可见。"
    - "前端、Rust、e2e、CI 和三平台 smoke 通过。"
  evidence_provenance:
    {
        "schema_version": 2,
        "head_commit": "a211033dc05c04089af8a879f47acb3b9aef2c53",
        "generated_plan_path": "docs/plans/2026-09-17-gitnexus-plan-word-like-gui.md",
        "global_dirty_digest": {
            "algorithm": "sha256",
            "canonicalization": "gitnexus-evidence-provenance-v2 NUL-framed UTF-8 records",
            "value": "0a6b20bb21345d134b50929212b3c01136203b1d4b01a6194460b432020de41f"
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
                "head_digest": "sha256:7ade550a1905c387d691b3588e7ea601bc5da81df7c6e93231187d4efd1283b6",
                "index_digest": "sha256:7ade550a1905c387d691b3588e7ea601bc5da81df7c6e93231187d4efd1283b6",
                "worktree_digest": "sha256:7ade550a1905c387d691b3588e7ea601bc5da81df7c6e93231187d4efd1283b6",
                "untracked_digest": "absent"
            },
            {
                "path": "engine/Cargo.toml",
                "object_kind": {
                    "head": "regular",
                    "index": "regular",
                    "worktree": "regular",
                    "untracked": "absent"
                },
                "state": "clean",
                "rename_from": null,
                "rename_to": null,
                "head_digest": "sha256:cc3a4054c364c8323e69fbb9b5de2e774b1cebdd9d86f8cf6d7c461f4932b64f",
                "index_digest": "sha256:cc3a4054c364c8323e69fbb9b5de2e774b1cebdd9d86f8cf6d7c461f4932b64f",
                "worktree_digest": "sha256:cc3a4054c364c8323e69fbb9b5de2e774b1cebdd9d86f8cf6d7c461f4932b64f",
                "untracked_digest": "absent"
            },
            {
                "path": "engine/crates/aludel/editor/dist/index.html",
                "object_kind": {
                    "head": "regular",
                    "index": "regular",
                    "worktree": "regular",
                    "untracked": "absent"
                },
                "state": "clean",
                "rename_from": null,
                "rename_to": null,
                "head_digest": "sha256:435e8561c70e8d106624e4b299b9e09357b2e225b5a1943bfdb5cbc39860997f",
                "index_digest": "sha256:435e8561c70e8d106624e4b299b9e09357b2e225b5a1943bfdb5cbc39860997f",
                "worktree_digest": "sha256:435e8561c70e8d106624e4b299b9e09357b2e225b5a1943bfdb5cbc39860997f",
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
                "head_digest": "sha256:4e7de6a05586b445a8d917b29b29725ccc3997c7452fccb0d18ef8b0d731a9aa",
                "index_digest": "sha256:4e7de6a05586b445a8d917b29b29725ccc3997c7452fccb0d18ef8b0d731a9aa",
                "worktree_digest": "sha256:4e7de6a05586b445a8d917b29b29725ccc3997c7452fccb0d18ef8b0d731a9aa",
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
                "head_digest": "sha256:bdedfe534442f2807eb65b52d5c02e96380d18cd0af5a7753bd6dff5e9c12e06",
                "index_digest": "sha256:bdedfe534442f2807eb65b52d5c02e96380d18cd0af5a7753bd6dff5e9c12e06",
                "worktree_digest": "sha256:bdedfe534442f2807eb65b52d5c02e96380d18cd0af5a7753bd6dff5e9c12e06",
                "untracked_diges
t": "absent"
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
                "head_digest": "sha256:1071d003c735545f884d4e0ae5ba09e25196768f85a608f9c82758df468d4f1e",
                "index_digest": "sha256:1071d003c735545f884d4e0ae5ba09e25196768f85a608f9c82758df468d4f1e",
                "worktree_digest": "sha256:1071d003c735545f884d4e0ae5ba09e25196768f85a608f9c82758df468d4f1e",
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
                "head_digest": "sha256:b326579bb5c3fa7c4fd17d44293232ef04bf0010cc916279ac489b142cfb5720",
                "index_digest": "sha256:b326579bb5c3fa7c4fd17d44293232ef04bf0010cc916279ac489b142cfb5720",
                "worktree_digest": "sha256:b326579bb5c3fa7c4fd17d44293232ef04bf0010cc916279ac489b142cfb5720",
                "untracked_digest": "absent"
            },
            {
                "path": "engine/crates/aludel/editor/vite.config.ts",
                "object_kind": {
                    "head": "regular",
                    "index": "regular",
                    "worktree": "regular",
                    "untracked": "absent"
                },
                "state": "clean",
                "rename_from": null,
                "rename_to": null,
                "head_digest": "sha256:54e18518c03bcaee4a2f7533aa05f6228c70eab04b8cf7f95ca1140a8bffe3ee",
                "index_digest": "sha256:54e18518c03bcaee4a2f7533aa05f6228c70eab04b8cf7f95ca1140a8bffe3ee",
                "worktree_digest": "sha256:54e18518c03bcaee4a2f7533aa05f6228c70eab04b8cf7f95ca1140a8bffe3ee",
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
                "head_digest": "sha256:992f086c5b8a337d1b83b850ef1e35c57bcc66cad139e572603611e1c47ad9cc",
                "index_digest": "sha256:992f086c5b8a337d1b83b850ef1e35c57bcc66cad139e572603611e1c47ad9cc",
                "worktree_digest": "sha256:992f086c5b8a337d1b83b850ef1e35c57bcc66cad139e572603611e1c47ad9cc",
                "untracked_digest": "absent"
            },
            {
                "path": "engine/crates/aludel/src/lib.rs",
                "object_kind": {
                    "head": "regular",
                    "index": "regular",
                    "worktree": "regular",
                    "untracked": "absent"
                },
                "state": "clean",
        
        "rename_from": null,
                "rename_to": null,
                "head_digest": "sha256:ea0fb5c71a687f5c993b8d1e1a1ae4ae715e86926653ab8f251d19be0ad7c4d1",
                "index_digest": "sha256:ea0fb5c71a687f5c993b8d1e1a1ae4ae715e86926653ab8f251d19be0ad7c4d1",
                "worktree_digest": "sha256:ea0fb5c71a687f5c993b8d1e1a1ae4ae715e86926653ab8f251d19be0ad7c4d1",
                "untracked_digest": "absent"
            },
            {
                "path": "engine/crates/aludel/src/session.rs",
                "object_kind": {
                    "head": "regular",
                    "index": "regular",
                    "worktree": "regular",
                    "untracked": "absent"
                },
                "state": "clean",
                "rename_from": null,
                "rename_to": null,
                "head_digest": "sha256:163ed91036cd7ea335f8252f39ce940cdd5d1ed00e46e0147fb1e4b977812127",
                "index_digest": "sha256:163ed91036cd7ea335f8252f39ce940cdd5d1ed00e46e0147fb1e4b977812127",
                "worktree_digest": "sha256:163ed91036cd7ea335f8252f39ce940cdd5d1ed00e46e0147fb1e4b977812127",
                "untracked_digest": "absent"
            },
            {
                "path": "engine/crates/aludel/tests/m6_2_e2e.rs",
                "object_kind": {
                    "head": "regular",
                    "index": "regular",
                    "worktree": "regular",
                    "untracked": "absent"
                },
                "state": "clean",
                "rename_from": null,
                "rename_to": null,
                "head_digest": "sha256:229afda79de5ffef34328879b2e332095560d2e14ede31d1119b0390b776e82c",
                "index_digest": "sha256:229afda79de5ffef34328879b2e332095560d2e14ede31d1119b0390b776e82c",
                "worktree_digest": "sha256:229afda79de5ffef34328879b2e332095560d2e14ede31d1119b0390b776e82c",
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
                "head_digest": "sha256:9584d420fcbbfbd08f97691e47c9717c42ae4204d2cb22e42f244aef59c35310",
                "index_digest": "sha256:9584d420fcbbfbd08f97691e47c9717c42ae4204d2cb22e42f244aef59c35310",
                "worktree_digest": "sha256:9584d420fcbbfbd08f97691e47c9717c42ae4204d2cb22e42f244aef59c35310",
                "untracked_digest": "absent"
            },
            {
                "path": "engine/crates/azodoc-container/src/revisions.rs",
                "object_kind": {
                    "head": "regular",
                    "index": "regular",
                    "worktree": "regular",
                    "untracked": "absent"
                },
                "state": "clean",
                "rename_from": null,
                "rename_to": null,
                "head_digest": "sha256:bf6298b8340082f3d28f3faf3268ca610c8606106f97edfb69ae9aeca5ab17e2",
                "index_digest": "sha256:bf6298b8340082f3d28f3faf3268ca610c8606106f97edfb69ae9aeca5ab17e2",
                "worktree_digest": "sha256:bf6298b8340082f3d28f3faf3268
ca610c8606106f97edfb69ae9aeca5ab17e2",
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
                "path": "engine/crates/azodoc-html/src/export.rs",
                "object_kind": {
                    "head": "regular",
                    "index": "regular",
                    "worktree": "regular",
                    "untracked": "absent"
                },
                "state": "clean",
                "rename_from": null,
                "rename_to": null,
                "head_digest": "sha256:994f7c68713592ed4a5a03fa742ffc88acb9825fe6a0832085c640f0b7c88731",
                "index_digest": "sha256:994f7c68713592ed4a5a03fa742ffc88acb9825fe6a0832085c640f0b7c88731",
                "worktree_digest": "sha256:994f7c68713592ed4a5a03fa742ffc88acb9825fe6a0832085c640f0b7c88731",
                "untracked_digest": "absent"
            },
            {
                "path": "engine/crates/azodoc-model/src/content.rs",
                "object_kind": {
                    "head": "regular",
                    "index": "regular",
                    "worktree": "regular",
                    "untracked": "absent"
                },
                "state": "clean",
                "rename_from": null,
                "rename_to": null,
                "head_digest": "sha256:552d74184fa695696db9948ce4679fc7d93a36f6002b0ae3700706c87ea0e811",
                "index_digest": "sha256:552d74184fa695696db9948ce4679fc7d93a36f6002b0ae3700706c87ea0e811",
                "worktree_digest": "sha256:552d74184fa695696db9948ce4679fc7d93a36f6002b0ae3700706c87ea0e811",
                "untracked_digest": "absent"
            },
            {
                "path": "engine/crates/azodoc-pm/src/gen.rs",
                "object_kind": {
                    "head": "regular",
                    "index": "regular",
                    "worktree": "regular",
                    "un
tracked": "absent"
                },
                "state": "clean",
                "rename_from": null,
                "rename_to": null,
                "head_digest": "sha256:12fcb761482e681d6df189fcb0af41872289b6d2990a4404a3d0d43d8f1dbd21",
                "index_digest": "sha256:12fcb761482e681d6df189fcb0af41872289b6d2990a4404a3d0d43d8f1dbd21",
                "worktree_digest": "sha256:12fcb761482e681d6df189fcb0af41872289b6d2990a4404a3d0d43d8f1dbd21",
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
                "head_digest": "sha256:37c92da9b3e087b479735c70004743ac2d2e69e4cd0bb9e4e47eda104ded4b6a",
                "index_digest": "sha256:37c92da9b3e087b479735c70004743ac2d2e69e4cd0bb9e4e47eda104ded4b6a",
                "worktree_digest": "sha256:37c92da9b3e087b479735c70004743ac2d2e69e4cd0bb9e4e47eda104ded4b6a",
                "untracked_digest": "absent"
            },
            {
                "path": "engine/crates/studio/src/jobs.rs",
                "object_kind": {
                    "head": "regular",
                    "index": "regular",
                    "worktree": "regular",
                    "untracked": "absent"
                },
                "state": "clean",
                "rename_from": null,
                "rename_to": null,
                "head_digest": "sha256:b7695aef12268474e13c66dcbf3fab862e80fd3582c9e39a44da58e536f2555c",
                "index_digest": "sha256:b7695aef12268474e13c66dcbf3fab862e80fd3582c9e39a44da58e536f2555c",
          
      "worktree_digest": "sha256:b7695aef12268474e13c66dcbf3fab862e80fd3582c9e39a44da58e536f2555c",
                "untracked_digest": "absent"
            },
            {
                "path": "engine/crates/studio/src/main.rs",
                "object_kind": {
                    "head": "regular",
                    "index": "regular",
                    "worktree": "regular",
                    "untracked": "absent"
                },
                "state": "clean",
                "rename_from": null,
                "rename_to": null,
                "head_digest": "sha256:af986d584ede5b90cfe8e2875c233daa3e6f1978963fc892b9fe196731570443",
                "index_digest": "sha256:af986d584ede5b90cfe8e2875c233daa3e6f1978963fc892b9fe196731570443",
                "worktree_digest": "sha256:af986d584ede5b90cfe8e2875c233daa3e6f1978963fc892b9fe196731570443",
                "untracked_digest": "absent"
            },
            {
                "path": "engine/crates/studio/tauri.conf.json",
                "object_kind": {
                    "head": "regular",
                    "index": "regular",
                    "worktree": "regular",
                    "untracked": "absent"
                },
                "state": "clean",
                "rename_from": null,
                "rename_to": null,
                "head_digest": "sha256:4b416842ff0a05c74dac6b094c619b8689082b7ad715f8d42498177bb5f4ecac",
                "index_digest": "sha256:4b416842ff0a05c74dac6b094c619b8689082b7ad715f8d42498177bb5f4ecac",
                "worktree_digest": "sha256:4b416842ff0a05c74dac6b094c619b8689082b7ad715f8d42498177bb5f4ecac",
                "untracked_digest": "absent"
            },
            {
                "path": "engine/crates/studio/ui/index.html",
                "object_kind": {
                    "head": "regular",
                    "index": "regular",
                    "worktree": "regular",
                    "untracked": "absent"
                },
                "state": "clean",
                "rename_from": null,
                "rename_to": null,
                "head_digest": "sha256:435e8561c70e8d106624e4b299b9e09357b2e225b5a1943bfdb5cbc39860997f",
                "index_digest": "sha256:435e8561c70e8d106624e4b299b9e09357b2e225b5a1943bfdb5cbc39860997f",
                "worktree_digest": "sha256:435e8561c70e8d106624e4b299b9e09357b2e225b5a1943bfdb5cbc39860997f",
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
  primary_symbols:
    - {symbol: "mountDocument", file: "engine/crates/aludel/editor/src/main.ts", lines: "118-132", role: "文档挂载和 UI 绑定"}
    - {symbol: "save", file: "engine/crates/aludel/editor/src/
main.ts", lines: "267-304", role: "前端保存编排"}
    - {symbol: "App::open/save", file: "engine/crates/aludel/src/api.rs", lines: "89-230", role: "DTO、校验和七步持久化"}
    - {symbol: "open_document/save_document", file: "engine/crates/studio/src/commands.rs", lines: "63-124", role: "Tauri session command"}
    - {symbol: "JobManager", file: "engine/crates/studio/src/jobs.rs", lines: "146-237", role: "任务排队、取消和 commit gate"}
  related_symbols:
    - {symbol: "boot/openDocument", relationship: "CALLS", relevance: "启动和打开路径"}
    - {symbol: "DocumentSession", relationship: "WRAPS", relevance: "transport-independent facade"}
    - {symbol: "schema/buildSchema", relationship: "IMPORTS/DEFINES", relevance: "generated PM schema DOM 映射"}
    - {symbol: "route", relationship: "HANDLES_ROUTE", relevance: "HTTP 兼容适配器"}
    - {symbol: "m6_2_e2e", relationship: "test-of", relevance: "打开/保存/verify/重定位回归"}
  execution_path:
    - "Tauri boot 注册 command 并加载统一 frontendDist"
    - "用户通过 dialog new/open 得到 session id 和 DocResponse"
    - "前端创建 EditorView，transaction 更新 store dirty 状态"
    - "save 携带 PM JSON、expected_revision、message 调用 session"
    - "App::save 按七步转换/校验/重定位/commit/write，返回 revision 和 diagnostics"
    - "前端更新 sidebar/status，不重建 view，继续编辑"
    - "import/export/publish 进入按 session 记录的 JobManager，commit 边界后回报结果"
  pdg_constraints:
    - {description: "PDG file anchor returned no persisted controls/flows; use source order as constraint", affected_statements: ["engine/crates/aludel/editor/src/main.ts:106-110", "engine/crates/aludel/src/api.rs:162-214", "engine/crates/studio/src/jobs.rs:313-409"], implementation_consequence: "保留 transaction→dirty、validate→write、Committing 不可取消的顺序"}
  architectural_patterns:
    - {pattern: "generated schema + hand-written DOM mapping", example_location: "engine/crates/aludel/editor/src/schema.ts:16-18", usage_guidance: "不要在 TS 复制 Prima 模型"}
    - {pattern: "transport-independent session facade", example_location: "engine/crates/aludel/src/session.rs:32-62", usage_guidance: "Tauri 和 HTTP 共用 App 语义"}
    - {pattern: "staged atomic job output", example_location: "engine/crates/studio/src/jobs.rs:289-409", usage_guidance: "输出先暂存，commit 后再写目标"}
  files_to_modify:
    - {file: "engine/crates/aludel/editor/src/main.ts", symbols: ["mountDocument", "save", "openDocument", "boot", "toolbar"], intended_change: "状态机、一次性绑定、dialog/command gateway"}
    - {file: "engine/crates/studio/src/commands.rs", symbols: ["StudioState", "open_document", "save_document"], intended_change: "session lifecycle and conflict-aware commands"}
    - {file: "engine/crates/aludel/src/api.rs", symbols: ["App::open", "App::save", "App::verify"], intended_change: "theme/diagnostics/revision DTO"}
    - {file: "engine/crates/studio/src/jobs.rs", symbols: ["JobManager", "run_job"], intended_change: "per-session jobs while retaining atomic boundaries"}
    - {file: "engine/crates/aludel/editor/src/schema.ts", symbols: ["nodeToDOM", "nodeParseDOM", "buildSchema"], intended_change: "editable DOM/clipboard/page presentation"}
  tests:
    - {file: "engine/crates/aludel/tests/m6_2_e2e.rs", scenarios: ["edit→save→human revision→verify", "unknown/dangling content rejected without file damage", "annotation relocation"]}
    - {file: "engine/crates/studio/tests/commands.rs", scenarios: ["new/open/save-as/close", "path safety", "revision conflict", "multi-session isolation"]}
    - {file: "engine/crates/aludel/editor/tests", scenarios: ["state transitions", "single event bin
ding", "selection/history preservation", "table/clipboard/unknown node interaction"]}
  verification_commands:
    - "cd engine/crates/aludel/editor && npm ci && npm run build"
    - "cd engine && cargo fmt --all -- --check"
    - "cd engine && cargo clippy --workspace --all-targets -- -D warnings"
    - "cd engine && cargo test --workspace"
  risks:
    - "Tauri macro/IPC callers are not complete in GitNexus incoming; verify source registration and smoke tests."
    - "Frontend build artifact duplication can leave a stale desktop UI."
    - "Theme/assets/table features can increase PM schema and bundle complexity."
    - "External file edits and close/save races can lose user intent if revision checks are absent."
  assumptions:
    - "首期单机桌面编辑，不做实时协作和远程同步。"
    - "主题先支持默认页布局和有限 patch，完整主题设计器后置。"
    - "所有模型结构仍由 azodoc-pm generated schema 驱动。"
  open_questions:
    - "Tauri dialog 使用官方插件还是现有 API；需按目标平台验证。"
    - "查找替换、分页符、批注编辑和 checkout 预览的首期优先级。"
    - "前端测试采用 Vitest/jsdom 还是浏览器 E2E，CI 运行预算是多少。"
  avoid:
    - "不要继续维护 studio/ui 与 editor/dist 两份独立页面。"
    - "不要在 TS 手写 Prima schema 或直接改 ProseMirror DOM。"
    - "不要移除 App::save 七步顺序、unknown/preserved 保真或 JobManager commit gate。"
    - "不要把 GitNexus UNKNOWN caller 结果解释为无影响。"
```

## 12. 默认假设、未决事项与明确后置

默认建议（可直接用于后续执行，不是本轮要求用户逐项确认）：
- [assumed] .azodoc 为原生编辑格式；打开 .docx 先导入，保存原生文件，DOCX 为有能力矩阵的导出格式。
- [assumed] Windows 作为首验平台，保留 macOS/Linux；跨平台“已支持”须有真实 smoke。
- [assumed] 使用现有 ProseMirror 与模块化 TypeScript；Tauri v2 官方 dialog；Vitest + Playwright；确切依赖版本在实施锁定。
- [assumed] 首期一窗口一文档，支持另开窗口；不同时引入标签页与多种窗口策略。
- [assumed] 自动恢复默认开启、正式自动保存作为可配置后续模式；恢复草稿不无限堆积 human 修订。

实施前仍需解决的技术验证：
1. 表格 schema adapter 和空表兼容的原型；先读 upstream 的具体属性约定并做转换夹具。
2. 分页 spike 的跨页长段/表格/中文输入验收。若失败，产品需明确连续编辑+只读预览的中间交付边界，最终分页目标仍未完成。
3. 格式兼容扩展、表现层快照和 DOCX 属性输出的准确降级规范；旧读取器保留与显示能力必须分开说明。
4. 原子替换在目标 Windows 文件系统/文件占用状态的故障语义；macOS/Linux 分别验证。
5. 性能数字、恢复频率和安装包外部工具策略要用目标机器基准确定。本轮没有性能测量。
6. PDG 查询空结果的具体原因尚未确定；实施有新图谱依赖时可由已确认 runner 执行 analyze --index-only --pdg（必要时强制刷新），不能伪造语句边。

后置：实时协作、云同步、VBA、邮件合并、复杂浮动环绕/分栏、完整 Word 修订接受拒绝、任意 DOCX 像素级等价、专业参考文献管理、delta/GC 重构。批注查看/定位与简单创建可在 M4 后增加；完整审阅不是现有 human 快照链的别名。

## 13. 完成定义

- [ ] 不输入路径、不使用命令行，即可新建、打开、保存、另存为、恢复、关闭文档。
- [ ] 一次完成中文报告：标题、不同字号、段落缩进、编号列表、合并表格、图片题注、脚注、链接。
- [ ] 保存时继续输入、快速切换文档、导出时继续编辑、退出时取消，均不误报已保存、不丢输入。
- [ ] Ctrl+S/B/I/U/F/H/Z 等、工具栏状态、选区/焦点、IME、撤销回保存点可用。
- [ ] 格式、主题、资源、ID、标注保存重开后保持；unknown/extra/preserved 保真。
- [ ] 跨页选择/输入/表格、手动分页、纸张/方向/边距、页码/页眉页脚通过支持范围内测试；纸张外观不算分页通过。
- [ ] 预览、PDF、打印使用同一版本和排版参数；导出 DOCX 的支持项通过夹具，不支持项明确报告。
- [ ] 保存中断、磁盘满、只读、外部改写、取消、依赖缺席、资源损坏有可恢复结果；原件保护通过故障注入。
- [ ] human revision、标注重定位、HTTP 兼容、现有 Rust 回归、前端交互测试、打包 smoke 均通过。
- [ ] 每次实际编辑前 impact；提交前 detect_changes(scope=all) 无 partial/truncated/未解释变化。
- [ ] 产品界面使用用户术语；开发诊断移入可展开详情；声明的平台都有验收记录。

本轮交付为方案，不代表以上能力已经实现或测试通过。

