# Athanor Studio

**万物入炉，一炼成书。**

> English documentation: [README.md](README.md)

Athanor Studio 是 **Azodoc**（`.azodoc`）结构化单文件文档容器与 **Athanor**
Rust 引擎的所在地——创建、校验、转换、版本追踪与出版，一站式完成。

> 命名典故（炼金术主题）：**格式**叫 Azodoc（前身 Azoth），**引擎/CLI** 叫
> Athanor，未来的**编辑器**叫 Aludel，本**工作区**即 Athanor Studio。

---

## 为什么是 Azodoc？

现有生态各自解决文档问题的一角：Markdown 便携但有损、DOCX 富厚但不透明、
PDF 定格但僵死、HTML 万能却与表现纠缠。Azodoc 把它们统一：

- **单文件结构化容器**：32 字节魔数头 + 标准 ZIP。任何 ZIP 工具都能打开
  （改名为 `.zip` 即可）；陌生软件永远能通过
  `compatibility/text/document.txt` 找回正文。
- **内容是唯一真源**：渲染格式都是派生、可丢弃、可重建的（`athanor upgrade`）。
- **绝不静默丢失**：每次转换产出机器可读的损失报告（五级：
  `none / partial / degraded / unsupported / preserved_raw`）；无法建模的内容
  原样保存在 `preserved/`。
- **可验证的优雅降级**：每个特性进入格式前必须回答"在 DOCX / HTML / PDF /
  Markdown / TXT / 陌生软件里各自怎样？"（**降级测试**）。
- **修订与 AI 作者是一等公民**：导入、人工编辑、AI 批量改写都落快照链，
  作者类型化（`human / ai / importer / converter / system`）。
- **语义标注与内容分离**：引用文本锚点经重定位管线在编辑后存活
  （重锚 → 迁移 → detached——绝不删除）。
- **出版即冻结历史**：每次 `athanor publish` 写入不可变 PDF 产物与
  `publication.json` 档案（来源修订、渲染器指纹、内容哈希、排版哈希）——
  只追加、可独立验证。

## 当前状态

路线图 **M0–M6 全部完成**（规范 → 容器核心 → 转换器 → 修订与语义 → DOCX →
PDF 出版 → **Aludel 编辑器原型**），并完成三项未来计划存档项：**A1**——
Aludel 编辑器（ProseMirror 前端 + 保存落链管线：编辑会话产生 `author: human`
修订、标注随编辑自动重定位）、**A2**——AI 管线示例（LLM 产出语义标注与
改写修订，`ai:*` 作者全程可审计）与 **B1**——Paged.js/CDP 出版（页脚页码、
运行头、无竞态分页）。**114 项测试全部通过。** CI 经 GitHub Actions 在
Ubuntu/Windows 双平台跑 rustfmt + clippy + 全量测试。
其余方向存档于[未来计划文档](design_docs/Future%20Work%20—%20可选项与后续计划.md)。

| 格式 | 导入 | 导出 |
|---|---|---|
| Markdown（GFM：表格/任务列表/脚注/数学/提示块） | ✅ | ✅ |
| HTML（自包含单文件输出，导入前消毒） | ✅ | ✅ |
| DOCX（经 Pandoc 子进程桥接——GPL 在进程边界隔离） | ✅ | ✅ |
| 纯文本（最后一道恢复通道，常驻容器缓存） | ✅ | ✅ |
| PDF 出版（无头 Chromium/Edge + Paged.js 分页：页码与运行头；出版档案） | — | ✅ |

DOCX 导入会清点 Pandoc 静默丢弃的部件（页眉/页脚/批注）并报告为
`unsupported`——没有任何东西会悄悄消失。未安装 Pandoc 时，DOCX 功能优雅缺席
并给出安装指引，其余功能不受影响。PDF 出版同理：任何 Chromium 系浏览器均可
（Windows 自带的 Edge 就够了）。

## 快速上手

