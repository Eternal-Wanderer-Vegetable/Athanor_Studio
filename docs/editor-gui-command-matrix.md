# Athanor GUI 命令目录与能力矩阵（G0 冻结）

OpenDoc 壳层改造的命令契约：**所有 surface（菜单/ribbon/命令面板/快捷键/上下文工具栏）
都从 `CommandRegistry` 的同一份 Command 元数据投影**，任何入口不得单独实现命令逻辑。
能力不可用的命令不渲染按钮（不可用 ≠ 灰按钮），除非该命令需要解释性禁用提示。

## 1. Surface 定义

| Surface | 说明 |
| --- | --- |
| `menu` | 顶栏应用菜单（文件/编辑/插入/视图/文档检查），下拉项 `role=menuitem` |
| `ribbon` | 功能区；命令按 `tab` + `group` + `order` 排布，溢出组进入“更多”菜单 |
| `palette` | 命令面板（Ctrl+K / 顶栏搜索框），全部可见命令可搜索执行 |
| `shortcut` | `shortcut` 字段声明的全局快捷键 |
| `floating` | 非空文本选区上的浮动格式条 |
| `statusbar` | 状态栏控件（缩放等） |

## 2. 命令清单（当前真实可用）

启用条件记号：`doc` = 需已挂载文档；`desk` = 需桌面（Tauri）能力；
`sel` = 需非空选区；`table` = 需光标/选区在表格内（contextual tab 出现时整体启用）。

### 文件（menu:文件 / topbar 快捷项）

| id | label | shortcut | tab/group | surfaces | 条件 | disabledReason |
| --- | --- | --- | --- | --- | --- | --- |
| file.new | 新建 | Ctrl+N | —/— | menu | desk | 仅桌面可新建文件 |
| file.open | 打开… | Ctrl+O | —/— | menu | desk | 仅桌面可打开文件 |
| file.save | 保存 | Ctrl+S | —/— | menu, topbar | doc | 无已打开文档 |
| file.saveAs | 另存为… | Ctrl+Shift+S | —/— | menu | doc, desk | 仅桌面可另存为 |
| file.close | 关闭 | — | —/— | menu | doc, desk | 仅桌面可关闭文件 |
| job.import | 导入… | — | —/— | menu | desk | 仅桌面可导入 |
| job.export | 导出 Markdown… | — | —/— | menu | doc, desk | 仅桌面可导出 |
| job.exportHtml | 导出 HTML… | — | —/— | menu | doc, desk | 仅桌面可导出 |
| job.exportText | 导出纯文本… | — | —/— | menu | doc, desk | 仅桌面可导出 |
| job.exportDocx | 导出 Word 文档… | — | —/— | menu | doc, desk | 仅桌面可导出 |
| job.publish | 导出 PDF… | — | —/— | menu | doc, desk | 仅桌面可出版 |
| job.cancel | 取消任务 | — | —/— | menu | doc, desk, busy | 没有在途任务 |

### 开始 Home（默认 tab）

| id | label | shortcut | group（优先级） | surfaces | 条件 |
| --- | --- | --- | --- | --- | --- |
| edit.undo | 撤销 | Ctrl+Z | 历史(10) | ribbon, menu | doc |
| edit.redo | 重做 | Ctrl+Y | 历史(10) | ribbon, menu | doc |
| format.font | 字体 | — | 字体(20) | ribbon(picker), floating | doc |
| format.size | 字号 | — | 字体(20) | ribbon(picker), floating | doc |
| format.color | 文字颜色 | — | 字体(20) | ribbon(picker), floating | doc |
| format.highlight | 高亮 | — | 字体(20) | ribbon(picker), floating | doc |
| format.strong | 加粗 | Ctrl+B | 字体(20) | ribbon, floating | doc |
| format.em | 斜体 | Ctrl+I | 字体(20) | ribbon, floating | doc |
| format.underline | 下划线 | Ctrl+U | 字体(20) | ribbon, floating | doc |
| format.strike | 删除线 | — | 字体(20) | ribbon, floating | doc |
| format.code | 行内代码 | — | 字体(20) | ribbon, floating | doc |
| format.link | 链接 | — | 字体(20) | ribbon, floating | doc |
| para.alignLeft | 左对齐 | — | 段落(30) | ribbon | doc |
| para.alignCenter | 居中 | — | 段落(30) | ribbon | doc |
| para.alignRight | 右对齐 | — | 段落(30) | ribbon | doc |
| para.alignJustify | 两端对齐 | — | 段落(30) | ribbon | doc |
| para.indentMore | 增加缩进 | — | 段落(30) | ribbon | doc |
| para.indentLess | 减少缩进 | — | 段落(30) | ribbon | doc |
| format.clear | 清除格式 | — | 段落(30) | ribbon | doc |
| block.para | 正文 | — | 样式(40) | ribbon | doc |
| block.h1 | 标题 1 | — | 样式(40) | ribbon | doc |
| block.h2 | 标题 2 | — | 样式(40) | ribbon | doc |
| block.h3 | 标题 3 | — | 样式(40) | ribbon | doc |
| block.quote | 引用 | — | 样式(40) | ribbon | doc |
| block.codeblock | 代码块 | — | 样式(40) | ribbon | doc |
| list.bullet | 项目符号 | — | 列表(50) | ribbon | doc |
| list.ordered | 编号 | — | 列表(50) | ribbon | doc |
| list.outdent | 减少列表缩进 | — | 列表(50) | ribbon | doc |
| edit.find | 查找 | Ctrl+F | 编辑(60) | ribbon, menu | doc |
| edit.replace | 替换 | Ctrl+H | 编辑(60) | ribbon, menu | doc |

