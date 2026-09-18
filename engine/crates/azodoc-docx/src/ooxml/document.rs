// This file is part of Athanor, the Azodoc document engine.
// Copyright (C) 2026 The Athanor Studio Developers
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published
// by the Free Software Foundation, version 3 of the License only.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! document.xml 正文 → Prima 映射（RFC B2 §4.2）。
//!
//! 损失报告按 feature 聚合（决策 4）：`Agg` 收集逐处损失，收口时一次性写入
//! LossLog；每个被评估的映射成功节点仍逐个 `node_none` 计数（R6）。

use super::comments::{CommentInfo, Comments};
use super::numbering::{num_pr_of, Numbering};
use super::package::{OoxmlPackage, Relationship};
use super::styles::{parse_run_flags, PageProps, Styles};
use super::xml::{el_to_xml, El};
use azodoc_convert::{plain_text_of_span, AssetSource, ImportJob, LossClass, LossLog};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

// ---------------------------------------------------------------- 聚合损失

pub const RUN_PROPS: &str = "docx_run_props";
pub const PARA_PROPS: &str = "docx_paragraph_props";
pub const TAB: &str = "docx_tab";
pub const PAGE_BREAK: &str = "docx_page_break";
pub const BOOKMARK: &str = "docx_bookmark";
pub const FIELD: &str = "docx_field";
pub const SDT: &str = "docx_sdt";
pub const ALT_CONTENT: &str = "docx_alternate_content";
pub const VML: &str = "docx_vml";
pub const FLOATING: &str = "docx_floating_image";
pub const INTERNAL_LINK: &str = "docx_internal_link";
pub const OMML: &str = "docx_omml";
pub const TRACKED: &str = "docx_tracked_changes";
pub const CROSS_BLOCK: &str = "docx_comment_cross_block";
pub const COMMENT_ANCHOR: &str = "docx_comment_anchor";
pub const ENDNOTES: &str = "docx_endnotes";
pub const NUM_EDGE: &str = "docx_numbering_edge";
pub const MEDIA_MISSING: &str = "docx_media_missing";
pub const FOOTNOTE_MISSING: &str = "docx_footnote_missing";
pub const TABLE_EDGE: &str = "docx_table_edge";
pub const SKIPPED: &str = "docx_unmodelled_element";

#[derive(Debug)]
struct AggEntry {
    class: LossClass,
    action: &'static str,
    count: u64,
    samples: Vec<String>,
}

/// 按 feature 聚合的损失收集器（BTreeMap：确定性输出序）。
#[derive(Debug, Default)]
pub struct Agg {
    entries: BTreeMap<String, AggEntry>,
}

impl Agg {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bump(&mut self, feature: &str, class: LossClass, action: &'static str, sample: String) {
        let e = self.entries.entry(feature.to_string()).or_insert(AggEntry {
            class,
            action,
            count: 0,
            samples: Vec::new(),
        });
        e.count += 1;
        if e.samples.len() < 3 && !sample.is_empty() && !e.samples.contains(&sample) {
            e.samples.push(sample);
        }
    }

    /// 收口写入 LossLog（每 feature 一条事件）。
    pub fn flush(self, log: &mut LossLog) {
        for (feature, e) in self.entries {
            let detail = if e.samples.is_empty() {
                format!("count={}", e.count)
            } else {
                format!("count={}; sample={}", e.count, e.samples.join(" | "))
            };
            log.record(
                e.class,
                &feature,
                None,
                e.action,
                &detail,
                agg_message(&feature, e.count),
            );
        }
    }
}

fn agg_message(feature: &str, count: u64) -> String {
    let what = match feature {
        RUN_PROPS => "run 直接格式属性在 v1 模型无对应",
        PARA_PROPS => "段落直接格式属性在 v1 模型无对应",
        TAB => "制表位退化为空格",
        PAGE_BREAK => "分页符以水平分隔线近似表达",
        BOOKMARK => "书签锚点无对应表达",
        FIELD => "域指令无法建模（结果文本已保留）",
        SDT => "内容控件被透明解包",
        ALT_CONTENT => "AlternateContent 取兼容表达（Fallback 优先）",
        VML => "VML/图形以原始形式保留",
        FLOATING => "浮动图片的定位信息丢失",
        INTERNAL_LINK => "文档内锚点链接在 v1 模型中无目标表达",
        OMML => "OMML 公式以原始形式保留（未转 LaTeX）",
        TRACKED => "修订（tracked changes）按终稿视图导入",
        CROSS_BLOCK => "批注锚定区间跨块，降级为块级标注",
        COMMENT_ANCHOR => "批注无法锚定到正文",
        ENDNOTES => "尾注按脚注表达（编号语义有差异）",
        NUM_EDGE => "编号列表的边角语义未建模",
        MEDIA_MISSING => "关系指向的媒体缺失",
        FOOTNOTE_MISSING => "脚注引用找不到对应定义",
        TABLE_EDGE => "表格合并的边角语义未建模",
        SKIPPED => "未建模的元素被跳过",
        _ => "已按损失规范处理",
    };
    format!("{what}（{count} 处）")
}

// ---------------------------------------------------------------- 修订统计

#[derive(Debug, Default)]
pub struct TrackedStats {
    /// author → (插入区域数, 删除区域数)
    pub authors: BTreeMap<String, (u64, u64)>,
    /// 文档是否启用修订标记（settings.xml trackChanges）
    pub enabled: bool,
}

impl TrackedStats {
    pub fn total(&self) -> u64 {
        self.authors.values().map(|(i, d)| i + d).sum()
    }
}

// ---------------------------------------------------------------- 正文上下文

