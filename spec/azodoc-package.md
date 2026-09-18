# Azodoc Package Specification v1.0-draft

> 规范 ZIP 内部的包结构：目录布局、manifest.json、各层文件的义务与写权限、兼容缓存状态、verify 算法、宽容解析规则。
> 依赖 [azodoc-container.md](azodoc-container.md)；内容模型见 [azodoc-model.md](azodoc-model.md)；降级见 [azodoc-loss.md](azodoc-loss.md)。

---

## 1. 通用 JSON 约定

适用于包内**一切 JSON 层文件**：

- 编码 MUST 为 UTF-8，无 BOM；写入器 SHOULD 以 `\n` 换行、2 空格缩进、非 ASCII 原样输出（`ensure_ascii=false` 语义）。
- 时间戳 MUST 为 RFC 3339 UTC，`Z` 后缀（如 `2026-09-14T12:00:00Z`）。
- 哈希 MUST 为小写十六进制 SHA-256（64 字符）。
- 数字 MUST 为有限数；尺寸/计数为非负整数。
- **宽容解析（R2 泛化）**：读取器遇到本规范未定义的**对象成员**或**数组元素**，MUST 将其原样保留在内存中，并在写回时**原样输出**。实现建议（非规范）：以「原始 JSON 树 + 已知字段视图」双层表示，或 serde `flatten` 进 `extra` 捕获结构。
- 各 JSON 文件 MUST 带 `schema_version` 字符串（本版为 `"1.0"`）；`manifest.json` 的版本信息在 `azodoc.format_version`。

---

## 2. 目录布局

```text
manifest.json                        ★ 必须；第一个条目；STORED
document/
  content.json                       ★ 必须；Prima 内容模型（唯一内容真源）
semantics/
  annotations.json                   ○ 可选；语义标注
presentation/
  theme.json                         ○ 可选；主题/样式/Layout hints
publication/
  publication.json                   ○ 可选；出版记录索引
revisions/
  chain.json                         ○ 可选；修订链索引
  <rev_id>/content.json              ○ 可选；各修订快照（快照模式）
  <rev_id>/theme.json                ○ 可选；该修订时的表现层快照（见 §5.3）
assets/
  registry.json                      ◆ 条件必须；content 引用了任何 asset:// 时必须存在
  <asset_id>/<filename>              ◆ 条件必须；与 registry 一一对应
preserved/                           ◇ 只进不出；无法建模的原始内容
  <origin>/<seq4>.<ext>
compatibility/                       ◇ 派生表示缓存；整目录可删可重建
  html/index.html
  markdown/document.md
  text/document.txt
  docx/document.docx
  pdf/document.pdf
reports/
  *.json                             ◇ conversion-report（见 azodoc-loss.md §4）
<未知路径>                           ◇ 未来版本扩展；R1 管辖，原样保留
```

图例：★ 必须（REQUIRED）；◆ 条件必须；○ 可选；◇ 见对应小节的专门规则。

- 内容层与修订层的组织 MAY 随版本演进（例如 future 的 delta 模式），条目保真义务不因本表而减少。
- 路径必须符合容器规范 §5.1。

---

## 3. manifest.json

完整 Schema：[json-schema/manifest.schema.json](json-schema/manifest.schema.json)。字段定义：

### 3.1 `azodoc`

| 字段 | 类型 | 约束 |
|---|---|---|
| `format_version` | string | `"主.次"`，MUST 与容器 Header 一致（v1.0 写 `"1.0"`） |
| `container_profile` | string | `"prefixed"` \| `"plain_zip"`（容器规范 §7） |
| `generator` | object | `{ name, version }`；产生本文件的工具 |

### 3.2 `document`

| 字段 | 类型 | 约束 |
|---|---|---|
| `id` | string | `doc_` + 26 位 ULID；文档生存期内**永不变更、永不复用** |
| `schema_version` | string | 内容层 Schema 版本（`"1.0"`） |
| `title` | string | 文档标题（与 content 内首 heading 无强制同步关系；编辑器 SHOULD 提供联动） |
| `language` | string | BCP 47（如 `zh-CN`） |
| `created_at` / `modified_at` | string | RFC 3339 UTC |

### 3.3 `current_revision`

- string（`rev_` 前缀 ID）或 `null`（文档从未落修订）。
- MUST 等于 `revisions/chain.json` 的 `head`（当后者存在时）；否则 verify 报错。

