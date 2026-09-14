# Azodoc Container Specification v1.0-draft

> 规范物理容器：文件字节布局、Header、内嵌 ZIP 档案规则、格式探测与恢复路径、安全限制。
> 关键词 MUST / MUST NOT / SHOULD / MAY 按 RFC 2119 解释。
> 上层规范：[azodoc-package.md](azodoc-package.md)（包结构与 manifest）。

---

## 1. 总览

一个 `.azodoc` 文件由两部分拼接而成：

```text
┌──────────────────────────────┐ 偏移 0
│ Azodoc Header（32 字节）      │
├──────────────────────────────┤ 偏移 header_size（v1 = 32）
│ 标准 ZIP 档案                 │ ← zip_offset
└──────────────────────────────┘
```

设计动机（承《SDOC Document Model v0.1》§2.2 / §4）：

- Header 提供稳定魔数与版本位，使识别**不依赖扩展名**（§4.1）。
- Header 位于文件头部，满足「文件开头稳定可识别」（§4.2）。
- ZIP 读取器通过文件尾部的 End of Central Directory（EOCD）定位，容忍头部前缀字节（自解压档案即工作于此机制）。因此任何标准 ZIP 工具**无需改名即可打开**本格式——「改名 `.zip` 后可读」是本规范的**可推导性质**，也是 M1 的实测验收项（§9）。

---

## 2. 规范性引用

| 引用 | 用途 |
|---|---|
| ZIP APPNOTE (.ZIP File Format Specification) 6.3.x | ZIP 结构 |
| RFC 2119 / RFC 8174 | 关键词 |
| RFC 3339 | 时间戳（仅 JSON 层；ZIP 内时间戳见 §5.6） |
| ULID spec（Crockford Base32 变体） | ID 生成（见 azodoc-model.md §3） |
| FIPS 180-4 | SHA-256 |
| CRC-32/ISO-HDLC（即 zlib `crc32`） | Header 校验 |

---

## 3. Header 布局（v1）

| 偏移 | 大小 | 类型 | 字段 | v1 取值 / 语义 |
|---|---|---|---|---|
| 0 | 8 | bytes | `magic` | ASCII `"AZODOC\r\n"`（`41 5A 4F 44 4F 43 0D 0A`） |
| 8 | 1 | u8 | `version_major` | `0x01`。**破坏性变更位**：布局断裂时 +1 |
| 9 | 1 | u8 | `version_minor` | `0x00`。向后兼容增量位 |
| 10 | 2 | u16 LE | `header_size` | `32`。未来版本可扩展 Header，读取器以本字段确定 ZIP 起点 |
| 12 | 4 | u32 LE | `flags` | bit0 = 使用了 ZIP64；bit1–31 保留，**必须为 0** |
| 16 | 8 | u64 LE | `zip_offset` | ZIP 档案起始偏移，v1 **必须等于** `header_size` |
| 24 | 4 | u32 LE | `header_crc32` | 前 24 字节的 CRC-32（IEEE，即 zlib crc32） |
| 28 | 4 | bytes | `reserved` | 全 0，读取器**必须忽略** |

多字节整数一律**小端（LE）**。

### 3.1 写入规则

1. 写入器 MUST 先写恰好 32 字节的 v1 Header，紧接 ZIP。
2. `flags` bit0 当且仅当本 ZIP 实际使用 ZIP64 结构时置 1。
3. `header_crc32` MUST 覆盖字节 [0, 24)。
4. `version_minor` 增量扩展若需占用 Header 空间，MUST 通过增大 `header_size` 实现；新增字段追加在 [32, header_size) 区间，不得改动既有字段语义。

### 3.2 Header 扩展的前向兼容

读取器遇到 `header_size > 32` 时：

- MUST 校验 `header_crc32` 后跳过 [32, header_size) 的未知扩展字节继续工作；
- 若需**原位重写** ZIP（`zip_offset` 与 `header_size` 不变），MUST 将扩展字节原样拷回（承接 R2）；
- 若需改变容器几何（如引入 ZIP64），而本实现不支持该 minor 的扩展语义，MUST 拒绝重写并提示升级——不得猜测扩展含义后重建 Header。

