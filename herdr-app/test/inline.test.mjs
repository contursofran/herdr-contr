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