### 3.4 `layers`

固定成员（值域统一为 `{ "path": string, "sha256": string }`）：

| 成员 | 指向 | 要求级别 |
|---|---|---|
| `content` | `document/content.json` | 必须 |
| `assets` | `assets/registry.json` | 条件（§4） |
| `semantics` | `semantics/annotations.json` | 可选 |
| `presentation` | `presentation/theme.json` | 可选 |
| `revisions` | `revisions/chain.json` | 可选 |
| `publication` | `publication/publication.json` | 可选 |

规则：

- `path` 为包内相对路径；两个 layer 成员 MUST NOT 指向同一路径。
- `sha256` MUST 与文件实际内容一致（verify 强制）。
- 未知 layer 成员（未来版本注册的新层）按 R2 原样保留；读取器不得因未知成员拒绝文件，MAY 提示。
- 某层文件存在于包内但未在 `layers` 登记 → verify 警告；登记了但不存在 → verify 报错。

### 3.5 `compatibility`

数组，每项：

```json
{
  "format": "html",
  "path": "compatibility/html/index.html",
  "revision": "rev_01JAVG3A9B2C4D6E8F0H1J3K5M7",
  "generated_at": "2026-09-14T12:05:00Z",
  "generator": { "name": "athanor", "version": "0.1.0" },
  "status": "fresh"
}
```

- `format` 已知 token：`html` / `markdown` / `text` / `docx` / `pdf`；未知 token 按 R2 保留。
- `revision`：生成时绑定的修订号；文档无修订时 MAY 为 `null`。
- 缓存状态机（§6）。

### 3.6 `reports`

数组：`{ "path": string, "kind": string, "sha256": string }`。`kind` 本版固定 `"conversion"`。
`reports/` 目录中存在而未登记的文件 → verify 警告；`reports/` 文件被工具删除 → MUST NOT（provenance，仅用户手动清理）。

### 3.7 `extra`

约定俗成的兜底成员：读取器捕获的未知成员 MAY 聚合于此，也可以原位保留；两种策略等效，**关键义务是写回时字节语义等价地还原**。

---

## 4. 资产一致规则

设 content（含修订快照）中出现的一切 `asset://` 引用集合为 A，registry 登记集合为 R：

1. A 中每个 ID MUST 在 R 中存在（verify 报错：悬空引用）；
2. `storage: "embedded"` 的资产，其 `path` 指向的条目 MUST 存在且 SHA-256 与登记一致；
3. `storage: "external"` 的资产 MUST 带 `url`，其 `path`/文件体 MUST 缺席；
4. R 中不被 A 引用的资产（孤儿）**允许存在**，MUST NOT 被工具自动清除（宁多勿删；垃圾回收是未来显式功能）。

### 4.1 登记字段（冻结）

registry 条目字段定义与约束见 [json-schema/assets.schema.json](json-schema/assets.schema.json)。补充语义：

- `id`：`as_` + 26 位 ULID（azodoc-model.md §3），分配后不可复用；同一资产在全部修订间共享 ID。
- `filename`：人读名，也是 `assets/<id>/<filename>` 的叶子名。写入器 MUST 在登记前清洗：仅保留 `[^\x00-\\x1f\\/:*?"<>|]` 之外的字符，剥离路径分隔与首尾空白/点号，结果为空则退回 `asset`；清洗前名 MAY 存入 `original_filename` 供追溯（R2 成员）。`filename` 不跨目录——资产路径恒为 `assets/<id>/<filename>` 两级。
- `path`：embedded 资产在包内的相对路径，MUST 为 `assets/<id>/<filename>` 形态且满足 §5.1 路径规则（无 `..`、非绝对路径、无反斜杠）。verify 校验路径合法性与内容 sha256。
- `sha256`：embedded = 资产文件体 SHA-256；external = URL 字符串字节（UTF-8）的 SHA-256，用于检测 URL 漂移而非内容一致性。
- `storage`：`embedded`（包内携带字节）或 `external`（只登记 URL）。external 资产不参与离线展示；写入器 MUST NOT 静默下载外部 URL 转 embedded（用户显式操作为例外，且属于一次新资产登记）。
- `relationship`：自由 token，本版约定 `inline`（正文内嵌）；未知 token 按 R2 保留。
- `mime`：RFC 2046 媒体类型。字节级解码上限、MIME 白名单等**摄入上限属于工具策略**（如编辑器/导入器的资源管制），不是包契约——包契约只要求上述完整性（容器规范 §6 的 DoS 限制依然适用）。

