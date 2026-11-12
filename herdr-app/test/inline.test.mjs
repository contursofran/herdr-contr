// Inline PR icons (#57, experimental setting): token content, states from a fake gh, the Textarea's life
// outside render, and the height estimate.
import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { fake, log, reset, theme } from "./stub/herdr.mjs";
import { made } from "./stub/ui.mjs";
import { KEY_CONFIG } from "../mods/herdr-contr/config.js";
import { markdownView } from "../mods/herdr-contr/mdblocks.js";
import { markdownHeight } from "../mods/herdr-contr/estimate.js";
import { isParagraph, inlineContent, inlineTick, inlineViews, inlineParagraph, forgetInline, stateOf, tokenLook,
  paragraphHeight, inlineLines } from "../mods/herdr-contr/inline.js";

const PR = "https://github.com/o/r/pull/51", IS = "https://github.com/o/r/issues/7";
const on = () => { fake.shared[KEY_CONFIG] = { file: { inline_icons: "on" }, bad: false }; };
// A fake gh: `api repos/o/r/issues/<n>` and `.../pulls/<n>` from `db`; counts the calls in flight.
let db, live, most;
const gh = (_c, args) => {
  const m = /issues\/(\d+)|pulls\/(\d+)/.exec(args[1]);
  const v = m[1] ? db.issues[m[1]] : db.pulls[m[2]];
  live++; most = Math.max(most, live);
  return new Promise((ok) => setTimeout(() => { live--; ok(v ? { code: 0, stdout: JSON.stringify(v) } : { code: 1, stdout: "" }); }, 1));
};
beforeEach(() => {
  reset(); forgetInline(); live = 0; most = 0;
  db = { issues: { 51: { state: "open", pull_request: {} }, 7: { state: "closed", state_reason: "not_planned" } }, pulls: { 51: { state: "open", draft: false } } };
  fake.exec = gh;
});

test("a plain paragraph only: no heading, list, quote, table or rule", () => {
  assert.ok(isParagraph("See PR #51\nand more"));
  for (const s of ["# PR #51", "- PR #51", "1. PR #51", "> PR #51", "| PR #51 |", "a\n\nb", "---"]) assert.ok(!isParagraph(s), s);
});

test("content: each reference a token, ** and backticks dropped, a link as its label", () => {
  const c = inlineContent("**Done** in PR #51, see `#9` and [docs](https://x.y) and #7.", "o/r");
  assert.equal(c.text, "Done in PR #51, see #9 and docs and #7.");
  assert.deepEqual(c.tokens.map((t) => [t.token.id, c.text.slice(t.range.start, t.range.end)]), [[PR, "PR #51"], [IS, "#7"]]);
  assert.equal(inlineContent("no refs, color #fff", "o/r"), null);
  assert.equal(inlineContent("bare #51", null), null, "no repo: a bare number stays text");
});

test("off (the default): prose stays TextView, no Textarea state is made", async () => {
  const before = made.textarea;
  const v = markdownView("r1", "See PR #51", theme(), 600, 1, "o/r");
  assert.equal(v.tag, "md");
  assert.equal(await inlineTick(new Set(["r1"])), false);
  assert.equal(made.textarea, before);
});

test("on: render asks, the tick makes the state, the next render draws a disabled Textarea with tokens", async () => {
  on();
  const before = made.textarea;
  assert.equal(markdownView("r1", "See PR #51", theme(), 600, 1, "o/r").tag, "md", "first frame: TextView");
  assert.equal(made.textarea, before, "never made while drawing");
  assert.equal(await inlineTick(new Set(["r1"])), true);
  assert.equal(made.textarea, before + 1);
  const el = markdownView("r1", "See PR #51", theme(), 600, 1, "o/r");
  assert.equal(el.tag, "ta");
  assert.equal(el._a.disabled, true);
  assert.equal(el._a.border, 0);
  assert.equal(el._a.p, 0);
  // A click on a token opens its URL.
  const opened = [];
  el._h.on_token_click({ token: { id: PR } }, { open_url: (u) => opened.push(u) });
  assert.deepEqual(opened, [PR]);
  // Headings and lists stay TextView.
  assert.equal(markdownView("r2", "# PR #51", theme(), 600, 1, "o/r").tag, "md");
});

