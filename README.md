# Athanor Studio

**One file in, every format out — and back.**
**万物入炉，一炼成书。**

Athanor Studio is the home of **Azodoc** (`.azodoc`) — a structured, single-file
document container — and **Athanor**, the Rust engine that creates, validates,
converts, version-tracks, and publishes Azodoc documents.

> Naming lore (from our alchemy theme): the **format** is Azodoc (née Azoth),
> the **engine/CLI** is Athanor, the future **editor** is Aludel, and this
> **workspace** is Athanor Studio.

---

## Why Azodoc?

Existing ecosystems each solve one piece of the document problem: Markdown is
portable but lossy, DOCX is rich but opaque, PDF is frozen but inert, HTML is
universal but entangled with presentation. Azodoc unifies them:

- **One file, structured container** — a 32-byte signature header followed by a
  standard ZIP archive. Any ZIP tool can open it (rename to `.zip` and it just
  works); unknown software can always recover the text via
  `compatibility/text/document.txt`.
- **Content is the single source of truth.** Rendering formats are derived,
  disposable, and regenerable (`athanor upgrade`).
- **Never silently lose data.** Every conversion produces a machine-readable
  loss report (five loss levels: `none / partial / degraded / unsupported /
  preserved_raw`). Content that cannot be modeled is preserved verbatim under
  `preserved/`.
- **Graceful degradation, verified.** Each feature must answer "what happens in
  DOCX / HTML / PDF / Markdown / TXT / unknown software?" before it enters the
  format (the *Degradation Test*).
- **Revisions and AI authorship are first-class.** Every import, human edit, or
  AI batch-rewrite lands on a snapshot chain with typed authorship
  (`human / ai / importer / converter / system`).
- **Semantic annotations are separate from content.** Text-quote anchors
  survive edits through a relocate pipeline (re-anchor → migrate → detach —
  never delete).
- **Publications are frozen history.** Each `athanor publish` writes an
  immutable PDF artifact plus a `publication.json` record (source revision,
  renderer fingerprint, content hash, layout hash) — append-only, independently
  verifiable.

## Current status

Roadmap **M0–M5 complete** (spec → container core → converters → revisions &
semantics → DOCX → PDF publishing). **77 tests passing.**

| Format | Import | Export |
|---|---|---|
| Markdown (GFM: tables, task lists, footnotes, math, alerts) | ✅ | ✅ |
| HTML (self-contained single-file output, sanitized re-import) | ✅ | ✅ |
| DOCX (via Pandoc subprocess bridge — GPL isolated at the process boundary) | ✅ | ✅ |
| Plain text (last-resort recovery channel, always cached in-container) | ✅ | ✅ |
| PDF publication (headless Chromium/Edge; publication records) | — | ✅ |