### 4.2 旧引用形态的读取与迁移（编辑器纪律）

早于 registry 的工具/编辑器可能把资产写成 `asset` 字段中的 `data:` URI 或裸 `http(s)` URL。处置规则（冻结）：

1. **读取**：读取器 MUST 能展示这些形态；`data:` URI 与外部 URL 不是 §4 的 `asset://` 引用，不纳入 A 集合，也不需要 registry 条目。verify 对非 `asset://` 的 `asset` 值按既有的「非 asset:// 引用」规则报告；编辑器读取路径 MUST NOT 因这些形态拒绝打开。
2. **写入**：保存（编辑器显式保存/另存为）时，工具 SHOULD 把可达的 `data:` URI 解码登记为新 `as_` 资产并把引用改写为 `asset://<id>/<filename>`；这是一次**用户驱动的迁移**，不是打开时的自动改写。迁移后 registry/sha256 按 §4 一致性规则约束。
3. **失败**：任一资产登记失败时 MUST 保留原引用并报告，不得产生悬空 `asset://` 引用；未提交的暂存资产随会话关闭/取消清理，孤儿资产按 §4.4 留待未来显式回收。

### 4.3 预览快照（工具派生，不进包）

预览/导出快照是**工具内存对象**，不落入包结构：

- 快照身份 = `{ content_sha256, theme_sha256?, assets_sha256?, render_settings }` 的规范化 JSON 的 SHA-256；`assets_sha256` 为 registry 字节哈希。
- `render_settings` 为工具自有的渲染参数对象（纸张/边距/字体/DPI 等），MUST 有 `version` 字段；预览、PDF、打印 MUST 复用同一快照与同一 `render_settings`。
- 生成外部报告时 MAY 把快照哈希作为可选字段携带（azodoc-loss.md §4）。

---

## 5. preserved/ 与 reports/ 的纪律

### preserved/（只进不出）

- 任何工具 MUST NOT 修改、重命名或删除 `preserved/` 内既有文件；只能新增。
- 命名：`preserved/<origin>/<seq4>.<ext>`；`origin` 为来源格式 token（`html`/`docx`/`ooxml`/`pdf`/…），`seq4` 为四位零填充序号，**在 origin 目录内单调递增不复用**；`ext` 为原始格式扩展名。
- 引用方：content 中 `unknown` 节点的 `payload_ref`（azodoc-model.md §8）。

### reports/

- 命名：`<direction>-<source_format>-<seq4>.json`，如 `import-markdown-0001.json`；`seq4` 全目录内单调递增。
- 由转换器在每次跨格式转换时写入并在 manifest 登记（azodoc-loss.md §4）。

### 5.3 表现层修订快照（可选）

修订快照 MAY 同时落一份该修订时刻的 `presentation/theme.json` 副本：

- 路径：`revisions/<rev_id>/theme.json`；存在时 `chain.json` 对应条目 MUST 携带 `theme_path` 与 `theme_sha256`（成员名冻结），缺任一为错误。
- 携带 `theme_path`/`theme_sha256` 但文件缺失或哈希不符为错误；未携带任一成员即视为**仅正文历史**（checkout 时表现层不落，工具 SHOULD 提示而非伪造主题）。
- 快照为 `presentation/theme.json` 的完整副本（schema_version 同主题层自身版本），非 diff；旧修订无此文件属正常历史形态，不是缺陷。

---

## 6. 兼容缓存状态机（Compatibility Cache）

```text
                 ┌──────────────────────────────────────────┐
                 │ compat.revision == manifest.current_revision │
                 └───────────────┬──────────────┬───────────┘
                              是  │              │ 否
                                 ▼              ▼
                              fresh          stale
```