pub struct BodyCtx<'a> {
    pub job: &'a mut ImportJob,
    pub log: &'a mut LossLog,
    pub pkg: &'a mut OoxmlPackage,
    pub agg: Agg,
    pub rels: Vec<Relationship>,
    pub styles: Styles,
    pub numbering: Numbering,
    pub comments: Comments,
    /// footnote/endnote id → 定义元素
    pub footnotes: BTreeMap<String, El>,
    pub endnotes: BTreeMap<String, El>,
    /// 已展开的 footnote 定义（id → 块 id），重复引用复用
    pub footnote_defs: BTreeMap<String, String>,
    pub pending_footnotes: Vec<Value>,
    pub pending_blocks: Vec<Value>,
    pub annotations: Vec<Value>,
    /// 正文实际引用的样式（首用序，theme 生成基准）
    pub styles_used: Vec<String>,
    pub tracked: TrackedStats,
    pub page: Option<PageProps>,
    /// RFC3339 当前时刻（批注 created_at 兜底）
    pub now: String,
}

impl BodyCtx<'_> {
    pub fn mark_style_used(&mut self, style_id: &str) {
        if !style_id.is_empty() && !self.styles_used.iter().any(|s| s == style_id) {
            self.styles_used.push(style_id.to_string());
        }
    }
}

// ---------------------------------------------------------------- 块序列

/// 段落产出：普通块 / 列表项（含编号信息）。
struct ParaOut {
    node: Option<Value>,
    extra: Vec<Value>,
    num: Option<(String, i64)>,
    num_ordered: Option<(bool, i64)>,
}

struct ListFrame {
    num_id: String,
    ilvl: i64,
    list: Value,
}

/// 多级列表装配：连续同 numId 同层级 = 同列表；层级加深 = 嵌套进上一层末项。
#[derive(Default)]
struct ListAssembler {
    stack: Vec<ListFrame>,
}

impl ListAssembler {
    fn make_list(ordered: bool, start: i64, ctx: &mut BodyCtx) -> Value {
        let mut b = json!({
            "id": ctx.job.idgen.uid("blk"),
            "type": "list",
            "style": if ordered { "ordered" } else { "bullet" },
            "items": [],
        });
        if ordered {
            b.as_object_mut()
                .expect("刚构造的 object")
                .insert("start".into(), json!(start));
        }
        b
    }

    #[allow(clippy::too_many_arguments)]
    fn add(
        &mut self,
        num_id: String,
        ilvl: i64,
        ordered: bool,
        start: i64,
        item: Value,
        ctx: &mut BodyCtx,
        out: &mut Vec<Value>,
    ) {
        // 关闭更深或更换编号的帧（同编号、层级 <= 当前则停下）
        while let Some(top) = self.stack.last() {
            if top.num_id == num_id && top.ilvl <= ilvl {
                break;
            }
            let frame = self.stack.pop().expect("刚检查非空");
            Self::close_frame(frame, &mut self.stack, out);
        }
        match self.stack.last_mut() {
            Some(top) if top.num_id == num_id && top.ilvl == ilvl => {
                top.list["items"]
                    .as_array_mut()
                    .expect("items 是数组")
                    .push(item);
            }
            _ => {
                let mut list = Self::make_list(ordered, start, ctx);
                list["items"]
                    .as_array_mut()
                    .expect("items 是数组")
                    .push(item);
                self.stack.push(ListFrame { num_id, ilvl, list });
            }
        }
    }

    fn close_frame(frame: ListFrame, stack: &mut [ListFrame], out: &mut Vec<Value>) {
        match stack.last_mut() {
            Some(parent) => {
                // 嵌套列表挂到父帧最后一个 item 的 children
                let items = parent.list["items"].as_array_mut().expect("items 是数组");
                if let Some(last_item) = items.last_mut() {
                    last_item["children"]
                        .as_array_mut()
                        .expect("list_item children 是数组")
                        .push(frame.list);
                } else {
                    out.push(frame.list); // 防御性：父帧空则提升到当前层
                }
            }
            None => out.push(frame.list),
        }
    }

    fn close_all(&mut self, out: &mut Vec<Value>) {
        while let Some(frame) = self.stack.pop() {
            Self::close_frame(frame, &mut self.stack, out);
        }
    }
}

/// 块序列映射：body / 表格单元格 / 脚注内容共用。
pub fn map_block_seq<'a>(els: impl IntoIterator<Item = &'a El>, ctx: &mut BodyCtx) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    let mut lists = ListAssembler::default();
    for el in els {
        match (el.prefix.as_str(), el.local.as_str()) {
            ("w", "p") => {
                let para = map_paragraph(el, ctx);
                match para.num {
                    Some((num_id, ilvl)) => {
                        let (ordered, start) = para.num_ordered.unwrap_or((false, 1));
                        let mut children = Vec::new();
                        if let Some(node) = para.node {
                            children.push(node);
                        }
                        children.extend(para.extra);
                        let item = json!({
                            "id": ctx.job.idgen.uid("li"),
                            "type": "list_item",
                            "children": children,
                        });
                        lists.add(num_id, ilvl, ordered, start, item, ctx, &mut out);
                    }
                    None => {
                        lists.close_all(&mut out);
                        if let Some(node) = para.node {
                            out.push(node);
                        }
                        out.extend(para.extra);
                    }
                }
            }
            ("w", "tbl") => {
                lists.close_all(&mut out);
                let tbl = map_table(el, ctx);
                out.push(tbl);
            }
            ("w", "sectPr") => capture_sect_pr(el, ctx),
            ("w", "bookmarkStart") => {
                let name = el.attr("w:name").unwrap_or("").to_string();
                ctx.agg.bump(BOOKMARK, LossClass::Partial, "dropped", name);
            }
            ("w", "bookmarkEnd") => {}
            _ => {
                ctx.agg.bump(
                    SKIPPED,
                    LossClass::Partial,
                    "dropped",
                    format!("{}:{}", el.prefix, el.local),
                );
            }
        }
    }
    lists.close_all(&mut out);
    out
}

