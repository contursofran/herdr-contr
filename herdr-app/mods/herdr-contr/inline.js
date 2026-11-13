// "Inline PR icons (experimental)" (#57), off by default: a prose paragraph or list item with a pull request
// or issue reference is drawn from plain elements: a wrapping row with one child for each word, each
// reference a Link with GitHub's icon, colored by the state gh gives, and the reference, in a pill. A
// Link opens its URL with no handler. The chat's list renderer refuses handlers and state (#70), so there
// are none here. inlineTick (the chat's timer) reads the states from gh for the URLs that render drew.
import { div, svg } from "gpui-kit";
import { h_flex, Link } from "gpui-base";
import { exec } from "herdr";
import { linkRefs, refsVersion } from "./refs.js";
import { icon } from "./state.js";
import { proseLines, lineHeight, FS } from "./estimate.js";

const REF_URL = /^https:\/\/github\.com\/([\w.-]+\/[\w.-]+)\/(pull|issues)\/(\d+)$/;
const LINK = /\[([^\]\n]*)\]\(([^)\n]*)\)|\*\*|`+/g;

// A block that is one plain paragraph: no heading, list, quote, table or rule line, no blank line.
export function isParagraph(md) {
  const lines = String(md).split("\n");
  return lines.every((l) => l.trim() && !/^\s*(#{1,6}\s|[-*+]\s|\d+[.)]\s|>|\||([-*_=])(\s*\2){2,}\s*$)/.test(l));
}

// The paragraph as styled runs: { text, bold?, code?, href?, ref? } (ref: href is a pull request or
// issue). "**" toggles bold; a code span keeps its text as written. null when it has no reference.
export function inlineRuns(md, repo = null) {
  const src = linkRefs(String(md).replace(/\n/g, " "), repo);
  const runs = [];
  let i = 0, bold = false, any = false;
  const text = (s) => { if (s) runs.push(bold ? { text: s, bold } : { text: s }); };
  for (let m; (m = LINK.exec(src));) {
    text(src.slice(i, m.index)); i = LINK.lastIndex;
    if (m[0] === "**") { bold = !bold; continue; }
    if (m[1] != null) {
      const ref = REF_URL.test(m[2]); any ||= ref;
      runs.push(ref ? { text: m[1], href: m[2], ref } : { text: m[1], href: m[2] });
      continue;
    }
    // A backtick run: the code span up to the same run of backticks, else the backticks as text.
    const close = src.indexOf(m[0], i);
    if (close < 0) { text(m[0]); continue; }
    runs.push({ text: src.slice(i, close).replace(/\[([^\]\n]*)\]\([^)\n]*\)/g, "$1"), code: true });
    i = LINK.lastIndex = close + m[0].length;
  }
  text(src.slice(i));
  return any ? runs : null;
}
// The runs cut into words: each word a list of runs with no space between them, so the punctuation
// after a reference ("PR #51," or "(#51)") stays with it. Links and code spans are not cut.
export function inlineWords(runs) {
  const words = [];
  let cur = [];
  const end = () => { if (cur.length) words.push(cur); cur = []; };
  for (const r of runs ?? []) {
    if (r.href || r.code) { cur.push(r); continue; }
    r.text.split(/(\s+)/).forEach((part, j) => {
      if (j % 2) end(); else if (part) cur.push({ ...r, text: part });
    });
  }
  end();
  return words;
}

// The paragraph as content: "**" and backticks dropped, [label](url) as its label, each
// reference a token (id: its URL). null when it has no reference.
export function inlineContent(md, repo = null) {
  const src = linkRefs(String(md).replace(/\n/g, " "), repo);
  let text = "", i = 0;
  const tokens = [];
  for (let m; (m = LINK.exec(src));) {
    text += src.slice(i, m.index); i = LINK.lastIndex;
    if (m[1] == null) continue;
    if (REF_URL.test(m[2])) tokens.push({ range: { start: text.length, end: text.length + m[1].length }, token: { id: m[2], text: m[1], label: m[1] } });
    text += m[1];
  }
  text += src.slice(i);
  return tokens.length ? { text, tokens } : null;
}

// --- States from gh: "pr-open", "pr-draft", "pr-merged", "pr-closed", "issue-open", "issue-done",
// "issue-notplanned". Open ones, and ones gh did not give, are read again after 60 s; closed and merged ones are kept.
// The gap between words. Measured on screen (#74, #78): 0.3 em drew 1.7 times a TextView space, 0.17 em half of one.
const SPACE_EM = 0.22;
const OPEN_MS = 60_000, MAX_GH = 3;
const states = new Map();   // "owner/repo#n" -> { s, at }
const asking = new Set();

async function ghJson(path) {
  const r = await exec("gh", ["api", path], { timeout: 15000 });
  return r.code === 0 ? JSON.parse(r.stdout) : null;
}
export async function readState(slug, n) {
  const is = await ghJson(`repos/${slug}/issues/${n}`);
  if (!is) return null;
  if (!is.pull_request) return is.state === "open" ? "issue-open" : is.state_reason === "not_planned" ? "issue-notplanned" : "issue-done";
  const pr = await ghJson(`repos/${slug}/pulls/${n}`);
  if (!pr) return null;
  return pr.merged_at ? "pr-merged" : pr.state === "closed" ? "pr-closed" : pr.draft ? "pr-draft" : "pr-open";
}
// Not known (gh failed) is asked again after 60 s too.
const fresh = (e, now) => e && ((e.s && !/open|draft/.test(e.s)) || now - e.at < OPEN_MS);
// The known state of a reference URL, or null.
export function stateOf(url) {
  const m = REF_URL.exec(url);
  return m ? states.get(`${m[1]}#${m[3]}`)?.s ?? null : null;
}
// Reads up to MAX_GH stale states at once; true when one changed.
async function fetchStates(urls, now) {
  const todo = [];
  for (const u of urls) {
    const m = REF_URL.exec(u); if (!m) continue;
    const key = `${m[1]}#${m[3]}`;
    if (fresh(states.get(key), now) || asking.has(key) || todo.some((x) => x.key === key)) continue;
    todo.push({ key, slug: m[1], n: m[3] });
  }
  const run = todo.slice(0, Math.max(0, MAX_GH - asking.size));
  for (const x of run) asking.add(x.key);
  const got = await Promise.all(run.map(async (x) => {
    let s = null;
    try { s = await readState(x.slug, x.n); } catch { s = null; }
    asking.delete(x.key);
    const was = states.get(x.key)?.s;
    states.set(x.key, { s: s ?? was ?? null, at: now });
    return s != null && s !== was;
  }));
  return got.some(Boolean);
}