---

## 4. 格式探测与打开流程（Reader）

读取器 MUST 按以下顺序判定：

```text
1. 文件长度 < 32？
   └─ 是 → 若前 4 字节为 "PK\x03\x04" 走步骤 3（可能为截断的 Azodoc）；
           否则报「不是 Azodoc 文档」。
2. 前 8 字节 == magic？
   ├─ 是 → 校验 version_major：
   │        · major > 本实现支持 → 拒绝打开（不解析！），输出恢复指引（§4.2）。
   │        · major == 1 → 校验 header_crc32：
   │            · 通过 → 按 zip_offset 打开 ZIP，成功。
   │            · 失败 → 警告「Header 可能损坏」，转入步骤 3 兜底。
   └─ 否 → 若前 4 字节为 "PK\x03\x04" → 步骤 3；
           否则报「不是 Azodoc 文档」（文件未损坏，仅格式不符）。
3. Plain-ZIP 探测（兜底）：
   · 定位 EOCD，读取中央目录，寻找名为 "manifest.json" 的条目；
   · 解析之：若含对象成员 "azodoc"（其内含字符串成员 "format_version"）→
     按 Azodoc 打开（Profile B 或前缀受损的 Profile A），
     manifest.azodoc.container_profile == "prefixed" 时 MUST 输出
     「容器前缀缺失，文件可能曾被截断或改造」警告；
   · 找不到 → 报「不是 Azodoc 文档」。
```

### 4.1 不得半解析（R4）

任何一步的「拒绝」都 MUST 发生在任何内容解析之前。拒绝路径 MUST 输出四段式信息：

```text
错误：<具体原因>
文件本身<没有损坏/可能损坏>。你可以：
  1. <升级/修复建议>
  2. 抢救内容：athanor recover <file> -o <dir>
```

### 4.2 版本策略

- `version_major` 相同的文件 MUST 全部可读（无论 minor 高低）；遇到更高 minor，SHOULD 提示「文件由更新版本创建，建议升级工具」。
- 拒绝更高 major 时，MUST NOT 尝试解析其 ZIP 内容后输出半份数据；只允许输出恢复指引。
- `manifest.azodoc.format_version` MUST 与 Header `major.minor` 一致；不一致 → `athanor verify` 报错（见 azodoc-package.md §9）。

---

## 5. ZIP 档案规则

### 5.1 条目命名

- MUST 为 UTF-8、正斜杠分隔的相对路径；
- MUST NOT 包含：空段、`.`/`..` 段、盘符、前导 `/`、NUL 字节；
- 单段长度 ≤ 255 字节，全路径 ≤ 1024 字节（UTF-8 计）。

### 5.2 条目顺序与 manifest

- 写入器 MUST 将 `manifest.json` 作为**第一个条目**且 **STORED（method 0，不压缩）**。
- 读取器遇到 manifest 非首条目时 MUST NOT 拒绝，SHOULD 警告（可能经过过第三方工具重排）。

### 5.3 压缩

- JSON 层默认 Deflate（method 8）；
- 图片 / 音视频 / PDF 等已压缩载荷（`assets/`、`compatibility/pdf/` 等）SHOULD 用 STORED；
- 其他压缩方法 MAY 被写入器使用，但读取器只需支持 0（STORED）与 8（Deflate）；遇到不支持的方法 MUST 报错而非跳过（跳过会造成静默缺层）。

### 5.4 加密

- 写入器 MUST NOT 生成加密条目（加密破坏「任何工具可读」性质）。
- 读取器遇到加密条目 MUST 明确报错并列出条目名，不得静默跳过。

### 5.5 未知条目的保真（R1）

工具重写容器时，对**不在本实现已知布局内**的条目（如未来版本新增的目录）：

- 条目名 MUST 不变；
- 条目**解压后内容 MUST 字节一致**；
- 压缩方法 MAY 改变（重压缩），但 SHOULD 保持 STORED 原样。
- 读取→写入往返 MUST NOT 丢弃、重命名、移动任何未知条目。

> 已知布局 = azodoc-package.md §2 所列路径树。未知路径一律落入本条管辖。