// ---------------------------------------------------------------- 段落

fn capture_sect_pr(pr: &El, ctx: &mut BodyCtx) {
    let Some(sz) = pr.child("w", "pgSz") else {
        return;
    };
    let (w, h) = match (sz.attr("w:w"), sz.attr("w:h")) {
        (Some(w), Some(h)) => match (w.parse::<i64>(), h.parse::<i64>()) {
            (Ok(w), Ok(h)) => (w, h),
            _ => return,
        },
        _ => return,
    };
    let mut page = PageProps {
        width: w,
        height: h,
        margin_top: 1440,
        margin_right: 1440,
        margin_bottom: 1440,
        margin_left: 1440,
    };
    if let Some(mar) = pr.child("w", "pgMar") {
        let g = |k: &str| mar.attr(k).and_then(|v| v.parse::<i64>().ok());
        if let Some(v) = g("w:top") {
            page.margin_top = v;
        }
        if let Some(v) = g("w:right") {
            page.margin_right = v;
        }
        if let Some(v) = g("w:bottom") {
            page.margin_bottom = v;
        }
        if let Some(v) = g("w:left") {
            page.margin_left = v;
        }
    }
    ctx.page = Some(page);
}

/// 行内映射状态：段落纯文本（批注锚定基准 §9）与批注标记。
#[derive(Default)]
struct InlineState {
    plain: String,
    /// (comment id, is_start, plain 字节偏移)，按出现序
    marks: Vec<(String, bool, usize)>,
    refs: Vec<String>,
}

fn push_span(out: &mut Vec<Value>, span: Value, st: &mut InlineState) {
    st.plain.push_str(&plain_text_of_span(&span));
    out.push(span);
}

fn map_paragraph(p: &El, ctx: &mut BodyCtx) -> ParaOut {
    let ppr = p.child("w", "pPr");

    // 编号信息
    let num = ppr.and_then(num_pr_of);
    if let Some((ref num_id, ilvl)) = num {
        if ctx.numbering.level(num_id, ilvl).is_none() {
            ctx.agg.bump(
                NUM_EDGE,
                LossClass::Partial,
                "degraded",
                format!("numId={num_id} ilvl={ilvl}"),
            );
        }
    }

    // 样式
    let style_id = ppr
        .and_then(|pr| pr.child("w", "pStyle"))
        .and_then(|s| s.attr("w:val"))
        .unwrap_or("")
        .to_string();
    if !style_id.is_empty() {
        ctx.mark_style_used(&style_id);
    }
    let heading = if style_id.is_empty() {
        None
    } else {
        ctx.styles.heading_level(&style_id)
    };

    // 段落直接格式（pStyle/numPr/sectPr/rPr 之外 → 聚合报告）
    if let Some(pr) = ppr {
        let mut sample = String::new();
        for c in &pr.children {
            if c.prefix == "w"
                && !matches!(c.local.as_str(), "pStyle" | "numPr" | "sectPr" | "rPr")
                && sample.len() < 60
            {
                if !sample.is_empty() {
                    sample.push(',');
                }
                sample.push_str(&c.local);
            }
        }
        if !sample.is_empty() {
            ctx.agg
                .bump(PARA_PROPS, LossClass::Partial, "dropped", sample);
        }
        if let Some(sect) = pr.child("w", "sectPr") {
            capture_sect_pr(sect, ctx);
        }
    }

    // 行内内容
    let mut st = InlineState::default();
    let children: Vec<&El> = p.children.iter().filter(|c| !c.is("w", "pPr")).collect();
    let mut spans = map_inlines(&children, ctx, &mut st);
    ctx.log.node_none(); // 段落节点已评估

    // 空段（无内容、无待挂块、无批注锚）：丢弃但计数
    let has_anchor = !st.marks.is_empty() || !st.refs.is_empty();
    if spans.is_empty() && ctx.pending_blocks.is_empty() && !has_anchor {
        return ParaOut {
            node: None,
            extra: Vec::new(),
            num: None,
            num_ordered: None,
        };
    }

    // 独段图片 → figure
    let mut node = if heading.is_none()
        && spans.len() == 1
        && spans[0].get("type").and_then(Value::as_str) == Some("inline_image")
    {
        let img = spans.remove(0);
        json!({
            "id": ctx.job.idgen.uid("blk"),
            "type": "figure",
            "asset": img["asset"].clone(),
            "alt": img["alt"].clone(),
            "caption": [],
        })
    } else {
        let mut b = json!({
            "id": ctx.job.idgen.uid("blk"),
            "type": if heading.is_some() { "heading" } else { "paragraph" },
            "content": spans,
        });
        if let Some(level) = heading {
            b.as_object_mut()
                .expect("刚构造的 object")
                .insert("level".into(), json!(level));
        }
        b
    };

    // 样式软引用（标题已带 level 不挂；默认段落样式跳过）
    if heading.is_none() && !style_id.is_empty() {
        let is_default = ctx.styles.default_paragraph.as_deref() == Some(style_id.as_str());
        if !is_default {
            if let Some(info) = ctx.styles.get(&style_id) {
                let slug = info.slug.clone();
                node.as_object_mut()
                    .expect("刚构造的 object")
                    .insert("style".into(), json!([slug]));
            }
        }
    }

    // 批注 → 标注（块 ID 锚定）
    if has_anchor {
        let block_id = node["id"].as_str().unwrap_or("").to_string();
        finalize_comment_annotations(&st, &block_id, ctx);
    }

    let extra = std::mem::take(&mut ctx.pending_blocks);
    let num_ordered = num.as_ref().map(|(num_id, ilvl)| {
        (
            ctx.numbering.is_ordered(num_id, *ilvl),
            ctx.numbering.start(num_id, *ilvl),
        )
    });
    ParaOut {
        node: Some(node),
        extra,
        num,
        num_ordered,
    }
}

