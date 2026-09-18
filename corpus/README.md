# 测试语料库

落地方案 §10「测试与验收体系」的语料资产。黄金样例 `.azodoc` 在 `spec/examples/`（M0 夹具，冻结勿动）。

## 结构

| 目录/文件 | 用途 |
|---|---|
| `markdown/basic.md` | MD 恒等语料：标题/段落/强调/列表/任务/表格/脚注/数学/front-matter |
| `markdown/lossy.md` | MD 有损语料：内联 HTML、HTML 注释、HTML 块（preserved_raw 断言用） |
| `html/basic.html` | HTML 恒等语料：标题/列表/表格/引用/代码/图/硬换行 |
| `html/lossy.html` | HTML 有损语料：script、onclick、javascript: URL、自定义元素、样式类、iframe、注释 |
| `golden/markdown-basic.txt` | TXT 兜底导出黄金文件（`AZODOC_WRITE_GOLDEN=1 cargo test` 重新生成） |
| `golden/html-basic.txt` | 同上（HTML 来源） |
| `editor/` | E0 契约夹具（editor 增强路线图）：`asset-mixed`（内嵌+外链+孤儿）、`asset-external`、`legacy-data-uri.json`（旧 data URI，非容器）、`table-spans`（span/width/role/header_row）、`table-irregular`（重叠+越界，仅诊断）、`theme-history`（rev2 带 theme 快照，rev1 仅正文）、`layout-hints`（脚注+分页提示）、`pixel.png`（1×1 PNG 原件）。由 `cargo run -p athanor-cli --example fixtures` 确定性生成 |

## 恒等判定

往返恒等 = **模型层恒等**（内容树在资产 ID 序号化后一致），不是字节恒等。
有损特性不参与恒等测试，由各转换器的 `mapping` 测试按损失分级断言（落地方案 §7 映射表）。