// --- The reference URLs that render drew, by row id: a plain Map, no Herdr state (render may not make any).
const drawn = new Map();     // row id -> Set of URLs

// Reads the gh states of the references drawn in the shown rows. `rowIds`: the ids of the rows in the
// shown feed; rows that left it are forgotten. Run from a timer or event, never from render.
// true when the chat should draw again.
export async function inlineTick(rowIds, now = Date.now()) {
  for (const row of drawn.keys()) if (!rowIds.has(row)) drawn.delete(row);
  const urls = [...drawn.values()].flatMap((u) => [...u]);
  return fetchStates(urls, now);
}
export function forgetInline() { drawn.clear(); states.clear(); asking.clear(); fits.clear(); }
export const inlineDrawn = () => drawn;

// The theme token and icon of a state; neutral (muted) until known.
const COLOR = { "pr-open": "pr_open", "pr-draft": "pr_draft", "pr-merged": "pr_merged", "pr-closed": "pr_closed",
  "issue-open": "issue_open", "issue-done": "issue_done", "issue-notplanned": "issue_notplanned" };
export function tokenLook(url, t) {
  const s = stateOf(url);
  const pr = s ? s.startsWith("pr-") : /\/pull\//.test(url);
  return { icon: icon(pr ? "pull-request" : "issue"), color: (s && t[COLOR[s]]) || t.muted };
}