/// 批注锚定三级降级：text_quote → block →（无定义时）报告并跳过。
fn finalize_comment_annotations(st: &InlineState, block_id: &str, ctx: &mut BodyCtx) {
    let mut consumed: BTreeSet<usize> = BTreeSet::new();
    let mut annotated: BTreeSet<String> = BTreeSet::new();

    // 1. start/end 配对（end 匹配其之前最近的未消费同 id start）
    for (ei, (eid, is_start, eoff)) in st.marks.iter().enumerate() {
        if *is_start {
            continue;
        }
        let mut start_idx = None;
        for (si, (sid, s, _)) in st.marks[..ei].iter().enumerate().rev() {
            if *s && sid == eid && !consumed.contains(&si) {
                start_idx = Some(si);
                break;
            }
        }
        let Some(si) = start_idx else { continue };
        consumed.insert(si);
        consumed.insert(ei);
        annotated.insert(eid.clone());
        let start_off = st.marks[si].2;
        let exact = st
            .plain
            .get(start_off..*eoff)
            .unwrap_or("")
            .trim()
            .to_string();
        let Some(info) = ctx.comments.get(eid).cloned() else {
            ctx.agg.bump(
                COMMENT_ANCHOR,
                LossClass::Partial,
                "degraded",
                format!("comment id={eid} 无 comments.xml 定义"),
            );
            continue;
        };
        let ann = if exact.is_empty() {
            block_annotation(ctx, &info, block_id)
        } else {
            let prefix: String = st.plain[..start_off]
                .chars()
                .rev()
                .take(32)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            let suffix: String = st.plain[*eoff..].chars().take(32).collect();
            json!({
                "id": ctx.job.idgen.uid("ann"),
                "type": "docx_comment",
                "target": {
                    "kind": "text_quote",
                    "block": block_id,
                    "selector": {
                        "type": "text_quote",
                        "exact": exact,
                        "prefix": prefix,
                        "suffix": suffix,
                    },
                },
                "value": { "text": info.text },
                "author": { "type": "human", "id": info.author },
                "created_at": info.date.clone().unwrap_or_else(|| ctx.now.clone()),
                "source": "docx",
            })
        };
        ctx.annotations.push(ann);
    }

    // 2. 未配对的 start（跨块区间）→ 块级标注 + 报告
    for (i, (id, is_start, _)) in st.marks.iter().enumerate() {
        if *is_start && !consumed.contains(&i) && annotated.insert(id.clone()) {
            ctx.agg.bump(
                CROSS_BLOCK,
                LossClass::Partial,
                "degraded",
                format!("comment id={id}"),
            );
            if let Some(info) = ctx.comments.get(id).cloned() {
                let ann = block_annotation(ctx, &info, block_id);
                ctx.annotations.push(ann);
            }
        }
    }

    // 3. 仅有 commentReference（无区间标记）→ 块级标注
    for id in &st.refs {
        if annotated.insert(id.clone()) {
            if let Some(info) = ctx.comments.get(id).cloned() {
                let ann = block_annotation(ctx, &info, block_id);
                ctx.annotations.push(ann);
            }
        }
    }

    // 4. 有锚但无定义
    for id in &annotated {
        if ctx.comments.get(id).is_none() {
            ctx.agg.bump(
                COMMENT_ANCHOR,
                LossClass::Partial,
                "degraded",
                format!("comment id={id} 无 comments.xml 定义"),
            );
        }
    }
}

fn block_annotation(ctx: &mut BodyCtx, info: &CommentInfo, block_id: &str) -> Value {
    json!({
        "id": ctx.job.idgen.uid("ann"),
        "type": "docx_comment",
        "target": { "kind": "block", "block": block_id },
        "value": { "text": info.text },
        "author": { "type": "human", "id": info.author },
        "created_at": info.date.clone().unwrap_or_else(|| ctx.now.clone()),
        "source": "docx",
    })
}

// ---------------------------------------------------------------- 行内

fn map_inlines(els: &[&El], ctx: &mut BodyCtx, st: &mut InlineState) -> Vec<Value> {
    let mut out = Vec::new();
    for el in els {
        map_inline(el, ctx, st, &mut out);
    }
    out
}