前置：[Rust](https://rustup.rs)（stable）。可选：Pandoc（DOCX）与
Chromium 系浏览器（PDF 出版）——两者都会自动定位，都是可选。

```bash
git clone <本仓库>
cd Athanor_Studio/engine
cargo build --release
```

CLI 位于 `engine/target/release/athanor.exe`（Linux/macOS：`athanor`）。

```bash
# 创建 / 查看 / 校验
athanor new doc.azodoc --title "演示" --lang zh-CN
athanor info doc.azodoc
athanor verify doc.azodoc            # 退出码 0 = 有效，1 = 无效

# 导入（Markdown / HTML / DOCX）
athanor import input.md -o doc.azodoc
athanor import input.docx -o doc.azodoc

# 导出（自动刷新容器内缓存并登记转换报告）
athanor transmute doc.azodoc --to html --out out.html
athanor transmute doc.azodoc --to md
athanor transmute doc.azodoc --to txt           # 最后一道恢复通道

# 修订与语义
athanor history doc.azodoc
athanor commit doc.azodoc --author ai:model-x --message "批量改写"
athanor checkout doc.azodoc --revision rev_01J… # content == 快照，ID 原样保留
athanor annotate doc.azodoc --block blk_01J… --type Concept \
    --value '{"label":"演示"}' --exact "引用文本" --author ai:model-x

# 出版（无头 Edge/Chrome + Paged.js 分页 → PDF + 冻结出版档案）
athanor publish doc.azodoc --out out.pdf
athanor publish doc.azodoc --no-paged            # Chromium 直印，无页码边盒

# AI 管线演示（默认离线 FakeProvider；LLM 产出带 confidence 的 ai:* 标注
# 与改写修订，可人工复核并回滚）
cargo run -p athanor-cli --example ai_pipeline -- doc.azodoc

# 编辑器原型（本机浏览器打开：编辑、保存自动落链 author:human、侧栏
# 修订历史 / 标注 / 损失盘点）
cargo run -p aludel -- doc.azodoc

# 抢救损坏文件（绝不修改源文件）
athanor recover broken.azodoc -o recovered/
```

所有失败都是四段式友好错误：*发生了什么 / 文件没有损坏 / 该做什么 / 恢复命令*。

## 容器速览

```
doc.azodoc  =  32 字节头（"AZODOC\r\n" + 版本 + CRC）  +  ZIP
├── manifest.json                     # 包清单（首条目，STORED）
├── document/content.json             # ★ Prima 内容模型——唯一真源
├── semantics/annotations.json        # W3C 风格标注（text_quote 锚点）
├── presentation/theme.json           # 主题、样式类、排版提示
├── publication/publication.json      # 冻结的出版档案
├── revisions/chain.json + 快照       # 快照链（作者类型化）
├── assets/registry.json + 二进制     # 不可变、内容寻址的资产
├── preserved/                        # 无法建模的原始内容——只进不出
├── compatibility/                    # md/html/txt/docx/pdf 派生缓存
└── reports/                          # 转换损失报告
```

前向兼容是规范级要求：读取器必须逐字节保留未知 ZIP 条目、原样保留未知
JSON 字段、把未知节点类型包装为带载荷的 `unknown` 节点。旧工具往返新文件，
不丢分毫。

## 仓库结构

| 路径 | 内容 |
|---|---|
| [`engine/`](engine/) | Athanor 引擎——Rust workspace（10 个 crate，114 项测试） |
| [`spec/`](spec/) | Azodoc v1.0 规范（容器/包/模型/降级）+ JSON Schema + 黄金夹具 |
| [`corpus/`](corpus/) | 测试语料、TXT 黄金导出、有损特性夹具 |
| [`design_docs/`](design_docs/) | 设计草案、落地方案、已存档的未来计划 |
| [`tools/`](tools/) | 本地运行时辅助（便携版 Pandoc）——已 gitignore；Paged.js 已 vendor 至 `engine/crates/azodoc-pdf/assets/` |

引擎 crate：`azodoc-model`（Prima 类型 + 校验）·
`azodoc-container`（头/ZIP/探测/保真重写/恢复）·
`azodoc-convert`（损失报告、TXT 导出）· `azodoc-md` · `azodoc-html` ·
`azodoc-docx`（Pandoc 桥）· `azodoc-pdf`（CDP + Paged.js 出版）·
`azodoc-pm`（Prima ↔ ProseMirror 转换与 schema 生成）· `athanor-cli` ·
`aludel`（编辑器原型：本地 server + Web 前端）。

```bash
cd engine && cargo test    # 114 项测试；DOCX/PDF e2e 在工具缺席时自动跳过
```

## 文档

- **[设计草案](design_docs/SDOC%20Document%20Model%20v0.1%20—%20草案.md)** — 最初的概念草案
- **[落地方案](design_docs/Azodoc%20v0.1%20—%20落地方案.md)** — 本仓库据此落地，附各里程碑验收记录
- **[M6 编辑器原型计划](design_docs/M6%20—%20Aludel%20编辑器原型计划.md)** — A1 的 RFC、Prima ↔ ProseMirror 映射表与分阶段实施记录
- **[未来计划 / 可选项](design_docs/Future%20Work%20—%20可选项与后续计划.md)** — 已存档的路线图候选（原生 OOXML 等）；A1（编辑器）、A2（AI 管线）与 B1（Paged.js 出版）已交付
- **[spec/](spec/)** — 容器/包/模型/降级的规范文本

## 许可证

版权所有 © 2026 Athanor Studio 全体开发者。

本项目以 **GNU Affero 通用公共许可证 v3.0（仅此版本，AGPL-3.0-only）** 发布——
详见 [LICENSE](LICENSE) 文件。所有源码文件均带对应版权声明。

简而言之：你可以自由使用、研究、修改和再分发 Azodoc 与 Athanor；但若分发修改版
（或以网络服务形式运行修改版），必须以同一 AGPL-3.0 许可完整公开对应源码。

DOCX 桥接在许可上是干净的：Pandoc 以用户独立安装的**独立进程**、经标准流调用，
其 GPL 代码从不被链接进引擎、也从不随引擎分发——因此 Athanor 可以保持 AGPL-3.0
的同时享受 Pandoc 的能力。