test("text that changes while the agent writes is set again; a row that leaves the feed is released", async () => {
  on();
  markdownView("r1", "See PR #51", theme(), 600, 1, "o/r");
  await inlineTick(new Set(["r1"]));
  const v = [...inlineViews().values()][0];
  let released = 0; v.state.release = () => { released++; return true; };
  markdownView("r1", "See PR #51 and #7", theme(), 600, 1, "o/r");
  assert.equal(await inlineTick(new Set(["r1"])), true);
  assert.equal(v.state.value().text, "See PR #51 and #7");
  await inlineTick(new Set());
  assert.equal(released, 1);
  assert.equal(inlineViews().size, 0);
});

test("states come from gh, cached: open again after 60 s, closed kept; neutral until known", async () => {
  on();
  const t = theme();
  assert.equal(tokenLook(PR, t).color, t.muted, "neutral before gh answers");
  assert.match(tokenLook(PR, t).icon, /pull-request\.svg$/);
  assert.match(tokenLook(IS, t).icon, /issue\.svg$/);
  markdownView("r1", "PR #51 and #7", theme(), 600, 1, "o/r");
  await inlineTick(new Set(["r1"]), 0);
  assert.equal(stateOf(PR), "pr-open");
  assert.equal(stateOf(IS), "issue-notplanned");
  assert.equal(tokenLook(PR, t).color, t.pr_open);
  const n = log.exec.length;
  await inlineTick(new Set(["r1"]), 30_000);
  assert.equal(log.exec.length, n, "cached");
  db.pulls[51] = { state: "closed", merged_at: "2026-11-12" };
  await inlineTick(new Set(["r1"]), 61_000);
  assert.equal(stateOf(PR), "pr-merged");
  assert.ok(log.exec.every(([c]) => c === "gh"));
});

test("each state of a pull request and an issue", async () => {
  on();
  db.issues = { 1: { state: "open", pull_request: {} }, 2: { state: "closed", pull_request: {} }, 3: { state: "open", pull_request: {} },
    4: { state: "open" }, 5: { state: "closed", state_reason: "completed" }, 6: {} };
  db.pulls = { 1: { state: "open", draft: true }, 2: { state: "closed" }, 3: { state: "open" } };
  for (const n of [1, 2, 3, 4, 5, 6]) { markdownView(`r${n}`, `#${n}`, theme(), 600, 1, "o/r"); }
  for (let i = 0; i < 4; i++) await inlineTick(new Set(["r1", "r2", "r3", "r4", "r5", "r6"]), 0);
  const s = (n) => stateOf(`https://github.com/o/r/issues/${n}`);
  assert.deepEqual([1, 2, 3, 4, 5].map(s), ["pr-draft", "pr-closed", "pr-open", "issue-open", "issue-done"]);
  assert.equal(s(6), "issue-done");
  assert.ok(most <= 3, `at most 3 gh calls at once, saw ${most}`);
});

test("gh fails: no state, the icon stays neutral", async () => {
  on();
  fake.exec = async () => ({ code: 1, stdout: "", stderr: "no gh" });
  markdownView("r1", "PR #51", theme(), 600, 1, "o/r");
  await inlineTick(new Set(["r1"]), 0);
  assert.equal(stateOf(PR), null);
  assert.equal(tokenLook(PR, theme()).color, theme().muted);
  fake.exec = gh;
  await inlineTick(new Set(["r1"]), 30_000);
  assert.equal(stateOf(PR), null, "not asked again at once");
  await inlineTick(new Set(["r1"]), 61_000);
  assert.equal(stateOf(PR), "pr-open", "asked again after 60 s");
});