fn map_inline(el: &El, ctx: &mut BodyCtx, st: &mut InlineState, out: &mut Vec<Value>) {
    match (el.prefix.as_str(), el.local.as_str()) {
        ("w", "r") => map_run(el, ctx, st, out),
        ("w", "hyperlink") => map_hyperlink(el, ctx, st, out),
        ("w", "commentRangeStart") => {
            if let Some(id) = el.attr("w:id") {
                st.marks.push((id.to_string(), true, st.plain.len()));
            }
        }
        ("w", "commentRangeEnd") => {
            if let Some(id) = el.attr("w:id") {
                st.marks.push((id.to_string(), false, st.plain.len()));
            }
        }
        ("w", "commentReference") => {
            if let Some(id) = el.attr("w:id") {
                st.refs.push(id.to_string());
            }
        }
        ("w", "ins") | ("w", "moveTo") => {
            let author = el.attr("w:author").unwrap_or("").to_string();
            ctx.tracked.authors.entry(author).or_default().0 += 1;
            for c in &el.children {
                map_inline(c, ctx, st, out);
            }
        }
        ("w", "del") | ("w", "moveFrom") => {
            let author = el.attr("w:author").unwrap_or("").to_string();
            ctx.tracked.authors.entry(author).or_default().1 += 1;
            // 终稿视图：删除内容剔除（w:delText 等不产出）
        }
        ("m", "oMath") | ("m", "oMathPara") => map_omml(el, ctx, st, out),
        ("w", "sdt") => {
            ctx.agg
                .bump(SDT, LossClass::Partial, "degraded", String::new());
            if let Some(content) = el.child("w", "sdtContent") {
                for c in &content.children {
                    map_inline(c, ctx, st, out);
                }
            }
        }
        ("mc", "AlternateContent") => {
            ctx.agg
                .bump(ALT_CONTENT, LossClass::Partial, "degraded", String::new());
            // 决策 8：Fallback 优先（确定性），无则 Choice
            let chosen = el
                .child("mc", "Fallback")
                .or_else(|| el.child("mc", "Choice"));
            if let Some(branch) = chosen {
                for c in &branch.children {
                    map_inline(c, ctx, st, out);
                }
            }
        }
        ("w", "smartTag") => {
            for c in &el.children {
                map_inline(c, ctx, st, out);
            }
        }
        ("w", "fldSimple") => {
            // 简单域：结果文本照常映射，域指令聚合报告
            let instr = el.attr("w:instr").unwrap_or("").trim().to_string();
            if !instr.is_empty() {
                ctx.agg.bump(FIELD, LossClass::Partial, "degraded", instr);
            }
            for c in &el.children {
                map_inline(c, ctx, st, out);
            }
        }
        ("w", "bookmarkStart") => {
            let name = el.attr("w:name").unwrap_or("").to_string();
            ctx.agg.bump(BOOKMARK, LossClass::Partial, "dropped", name);
        }
        _ => {
            // 已知渲染噪声静默跳过；其余透明解包子内容
            if el.prefix == "w"
                && matches!(
                    el.local.as_str(),
                    "proofErr" | "lastRenderedPageBreak" | "bookmarkEnd" | "fldChar"
                )
            {
                return;
            }
            for c in &el.children {
                map_inline(c, ctx, st, out);
            }
        }
    }
}

fn map_hyperlink(el: &El, ctx: &mut BodyCtx, st: &mut InlineState, out: &mut Vec<Value>) {
    let mut url: Option<String> = None;
    if let Some(rid) = el.attr("r:id") {
        if let Some(rel) = ctx.rels.iter().find(|r| r.id == rid) {
            url = Some(rel.resolve("word/document.xml"));
        }
    }
    if url.is_none() {
        if let Some(anchor) = el.attr("w:anchor") {
            ctx.agg.bump(
                INTERNAL_LINK,
                LossClass::Partial,
                "degraded",
                format!("#{anchor}"),
            );
            url = Some(format!("#{anchor}"));
        }
    }
    let kids: Vec<&El> = el.children.iter().collect();
    let mut content = map_inlines(&kids, ctx, st);
    match url {
        Some(u) => {
            ctx.log.node_none();
            out.push(json!({"type": "link", "url": u, "content": content}));
        }
        None => out.append(&mut content), // 无 URL：透明提升子内容
    }
}

fn map_run(r: &El, ctx: &mut BodyCtx, st: &mut InlineState, out: &mut Vec<Value>) {
    let rpr = r.child("w", "rPr");
    let mut flags = rpr.map(parse_run_flags).unwrap_or_default();

    // 字符样式 flags 合并 + 使用登记
    if let Some(rsid) = rpr
        .and_then(|pr| pr.child("w", "rStyle"))
        .and_then(|s| s.attr("w:val"))
    {
        ctx.mark_style_used(rsid);
        let (eff, _, _) = ctx.styles.resolve_effective(rsid);
        flags.bold |= eff.bold;
        flags.italic |= eff.italic;
        flags.underline |= eff.underline;
        flags.strike |= eff.strike;
    }

    // 其余 rPr 属性 → 聚合报告（颜色/高亮/上下标等）
    if let Some(pr) = rpr {
        let mut sample = String::new();
        for c in &pr.children {
            if c.prefix == "w"
                && !matches!(
                    c.local.as_str(),
                    "b" | "bCs"
                        | "i"
                        | "iCs"
                        | "u"
                        | "strike"
                        | "dstrike"
                        | "rStyle"
                        | "rFonts"
                        | "sz"
                        | "szCs"
                        | "lang"
                        | "vanish"
                        | "webHidden"
                        | "noProof"
                )
                && sample.len() < 60
            {
                if !sample.is_empty() {
                    sample.push(',');
                }
                sample.push_str(&c.local);
            }
        }
        if !sample.is_empty() {
            ctx.agg
                .bump(RUN_PROPS, LossClass::Partial, "dropped", sample);
        }
    }

    let wrapped: Option<&str> = if flags.strike {
        Some("strike")
    } else if flags.underline {
        Some("underline")
    } else if flags.italic {
        Some("em")
    } else if flags.bold {
        Some("strong")
    } else {
        None
    };

    let mut inner = Vec::new();
    for c in &r.children {
        if c.is("w", "rPr") {
            continue;
        }
        map_run_child(c, ctx, st, &mut inner);
    }
    if inner.is_empty() {
        return;
    }
    ctx.log.node_none();
    match wrapped {
        Some(kind) => out.push(json!({"type": kind, "content": inner})),
        None => out.append(&mut inner),
    }
}

