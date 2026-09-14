# Azodoc Loss & Conversion Report Specification v1.0-draft

> 规范损失分级、conversion-report、安全例外条款、各目标格式的 writer 义务。
> 本规范是草案 Principle 2/6（不静默丢弃 / 一切降级可检测）的实施细则。
> Schema：[json-schema/conversion-report.schema.json](json-schema/conversion-report.schema.json)。

---

## 1. 损失分级

对「节点 N（块/行内/属性）写入目标格式 T」的判定，五级封闭枚举（JSON 中用小写）：

| 级别 | JSON 值 | 定义 |
|---|---|---|
| NONE | `"none"` | T 能无损表达 N 的类型与全部属性 |
| PARTIAL | `"partial"` | T 能表达 N 的主体，部分属性丢失，但语义与视觉可接受 |
| DEGRADED | `"degraded"` | N 在 T 中语义或视觉明显退化（仍可用） |
| UNSUPPORTED | `"unsupported"` | N 无法表达且**未存储**原始表示（流式输出目标、或 §3 安全例外） |
| PRESERVED_RAW | `"preserved_raw"` | N 无法表达，但原始表示已存入 `preserved/`，目标处放置占位 |

### 1.1 判定流程（writer 义务）

```text
1. T 能无损表达？ ──是──► NONE
2. 部分属性可表达？ ──是──► PARTIAL（detail 列出丢弃属性）
3. 可表达但明显退化？ ──是──► DEGRADED
4. 无法表达；能取得原始表示且载体允许落盘？
   ├─ 是 ──► PRESERVED_RAW（入 preserved/，目标处放占位）
   └─ 否 ──► UNSUPPORTED
```

原则：**宁过度报告**。两个候选级别之间取更差者；工具对边界情形的报告偏差可接受，漏报不可接受（R6）。

---

## 2. 报告的触发条件

以下情形 MUST 产出 conversion-report：

- 任何 import（外部格式 → Azodoc）；
- 任何 export/transmute（Azodoc → 外部格式）；
- `athanor upgrade` 的每次缓存重建（可选：建议开启）。

`athanor new`（空文档）不需要报告。

---

## 3. 安全例外（对「不静默丢弃」的显式让位）

**活动内容（active content）**指一切可能在阅读器中执行代码或发起意外导航的构造：

- `<script>`（任意位置、任意形式）；
- 事件处理器属性（`on*`，如 `onclick`、`onload`）；
- `javascript:` 与 `data:text/html` 形态的 URL（出现在 href/src 等导航/嵌入上下文）；
- 含脚本的 SVG、`<iframe srcdoc>` 携带脚本、`<meta http-equiv="refresh">`；
- 未来发现的同类构造（判断标准：**是否可执行/可劫持导航**，而非枚举）。

政策：

1. import 时活动内容默认**不保留、不执行**；每处 MUST 记为 issue（`loss_class: "unsupported"`，`action: "removed"`，`detail` 注明 `security`）。
2. 提供显式取证开关 `--preserve-raw-active`：活动内容改存 `preserved/`（`loss_class: "preserved_raw"`，`action: "preserved_quarantined"`），**仍不参与渲染**。
3. export 时 writer MUST NOT 把 `preserved/` 载荷内联进 HTML 输出，除非载荷经消毒（sanitized）或用户显式传 `--allow-active-html`。**安全例外与 R6 的关系**：这不是静默丢弃——每处都有 issue、有计数、有汇总。

---

## 4. conversion-report 规范

文件位于 `reports/<direction>-<source_format>-<seq4>.json`（azodoc-package.md §5）。

```json
{
  "report_version": "1.0",
  "direction": "import",
  "source": { "format": "markdown", "path": "input.md", "sha256": "…" },
  "target": { "format": "azodoc", "document_id": "doc_…", "revision": "rev_…" },
  "engine": { "name": "athanor", "version": "0.1.0", "converter": "athanor-md/0.1.0" },
  "status": "partial",
  "summary": {
    "counts": { "blocks": 120, "spans": 640, "assets": 4 },
    "loss": { "none": 118, "partial": 1, "degraded": 0, "unsupported": 0, "preserved_raw": 1 }
  },
  "issues": [ … ]
}
```

### 4.1 字段语义

