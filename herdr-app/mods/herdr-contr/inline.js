// "Inline PR icons (experimental)" (#57), off by default: a prose paragraph with a pull request or issue
// reference is drawn as a disabled Textarea whose references are tokens: GitHub's icon, colored by the
// state gh gives, and the reference underlined. A click opens it. TextView cannot draw icons or colors.
// A TextareaState may not be made while a view draws: render asks for one (want), and inlineTick
// (the chat's timer) makes it, updates its text, releases it when its row leaves the feed, and reads
// the states from gh.
import { div, svg } from "gpui-kit";
import { h_flex, Textarea, TextareaState } from "gpui-base";
import { exec } from "herdr";
import { linkRefs } from "./refs.js";
import { icon } from "./state.js";
import { proseLines, lineHeight, FS } from "./estimate.js";

const REF_URL = /^https:\/\/github\.com\/([\w.-]+\/[\w.-]+)\/(pull|issues)\/(\d+)$/;
const LINK = /\[([^\]\n]*)\]\(([^)\n]*)\)|\*\*|`+/g;

// A block that is one plain paragraph: no heading, list, quote, table or rule line, no blank line.
export function isParagraph(md) {
  const lines = String(md).split("\n");
  return lines.every((l) => l.trim() && !/^\s*(#{1,6}\s|[-*+]\s|\d+[.)]\s|>|\||([-*_=])(\s*\2){2,}\s*$)/.test(l));
}

// The paragraph as Textarea content: "**" and backticks dropped, [label](url) as its label, each
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

// --- The Textarea states, by key "<row id>-<block index>".
const views = new Map();     // key -> { row, state, text, content }
const wanted = new Map();    // key -> { row, text, content }: asked for while drawing

function want(key, row, text, content) { wanted.set(key, { row, text, content }); }

// Makes, updates and releases the states; reads gh. `rowIds`: the ids of the rows in the shown feed.
// Run from a timer or event, never from render. true when the chat should draw again.
export async function inlineTick(rowIds, now = Date.now()) {
  let changed = false;
  for (const [key, w] of wanted) {
    const v = views.get(key);
    if (!v) {
      const state = TextareaState.new({ value: w.content.text });
      state.set_soft_wrap?.(true);
      state.set_value(w.content);
      views.set(key, { row: w.row, state, text: w.text, content: w.content });
      changed = true;
    } else if (v.text !== w.text) {
      v.state.set_value(w.content); v.text = w.text; v.content = w.content; changed = true;
    }
  }
  wanted.clear();
  for (const [key, v] of views) {
    if (!rowIds.has(v.row)) { v.state.release?.(); views.delete(key); changed = true; }
  }
  const urls = [...views.values()].flatMap((v) => v.content.tokens.map((t) => t.token.id));
  if (await fetchStates(urls, now)) changed = true;
  return changed;
}
export function forgetInline() { for (const v of views.values()) v.state.release?.(); views.clear(); wanted.clear(); states.clear(); asking.clear(); }
export const inlineViews = () => views;

// The theme token and icon of a state; neutral (muted) until known.
const COLOR = { "pr-open": "pr_open", "pr-draft": "pr_draft", "pr-merged": "pr_merged", "pr-closed": "pr_closed",
  "issue-open": "issue_open", "issue-done": "issue_done", "issue-notplanned": "issue_notplanned" };
export function tokenLook(url, t) {
  const s = stateOf(url);
  const pr = s ? s.startsWith("pr-") : /\/pull\//.test(url);
  return { icon: icon(pr ? "pull-request" : "issue"), color: (s && t[COLOR[s]]) || t.muted };
}

// The paragraph drawn with icons, or null (no reference, or its state is not made yet: TextView then).
// Its height is the estimate's for the same block (estimate.js), so the row heights hold; when the icons
// would need one more line, it is null too.
export function inlineParagraph(key, rowId, md, repo, t, width, k = 1) {
  if (!isParagraph(md)) return null;
  return inlinePiece(key, rowId, md, repo, t, width, k, paragraphHeight(md, width, k));
}
// One paragraph or list item's text (no marker) drawn with icons in `h` px, the estimate's height for it;
// null as above. mdblocks.js gives `width` and `h` for a list item from its indented width.
export function inlinePiece(key, rowId, md, repo, t, width, k, h) {
  const content = inlineContent(md, repo);
  if (!content) return null;
  want(key, rowId, md, content);
  const v = views.get(key);
  if (!v) return null;
  const fs = FS * k;
  // The icons would need one more line than the estimate counts: TextView, so the row heights hold.
  if (inlineLines(content, width, k) * lineHeight(fs) > h + 0.5) return null;
  return Textarea.new(v.state).id(key).disabled(true).w_full().h(h)
    .border(0).bg(t.transparent ?? "#00000000").p(0).text_size(fs).line_height(lineHeight(fs)).text_color(t.foreground)
    .token((tok) => {
      const look = tokenLook(tok.token.id, t);
      return h_flex().gap(2).items_center()
        .child(svg(look.icon).size(Math.round(14 * k)).flex_none().text_color(look.color))
        .child(div().underline().child(tok.token.text));
    })
    .on_token_click((e, cx) => cx.open_url(e.token.id));
}
// The paragraph's height as estimate.js counts it (markdownHeight: lines joined, "**" dropped).
export function paragraphHeight(md, width, k = 1) {
  const fs = FS * k;
  return proseLines(String(md).split("\n").map((l) => l.trim()).join(" ").replace(/\*\*/g, ""), width, fs, k) * lineHeight(fs);
}
// Lines the drawn paragraph takes: each token is its text and an icon (14 px and a 2 px gap: about "MI" wide).
export function inlineLines(content, width, k = 1) {
  let s = content.text;
  for (const tk of [...content.tokens].reverse()) s = s.slice(0, tk.range.start) + "MI" + s.slice(tk.range.start);
  return proseLines(s, width, FS * k, k);
}