fn map_run_child(c: &El, ctx: &mut BodyCtx, st: &mut InlineState, out: &mut Vec<Value>) {
    match (c.prefix.as_str(), c.local.as_str()) {
        ("w", "t") => {
            let text = c.text.clone();
            if !text.is_empty() {
                push_span(out, json!({"type": "text", "text": text}), st);
            }
        }
        ("w", "tab") => {
            ctx.agg
                .bump(TAB, LossClass::Partial, "degraded", String::new());
            push_span(out, json!({"type": "text", "text": " "}), st);
        }
        ("w", "br") => match c.attr("w:type") {
            Some("page") => {
                ctx.agg
                    .bump(PAGE_BREAK, LossClass::Partial, "degraded", String::new());
                ctx.pending_blocks.push(json!({
                    "id": ctx.job.idgen.uid("blk"),
                    "type": "horizontal_rule",
                }));
            }
            Some("column") => {
                ctx.agg
                    .bump(PAGE_BREAK, LossClass::Partial, "degraded", "column".into());
                push_span(out, json!({"type": "hard_break"}), st);
            }
            _ => push_span(out, json!({"type": "hard_break"}), st),
        },
        ("w", "cr") => push_span(out, json!({"type": "hard_break"}), st),
        ("w", "drawing") => {
            if let Some(span) = map_drawing(c, ctx) {
                push_span(out, span, st);
            }
        }
        ("w", "pict") => map_pict(c, ctx, st, out),
        ("w", "object") => {
            // OLE 内嵌对象：隔离保留（C4 语义在 DOCX 侧的落地）
            let xml = el_to_xml(c);
            let payload = ctx.job.add_preserved("docx", "xml", xml.into_bytes());
            ctx.log.record(
                LossClass::PreservedRaw,
                "docx_ole_object",
                None,
                "preserved_quarantined",
                &payload,
                "OLE 对象隔离保留，不参与渲染（security）".to_string(),
            );
            push_span(
                out,
                json!({
                    "type": "unknown",
                    "origin": "docx",
                    "loss_class": "preserved_raw",
                    "summary": "OLE 嵌入对象",
                    "payload_ref": payload,
                    "content": [],
                }),
                st,
            );
        }
        ("w", "footnoteReference") => {
            if let Some(id) = c.attr("w:id") {
                map_footnote_ref(id, false, ctx, st, out);
            }
        }
        ("w", "endnoteReference") => {
            if let Some(id) = c.attr("w:id") {
                ctx.agg
                    .bump(ENDNOTES, LossClass::Partial, "degraded", String::new());
                map_footnote_ref(id, true, ctx, st, out);
            }
        }
        ("w", "instrText") => {
            let instr = c.text.trim().to_string();
            if !instr.is_empty() {
                ctx.agg.bump(FIELD, LossClass::Partial, "degraded", instr);
            }
        }
        ("w", "commentReference") => {
            if let Some(id) = c.attr("w:id") {
                st.refs.push(id.to_string());
            }
        }
        ("w", "delText") | ("w", "delInstrText") => {
            // 终稿视图：删除文本剔除
        }
        ("m", "oMath") => map_omml(c, ctx, st, out),
        _ => {
            // 其余 run 子元素：透明解包子内容
            for k in &c.children {
                map_inline(k, ctx, st, out);
            }
        }
    }
}

/// OMML 公式：整段 XML preserved + unknown 节点（content 带提取文本）。
fn map_omml(el: &El, ctx: &mut BodyCtx, st: &mut InlineState, out: &mut Vec<Value>) {
    let xml = el_to_xml(el);
    let payload = ctx.job.add_preserved("docx", "xml", xml.into_bytes());
    let text = collect_m_text(el).trim().to_string();
    ctx.agg
        .bump(OMML, LossClass::Degraded, "degraded", payload.clone());
    let content = if text.is_empty() {
        Vec::new()
    } else {
        vec![json!({"type": "text", "text": text})]
    };
    if el.local == "oMathPara" {
        ctx.pending_blocks.push(json!({
            "id": ctx.job.idgen.uid("blk"),
            "type": "unknown",
            "origin": "docx",
            "loss_class": "preserved_raw",
            "summary": "OMML 展示公式（未转 LaTeX）",
            "payload_ref": payload,
            "content": [],
        }));
    } else {
        push_span(
            out,
            json!({
                "type": "unknown",
                "origin": "docx",
                "loss_class": "preserved_raw",
                "summary": "OMML 公式（未转 LaTeX）",
                "payload_ref": payload,
                "content": content,
            }),
            st,
        );
    }
}

fn collect_m_text(el: &El) -> String {
    let mut s = String::new();
    if el.prefix == "m" && el.local == "t" {
        s.push_str(&el.text);
    }
    for c in &el.children {
        s.push_str(&collect_m_text(c));
    }
    s
}

/// w:drawing → inline_image span（图片资产登记）。取不到 blip 时返回 None。
fn map_drawing(d: &El, ctx: &mut BodyCtx) -> Option<Value> {
    if d.child("wp", "anchor").is_some() {
        ctx.agg
            .bump(FLOATING, LossClass::Partial, "degraded", String::new());
    }
    let blip = d.find_descendant("a", "blip");
    let rid = blip
        .and_then(|b| b.attr_local("embed"))
        .or_else(|| blip.and_then(|b| b.attr_local("link")))?;
    let rel = ctx.rels.iter().find(|r| r.id == rid)?.clone();
    let (filename, source) = if rel.external {
        let name = rel
            .target
            .rsplit(['/', '?'])
            .next()
            .unwrap_or("image")
            .to_string();
        (name, AssetSource::External(rel.target.clone()))
    } else {
        let part = rel.resolve("word/document.xml");
        let name = part.rsplit('/').next().unwrap_or("image").to_string();
        match ctx.pkg.read_part(&part) {
            Ok(bytes) => (name, AssetSource::Embedded(bytes)),
            Err(_) => {
                ctx.agg
                    .bump(MEDIA_MISSING, LossClass::Partial, "dropped", part);
                return None;
            }
        }
    };
    let mime = azodoc_convert::guess_mime(&filename).to_string();
    let asset = ctx.job.add_asset(&filename, &mime, source);
    let alt = d
        .find_descendant("wp", "docPr")
        .and_then(|dp| dp.attr("descr").or_else(|| dp.attr("name")))
        .unwrap_or("")
        .to_string();
    Some(json!({"type": "inline_image", "asset": asset, "alt": alt}))
}

