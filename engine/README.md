# Athanor Engine

Azodoc 格式的 Rust 引擎（落地方案 §5 的 workspace 结构）。**当前状态：M4 已交付。**

## 布局

```text
engine/
├── Cargo.toml                    # workspace（固定依赖版本，见 [workspace.dependencies]）
└── crates/
    ├── azodoc-model/             # Prima 内容模型类型 + manifest 类型 + Value 层校验
    │                             # + schemars Schema 生成（schema_gen）
    ├── azodoc-container/         # 32 字节 Header、ZIP、探测算法、保真重写（R1/R2）、
    │                             # 资源限制、两级恢复（recover）、最小文档构建（builder）
    ├── azodoc-convert/           # 损失日志（R6）、conversion-report 生成、导入产物装配、
    │                             # 纯文本算法、TXT 兜底导出、相邻 text span 合并归一化
    ├── azodoc-md/                # Markdown 读写器（comrak 桥接：GFM 表格/任务/脚注/数学/提示块）
    ├── azodoc-html/              # HTML 读写器（html5ever 桥接）+ 片段净化（安全例外）
    ├── azodoc-docx/              # DOCX 读写器（Pandoc 子进程桥接，GPL 进程级隔离；
    │                             #   docx 部件清点：页眉/页脚/批注丢失必报）
    ├── azodoc-convert/…          # + semantics：语义标注的编辑后重定位（text_quote
    │                             #   上下文重锚 / 唯一迁移 / detached 标记）
    └── athanor-cli/              # 二进制 `athanor`：new / info / verify / recover /
                                  # import / transmute / upgrade / history / commit /
                                  # checkout / annotate / annotations
```

**DOCX 依赖 Pandoc（运行时可选）**：查找顺序 `AZODOC_PANDOC_PATH` → PATH →
各级目录 `tools/pandoc-*/`。未安装时 DOCX 功能优雅缺席并给出安装指引，
其余功能不受影响。便携版安装：从
[pandoc releases](https://github.com/jgm/pandoc/releases) 下载 zip 解压到
`tools/pandoc-3.11/`（已被 gitignore）。

## 构建与测试

```bash
cd engine
cargo build            # 调试构建；产物 target/debug/athanor.exe
cargo test             # 全部测试（M1–M4，共 75 项；DOCX e2e 需 Pandoc，缺席自动跳过）
cargo fmt && cargo clippy   # 提交前建议

# 重新生成 TXT 黄金文件（有意变更 TXT 输出时）
AZODOC_WRITE_GOLDEN=1 cargo test -p athanor-cli txt_golden
```

## CLI 用法

```bash
athanor new doc.azodoc --title "新文档" --lang zh-CN
athanor import input.md -o doc.azodoc [--report r.json] [--strict-loss]
athanor import input.html -o doc.azodoc
athanor info doc.azodoc [--json]
athanor verify doc.azodoc                 # 退出码 0=有效，1=无效
athanor transmute doc.azodoc --to html --out out.html [--no-cache] [--strict-loss]
athanor transmute doc.azodoc --to md | txt
athanor upgrade doc.azodoc                # 重建 stale 的兼容缓存
athanor recover broken.azodoc -o out/
athanor history doc.azodoc [--json]
athanor commit doc.azodoc --author ai:model-x --message "AI 批量改写"
athanor checkout doc.azodoc --revision rev_01J… [-o dir/]
athanor annotate doc.azodoc --block blk_01J… --type Concept --value '{"label":"x"}'     --exact "引用文本" --author ai:model-x
athanor annotations doc.azodoc [--json]
```

导入自动落 `importer` 初始修订；checkout 后 content 与快照哈希一致、标注自动重定位、
兼容缓存标记 stale（upgrade 重建）。

`transmute` 默认把输出同时写回容器内的兼容缓存（R5 边界内）并登记 conversion-report；
`--strict-loss` 在存在任何损失时以退出码 3 结束（loss 规范 §6）。

## M2 验收对照（落地方案 §9）

| 验收项 | 状态 | 证据 |
|---|---|---|
| ① 语料库 md/html 往返恒等 | ✅ | azodoc-md / azodoc-html 的 `roundtrip` 测试（模型层恒等：资产 ID 序号化后内容树一致） |
| ② 映射表行损失级别断言 | ✅ | azodoc-md `mapping`（9 项）+ azodoc-html `mapping`（10 项，含安全例外条款） |
| ③ `--to txt` 黄金文件 | ✅ | `corpus/golden/*.txt` + `m2_tests::txt_golden_*` |
| ④ stale 标记与 upgrade 生效 | ✅ | `m2_tests::upgrade_rebuilds_stale_caches`（content_sha256 岔度检测，spec v1.0.1 增补） |

附带修复（M2 过程中发现）：容器重写时新增条目（如导出报告）未写入 ZIP——已修复并有回归覆盖。

## 与落地方案的偏差（有意为之）

- `miette` 暂未引入：四段式输出自绘即可，后续编辑器诊断再评估。
- 验收 ④ 的 Schema 比对形式是「双向接受性测试」而非字面 diff（见 azodoc-model crate 测试）。
- 跨工具矩阵中的 Java `ZipFile` 项待接入 CI（spec/azodoc-container.md §9）。
- `import` 的本地图片若文件缺失则降级为外链引用并报告（PARTIAL），不静默丢弃。

## M3 验收对照（落地方案 §9）

| 验收项 | 状态 | 证据 |
|---|---|---|
| AI/人工修订均落链 | ✅ | `m3_tests::ai_and_human_commits_land_on_chain`（parent 链式、author type/id 落盘、非法作者类型拒绝） |
| checkout 后 content 与快照哈希一致 | ✅ | `m3_tests::checkout_restores_snapshot_hash_and_ids`（ID 原样保留；verify 通过；缓存标记 stale） |
| 标注「编辑后重定位」存活 | ✅ | azodoc-convert `semantics` 7 项：未变/重锚/迁移/detached 全覆盖；典型编辑场景存活率 80% ≥ 60% |

附加：import 自动落 importer 初始修订；verify 新增修订成员规则（current ∈ 链，
checkout 状态提示）与标注新鲜度检查（目标块存在性 + 引用文本匹配）。

## M4 验收对照（落地方案 §9）

| 验收项 | 状态 | 证据 |
|---|---|---|
| DOCX 导入/导出（Pandoc 桥） | ✅ | `m4_tests::docx_roundtrip_e2e`（docx→azodoc→docx→回导全链路） |
| 无法建模部件保留/报告 | ✅ | docx 部件清点（azodoc-docx `inventory`）：页眉/页脚/批注丢失必报（UNSUPPORTED）；RawBlock/RawInline → preserved_raw |
| 无 Pandoc 优雅缺席 | ✅ | 友好错误含安装指引（winget/便携版/AZODOC_PANDOC_PATH）；核心功能不受影响 |

映射表（§8.1）要点：样式丢弃（PARTIAL）、独立图片→Figure/figure、
表格含合并单元格（colSpan/rowSpan 保留）、脚注 Note→footnote、
数学 Math→inline_math/math_block、tracked changes 以 `--track-changes=all`
读入（标记为 span class，降级提升）。

## 下一步（M5）

PDF 出版管线：Prima→HTML→Paged.js→headless Chromium（Typst 备选），
publication.json 记录（source_revision/渲染器指纹/content_hash/layout_hash）。
