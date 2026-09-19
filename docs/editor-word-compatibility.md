# Athanor 编辑器 Word 兼容矩阵

本文件是 Word 等效编辑器界面的验收表：列出每个命令的稳定 ID、Word 习惯
快捷键、入口 surface、能力要求与当前状态。所有入口都投影同一个
`CommandRegistry`（`engine/crates/aludel/editor/src/app/command-registry.ts`），
命令 ID 不因视觉改版改名。

状态取值：

- **implemented**：已有真实 run()，入口可点击且行为可用。
- **gated**：入口存在但当前上下文禁用，disabledReason 向用户解释原因。
- **capability-gated**：能力不可用时整条入口不渲染（HTTP 模式无桌面能力）。
- **planned**：模型/网关能力未就绪，本期不放可点击假入口。

## 快捷键合同（Word 习惯，不得因换皮改变语义）

| 快捷键 | 命令 ID | 行为 |
| --- | --- | --- |
| Ctrl+S | file.save | 保存 |
| Ctrl+Shift+S | file.saveAs | 另存为（桌面） |
| Ctrl+Z | edit.undo | 撤销 |
| Ctrl+Y | edit.redo | 重做 |
| Ctrl+B | format.strong | 加粗 |
| Ctrl+I | format.em | 斜体 |
| Ctrl+U | format.underline | 下划线 |
| Ctrl+F | edit.find | 查找 |
| Ctrl+H | edit.replace | 替换 |
| Ctrl+P | view.preview | 打印预览 |
| Ctrl+K | view.palette | 命令面板 |
| Ctrl+N / Ctrl+O | file.new / file.open | 新建/打开（桌面） |

## 命令矩阵