/// w:pict（VML）：图片数据可取则映射为图，否则整段 preserved。
fn map_pict(p: &El, ctx: &mut BodyCtx, st: &mut InlineState, out: &mut Vec<Value>) {
    let imagedata = p.find_descendant("v", "imagedata");
    if let Some(rid) = imagedata.and_then(|i| i.attr_local("id").map(String::from)) {
        let rel = ctx.rels.iter().find(|r| r.id == rid).cloned();
        if let Some(rel) = rel {
            let part = rel.resolve("word/document.xml");
            let name = part.rsplit('/').next().unwrap_or("image").to_string();
            match ctx.pkg.read_part(&part) {
                Ok(bytes) => {
                    let mime = azodoc_convert::guess_mime(&name).to_string();
                    let asset = ctx
                        .job
                        .add_asset(&name, &mime, AssetSource::Embedded(bytes));
                    ctx.log.node_none();
                    push_span(
                        out,
                        json!({"type": "inline_image", "asset": asset, "alt": ""}),
                        st,
                    );
                    return;
                }
                Err(_) => {
                    ctx.agg
                        .bump(MEDIA_MISSING, LossClass::Partial, "dropped", part);
                }
            }
        }
    }
    let xml = el_to_xml(p);
    let payload = ctx.job.add_preserved("docx", "xml", xml.into_bytes());
    ctx.agg
        .bump(VML, LossClass::PreservedRaw, "preserved", payload.clone());
    push_span(
        out,
        json!({
            "type": "unknown",
            "origin": "docx",
            "loss_class": "preserved_raw",
            "summary": "VML 图形",
            "payload_ref": payload,
            "content": [],
        }),
        st,
    );
}

/// 脚注/尾注引用 → footnote_ref + 文末定义（重复引用复用同一块 id）。
fn map_footnote_ref(
    id: &str,
    endnote: bool,
    ctx: &mut BodyCtx,
    st: &mut InlineState,
    out: &mut Vec<Value>,
) {
    if let Some(block_id) = ctx.footnote_defs.get(id).cloned() {
        ctx.log.node_none();
        push_span(out, json!({"type": "footnote_ref", "id": block_id}), st);
        return;
    }
    let source = if endnote {
        ctx.endnotes.get(id)
    } else {
        ctx.footnotes.get(id)
    };
    let Some(def_el) = source else {
        ctx.agg.bump(
            FOOTNOTE_MISSING,
            LossClass::Partial,
            "dropped",
            format!("id={id}"),
        );
        return;
    };
    let def_el = def_el.clone(); // 脱离借用以便递归映射
    let kids: Vec<&El> = def_el
        .children
        .iter()
        .filter(|c| c.is("w", "p") || c.is("w", "tbl"))
        .collect();
    let children = map_block_seq(kids, ctx);
    let block_id = ctx.job.idgen.uid("blk");
    ctx.footnote_defs.insert(id.to_string(), block_id.clone());
    let label = (ctx.pending_footnotes.len() + 1).to_string();
    ctx.pending_footnotes.push(json!({
        "id": block_id,
        "type": "footnote",
        "label": label,
        "children": children,
    }));
    ctx.log.node_none();
    push_span(out, json!({"type": "footnote_ref", "id": block_id}), st);
}

// ---------------------------------------------------------------- 表格

struct RawCell {
    col: usize,
    col_span: usize,
    row_span: usize,
    /// vMerge continue：内容并入上方 merge 起点单元格，本 cell 不产出
    shadow: bool,
    children: Vec<Value>,
}

