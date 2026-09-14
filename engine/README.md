# Athanor Engine

Azodoc 格式的 Rust 引擎（落地方案 §5 的 workspace 结构）。**当前状态：M1 已交付。**

## 布局

```text
engine/
├── Cargo.toml                    # workspace（固定依赖版本，见 [workspace.dependencies]）
└── crates/
    ├── azodoc-model/             # Prima 内容模型类型 + manifest 类型 + Value 层校验
    │                             # + schemars Schema 生成（schema_gen）
    ├── azodoc-container/         # 32 字节 Header、ZIP、探测算法、保真重写（R1/R2）、
    │                             # 资源限制、两级恢复（recover）、最小文档构建（builder）
    └── athanor-cli/              # 二进制 `athanor`：new / info / verify / recover
```

## 构建与测试

```bash
cd engine
cargo build            # 调试构建；产物 target/debug/athanor.exe
cargo test             # 全部 26 个测试（含黄金样例集成测试）
cargo fmt && cargo clippy   # 提交前建议
```

## CLI 用法

```bash
athanor new doc.azodoc --title "新文档" --lang zh-CN
athanor info doc.azodoc [--json]
athanor verify doc.azodoc          # 退出码 0=有效，1=无效
athanor recover broken.azodoc -o out/
```

所有打开/解析错误均按规范输出四段式友好信息（发生了什么 / 文件是否损坏 / 建议 / 恢复命令）。

## M1 验收对照（落地方案 §9）

| 验收项 | 状态 | 证据 |
|---|---|---|
| ① 任意 v1 文件读→写字节级稳定 | ✅ | `noop_roundtrip_is_byte_identical`（三个黄金样例；未修改容器原样返回，不重写） |
| ② forward-compat 夹具未知条目/字段往返保真（R1/R2） | ✅ | `modified_rewrite_preserves_unknown_entries_and_fields`（编辑 content 后重写：未知条目字节不变、manifest 未知字段/未知层保留、sha256 自动同步） |
| ③ 损坏文件全走友好报错（R4） | ✅ | 坏魔数/坏 CRC/高主版本/截断/非 Azodoc 各有专项测试；版本拒绝输出恢复指引 |
| ④ schemars Schema 与 spec/ 双向比对 | ✅ | `golden_layers_accepted_by_both_schemas`：黄金样例层文件在生成 Schema 与 spec Schema 下均被接受 |

附加：ZIP 炸弹（压缩比）拒绝、加密条目拒绝、recover 模式 A/B（中央目录 + Local Header 扫描）、
`athanor verify` 实现 package 规范 §9 的步骤 1–11。

## 与落地方案的偏差（有意为之）

- `miette` 暂未引入：四段式输出自绘即可，M2 转换诊断再评估。
- `azodoc-convert` crate 推迟到 M2（转换器注册表 + 损失报告引擎）。
- 验收 ④ 的形式是「双向接受性测试」而非字面 diff：schemars 生成物是规范的机器投影，
  手写 spec Schema 始终是 normative。
- 跨工具矩阵中的 Java `ZipFile` 项待接入 CI（spec/azodoc-container.md §9）。

## 下一步（M2）

azodoc-md / azodoc-html 读写器、conversion-report 引擎、兼容缓存刷新与 stale 标记、
`athanor import` / `athanor transmute`（含 `--to txt` 的最后一道恢复通道刷新）。
