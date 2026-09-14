# Azodoc 规范包（v1.0-draft）

> **Status:** M0 交付稿，待评审签署
> **上游文档:** 《Azodoc v0.1 — 落地方案》（design_docs/）、《SDOC Document Model v0.1 — 草案》
> **关键词约定:** 「必须（MUST）/不得（MUST NOT）/应当（SHOULD）/可以（MAY）」按 RFC 2119 / RFC 8174 解释。

## 文件索引与阅读顺序

| 文件 | 内容 | 对应落地方案 |
|---|---|---|
| [azodoc-container.md](azodoc-container.md) | 物理容器：Header 字节布局、ZIP 规则、探测与恢复、安全限制 | §4.1 / §4.2 |
| [azodoc-package.md](azodoc-package.md) | 包结构：目录布局、manifest、各层文件义务、兼容缓存、verify 算法 | §4.2 / §4.3 |
| [azodoc-model.md](azodoc-model.md) | Prima 内容模型：Block/Inline 目录、ID 与引用、纯文本化算法 | §4.4 / §4.5 |
| [azodoc-loss.md](azodoc-loss.md) | 降级：损失分级、conversion-report、安全例外、writer 义务 | §4.11 |
| [json-schema/](json-schema/) | 各层 JSON Schema（draft 2020-12），与上述规范配套 | §4.3–§4.11 |
| [examples/](examples/) | 3 个黄金样例 `.azodoc` + 生成/校验脚本 | M0 验收物 |

## JSON Schema ↔ 层文件对照

| 容器内文件 | Schema |
|---|---|
| `manifest.json` | `json-schema/manifest.schema.json` |
| `document/content.json`（含修订快照） | `json-schema/content.schema.json` |
| `semantics/annotations.json` | `json-schema/annotations.schema.json` |
| `presentation/theme.json` | `json-schema/theme.schema.json` |
| `publication/publication.json` | `json-schema/publication.schema.json` |
| `revisions/chain.json` | `json-schema/revisions.schema.json` |
| `assets/registry.json` | `json-schema/assets.schema.json` |
| `reports/*.json` | `json-schema/conversion-report.schema.json` |

Schema 的定位：**校验 v1 已知结构**。所有对象默认允许未知成员（`additionalProperties: true`），配合保真规则 R2 实现前向兼容；遇到 Schema 之外的节点类型按 R3 包装处理，不视为致命错误（见 azodoc-package.md §8「宽容解析」）。

## 黄金样例

| 样例 | 覆盖点 |
|---|---|
| `examples/minimal.azodoc` | 最小合法容器：Header + ZIP + manifest + content + TXT 兼容缓存；验证层的可选性 |
| `examples/rich.azodoc` | 全功能：全部 v1 块/行内类型、资产、语义标注、主题、双修订快照、html/md/txt 兼容缓存、转换报告 |
| `examples/forward-compat.azodoc` | R1/R2/R3 夹具：未知 ZIP 条目、manifest 未知字段与未知层、unknown 块/行内节点、preserved/ 载荷 |

## M0 验收清单（签署用）

- [x] 三个样例通过 `python spec/examples/generate.py --verify`（Header 字节、ZIP 完整性、层文件 sha256、Schema 校验；输出字节确定，两次生成哈希一致）
- [x] 三个样例可被 Info-ZIP `unzip -l` 直接列出（无需改名；带 SFX 前缀的标准警告，见容器规范 §9）
- [ ] 三个样例可被 Windows 资源管理器 / 7-Zip GUI 直接打开（人工验证）
- [ ] 四份规范评审通过
- [ ] 签署：__________ 日期：__________

## 校验命令

```bash
# 结构与 Schema 全量校验
python spec/examples/generate.py --verify

# 跨工具打开（Git Bash）
unzip -l spec/examples/minimal.azodoc
unzip -p spec/examples/minimal.azodoc compatibility/text/document.txt
```