fn map_table(tbl: &El, ctx: &mut BodyCtx) -> Value {
    // 列数：tblGrid 优先，回退最大行宽
    let grid_cols = tbl
        .child("w", "tblGrid")
        .map(|g| g.children.iter().filter(|c| c.is("w", "gridCol")).count())
        .unwrap_or(0);
    // 列宽：gridCol@w:w（twips，1/20 pt）原样作相对单位（spec §6.5 仅比例有效）
    let grid_widths: Vec<i64> = tbl
        .child("w", "tblGrid")
        .map(|g| {
            g.children
                .iter()
                .filter(|c| c.is("w", "gridCol"))
                .map(|c| {
                    c.attr("w:w")
                        .and_then(|v| v.parse::<i64>().ok())
                        .unwrap_or(0)
                        .max(0)
                })
                .collect()
        })
        .unwrap_or_default();

    // 表样式登记
    if let Some(sid) = tbl
        .child("w", "tblPr")
        .and_then(|pr| pr.child("w", "tblStyle"))
        .and_then(|s| s.attr("w:val"))
    {
        ctx.mark_style_used(sid);
    }

    let mut all_cells: Vec<RawCell> = Vec::new();
    // 每行：本行产出的 cell 全局下标（出现序）
    let mut row_cell_indices: Vec<Vec<usize>> = Vec::new();
    let mut row_headers: Vec<bool> = Vec::new();
    // (row, col) → cell 全局下标（占用网格，vMerge/colSpan 都占位）
    let mut occupied: Vec<Vec<Option<usize>>> = Vec::new();
    // col → 当前开放 merge 的 cell 全局下标
    let mut merge_open: BTreeMap<usize, usize> = BTreeMap::new();

    for tr in tbl.children.iter().filter(|c| c.is("w", "tr")) {
        let header = tr
            .child("w", "trPr")
            .map(|pr| pr.child("w", "tblHeader").is_some())
            .unwrap_or(false);
        let mut this_row: Vec<usize> = Vec::new();
        let mut occupied_row: Vec<Option<usize>> = vec![None; grid_cols];
        let mut cursor = 0usize;

        for tc in tr.children.iter().filter(|c| c.is("w", "tc")) {
            let tcpr = tc.child("w", "tcPr");
            let col_span = tcpr
                .and_then(|pr| pr.child("w", "gridSpan"))
                .and_then(|g| g.attr("w:val"))
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(1)
                .max(1);
            let vmerge_state: Option<String> = tcpr
                .and_then(|pr| pr.child("w", "vMerge"))
                .map(|v| v.attr("w:val").unwrap_or("continue").to_string());

            while occupied_row
                .get(cursor)
                .map(|o| o.is_some())
                .unwrap_or(false)
            {
                cursor += 1;
            }
            let col = cursor;

            let kids: Vec<&El> = tc
                .children
                .iter()
                .filter(|c| c.is("w", "p") || c.is("w", "tbl"))
                .collect();
            let mut children = map_block_seq(kids, ctx);
            if children.is_empty() {
                children.push(json!({
                    "id": ctx.job.idgen.uid("blk"),
                    "type": "paragraph",
                    "content": [],
                }));
            }
            ctx.log.node_none(); // 单元格已评估

            let mut shadow = false;
            match vmerge_state.as_deref() {
                Some("restart") => {
                    merge_open.insert(col, all_cells.len());
                }
                Some(_) => match merge_open.get(&col).copied() {
                    Some(start_idx) => {
                        all_cells[start_idx].row_span += 1;
                        shadow = true;
                        let content_text = children
                            .iter()
                            .map(azodoc_convert::plain_text_of_block)
                            .collect::<String>();
                        if !content_text.trim().is_empty() {
                            ctx.agg.bump(
                                TABLE_EDGE,
                                LossClass::Partial,
                                "dropped",
                                "vMerge 续格含内容".to_string(),
                            );
                        }
                    }
                    None => {
                        ctx.agg.bump(
                            TABLE_EDGE,
                            LossClass::Partial,
                            "degraded",
                            "孤儿 vMerge continue".to_string(),
                        );
                    }
                },
                _ => {}
            }

            all_cells.push(RawCell {
                col,
                col_span,
                row_span: 1,
                shadow,
                children,
            });
            let idx = all_cells.len() - 1;
            if !shadow {
                this_row.push(idx);
            }
            for c in col..col + col_span {
                while occupied_row.len() <= c {
                    occupied_row.push(None);
                }
                occupied_row[c] = Some(idx);
            }
            cursor = col + col_span;
        }
        occupied.push(occupied_row);
        row_cell_indices.push(this_row);
        row_headers.push(header);
    }

    let header_row = row_headers.first().copied().unwrap_or(false);
    let n_cols = grid_cols.max(occupied.iter().map(|r| r.len()).max().unwrap_or(0));
    let mut columns: Vec<Value> = Vec::new();
    for i in 0..n_cols {
        let mut col = json!({"id": ctx.job.idgen.uid("col"), "name": ""});
        match grid_widths.get(i).copied() {
            Some(w) if w > 0 => {
                col.as_object_mut()
                    .expect("object")
                    .insert("width".into(), json!(w));
            }
            _ => {}
        }
        columns.push(col);
    }

    let mut out_rows: Vec<Value> = Vec::new();
    for indices in &row_cell_indices {
        let mut cells_out: Vec<Value> = Vec::new();
        for idx in indices {
            let cell = &all_cells[*idx];
            if cell.shadow {
                continue;
            }
            let mut cell_obj = json!({
                "id": ctx.job.idgen.uid("cel"),
                "column": cell.col as i64,
                "children": cell.children,
            });
            if cell.col_span > 1 {
                cell_obj
                    .as_object_mut()
                    .expect("object")
                    .insert("colSpan".into(), json!(cell.col_span as i64));
            }
            if cell.row_span > 1 {
                cell_obj
                    .as_object_mut()
                    .expect("object")
                    .insert("rowSpan".into(), json!(cell.row_span as i64));
            }
            cells_out.push(cell_obj);
        }
        if cells_out.is_empty() {
            cells_out.push(json!({
                "id": ctx.job.idgen.uid("cel"),
                "column": 0,
                "children": [{
                    "id": ctx.job.idgen.uid("blk"),
                    "type": "paragraph",
                    "content": [],
                }],
            }));
        }
        out_rows.push(json!({"id": ctx.job.idgen.uid("row"), "cells": cells_out}));
    }

    // header_row → columns[].name 取首行文本；首行 cells 补 role:"header"
    // （spec §6.5：header_row 与 role 正交，tblHeader 同时声明两者）
    if header_row {
        if let Some(first) = out_rows.first_mut() {
            if let Some(cells) = first.get_mut("cells").and_then(Value::as_array_mut) {
                for (i, cell) in cells.iter_mut().enumerate() {
                    cell.as_object_mut()
                        .expect("object")
                        .insert("role".into(), json!("header"));
                    let mut name = String::new();
                    if let Some(children) = cell.get("children").and_then(Value::as_array) {
                        for ch in children {
                            name.push_str(&azodoc_convert::plain_text_of_block(ch));
                        }
                    }
                    if let Some(col) = columns.get_mut(i) {
                        col.as_object_mut()
                            .expect("object")
                            .insert("name".into(), json!(name.trim()));
                    }
                }
            }
        }
    }

    json!({
        "id": ctx.job.idgen.uid("blk"),
        "type": "table",
        "header_row": header_row,
        "columns": columns,
        "rows": out_rows,
    })
}