test("height: the Textarea is the estimate's paragraph height, and the drawn lines (icons too) fit in it", async () => {
  on();
  const texts = ["Fixed in PR #51.", "Merged **PR #51** and closed #7; the `inline` part waits on o/r#9 and https://github.com/o/r/pull/12 for the review.",
    "A long one: ".concat("word ".repeat(60), "then PR #51 and #7 at the end.")];
  let drawn = 0, fell = 0;
  for (const [i, md] of texts.entries()) for (const w of [300, 600]) for (const k of [0.875, 1, 1.125]) {
    markdownView(`h${i}`, md, theme(), w, k, "o/r");
    await inlineTick(new Set([`h${i}`]));
    const el = inlineParagraph(`md-h${i}-0`, `h${i}`, md, "o/r", theme(), w, k);
    const lh = paragraphHeight("x", w, k);
    if (inlineLines(inlineContent(md, "o/r"), w, k) * lh > paragraphHeight(md, w, k)) { assert.equal(el, null, "icons need a line more: TextView"); fell++; continue; }
    assert.equal(el._a.h, paragraphHeight(md, w, k));
    assert.ok(el._a.h <= markdownHeight(md, w, null, k), "within the row estimate");
    drawn++;
    assert.ok(inlineLines(inlineContent(md, "o/r"), w, k) * lh <= el._a.h, `${i} at ${w}/${k}`);
  }
  assert.ok(drawn > 0 && fell > 0, `both cases seen: ${drawn} drawn, ${fell} fell back`);
});

// A coordinator message as the owner sees it (#68): paragraphs, bullets with references, a numbered list,
// a nested list and a contr-status block.
import { splitPieces } from "../mods/herdr-contr/blocks.js";
import { markdownRaw } from "../mods/herdr-contr/estimate.js";
import { splitBlocks } from "../mods/herdr-contr/mdblocks.js";
const COORD = [
  "Status after the last round. PR #63 merged on green CI, and #57 is closed with it.",
  "",
  "Open work:",
  "- PR #63: inline icons behind the experimental setting, merged.",
  "- The phone page waits on #51; the builder is on it.",
  "- Nothing else is queued.",
  "",
  "Next steps:",
  "1. Merge PR #51 when CI passes on its head.",
  "2. Install merged main and look at the chat.",
  "   - nested, see #7",
  "",
  "```contr-status",
  "workers: 1/4  ci: 2 queued",
  "```",
  "",
  "I will report when #51 lands.",
].join("\n");
// The Textareas and the TextViews in a drawn tree, and the slots (the divs with a height) under `el`.
const walk = (el, f) => { if (el && typeof el === "object") { f(el); (el.kids || []).forEach((c) => walk(c, f)); } };
const tagged = (el, tag) => { const o = []; walk(el, (e) => e.tag === tag && o.push(e)); return o; };

test("pieces: paragraphs and top-level items; a nested list keeps its item whole", () => {
  const ps = splitPieces(splitBlocks(COORD)[0].text);
  assert.deepEqual(ps.map((p) => p.k), ["para", "para", "item", "item", "item", "para", "item", "other"]);
  assert.deepEqual(ps.map((p) => p.mark ?? null).filter(Boolean), ["-", "-", "-", "1.", "2."]);
  assert.equal(ps[1].lines.length, 1, "a line right above a list is its own paragraph");
  assert.equal(ps.flatMap((p) => [...p.lines, ...p.tail]).join("\n"), splitBlocks(COORD)[0].text, "the pieces give the block again");
  assert.deepEqual(splitPieces("- a\n  more\n- b").map((p) => [p.k, p.body.join(" ")]), [["item", "a more"], ["item", "b"]]);
  assert.deepEqual(splitPieces("- a\n\n  later").map((p) => p.k), ["other"], "a second paragraph in an item: TextView");
  assert.deepEqual(splitPieces("# H #5\ntext").map((p) => p.k), ["other", "para"]);
  assert.deepEqual(splitPieces("> q #5\nlazy").map((p) => p.k), ["other"]);
});