| 命令 ID | 标签 | 菜单 | Ribbon 页签/组 | 浮动条 | 快捷键 | 能力 | 状态 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| file.new | 新建 | 文件 | — | — | Ctrl+N | desktopFileDialogs | capability-gated |
| file.open | 打开… | 文件 | — | — | Ctrl+O | desktopFileDialogs | capability-gated |
| file.save | 保存 | 文件 + 顶栏 | — | — | Ctrl+S | — | implemented |
| file.saveAs | 另存为… | 文件 | — | — | Ctrl+Shift+S | desktopFileDialogs | capability-gated |
| file.close | 关闭 | 文件 | — | — | — | desktopFileDialogs | capability-gated |
| job.import | 导入… | 文件 | — | — | — | backgroundJobs | capability-gated |
| job.export | 导出 Markdown… | 文件 | — | — | — | backgroundJobs | capability-gated |
| job.exportHtml | 导出 HTML… | 文件 | — | — | — | backgroundJobs | capability-gated |
| job.exportText | 导出纯文本… | 文件 | — | — | — | backgroundJobs | capability-gated |
| job.exportDocx | 导出 Word 文档… | 文件 | — | — | — | backgroundJobs | capability-gated |
| job.publish | 导出 PDF… | 文件 | — | — | — | backgroundJobs | capability-gated |
| job.cancel | 取消任务 | 文件 | — | — | — | backgroundJobs | capability-gated |
| edit.undo | 撤销 | 编辑 | 开始/编辑 | — | Ctrl+Z | — | implemented |
| edit.redo | 重做 | 编辑 | 开始/编辑 | — | Ctrl+Y | — | implemented |
| edit.find | 查找 | 编辑 | 开始/编辑 | — | Ctrl+F | — | implemented |
| edit.replace | 替换 | 编辑 | 开始/编辑 | — | Ctrl+H | — | implemented |
| edit.pasteKeep | 粘贴（保留格式） | 编辑 | — | — | Ctrl+V（原生） | — | implemented |
| edit.pastePlain | 粘贴为纯文本 | 编辑 | — | — | Ctrl+Shift+V（原生） | — | implemented |
| edit.pasteMatch | 粘贴（匹配格式） | 编辑 | — | — | Ctrl+Alt+V（原生） | — | implemented |
| edit.copy | 复制 | 编辑 | — | — | Ctrl+C（原生） | — | implemented |
| edit.cut | 剪切 | 编辑 | — | — | Ctrl+X（原生） | — | implemented |
| format.font | 字体 | — | 开始/字体 | ✓ | — | — | implemented（picker） |
| format.size | 字号 | — | 开始/字体 | ✓ | — | — | implemented（picker） |
| format.color | 文字颜色 | — | 开始/字体 | ✓ | — | — | implemented（picker） |
| format.highlight | 高亮 | — | 开始/字体 | ✓ | — | — | implemented（picker） |
| format.strong | 加粗 | 格式 | 开始/字体 | ✓ | Ctrl+B | — | implemented |
| format.em | 斜体 | 格式 | 开始/字体 | ✓ | Ctrl+I | — | implemented |
| format.underline | 下划线 | 格式 | 开始/字体 | ✓ | Ctrl+U | — | implemented |
| format.strike | 删除线 | 格式 | 开始/字体 | ✓ | — | — | implemented |
| format.code | 行内代码 | 格式 | 开始/字体 | ✓ | — | — | implemented |
| format.subscript | 下标 | 格式 | 开始/字体 | — | — | — | implemented |
| format.superscript | 上标 | 格式 | 开始/字体 | — | — | — | implemented |
| format.link | 链接 | 插入 | 插入/链接 | ✓ | — | — | implemented |
| format.clear | 清除格式 | 格式 | 开始/段落 | — | — | — | implemented |
| para.alignLeft | 左对齐 | 格式 | 开始/段落 | — | — | — | implemented |
| para.alignCenter | 居中 | 格式 | 开始/段落 | — | — | — | implemented |
| para.alignRight | 右对齐 | 格式 | 开始/段落 | — | — | — | implemented |
| para.alignJustify | 两端对齐 | 格式 | 开始/段落 | — | — | — | implemented |
| para.indentMore | 增加缩进 | 格式 | 开始/段落 | — | — | — | implemented |
| para.indentLess | 减少缩进 | 格式 | 开始/段落 | — | — | — | implemented |
| para.lineHeight | 行距 | — | 开始/段落 | — | — | — | implemented（picker） |
| para.spacing | 段间距… | 格式 | 开始/段落 | — | — | — | implemented |
| block.style | 样式 | — | 开始/样式 | — | — | — | implemented（picker） |
| block.para | 正文 | — | 开始/样式 | — | — | — | implemented |
| block.h1 / h2 / h3 | 标题 1/2/3 | — | 开始/样式 | — | — | — | implemented |
| block.quote | 引用 | — | 开始/样式 | — | — | — | implemented |
| block.codeblock | 代码块 | — | 开始/样式 | — | — | — | implemented |
| list.bullet | 项目符号 | — | 开始/段落 | — | — | — | implemented |
| list.ordered | 编号 | — | 开始/段落 | — | — | — | implemented |
| list.outdent | 减少列表缩进 | — | 开始/段落 | — | — | — | implemented |
| layout.pageSettings | 页面设置… | 视图 | 布局/页面设置 | — | — | — | implemented |
| insert.table | 表格 | 插入 | 插入/表格 | — | — | — | implemented |
| insert.image | 图片 | 插入 | 插入/插图 | — | — | assetRegistry | implemented |
| insert.math | 数学块 | 插入 | 插入/符号 | — | — | — | implemented |
| insert.footnote | 脚注 | 插入 | 引用/脚注 | — | — | — | implemented |
| footnote.goto | 跳转脚注 | — | 引用/脚注 | — | — | — | implemented |
| insert.rule | 分隔线 | 插入 | 插入/符号 | — | — | — | implemented |
| insert.pageBreak | 分页符 | 插入 | 插入/页面 | — | — | — | implemented |
| view.preview | 打印预览 | 视图 | 视图/预览 | — | Ctrl+P | previewPaged | implemented |
| view.modeContinuous | 连续编辑 | 视图 | 视图/显示 + 状态栏 | — | — | — | implemented |
| view.modePageWidth | 页宽视图 | 视图 | 视图/显示 + 状态栏 | — | — | — | implemented |
| view.pageView | 页面视图（预览） | 视图 | 视图/显示 + 状态栏 | — | — | previewPaged | implemented |
| view.toggleOutline | 大纲面板 | 视图 | 视图/面板 | — | — | — | implemented |
| view.toggleInspector | 检查面板 | 视图 | 视图/面板 | — | — | — | implemented |
| view.ribbonCollapse | 折叠功能区 | 视图 | 视图/面板 | — | — | — | implemented |
| view.theme | 深色主题 | 视图 | 视图/外观 | — | — | — | implemented |
| view.palette | 命令面板 | 工具 | 视图/工具 | — | Ctrl+K | — | implemented |
| view.zoomIn / zoomOut / zoomReset | 放大/缩小/100% | 视图 | 视图/缩放 + 状态栏 | — | — | — | implemented |
| file.verify | 检查文档 | 工具 | 审阅/校验 | — | — | — | implemented |
| help.shortcuts | 快捷键一览 | 帮助 | — | — | — | — | implemented |
| table.*（11 个） | 表格行列/单元格/属性 | — | 表格（上下文） | — | — | — | implemented（inTable gated） |

## 能力清单（capability projection）

| CapabilityKey | 含义 | HTTP | Tauri |
| --- | --- | --- | --- |
| desktopFileDialogs | 系统文件对话框（新建/打开/另存/导入源） | ✗ | ✓ |
| backgroundJobs | 后台任务（导入/导出/出版/取消） | ✗ | ✓ |
| recoveryDrafts | 恢复草稿读写 | ✗ | ✓ |
| previewPaged | Paged.js 分页预览 | ✓ | ✓ |
| assetRegistry | 资产暂存与 asset:// 引用 | ✓ | ✓ |
| comments | 批注线程 | ✗ | ✗（planned） |
| trackedChanges | 修订追踪 | ✗ | ✗（planned） |
| fields | 字段/自动目录更新 | ✗ | ✗（planned） |
| toc | 目录域 | ✗ | ✗（planned） |
| referenceCitations | 交叉引用/引文 | ✗ | ✗（planned） |

## 本期明确 planned（不放假入口）

- 批注线程、实时协作、修订追踪开关、字段/目录自动更新、交叉引用：
  文档模型或网关能力未就绪，不显示可点击按钮；审阅页签只投影已有的
  校验/修订历史/损失盘点命令。
- Pages 缩略图面板：只有真实 PreviewSurface LayoutIndex 时才显示页数，
  分页失败显示“未分页预览”回退，不假造页数。