| 字段 | 语义 |
|---|---|
| `direction` | `"import"`（→ Azodoc）或 `"export"`（Azodoc →） |
| `source.format` / `target.format` | 格式 token；Azodoc 侧固定 `"azodoc"` |
| `summary.loss` | **按节点计**（块与行内 span 都计）；五键必须齐全，缺省为 0 |
| `summary.counts` | 自由计数字典（blocks/spans/assets 至少给出 blocks） |
| `status` | `"complete"` 当且仅当 loss 仅含 none；否则 `"partial"` |
| `issues[]` | 每处非 NONE 损失一条（v1 不合并重复项） |

### 4.2 issue 条目

| 字段 | 语义 |
|---|---|
| `id` | `iss_` + ULID |
| `feature` | 特性 token（封闭注册起步：`inline_html`、`html_comment`、`active_content`、`math_export`、`merged_cell`、`footnote`、`front_matter_key`、`style_attribute`…；未知 token 按 R2 保留） |
| `loss_class` | §1 五级之一（小写） |
| `location` | import：`{"source_line": n, "source_column": n?}`；export：`{"block": "blk_…"}`；均可带 `context: "<节选文本>"` |
| `action` | `"preserved"` / `"preserved_quarantined"` / `"removed"` / `"degraded"` / `"dropped"` |
| `detail` | 机器可读细节（如 `preserved/html/part-0001.html`） |
| `message` | 面向用户的单句说明（本地化语言随引擎） |

### 4.3 一致性义务

- `summary.loss` 中每个非零项 MUST 至少对应一条 issue；issue 数与对应级别计数一致（v1 不合并；`athanor verify` 校验此一致性）。
- 报告写入容器后 MUST 在 manifest.reports 登记（package §3.6）。
- 同一文档多次转换产生多个报告，**互相独立、追加不覆盖**。

---

## 5. 各目标格式的 writer 义务矩阵

### 5.1 HTML

- 资产 MUST 以 data URI 内嵌（自包含、离线可读）；
- MUST 嵌入血统标记：`<meta name="azodoc.document-id">` 与 `<meta name="azodoc.revision">`——使「浏览器另存 → 再导入」可识别回 Azodoc 血统；
- 数学：v1 输出 `<span class="math" data-latex="…">latex 原文</span>`，记 DEGRADED（KaTeX 内嵌是未来 RFC）；
- preserved/ 载荷：`payload_ref` 指向 HTML 时 MAY 原样输出，**但必须过 §3 第 3 条的安全检查**；否则输出带边框占位框。

### 5.2 Markdown

- unknown 块输出占位注释：`<!-- azodoc: unsupported <summary>，原文见 <payload_ref> -->`；
- 无法表达的 span 退化为文本 + 记 PARTIAL/DEGRADED；
- 任务列表、脚注、表格按目标方言能力输出（GFM 基线），方言不支持的结构记 DEGRADED。

### 5.3 TXT（最后一道恢复）

- 在 azodoc-model.md §9 纯文本算法之上叠加排版增强：heading 下方加 `====/----` 下划线（长度随文字宽度，按显示宽度计，CJK 记 2）；列表加编号/圆点；代码块整体缩进两格；
- unknown 块输出 `[未能显示的内容: <summary>]`；
- TXT 输出 MUST 同步刷新 `compatibility/text/document.txt` 与 manifest 条目——这是「陌生软件恢复通道」的新鲜度保证。

### 5.4 DOCX / PDF（M4/M5 实现，规范现在即生效）

- 同样走 §1 判定流程；占位规则：DOCX 占位段落（样式 `AzodocUnsupported`），PDF 占位框；
- publication 管线（PDF）的确定性要求见 azodoc-package.md 与落地方案 §8.2。

---

## 6. CLI 行为约定

| 场景 | 行为 |
|---|---|
| 转换完成，存在非 NONE 损失 | 退出码 0；stderr 打印摘要：`完成，含 N 处降级（preserved_raw 1, partial 1）— 详情: reports/…json` |
| `--strict-loss` | 存在任何非 NONE 损失 → 退出码 3 |
| 转换失败（解析错误等） | 退出码 1，四段式友好错误 |
| `--report <path>` | 额外把报告副本写到容器外（流水线用），容器内照常登记 |

---

## 7. 测试义务（对接落地方案 §10）

1. 映射表（落地方案 §7.2/§7.3）**每一行**必须存在对应用例，断言其损失级别；
2. 每个转换器 MUST 通过「summary.loss 与 issues 一致」的元测试（对语料库全部样例）;
3. 安全例外套件：含 `<script>`、`onclick`、`javascript:` URL 的输入 → 断言全部被移除且 issue 计数正确；`--preserve-raw-active` → 断言进入 preserved/ 且 HTML 导出不再含活动内容。