- 判定**只看修订绑定**；`revision == null` 的无修订缓存视为 fresh，但升级时 SHOULD 重建。
- 另一条 SHOULD：`compat.generator` 的工具版本低于当前工具大版本时，工具 MAY 将其视为 stale（自便）。
- 工具义务：
  - 打开文档时：对声明 `fresh` 但实际 `stale` 的缓存 MUST 视为 stale（以判定为准，不信任 status 字段）；
  - `athanor upgrade`：对本工具**支持**的 stale 格式逐个重建（转换器义务见 azodoc-loss.md §6），支持的格式重建后写回并更新条目；**不支持**的格式条目原样保留；
  - 重建只允许写 `compatibility/` 与 `reports/`，以及 manifest 的 `compatibility`/`reports` 成员——这是 R5 的边界；
  - 任何读取路径 MUST NOT 把 `compatibility/` 当作内容来源（内容真源唯一：`document/content.json`）。

> **v1.0.1 增补（M2）**：兼容条目 MAY 额外携带 `content_sha256`（生成时内容层的 SHA-256，
> 经 R2 通道存储于条目的未知成员中）。新鲜度判定在修订绑定之外叠加该哈希比对：
> 哈希存在且与当前内容层不一致 → stale。这使「无修订文档的缓存过期」可被检测，
> 是 `athanor upgrade` 在修订层（M3）就绪前的岔度检测基础。

---

## 7. 写权限矩阵（谁可以写什么）

| 角色 | content 层 | semantics/presentation/revisions/publication | assets | preserved/ | compatibility/ | reports/ |
|---|---|---|---|---|---|---|
| 编辑器 / 导入器 | ✅ | ✅ | ✅ | ✅ | ✅（顺带刷新） | ✅ |
| 转换器（export 方向） | ❌ 只读 | ❌ 只读 | ❌ 只读 | ❌ 只读 | ✅ 只写自己格式 | ✅ |
| `upgrade` | ❌ | ❌ | ❌ | ❌ | ✅ | ✅ |
| 第三方工具（不理解 Azodoc） | 全部条目原样保留（R1） | | | | | |

> 「❌ 只读」是 R5 的具体化：转换器绝不改动内容真源，即使发现内容「有错」也只报告。

---

## 8. 宽容解析与已知布局的边界

- 容器规范 §5.5 管辖「未知 ZIP 条目」；本规范 §1 管辖「未知 JSON 成员/元素」；azodoc-model.md §8 管辖「未知节点类型」。三层防线合起来实现：**旧工具不毁新文件，新工具不毁旧文件**。
- verify 对未知内容的处置一律是**警告/信息级**，不得作为错误：
  - 示例输出：`提示：文件含 3 处本版本未知结构（未知条目 1、未知字段 2），已按规范原样保留。`

---

## 9. verify 算法（`athanor verify`）

按序执行；**错误**导致退出码 1，**警告**不影响退出码：

| # | 步骤 | 级别 |
|---|---|---|
| 1 | 容器探测与 Header 校验（magic、版本、CRC、zip_offset）——按容器规范 §4 | 错误 |
| 2 | ZIP 结构完整、无加密条目、资源限制内 | 错误 |
| 3 | `manifest.json` 存在、JSON 语法、Schema 校验 | 错误 |
| 4 | `azodoc.format_version` == Header 版本 | 错误 |
| 5 | `layers` 登记：每项 path 存在、sha256 一致、无重复路径 | 错误 |
| 6 | 包内未登记的已知路径文件 → 警告；`content` 层缺失 → 错误 | 混合 |
| 7 | content.json Schema 校验（azodoc-model.md） | 错误 |
| 8 | 资产一致性（§4 四条） | 错误 |
| 9 | `current_revision` 与 `chain.head` 一致；各修订 `path`/`sha256` 有效 | 错误 |
| 9b | 各修订 `theme_path`/`theme_sha256` 若携带：文件存在、哈希一致、两成员成对 | 错误 |
| 10 | compatibility 状态重算（§6）；`status` 与判定不符 → 警告 | 警告 |
| 11 | 未知内容盘点（§8） | 信息 |
| 12 | 输出汇总：`有效/无效` + 错误数 + 警告数 + 损失档案摘要（聚合各 report 的 summary.loss） | — |

`verify` MUST NOT 修改文件；MUST 对截断、坏 CRC、炸弹等给出四段式友好输出。

---

## 10. 最小合法容器

同时满足以下条件即为合法（`minimal.azodoc` 样例即此形态）：

1. 容器层合法（容器规范）；
2. `manifest.json` + `document/content.json` 存在且 Schema 合法；
3. content 未引用资产（故无 assets 层）、无修订（`current_revision: null`）；
4. 其余层缺席。