// The paragraph drawn with icons, or null (no reference: TextView then).
// Its height is the estimate's for the same block (estimate.js), so the row heights hold; when the icons
// would need one more line, it is null too.
export function inlineParagraph(key, rowId, md, repo, t, width, k = 1) {
  if (!isParagraph(md)) return null;
  return inlinePiece(key, rowId, md, repo, t, width, k, paragraphHeight(md, width, k));
}
// One paragraph or list item's text (no marker) drawn with icons in `h` px, the estimate's height for it;
// null as above. mdblocks.js gives `width` and `h` for a list item from its indented width.
// Plain elements only: a wrapping row with one child for each word, and a Link for each reference.
export function inlinePiece(key, rowId, md, repo, t, width, k, h) {
  const words = inlineFit(md, repo, width, k, h);
  if (!words) return null;
  const fs = FS * k;
  const urls = new Set();
  let n = 0;
  const seg = (r) => {
    if (r.ref) {
      urls.add(r.href);
      const look = tokenLook(r.href, t);
      // A pill (#76): the icon and the text in a rounded box, a little smaller than the prose so it fits in the line.
      return Link.new(`${key}-r${n++}`).href(r.href).cursor_pointer().child(h_flex().gap(4).items_center().flex_none()
        .px(6).rounded(999).bg(t.raised).border(1).border_color(t.border)
        .text_size(fs * 0.875).line_height(1.35)
        .child(svg(look.icon).size(Math.round(13 * k)).flex_none().text_color(look.color))
        .child(div().child(r.text)));
    }
    if (r.href) return Link.new(`${key}-l${n++}`).href(r.href).cursor_pointer().child(div().underline().child(r.text));
    if (r.code) return div().font_family("Menlo").text_size(14 * k).px(4).rounded(4).bg(t.raised).child(r.text);
    if (r.bold) return div().font_weight(700).child(r.text);
    return r.text;
  };
  const row = h_flex().flex_wrap().items_center().gap_x(fs * SPACE_EM).w_full().h(h)
    .text_size(fs).line_height(lineHeight(fs) / fs).text_color(t.foreground)
    .children(words.map((w) => w.length === 1 ? seg(w[0]) : h_flex().flex_none().items_center().children(w.map(seg))));
  drawn.set(rowId, new Set([...(drawn.get(rowId) ?? []), ...urls]));
  return row;
}
// The words of a piece that fits in `h` px, or null (no reference, or the icons would need one more line
// than the estimate counts: TextView then, so the row heights hold). Kept for each text: the chat draws
// each shown row again on each frame, and this work is the slow part (#83).
const fits = new Map();
export function inlineFit(md, repo, width, k, h) {
  const key = `${width}|${k}|${h}|${repo ?? ""}|${refsVersion()}|${md}`;
  if (fits.has(key)) return fits.get(key);
  const content = inlineContent(md, repo);
  const words = content && inlineLines(content, width, k) * lineHeight(FS * k) <= h + 0.5 ? inlineWords(inlineRuns(md, repo)) : null;
  if (fits.size >= 2000) fits.clear();
  fits.set(key, words);
  return words;
}
// The paragraph's height as estimate.js counts it (markdownHeight: lines joined, "**" dropped).
export function paragraphHeight(md, width, k = 1) {
  const fs = FS * k;
  return proseLines(String(md).split("\n").map((l) => l.trim()).join(" ").replace(/\*\*/g, ""), width, fs, k) * lineHeight(fs);
}
// Lines the drawn paragraph takes: each token is its text and an icon (14 px and a 2 px gap: in a pill with a border and padding: about "MIM" wide).
export function inlineLines(content, width, k = 1) {
  let s = content.text;
  for (const tk of [...content.tokens].reverse()) s = s.slice(0, tk.range.start) + "MIM" + s.slice(tk.range.start);
  return proseLines(s, width, FS * k, k);
}