DOCX import inventories parts that Pandoc silently drops (headers, footers,
comments) and reports them as `unsupported` — nothing disappears quietly.
Without Pandoc installed, DOCX features gracefully step aside with install
guidance; everything else keeps working. Same for PDF publishing: any
Chromium-family browser works (Windows' built-in Edge is enough).

## Quick start

Prerequisites: [Rust](https://rustup.rs) (stable). Optional: Pandoc (DOCX) and
a Chromium-family browser (PDF publishing) — both auto-discovered, both
optional.

```bash
git clone <this-repo>
cd Athanor_Studio/engine
cargo build --release
```

The CLI lives at `engine/target/release/athanor.exe` (Linux/macOS: `athanor`).

```bash
# Create / inspect / validate
athanor new doc.azodoc --title "Demo" --lang en
athanor info doc.azodoc
athanor verify doc.azodoc            # exit 0 = valid, 1 = invalid

# Import (Markdown / HTML / DOCX)
athanor import input.md -o doc.azodoc
athanor import input.docx -o doc.azodoc

# Export (caches are refreshed in-container and conversion reports are filed)
athanor transmute doc.azodoc --to html --out out.html
athanor transmute doc.azodoc --to md
athanor transmute doc.azodoc --to txt           # last-resort recovery channel

# Revisions & semantics
athanor history doc.azodoc
athanor commit doc.azodoc --author ai:model-x --message "batch rewrite"
athanor checkout doc.azodoc --revision rev_01J… # content == snapshot, IDs preserved
athanor annotate doc.azodoc --block blk_01J… --type Concept \
    --value '{"label":"demo"}' --exact "quoted text" --author ai:model-x

# Publish (headless Edge/Chrome → PDF + frozen publication record)
athanor publish doc.azodoc --out out.pdf

# Rescue a damaged file (source is never modified)
athanor recover broken.azodoc -o recovered/
```

All failures are four-line friendly errors: *what happened / the file is not
broken / what to do / the recovery command*.

## Container at a glance

```
doc.azodoc  =  32-byte header ("AZODOC\r\n" + version + CRC)  +  ZIP
├── manifest.json                     # package manifest (first entry, stored)
├── document/content.json             # ★ Prima content model — the only truth
├── semantics/annotations.json        # W3C-style annotations (text-quote anchors)
├── presentation/theme.json           # theme, style classes, layout hints
├── publication/publication.json      # frozen publication records
├── revisions/chain.json + snapshots  # snapshot chain with typed authorship
├── assets/registry.json + blobs      # immutable, content-addressed assets
├── preserved/                        # unmodelable originals — append-only
├── compatibility/                    # derived md/html/txt/docx/pdf caches
└── reports/                          # conversion loss reports
```

Forward compatibility is normative: readers must preserve unknown ZIP entries
byte-for-byte, unknown JSON fields verbatim, and unknown node types as
`unknown` wrapper nodes with preserved payloads. An old tool round-tripping a
newer file loses nothing.

## Repository layout

| Path | What |
|---|---|
| [`engine/`](engine/) | Athanor engine — Rust workspace (7 crates, 77 tests) |
| [`spec/`](spec/) | Azodoc v1.0 specifications (container, package, model, loss) + JSON Schemas + golden fixtures |
| [`corpus/`](corpus/) | Test corpus, golden TXT exports, lossy-feature fixtures |
| [`design_docs/`](design_docs/) | Design drafts, the implementation plan, and the archived future-work roadmap |
| [`tools/`](tools/) | Local runtime helpers (portable Pandoc, Paged.js) — gitignored |

Engine crates: `azodoc-model` (Prima types + validation) ·
`azodoc-container` (header/ZIP/detection/fidelity rewrite/recovery) ·
`azodoc-convert` (loss reporting, TXT export) · `azodoc-md` · `azodoc-html` ·
`azodoc-docx` (Pandoc bridge) · `azodoc-pdf` (headless printing) ·
`athanor-cli`.

```bash
cd engine && cargo test    # 77 tests; DOCX/PDF e2e auto-skip if tools are absent
```

## Documentation

- **[设计草案 (Chinese)](design_docs/SDOC%20Document%20Model%20v0.1%20—%20草案.md)** — the original concept draft
- **[落地方案 (Chinese)](design_docs/Azodoc%20v0.1%20—%20落地方案.md)** — the implementation plan this repo was built from, with milestone acceptance records
- **[Future work / 可选项](design_docs/Future%20Work%20—%20可选项与后续计划.md)** — archived roadmap options (editor prototype, Paged.js publishing, native OOXML, AI pipeline, licensing unification, …)
- **[spec/](spec/)** — normative container/package/model/loss specifications

## License

The repository root currently carries **AGPL-3.0** (from project inception),
while engine crates are declared `MIT OR Apache-2.0`. Unifying this is an open
decision tracked in the [future-work document](design_docs/Future%20Work%20—%20可选项与后续计划.md).
Note: the Pandoc DOCX bridge is license-clean by construction — Pandoc is
invoked as a separate, user-installed process over standard streams; its GPL
code is never linked or distributed with the engine.

---

## 中文简介

Athanor Studio 是 **Azodoc**（`.azodoc`）结构化文档容器与 **Athanor** Rust 引擎的所在地。

- **单文件结构化容器**：32 字节魔数头 + 标准 ZIP，任何 ZIP 工具可直接打开；
  正文可通过 `compatibility/text/document.txt` 永远找回。
- **内容是唯一真源**：Markdown / HTML / DOCX / TXT / PDF 都是可再生的派生表示。
- **不静默丢失**：每次转换产出五级损失报告（none/partial/degraded/unsupported/preserved_raw），
  无法建模的原始内容进 `preserved/`。
- **修订与 AI 作者一等公民**：导入、人工编辑、AI 批量改写都落快照链；
  语义标注与内容分离，编辑后自动重定位（重锚/迁移/detached，绝不删除）。
- **出版即冻结历史**：每次 `athanor publish` 产出独立 PDF 与完整出版档案。

快速上手：

```bash
cd engine && cargo build --release
athanor import input.md -o doc.azodoc     # 导入
athanor transmute doc.azodoc --to html    # 导出
athanor publish doc.azodoc --out out.pdf  # 出版
athanor verify doc.azodoc                 # 校验
```

详细文档见上方 Documentation 一节；未来计划与可选项存档于
[design_docs/Future Work — 可选项与后续计划](design_docs/Future%20Work%20—%20可选项与后续计划.md)。

许可证：仓库根目前为 **AGPL-3.0**，引擎 crate 声明为 MIT OR Apache-2.0，
统一方案待定（见未来计划文档）。Pandoc 桥接以独立进程调用、不经标准流以外
方式交互，核心许可不受 GPL 传染。
