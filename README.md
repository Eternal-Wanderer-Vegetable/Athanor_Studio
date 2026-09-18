# Athanor Studio

**One file in, every format out — and back.**

> 中文文档：[README.zh-CN.md](README.zh-CN.md)

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

Roadmap **M0–M6 complete** (spec → container core → converters → revisions &
semantics → DOCX → PDF publishing → **the Aludel editor prototype**), plus three
follow-ups from the future-work archive: **A1** — the Aludel editor (ProseMirror
front end + save-with-commit pipeline: editing sessions produce
`author: human` revisions and annotations relocate automatically), **A2** — an
AI pipeline example (LLM → annotations & revisions with auditable `ai:*`
authorship) and **B1** — Paged.js/CDP publishing (page-number footers, running
headers, race-free pagination). **169 tests passing.** CI runs rustfmt +
clippy + the full test matrix on Ubuntu & Windows via GitHub Actions.
Remaining directions are archived in the
[future-work document](design_docs/Future%20Work%20—%20可选项与后续计划.md).

Try the editor prototype: from the `engine` directory run
`cargo run -p aludel -- doc.azodoc` —
it opens in a local browser where you can edit, save (revisions commit
automatically) and verify.

| Format | Import | Export |
|---|---|---|
| Markdown (GFM: tables, task lists, footnotes, math, alerts) | ✅ | ✅ |
| HTML (self-contained single-file output, sanitized re-import) | ✅ | ✅ |
| DOCX (via Pandoc subprocess bridge — GPL isolated at the process boundary) | ✅ | ✅ |
| Plain text (last-resort recovery channel, always cached in-container) | ✅ | ✅ |
| PDF publication (headless Chromium/Edge + Paged.js pagination: page numbers & running headers; publication records) | — | ✅ |

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

# Publish (headless Edge/Chrome + Paged.js pagination → PDF + frozen record)
athanor publish doc.azodoc --out out.pdf
athanor publish doc.azodoc --no-paged            # plain Chromium print, no margin boxes

# AI pipeline demo (offline FakeProvider by default; LLM writes ai:*-authored
# annotations with confidence + a rewrite revision you can review & roll back)
cargo run -p athanor-cli --example ai_pipeline -- doc.azodoc

# Editor prototype (opens in a local browser: edit, save auto-commits as
# author:human, sidebar with revision history / annotations / loss inventory)
cargo run -p aludel -- doc.azodoc

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
| [`engine/`](engine/) | Athanor engine — Rust workspace (10 crates, 169 tests) |
| [`spec/`](spec/) | Azodoc v1.0 specifications (container, package, model, loss) + JSON Schemas + golden fixtures |
| [`corpus/`](corpus/) | Test corpus, golden TXT exports, lossy-feature fixtures |
| [`design_docs/`](design_docs/) | Design drafts, the implementation plan, and the archived future-work roadmap |
| [`tools/`](tools/) | Local runtime helpers (portable Pandoc) — gitignored; Paged.js is vendored at `engine/crates/azodoc-pdf/assets/` |

Engine crates: `azodoc-model` (Prima types + validation) ·
`azodoc-container` (header/ZIP/detection/fidelity rewrite/recovery) ·
`azodoc-convert` (loss reporting, TXT export) · `azodoc-md` · `azodoc-html` ·
`azodoc-docx` (Pandoc bridge) · `azodoc-pdf` (CDP + Paged.js publishing) ·
`azodoc-pm` (Prima ↔ ProseMirror conversion & schema generation) ·
`athanor-cli` · `aludel` (editor prototype: local server + web front end).

```bash
cd engine && cargo test    # 169 tests; DOCX/PDF e2e auto-skip if tools are absent
```

## Documentation

- **[Design draft (Chinese)](design_docs/SDOC%20Document%20Model%20v0.1%20—%20草案.md)** — the original concept draft
- **[Implementation plan (Chinese)](design_docs/Azodoc%20v0.1%20—%20落地方案.md)** — the plan this repo was built from, with milestone acceptance records
- **[M6 editor prototype plan (Chinese)](design_docs/M6%20—%20Aludel%20编辑器原型计划.md)** — A1's RFC, the Prima ↔ ProseMirror mapping table, and staged implementation records
- **[Editor guide (Chinese)](docs/editor.md)** — Aludel feature inventory, feature flags, migration & compatibility notes, DOCX capability matrix, failure recovery, and debug-bundle collection
- **[Future work / 可选项](design_docs/Future%20Work%20—%20可选项与后续计划.md)** — archived roadmap options (native OOXML, …); A1 (editor), A2 (AI pipeline) and B1 (Paged.js publishing) have shipped
- **[spec/](spec/)** — normative container/package/model/loss specifications

## License

Copyright © 2026 The Athanor Studio Developers.

This project is licensed under the **GNU Affero General Public License
v3.0 only** (**AGPL-3.0-only**) — see the [LICENSE](LICENSE) file. All source
files carry the corresponding copyright notice.

This means, in brief: you may use, study, modify, and redistribute Azodoc and
Athanor freely; if you distribute a modified version (or run a modified version
as a network service), you must release its complete corresponding source under
the same AGPL-3.0 license.

The DOCX bridge is license-clean by construction: Pandoc is invoked as a
separate, user-installed process over standard streams. Its GPL code is never
linked into — or distributed with — the engine, which is why Athanor can remain
AGPL-3.0 while benefiting from Pandoc.