### 5.6 时间戳与确定性

- ZIP 条目的 DOS 时间戳字段 SHOULD 统一写固定值（推荐 `1980-01-01 00:00:00`）或文档修改时间；读取器 MUST 忽略之。
- 确定性出版（publication.layout_hash）依赖的是**内容层**，不依赖 ZIP 字节；但写入器 SHOULD 保持字节确定性以便整文件级去重与内容寻址。
- 写入器 SHOULD 设置条目外部属性为 `0644`（即 `(0o100644) << 16`）。

### 5.7 结构约束

- MUST NOT 使用多卷/分卷档案；EOCD 的磁盘号字段必须为 0。
- Data Descriptor（GP flag bit3）写入器 SHOULD 避免使用；读取器 MUST 支持。
- ZIP64：文件 ≥ 4 GiB 或条目数 ≥ 65536 时 MUST 使用 ZIP64 并置 `flags` bit0。

---

## 6. 资源限制（DoS / ZIP 炸弹防护）

读取器 MUST 强制以下默认上限（可配置放宽，但 MUST 有上限）：

| 项 | 默认上限 |
|---|---|
| 条目总数 | 65 536 |
| 单条目解压后尺寸 | 512 MiB |
| 全部条目解压后总尺寸 | 2 GiB |
| 单条目压缩比 | 200 : 1（超过即拒绝，防炸弹） |
| 解压超时 | 30 s（可选实现） |

超限 MUST 产出友好错误（四段式），MUST NOT 部分解析后继续。

---

## 7. 容器 Profile

| Profile | `manifest.azodoc.container_profile` | 布局 | 使用场景 |
|---|---|---|---|
| **A `prefixed`**（默认/标准） | `"prefixed"` | 32 字节 Header + ZIP | 一切正常写入 |
| **B `plain_zip`**（兼容回退） | `"plain_zip"` | 无前缀，ZIP 开头即 `PK\x03\x04` | 实测矩阵（§9）证明某关键工具不容忍前缀时，或修复受损文件时 |

- 两种 Profile 的探测统一走 §4 流程（Profile B 命中步骤 3）。
- Profile B 是**过渡性降级**，不改变包内布局与任何语义；规范层面对两者一视同仁。

---

## 8. 恢复命令契约（`athanor recover`）

`recover` MUST 尽最大努力从受损文件提取内容，按优先级输出：

1. 若 ZIP 结构完好：直接按普通 ZIP 解出全部层文件；
2. 若中央目录损坏：扫描 Local File Header 签名逐条目抢救；
3. 输出目录结构 = 包内路径树；同时产出 `recovery-report.txt` 说明每条目状态。
4. `recover` 永不修改源文件。

---

## 9. 跨工具实测矩阵（M1 验收项）

以下工具打开 Profile A 样例 MUST 能列出/解出全部条目（识别为普通 ZIP 即可）：

| 工具 | 方式 | 结果记录 |
|---|---|---|
| Info-ZIP unzip | `unzip -l` / `unzip -p` | **M0 已验**：完整列出/解出条目；输出 `32 extra bytes at beginning` 警告并以警告码（1）退出——SFX 前缀的标准行为，读取不受影响 |
| Python `zipfile` | 整文件只读打开 + testzip | **M0 已验**：直接打开，无需切片前缀 |
| Windows 资源管理器 | 双击 / 另存解压 | 待人工验证 |
| 7-Zip GUI | 打开档案 | 待人工验证 |
| macOS 归档实用工具 | 双击 | 待人工验证（无 macOS 环境时豁免） |
| Java `java.util.zip.ZipFile` | 程序化读取 | M1 CI 项 |

任一主流工具失败 → 启用 Profile B 评估（落地方案 §4.1），并在此表记录。

---

## 10. 与保真规则的对应

| 本规范条款 | 保真规则 |
|---|---|
| §3.2 Header 扩展保留 | R2 |
| §4.1 不得半解析 | R4 |
| §5.5 未知条目保真 | R1 |
| §5.3 不支持的压缩方法必须报错 | R6（不静默缺层） |
| §5.4 加密条目明确报错 | R6 |
