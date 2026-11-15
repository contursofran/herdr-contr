// Markdown with our own code boxes and tables: herdr's TextView draws them square and takes no CSS,
// so the text is split into blocks (blocks.js, the same split estimate.js counts); code fences and tables
// are drawn here (rounded), the rest stays TextView markdown.
import { div, image } from "gpui-kit";
import { v_flex, h_flex, TextView } from "gpui-base";
import { colWidths, markdownHeight, markdownRaw, lineHeight } from "./estimate.js";
import { splitBlocks, splitPieces, tableCells } from "./blocks.js";
import { linkRefs, refsVersion } from "./refs.js";
import { inlinePiece, inlineFit, inlineContent } from "./inline.js";
import { setting } from "./config.js";
import { isMermaid, mermaidOf, mermaidSize } from "./mermaid.js";
import { agentImageOf, agentImageSize, noteImages } from "./agentimages.js";

export { splitBlocks, tableCells };

// The block as written, for Copy: CRs dropped, normal spaces and tabs.
export function codeCopyText(b) {
  return b.text.split("\n").map((l) => l.replace(/\r$/, "")).join("\n");
}

// The block as one markdown paragraph, so a drag selects across its lines: every ASCII punctuation
// escaped (drawn as text, not markup), leading spaces as no-break spaces (markdown drops them), tabs
// as 4, an empty line as one no-break space (else the paragraph ends), lines joined by hard breaks.
const NBSP = "\u00a0";
export function codeMarkdown(text) {
  return codeCopyText({ text }).split("\n").map((l) => {
    const e = l.replace(/\t/g, "    ").replace(/[!-/:-@[-`{-~]/g, "\\$&");
    return e.replace(/^ +/, (s) => NBSP.repeat(s.length)) || NBSP;
  }).join("\\\n");
}

// The code box under `y` (px from the text's top, at `width`), as written for Copy; null when `y` is not
// on one. Box places are the estimate's, the same guess the row heights use.
export function codeBlockAt(text, width, y, k = 1, zoom = 1, log = null) {
  const spans = [];
  markdownHeight(String(text ?? ""), width, spans, k, zoom, log);
  const s = spans.find((b) => y >= b.top && y <= b.bottom);
  return s ? s.lines.join("\n") : null;
}

// A mermaid block with its diagram (mermaid.js): the same box, with the image in it at the size estimate.js
// counts (no wider than the text). Else the code as written.
function codeBox(key, b, t, k, width) {
  const d = isMermaid(b) ? mermaidOf(b.text, t) : null;
  if (d) {
    const s = mermaidSize(d, width);
    return v_flex().w_full().items_center().my(6).px(14).py(10).rounded(12).bg(t.raised).border(1).border_color(t.border)
      .child(image(d.path).w(s.w).h(s.h).flex_none());
  }
  return v_flex().w_full().my(6).px(14).py(10).rounded(12).bg(t.raised).border(1).border_color(t.border)
    .font_family("Menlo").text_size(13 * k).text_color(t.foreground)
    .child(TextView.markdown(key, codeMarkdown(b.text)).selectable(true));
}

// An image line with its copy (agentimages.js): the image in a rounded border, at the size estimate.js counts
// (no wider than the text, at most IMG_MAX_H high).
function imageBox(b, t, width) {
  const s = agentImageSize(b.img, width);
  return div().flex_none().my(6).w(s.w + 2).h(s.h + 2).rounded(8).border(1).border_color(t.border).bg(t.chrome).overflow_hidden()
    .child(image(b.img.rel).w(s.w).h(s.h).rounded(7));
}

// Cells get a fixed width: a TextView in a flexible cell keeps its full one-line width and spills
// out of the table. Widths follow the content (colWidths); estimate.js guesses the row heights from
// the same widths.
function table(id, b, t, width, k) {
  const n = Math.max(b.head.length, ...b.rows.map((r) => r.length));
  const ws = colWidths(width, [b.head, ...b.rows], n, k);
  const cell = (key, text, c, head) => div().w(ws[c]).flex_none().overflow_hidden().px(12).py(8)
    .when(b.align[c] === "right", (el) => el.text_right()).when(b.align[c] === "center", (el) => el.text_center())
    .when(head, (el) => el.font_semibold())
    .child(TextView.markdown(key, text ?? "").selectable(true));
  const row = (cells, r, head) => h_flex().w_full().items_start()
    .when(head, (el) => el.bg(t.raised).rounded_t(11))
    .when(!head, (el) => el.border_t(1).border_color(t.border))
    .children(Array.from({ length: n }, (_v, c) => cell(`${id}-c${r}-${c}`, cells[c], c, head)));
  return v_flex().w_full().my(6).rounded(12).border(1).border_color(t.border).text_size(13.5 * k).overflow_hidden()
    .child(row(b.head, "h", true))
    .children(b.rows.map((r, i) => row(r, i, false)));
}

function textView(key, md, repo, k) {
  const view = TextView.markdown(key, linkRefs(md, repo)).selectable(true);
  return k === 1 ? view : view.text_size(16 * k);
}

// The whole text: one element. `id` keeps the TextView ids unique per row; `width` is the text's
// width in px (tables need it). A text block's id is always md-<id>-<i>, one block or many: a message that
// grows a code box while the agent writes keeps its first view (and the selection in it).
// `k`: the chat's text size over the default 16 px (settings: text_size). At 1 nothing sets a size.
// `repo`: "owner/repo" for a bare "#51" (refs.js); PR and issue references in prose become links.
// `log`: an assistant row's log on this machine: its image lines show their images (agentimages.js), else null.
export function markdownView(id, text, t, width, k = 1, repo = null, log = null) {
  if (log) noteImages(log, text);
  const blocks = splitBlocks(text, log ? (src) => agentImageOf(log, src) : null);
  // "Inline PR icons" on: paragraphs and list items with a reference drawn with icons (#68).
  const inline = setting("inline_icons") === "on";
  const prose = (key, md, next) => (inline && safeSplit(key, id, md, next, repo, t, width, k)) || textView(key, md, repo, k);
  if (blocks.length === 1 && blocks[0].k === "md") return prose(`md-${id}-0`, blocks[0].text, null);
  return v_flex().w_full().children(blocks.map((b, i) =>
    b.k === "code" ? codeBox(`md-${id}-k${i}`, b, t, k, width)
      : b.k === "table" ? table(`md-${id}-t${i}`, b, t, width, k)
        : b.k === "img" ? imageBox(b, t, width)
        : prose(`md-${id}-${i}`, b.text, blocks[i + 1]?.k ?? null)));
}

// inlineSplit, or null when it throws: the TextView then, so the chat never goes blank because of the
// setting (#70). Each message text logs its error once.
const failed = new Set();
function safeSplit(key, rowId, md, next, repo, t, width, k) {
  try { return inlineSplit(key, rowId, md, next, repo, t, width, k); } catch (e) {
    if (!failed.has(md)) { failed.add(md); console.error("herdr-contr: inline PR icons failed, TextView drawn:", e); }
    return null;
  }
}

// A text block cut at its paragraphs and top-level list items (blocks.js: splitPieces). A piece with a
// reference is drawn with icons (inline.js); the pieces between stay together in one TextView. Each part
// is as tall as estimate.js counts it (markdownRaw of the text up to its end, less that up to its start),
// with the gap the estimate counts above it as top padding, so the rows neither clip nor leave a gap.
// null when no piece has icons (yet): the block is one TextView, as with the setting off.
// `next`: the kind of the block after it; the estimate drops a blank line's gap above a code box.
// The cut of a text block, kept for each text (#83): the parts with their heights, or null when no piece
// has icons. The chat draws each shown row again on each frame; only the elements are built then.
const plans = new Map();
function splitPlan(md, next, repo, width, k) {
  const id = `${width}|${k}|${next ?? ""}|${repo ?? ""}|${refsVersion()}|${md}`;
  if (plans.has(id)) return plans.get(id);
  const plan = cutPlan(md, next, repo, width, k);
  if (plans.size >= 1000) plans.clear();
  plans.set(id, plan);
  return plan;
}
function cutPlan(md, next, repo, width, k) {
  // No reference in the whole block: nothing to cut.
  if (!inlineContent(md, repo)) return null;
  const text = next === "code" ? md.replace(/(\n[ \t]*)+$/, "") : md;
  const pieces = splitPieces(text);
  const raw = (lines) => markdownRaw(lines.join("\n"), width, k);
  const up = Math.max(1, k);
  const parts = []; let src = [], group = null, any = false;
  for (const [j, p] of pieces.entries()) {
    const before = raw(src);
    src = src.concat(p.lines);
    const own = raw(p.lines);
    const lead = Math.max(0, raw(src) - before - own);
    src = src.concat(p.tail);
    const slot = raw(src) - before;
    if (p.k === "para" || p.k === "item") {
      const w = p.k === "item" ? width - 22 * up : width;
      const body = p.body.join("\n");
      if (inlineFit(body, repo, w, k, own)) {
        any = true; group = null;
        parts.push({ j, item: p.k === "item", mark: p.mark, body, w, own, h: slot, lead });
        continue;
      }
    }
    if (group) { group.text += "\n" + [...p.lines, ...p.tail].join("\n"); group.h += slot; continue; }
    parts.push(group = { j, text: [...p.lines, ...p.tail].join("\n"), h: slot, lead });
  }
  if (!any) return null;
  // One plain paragraph: the drawn row itself, with no slot around it.
  const bare = parts.length === 1 && pieces[0].k === "para" && parts[0].lead === 0 && !pieces[0].tail.length;
  return { parts, one: pieces.length === 1, bare };
}
export function inlineSplit(key, rowId, md, next, repo, t, width, k = 1) {
  const plan = splitPlan(md, next, repo, width, k);
  if (!plan) return null;
  const up = Math.max(1, k), lh = lineHeight(16 * k);
  const draw = (q) => {
    if (q.body == null) return textView(`${key}-g${q.j}`, q.text, repo, k);
    const body = inlinePiece(plan.one ? key : `${key}-p${q.j}`, rowId, q.body, repo, t, q.w, k, q.own);
    return !q.item ? body : h_flex().w_full().items_start()
      .child(div().w(22 * up).flex_none().h(lh).text_size(16 * k).child(/^\d/.test(q.mark) ? q.mark : "\u2022"))
      .child(div().flex_1().min_w(0).child(body));
  };
  if (plan.bare) return draw(plan.parts[0]);
  return v_flex().w_full().children(plan.parts.map((q) => div().w_full().h(q.h).pt(q.lead).overflow_hidden().child(draw(q))));
}