### 插入 Insert

| id | label | shortcut | group | surfaces | 条件 |
| --- | --- | --- | --- | --- | --- |
| insert.table | 表格 | — | 表格(10) | ribbon, menu | doc |
| insert.image | 图片 | — | 插图(20) | ribbon, menu | doc |
| insert.math | 数学块 | — | 符号(30) | ribbon, menu | doc |
| insert.footnote | 脚注 | — | 符号(30) | ribbon, menu | doc |
| footnote.goto | 跳转脚注 | — | 符号(30) | ribbon | doc |
| format.link | 链接 | — | 链接(40) | ribbon, menu | doc |
| insert.rule | 分隔线 | — | 符号(50) | ribbon, menu | doc |
| insert.pageBreak | 分页符 | — | 页面(60) | ribbon | doc |

### 视图 View

| id | label | shortcut | group | surfaces | 条件 |
| --- | --- | --- | --- | --- | --- |
| view.preview | 打印预览 | Ctrl+P | 预览(10) | ribbon, menu, topbar | doc |
| layout.pageSettings | 页面设置… | — | 页面(20) | ribbon, menu | doc |
| view.toggleOutline | 大纲面板 | — | 面板(30) | ribbon, menu | doc |
| view.toggleInspector | 检查面板 | — | 面板(30) | ribbon, menu | doc |
| view.ribbonCollapse | 折叠功能区 | — | 面板(30) | ribbon | — |
| view.theme | 深色主题 | — | 外观(40) | ribbon, menu | — |
| view.palette | 命令面板 | Ctrl+K | 工具(50) | ribbon, menu | — |
| view.zoomIn | 放大 | — | 缩放(60) | ribbon, statusbar | doc |
| view.zoomOut | 缩小 | — | 缩放(60) | ribbon, statusbar | doc |
| view.zoomReset | 100% | — | 缩放(60) | ribbon, statusbar | doc |

### 文档检查 Review

| id | label | shortcut | group | surfaces | 条件 |
| --- | --- | --- | --- | --- | --- |
| file.verify | 检查文档 | — | 校验(10) | ribbon, menu | doc |
| view.toggleInspector | 修订/标注/损耗面板 | — | 面板(20) | ribbon | doc |
| （历史还原） | 还原按钮 | — | 面板内列表项 | panel | doc（非当前修订才显示） |

### 表格 Table（contextual — 仅选区在表格内出现）

| id | label | group | 条件 |
| --- | --- | --- | --- |
| table.rowAddBefore | 在上方插入行 | 行(10) | table |
| table.rowAdd | 在下方插入行 | 行(10) | table |
| table.rowDelete | 删除行 | 行(10) | table |
| table.colAddBefore | 在左侧插入列 | 列(20) | table |
| table.colAdd | 在右侧插入列 | 列(20) | table |
| table.colDelete | 删除列 | 列(20) | table |
| table.merge | 合并单元格 | 单元格(30) | table |
| table.split | 拆分单元格 | 单元格(30) | table |
| table.header | 切换表头行 | 属性(40) | table |
| table.delete | 删除表格 | 属性(40) | table |
| table.fix | 修复表格结构 | 属性(40) | table |

## 3. 明确延期（不渲染入口）

| 能力 | 原因 |
| --- | --- |
| 建议修订模式（tracked changes） | 无文档模型与保存往返支持 |
| 编辑/阅读模式切换 | 需先实现 PM editable 与命令限制；本期不做 |
| 字段插入、Shapes、Dictate | 无后端/模型能力 |
| Pages 缩略图轨 | 仅在预览分页成功后才有真实页图；连续编辑模式不显示过期页码 |
| 标尺拖拽 | 无命中模型 |
| 评论线程 | 语义标注 ≠ 评论线程，不包装 |
| 主题/字体的实时预览 picker | 首期 picker 只提交一次 transaction |

## 4. 能力矩阵（HTTP vs Tauri）

| 能力 | HTTP 浏览器 | Tauri 桌面 |
| --- | --- | --- |
| 编辑/保存/恢复会话 | 单文档服务端会话（启动即打开） | 多文件、新建/打开/另存 |
| 文件对话框（open/saveAs/import/export） | 不可用 → 对应菜单项不渲染 | `pick*` gateway |
| 任务（import/export/publish/cancel） | 不可用 → 不渲染 | `runJob`/`cancelJob` |
| 打印预览/页面设置/缩放/主题 | 可用 | 可用 |
| 崩溃恢复草稿 | 空实现（不提示） | `listRecoveries` |

## 5. 焦点与异步契约

- 即时格式命令执行后焦点还正文（保留选区）；picker/查找/预览打开期间焦点留在控件；
  关闭/确认后回到触发控件或正文。
- 命令一律经 PM transaction 或 `DocumentController` 入口；surface 不直接改 DOM 文档。
- 预览/对话框/任务响应以 session epoch + openSeq 判定过期；切换 tab/主题/面板不销毁
  EditorView、不清 undo、不断 IME。