test("a coordinator message: references drawn with icons, the rest TextView, heights as the estimate counts", async () => {
  on();
  let drawn = 0;
  for (const w of [360, 600, 900]) for (const k of [0.875, 1, 1.125]) {
    forgetInline();
    markdownView("c", COORD, theme(), w, k, "o/r");
    await inlineTick(new Set(["c"]));
    const el = markdownView("c", COORD, theme(), w, k, "o/r");
    const blocks = splitBlocks(COORD);
    assert.equal(el.kids.length, blocks.length);
    // Each text block's slots add up to its part of the estimate; with the code box, to the whole.
    let sum = 0;
    for (const [i, b] of blocks.entries()) {
      if (b.k === "code") { sum += markdownRaw("```\n" + b.text + "\n```", w, k); continue; }
      const slots = el.kids[i].kids.map((d) => d._a.h);
      const own = markdownRaw(blocks[i + 1]?.k === "code" ? b.text.replace(/(\n[ \t]*)+$/, "") : b.text, w, k);
      assert.ok(Math.abs(slots.reduce((a, x) => a + x, 0) - own) < 1e-9, `block ${i} at ${w}/${k}`);
      for (const d of el.kids[i].kids) assert.ok(d._a.pt >= 0 && d._a.pt <= d._a.h);
      sum += own;
    }
    assert.ok(Math.abs(sum - markdownRaw(COORD, w, k)) < 1e-9, `whole at ${w}/${k}`);
    // Most pieces with a reference draw with icons (the one-more-line fallback is rare).
    const tas = tagged(el.kids[0], "ta").length;
    assert.ok(tas >= 3, `${tas} of 4 drawn at ${w}/${k}`);
    // Pieces without references stay TextView; the nested list stays in its item's TextView.
    const mds = tagged(el.kids[0], "md").map((m) => m._a.text);
    assert.ok(mds.some((t) => /Nothing else is queued/.test(t)));
    assert.ok(mds.some((t) => /2\. Install merged main[\s\S]*nested/.test(t)));
    assert.equal(tagged(el.kids[2], "ta").length, 1, "the last paragraph");
    drawn += tas;
  }
  assert.ok(drawn >= 34, `${drawn} of 36 pieces with references drawn with icons`);
});

test("a list item: its bullet or number outside the Textarea, the text indented", async () => {
  on();
  const md = "Intro.\n\n- PR #51 merged\n3. then #7";
  markdownView("l", md, theme(), 600, 1, "o/r");
  await inlineTick(new Set(["l"]));
  const el = markdownView("l", md, theme(), 600, 1, "o/r");
  const rows = tagged(el, "h").filter((h) => h.kids.length === 2 && h.kids[1].kids[0]?.tag === "ta");
  assert.deepEqual(rows.map((r) => r.kids[0].kids[0]), ["•", "3."]);
  assert.equal(rows[0].kids[0]._a.w, 22);
  const ta = rows[0].kids[1].kids[0];
  assert.equal(ta._a.h, markdownRaw("- PR #51 merged", 600, 1), "the item's height at its indented width");
  assert.equal(tagged(el, "md")[0]._a.text, "Intro.\n");
});

test("on, no reference in the block: one TextView, as with the setting off", async () => {
  on();
  const md = "One.\n\n- two\n- three";
  markdownView("n", md, theme(), 600, 1, "o/r");
  await inlineTick(new Set(["n"]));
  const el = markdownView("n", md, theme(), 600, 1, "o/r");
  assert.equal(el.tag, "md");
  assert.equal(el._a.id, "md-n-0");
});

test("off: a coordinator message is drawn as before, one TextView per text block", async () => {
  const el = markdownView("c", COORD, theme(), 600, 1, "o/r");
  assert.deepEqual(el.kids.map((e) => e.tag), ["md", "v", "md"]);
  assert.equal(await inlineTick(new Set(["c"])), false);
});
