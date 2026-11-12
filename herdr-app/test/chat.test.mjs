import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { fake, log, reset } from "./stub/herdr.mjs";
import { textOf, byId, calls, lastList } from "./stub/ui.mjs";
import { fakeCx } from "./fake-cx.mjs";
import { agent, space } from "./fixtures.mjs";
import { ChatView, chipIcon, oneLine } from "../mods/herdr-contr/chat.js";
import { parseAnswer, forgetCommands } from "../mods/herdr-contr/sidecar.js";
import { emptyFeed, applyFeed, applyOlder, olderFrom, addEcho, dropEcho, settleEchoes, shownRows, ECHO_MS } from "../mods/herdr-contr/feedstate.js";
import { wrapLines, markdownHeight, rowHeight, LH } from "../mods/herdr-contr/estimate.js";

const jl = (...lines) => ({ code: 0, stdout: lines.map((l) => JSON.stringify(l)).join("\n") + "\n", stderr: "" });
const header = { type: "header", protocol: 1, sidecar: "0.1.0", command: "feed" };
const end = { type: "end", ok: true };
const logLine = (cursor, path = "/logs/s.jsonl") => ({ type: "log", agent: "claude", path, start: 0, size: 10, cursor, model: "claude-opus-5-5" });
const user = (id, text) => ({ type: "row", id, k: "user", text, ts: 1 });
const asst = (id, text) => ({ type: "row", id, k: "assistant", text, ts: 2 });
const tools = (id, done = true) => ({ type: "row", id, k: "tools", ts: 3, chips: [{ symbol: "terminal", text: "Ran 2 commands",
  calls: [{ name: "Bash", summary: "swift test", done, error: false }, { name: "Bash", summary: "git status", done: true, error: false }] }] });

beforeEach(() => {
  reset(); calls.length = 0;
  fake.spaces = [space()];
  fake.agents = [agent("t1", "working")];
});

test("parseAnswer splits header, end, error and the lines between", () => {
  const a = parseAnswer(jl(header, logLine("c1"), { type: "error", code: "read", message: "x" }, end).stdout + "not json\n");
  assert.equal(a.header.protocol, 1); assert.equal(a.end.ok, true); assert.equal(a.error.code, "read");
  assert.deepEqual(a.lines.map((l) => l.type), ["log"]);
});

test("applyFeed: splice replaces from `at`, unchanged keeps rows, a new log path starts over", () => {
  const f = emptyFeed("t1");
  assert.equal(applyFeed(f, parseAnswer(jl(header, logLine("c1"), { type: "splice", at: 0, count: 2 }, user("u1", "hi"), asst("a1", "yo"), end).stdout)), 0);
  assert.deepEqual(f.rows.map((r) => r.id), ["u1", "a1"]); assert.equal(f.cursor, "c1");
  assert.equal(applyFeed(f, parseAnswer(jl(header, logLine("c1"), { type: "unchanged" }, end).stdout)), -1);
  assert.equal(applyFeed(f, parseAnswer(jl(header, logLine("c2"), { type: "splice", at: 1, count: 3 }, asst("a1", "yo!"), tools("a1-t2"), end).stdout)), 1);
  assert.deepEqual(f.rows.map((r) => r.text ?? r.k), ["hi", "yo!", "tools"]);
  applyFeed(f, parseAnswer(jl(header, logLine("c3", "/logs/other.jsonl"), { type: "splice", at: 3, count: 4 }, user("u9", "new"), end).stdout));
  assert.deepEqual(f.rows.map((r) => r.id), ["u9"], "rows of the old log are dropped");
});

test("applyFeed: an error shows once, and the next good answer clears it", () => {
  const f = emptyFeed("t1");
  const bad = { header: null, end: null, lines: [], error: { code: "core", message: "down" } };
  assert.equal(applyFeed(f, bad), 0);
  assert.equal(applyFeed(f, bad), -1);
  assert.equal(applyFeed(f, parseAnswer(jl(header, logLine("c1"), { type: "unchanged" }, end).stdout)), 0);
  assert.equal(f.error, null);
});

test("estimates: wide characters wrap sooner; code and headings add height", () => {
  assert.ok(wrapLines("中".repeat(100), 400) > wrapLines("a".repeat(100), 400));
  assert.ok(markdownHeight("# Title\n\ntext", 600) > markdownHeight("text", 600));
  assert.ok(markdownHeight("```\na\nb\n```", 600) > markdownHeight("a b", 600));
  const t = tools("x").chips ? { k: "tools", chips: tools("x").chips } : null;
  assert.ok(rowHeight(t, 600, true) > rowHeight(t, 600, false));
  assert.ok(rowHeight({ k: "assistant", text: "line\n\n".repeat(400) }, 600) > 4000);
});

test("estimates: code boxes and tables are guessed at their drawn height (no wrap margin, no gap)", () => {
  // codeBox: 6 px margin + 10 px padding + 1 px border, each side; 21 px per line (a TextView line at 13 px).
  assert.equal(markdownHeight("```\na\nb\nc\n```", 600), 3 * 21 + 34);
  // table: 6 px margin + 1 px border each side; a row is 8 + 22 + 8 px plus a 1 px line (1 px high on the head row, on purpose).
  assert.equal(markdownHeight("| A | B |\n|---|---|\n| a | b |", 600), 14 + 17 + 22 + 17 + 22);
  // The prose gets a small pad at the end, once per text, so a long answer with code is not guessed far too tall.
  const prose = "word ".repeat(30);
  const code = "```\n" + "x = 1\n".repeat(30) + "```";
  const pad = markdownHeight("a", 600) - LH;
  // The blank lines around a box add nothing: the box's own margin is the gap.
  const both = markdownHeight(`${prose}\n\n${code}\n\n${prose}`, 600);
  assert.ok(Math.abs(both - (2 * markdownHeight(prose, 600) - pad + markdownHeight(code, 600))) <= 1, `${both}`);  // 1 px: rounding
  // Between two paragraphs the blank line stays a 10 px gap.
  assert.equal(markdownHeight("a\n\nb", 600), 2 * LH + 10 + pad);
});

test("chipIcon maps SF Symbols names to the mod's icons", () => {
  assert.match(chipIcon("terminal"), /terminal\.svg$/);
  assert.match(chipIcon("doc.text"), /doc\.svg$/);
  assert.match(chipIcon("magnifyingglass"), /search\.svg$/);
  assert.match(chipIcon("hammer"), /tool\.svg$/);
});

test("the chat loads the shown agent's feed, opens at the newest row, and passes the cursor back", async () => {
  fake.exec = (cmd, args) => {
    if (!args.includes("--cursor")) return jl(header, logLine("c1"), { type: "splice", at: 0, count: 3 }, user("u1", "Fix the login bug"), asst("a1", "Looking at it."), tools("a1-t2", false), end);
    return jl(header, logLine("c2"), { type: "splice", at: 2, count: 4 }, tools("a1-t2"), { type: "row", id: "f-a1", k: "footer", text: "01:41 · Worked 36s · 2 tools" }, end);
  };
  const cx = fakeCx();
  const v = new ChatView(cx); await cx.settle();
  const feeds = () => log.exec.filter(([, a]) => a[0] === "feed");
  const [cmd, args] = feeds()[0];
  assert.match(cmd, /herdr-contr\/bin\/herdr-contr-feed$/);
  assert.deepEqual(args.slice(0, 3), ["feed", "--terminal", "t1"]);
  v.width = 900;
  v.render(cx);
  assert.equal(lastList._args.n, 3);
  assert.deepEqual(calls.at(-1), ["scroll_to_item", 2, "top"]);
  const rendered = lastList._args.render({ start: 0, end: 3 });
  assert.match(textOf(rendered[0]), /Fix the login bug/);
  assert.match(textOf(rendered[2]), /Ran 2 commands/);

  v.lastPoll = 0; await v.tick(cx); await cx.settle();
  assert.ok(feeds()[1][1].includes("c1"), "the second feed passes the cursor");
  v.render(cx);
  assert.equal(lastList._args.n, 4);
  assert.match(textOf(lastList._args.render({ start: 3, end: 4 })[0]), /Worked 36s/);
});

test("a click on a tool row shows its calls; the row grows", async () => {
  fake.exec = () => jl(header, logLine("c1"), { type: "splice", at: 0, count: 1 }, tools("a1-t2"), end);
  const cx = fakeCx(); const v = new ChatView(cx); await cx.settle(); v.width = 900;
  v.render(cx); const closed = lastList._args.sizes[0];
  lastList._h.on_item_click("a1-t2", cx);
  v.render(cx);
  assert.ok(lastList._args.sizes[0] > closed);
  assert.match(textOf(lastList._args.render({ start: 0, end: 1 })[0]), /git status/);
});

test("a tool call with a multi-line input shows one line, cut with …, and its row is clipped", async () => {
  const many = { type: "row", id: "a1-t3", k: "tools", ts: 3, chips: [{ symbol: "terminal", text: "Ran 3 commands\nmore",
    calls: [
      { name: "Bash", summary: "cd /x; python3 - <<'EOF'\np = 'a'\nprint(p)\nEOF", done: true, error: false },
      { name: "Bash", summary: "\r\n  \r\necho one\r\necho two", done: true, error: false },
      { name: "Bash", summary: "git status", done: true, error: false },
    ] }] };
  fake.exec = () => jl(header, logLine("c1"), { type: "splice", at: 0, count: 1 }, many, end);
  const cx = fakeCx(); const v = new ChatView(cx); await cx.settle(); v.width = 900;
  v.render(cx);
  lastList._h.on_item_click("a1-t3", cx);
  v.render(cx);
  const el = lastList._args.render({ start: 0, end: 1 })[0];
  const text = textOf(el);
  assert.ok(!/[\r\n]/.test(text), JSON.stringify(text));
  assert.match(text, /cd \/x; python3 - <<'EOF' …/);
  assert.match(text, /echo one …/);
  assert.match(text, /git status(?! …)/);
  assert.match(text, /Ran 3 commands …/);
  // Each call row is one line tall and clips what does not fit.
  const rows = [];
  const walk = (n) => { if (!n || typeof n !== "object") return; if (n._a?.h === 26) rows.push(n); (n.kids || []).forEach(walk); };
  walk(el);
  assert.equal(rows.length, 3);
  for (const r of rows) assert.ok("overflow_hidden" in r._a, "a call row clips its content");
  assert.equal(v.height(lastList._args.render ? v.feed().rows[0] : null), rowHeight(many, v.colWidth(), true));
  assert.equal(rowHeight(many, 760, true), 40 + 3 * 26 + 8);
});

test("a note from another agent (hc) is one short line with its sender; a click shows the whole note", async () => {
  const note = { type: "row", id: "n1", k: "user", from: "t2 docs-worker", text: "Done: notes updated.\n\nSee notes.md for the list.", ts: 2 };
  const bare = { type: "row", id: "n2", k: "user", from: "t3", text: "Short note", ts: 3 };
  fake.exec = () => jl(header, logLine("c1"), { type: "splice", at: 0, count: 3 }, user("u1", "Fix it"), note, bare, end);
  const cx = fakeCx(); const v = new ChatView(cx); await cx.settle(); v.width = 900;
  v.render(cx);
  const closed = lastList._args.sizes[1];
  let rendered = lastList._args.render({ start: 0, end: 3 });
  assert.match(textOf(rendered[0]), /Fix it/, "the owner's message stays a bubble");
  assert.match(textOf(rendered[1]), /↳ docs-worker: Done: notes updated\. …/, "the sender's name and the first line");
  assert.doesNotMatch(textOf(rendered[1]), /See notes\.md/);
  assert.match(textOf(rendered[2]), /↳ t3: Short note/, "no name: the terminal");
  assert.ok(closed < rowHeight({ k: "user", text: note.text }, v.colWidth()), "shorter than a bubble");
  lastList._h.on_item_click("n1", cx);
  v.render(cx);
  assert.ok(lastList._args.sizes[1] > closed, "open, the row grows");
  rendered = lastList._args.render({ start: 0, end: 3 });
  assert.match(textOf(rendered[1]), /See notes\.md for the list\./);
  assert.equal(v.height(v.feed().rows[1]), rowHeight(note, v.colWidth(), true));
  lastList._h.on_item_click("u1", cx);
  assert.ok(!v.open.has("u1"), "a click on the owner's message opens nothing");
});

test("oneLine keeps the first line with text and marks the rest with …", () => {
  assert.equal(oneLine("a\nb"), "a …");
  assert.equal(oneLine("\n\n  x  \r\n"), "x");
  assert.equal(oneLine("one"), "one");
  assert.equal(oneLine(""), "");
  assert.equal(oneLine(undefined), "");
  assert.equal(oneLine("tab\there"), "tab here");
});

test("scrolling up stops following and fades in the ↓ button; reaching the end follows again", async () => {
  fake.exec = () => jl(header, logLine("c1"), { type: "splice", at: 0, count: 2 }, user("u1", "a"), asst("a1", "b"), end);
  const cx = fakeCx(); const v = new ChatView(cx); await cx.settle(); v.width = 900;
  let el = v.render(cx);
  assert.equal(byId(el, "jump")._a.opacity, 0, "hidden while following");
  const find = (n) => (n?._h?.on_scroll_wheel ? n : (n?.kids ?? []).map(find).find(Boolean));
  find(el)._h.on_scroll_wheel({ delta: { x: 0, y: 40 } }, cx);
  el = v.render(cx);
  assert.equal(byId(el, "jump")._a.opacity, 1);
  assert.ok(byId(el, "jump").kids.some((k) => k.tag === "svg" && k._a.path.endsWith("arrow-down.svg")));
  byId(el, "jump")._h.on_click({}, cx);
  assert.equal(v.follow, true);
  assert.deepEqual(calls.at(-1), ["scroll_to_item", 1, "top"]);
});

test("a tall row shows in full: its whole guessed height, no Show all", async () => {
  const long = "paragraph\n\n".repeat(300);
  fake.exec = () => jl(header, logLine("c1"), { type: "splice", at: 0, count: 1 }, asst("a1", long), end);
  const cx = fakeCx(); const v = new ChatView(cx); await cx.settle(); v.width = 900;
  v.render(cx);
  assert.ok(lastList._args.sizes[0] > 4000);
  assert.doesNotMatch(textOf(lastList._args.render({ start: 0, end: 1 })[0]), /Show all/);
});

test("messages for a missing sidecar, a wrong protocol and a non-agent terminal", async () => {
  const show = async (exec) => { reset(); fake.spaces = [space()]; fake.agents = [agent("t1", "idle")]; fake.exec = exec;
    const cx = fakeCx(); const v = new ChatView(cx); await cx.settle(); return textOf(v.render(cx)); };
  const missing = await show(null);
  assert.match(missing, /sidecar did not answer/);
  assert.match(missing, /helper is missing\. Build it: cargo build --release -p herdr-contr-feed \(in xplat\), then herdr-contr-feed install --helper <it>\./);
  assert.doesNotMatch(missing, /flockbook/i);
  assert.match(await show(() => jl({ ...header, protocol: 2 }, end)), /protocol 2; this mod needs 1/);
  assert.match(await show(() => jl(header, { type: "error", code: "not-agent", message: "t1 runs no agent" }, { type: "end", ok: false })), /runs no agent/);
  assert.match(await show(() => jl(header, logLine("c1", null), { type: "splice", at: 0, count: 0 }, end)), /No conversation yet/);
});

test("switching agents shows the other agent's rows", async () => {
  fake.agents = [agent("t1", "idle"), agent("t2", "idle")];
  fake.exec = (_c, args) => jl(header, logLine("c-" + args[2]), { type: "splice", at: 0, count: 1 }, user("u-" + args[2], "from " + args[2]), end);
  const cx = fakeCx(); const v = new ChatView(cx); await cx.settle(); v.width = 900;
  fake.shared["herdr-contr.agent"] = "t2";
  await v.tick(cx); await cx.settle();
  v.render(cx);
  assert.match(textOf(lastList._args.render({ start: 0, end: 1 })[0]), /from t2/);
});

test("a call without a result shows as running, not as an approval", async () => {
  fake.exec = () => jl(header, logLine("c1"), { type: "splice", at: 0, count: 1 },
    { type: "row", id: "p1", k: "perm", text: "Bash command", perm: { title: "Bash command", command: "swift test", tool: "Bash", toolUseId: "toolu_1", state: "pending" } }, end);
  const cx = fakeCx(); const v = new ChatView(cx); await cx.settle(); v.width = 900;
  v.render(cx);
  const text = textOf(lastList._args.render({ start: 0, end: 1 })[0]);
  assert.match(text, /Running · Bash command/);
  assert.doesNotMatch(text, /approval/i);
});

test("cmd-alt-up / down switch spaces from the chat; other keys pass", async () => {
  const { spaceKey } = await import("../mods/herdr-contr/chat.js");
  const m = (x) => ({ shift: false, control: false, alt: false, platform: false, function: false, ...x });
  assert.equal(spaceKey({ key: "up", modifiers: m({ platform: true, alt: true }) }), "space.prev");
  assert.equal(spaceKey({ key: "down", modifiers: m({ platform: true, alt: true }) }), "space.next");
  assert.equal(spaceKey({ key: "up", modifiers: m({ platform: true }) }), null);
  assert.equal(spaceKey({ key: "a", modifiers: m({ platform: true, alt: true }) }), null);
});

test("every chat opens at its newest row: switching back to a read chat, and coming back from another screen", async () => {
  fake.agents = [agent("t1", "idle"), agent("t2", "idle")];
  fake.exec = (_c, args) => jl(header, logLine("c-" + args[2]), { type: "splice", at: 0, count: 2 }, user("u-" + args[2], "a"), asst("a-" + args[2], "b"), end);
  const cx = fakeCx(); const v = new ChatView(cx); await cx.settle(); v.width = 900;
  fake.shared["herdr-contr.agent"] = "t2"; await v.tick(cx); await cx.settle();
  v.follow = false;                                  // scrolled up in t2
  fake.exec = () => jl(header, logLine("c-t1"), { type: "unchanged" }, end);
  calls.length = 0;
  fake.shared["herdr-contr.agent"] = "t1"; await v.tick(cx); await cx.settle();
  assert.equal(v.follow, true);
  assert.deepEqual(calls.find((c) => c[0] === "scroll_to_item"), ["scroll_to_item", 1, "top"], "t1 is read and unchanged, still at its end");
  fake.shared["herdr-contr.screen"] = "overview"; await v.tick(cx);
  v.follow = false; calls.length = 0;
  fake.shared["herdr-contr.screen"] = "chat"; await v.tick(cx);
  assert.equal(v.follow, true);
  assert.deepEqual(calls.find((c) => c[0] === "scroll_to_item"), ["scroll_to_item", 1, "top"]);
});

test("applyOlder puts older rows on top; a new tail start drops them; start 0 means the first row is shown", () => {
  const f = emptyFeed("t1");
  const tail = (start, ...rows) => parseAnswer(jl(header, { ...logLine("c1"), start }, { type: "splice", at: 0, count: rows.length }, ...rows, end).stdout);
  applyFeed(f, tail(5000, user("u3", "c"), asst("a3", "d")));
  assert.equal(olderFrom(f), 5000);
  const older = (start, ...rows) => parseAnswer(jl(header, { type: "older", path: "/logs/s.jsonl", start, count: rows.length }, ...rows, end).stdout);
  assert.equal(applyOlder(f, older(2000, user("u2", "b"), asst("a2", "x"), user("u3", "dup")), 5000), 2, "a row already shown is not added twice");
  assert.deepEqual(f.rows.map((r) => r.id), ["u2", "a2", "u3", "a3"]);
  assert.equal(olderFrom(f), 2000);
  assert.equal(applyOlder(f, older(0, user("u1", "a")), 5000), 0, "an answer for an old request is dropped");
  assert.equal(applyOlder(f, older(0, user("u1", "a")), 2000), 1);
  assert.equal(olderFrom(f), null);
  // A splice counts in tail rows; the index it returns counts what is shown.
  assert.equal(applyFeed(f, parseAnswer(jl(header, { ...logLine("c2"), start: 5000 }, { type: "splice", at: 1, count: 2 }, asst("a3", "d!"), end).stdout)), 4);
  assert.deepEqual(f.rows.map((r) => r.id), ["u1", "u2", "a2", "u3", "a3"]);
  applyFeed(f, tail(9000, user("u9", "z")));
  assert.deepEqual(f.rows.map((r) => r.id), ["u9"]); assert.equal(olderFrom(f), 9000);
});

test("reaching the top asks for older rows once and keeps the same row in view", async () => {
  fake.exec = (_c, args) => args.includes("--before")
    ? jl(header, { type: "older", path: "/logs/s.jsonl", start: 0, count: 2 }, user("u0", "old"), asst("a0", "older"), end)
    : jl(header, { ...logLine("c1"), start: 4000 }, { type: "splice", at: 0, count: 2 }, user("u1", "a"), asst("a1", "b"), end);
  const cx = fakeCx(); const v = new ChatView(cx); await cx.settle(); v.width = 900;
  await v.tick(cx); await cx.settle();   // the chat screen is up (its first tick opens it at the end)
  v.autoOlder = 5;   // loads by themselves used up (they fill a short chat): here only the wheel loads
  let el = v.render(cx); lastList._args.render({ start: 0, end: 1 });
  assert.ok(!v.wantOlder, "the first row on screen alone loads nothing");
  const wheel = (n) => (n?._h?.on_scroll_wheel ? n : (n?.kids ?? []).map(wheel).find(Boolean));
  wheel(el)._h.on_scroll_wheel({ delta: { x: 0, y: -40 } }, cx);
  assert.ok(!v.wantOlder, "scrolling down loads nothing");
  wheel(el)._h.on_scroll_wheel({ delta: { x: 0, y: 40 } }, cx);
  assert.equal(v.wantOlder, true);
  calls.length = 0;
  await v.tick(cx); await cx.settle();
  assert.deepEqual(v.feed().rows.map((r) => r.id), ["u0", "a0", "u1", "a1"]);
  assert.deepEqual(calls.filter((c) => c[0] === "scroll_to_item").at(-1), ["scroll_to_item", 2, "top"]);
  const asks = log.exec.filter((e) => e[1]?.includes?.("--before") || JSON.stringify(e).includes("--before"));
  assert.equal(asks.length, 1);
  assert.equal(olderFrom(v.feed()), null);
});

// A log whose last 2 MB holds few rows (big tool results): rows before `start`, read 2 MB at a time.
const shortTail = (olderAnswers) => {
  const asked = [];
  fake.exec = (_c, args) => {
    const i = args.indexOf("--before");
    if (i < 0) return jl(header, { ...logLine("c1"), start: 9000 }, { type: "splice", at: 0, count: 1 }, asst("a9", "last"), end);
    const before = Number(args[i + 1]); asked.push(before);
    const [start, rows] = olderAnswers(before);
    return jl(header, { type: "older", path: "/logs/s.jsonl", start, count: rows.length }, ...rows, end);
  };
  return asked;
};
const runTicks = async (v, cx, n = 12) => { for (let k = 0; k < n; k++) { v.render(cx); lastList._args.render({ start: 0, end: lastList._args.n }); await v.tick(cx); await cx.settle(); } };

test("rows that all fit on screen load older rows by themselves, until the log's start", async () => {
  const asked = shortTail((before) => [before - 3000, [user(`u${before}`, "q"), asst(`a${before}`, "r")]]);
  const cx = fakeCx(); const v = new ChatView(cx); await cx.settle(); v.width = 900;
  await runTicks(v, cx);
  assert.deepEqual(asked, [9000, 6000, 3000]);
  assert.equal(v.feed().rows.length, 7);
  assert.equal(olderFrom(v.feed()), null, "the start of the log was reached");
});

test("loading older rows by themselves stops after 5 loads per open (a log of huge tool results)", async () => {
  const asked = shortTail((before) => [before - 1, []]);   // only tool output: no rows, every time
  const cx = fakeCx(); const v = new ChatView(cx); await cx.settle(); v.width = 900;
  await runTicks(v, cx, 20);
  assert.deepEqual(asked, [9000, 8999, 8998, 8997, 8996], "a range with no rows goes on back, at most 5 times");
});

test("the first row on screen is the top, also when the last row is on screen too", async () => {
  shortTail(() => [0, []]);
  const cx = fakeCx(); const v = new ChatView(cx); await cx.settle(); v.width = 900;
  v.render(cx); lastList._args.render({ start: 0, end: 1 });
  assert.equal(v.topSeen, true);
});

test("older rows added while following the end keep the end in view and the ↓ button hidden", async () => {
  shortTail((before) => [0, [user("u0", "q"), asst("a0", "r")]]);
  const cx = fakeCx(); const v = new ChatView(cx); await cx.settle(); v.width = 900;
  calls.length = 0;
  await runTicks(v, cx, 3);
  assert.equal(v.feed().rows.length, 3);
  assert.equal(v.follow, true);
  assert.deepEqual(calls.filter((c) => c[0] === "scroll_to_item").at(-1), ["scroll_to_item", 2, "top"], "the newest row, not the old first one");
  assert.equal(byId(v.render(cx), "jump")._a.opacity, 0);
});

test("no element above a chat reply's text stops the press (Herdr 0.1.13: that makes the text unselectable)", async () => {
  fake.exec = () => jl(header, logLine("c1"), { type: "splice", at: 0, count: 2 }, user("u1", "hi"), asst("a1", "Some **text** to select."), end);
  const cx = fakeCx(); const v = new ChatView(cx); await cx.settle(); v.width = 900;
  const root = v.render(cx);
  const stops = [];
  const walk = (el, above) => {
    if (!el || typeof el !== "object") return;
    if (el.tag === "md") {
      for (const a of above) {
        const down = a._h?.on_mouse_down;
        if (!down) continue;
        let stopped = false;
        down({ button: "left", position: { x: 0, y: 0 } }, { stop_propagation: () => { stopped = true; }, notify() {} });
        if (stopped) stops.push(a.tag + (a._a?.id ? `#${a._a.id}` : ""));
      }
      return;
    }
    const kids = el.tag === "vl" ? el._args.render({ start: 0, end: el._args.n }) : (el.kids || []);
    for (const k of kids) walk(k, [...above, el]);
  };
  walk(root, []);
  assert.ok(lastList, "the chat drew its list");
  assert.deepEqual(stops, [], "an element above the reply text stops the left press");
});

test("right-click on a code box opens a menu with Copy code (that box as written) and Copy message", async () => {
  const { MenuView } = await import("../mods/herdr-contr/menu.js");
  const { BAR_H } = await import("../mods/herdr-contr/top.js");
  const { codeBlockAt } = await import("../mods/herdr-contr/mdblocks.js");
  const text = "Run:\n\n```\n  tailscale serve 8787\n```\nThen:\n\n```\necho two\n```";
  fake.exec = () => jl(header, logLine("c1"), { type: "splice", at: 0, count: 2 }, user("u1", "hi"), asst("a1", text), end);
  const cx = fakeCx(); const v = new ChatView(cx); await cx.settle(); v.width = 900;
  v.render(cx);
  const w = v.colWidth();
  const yOf = (want) => { for (let y = 0; y < 2000; y++) if (codeBlockAt(text, w, y) === want) return y + 3; return -1; };
  const press = (key, y) => lastList._h.on_item_secondary_click(key, { button: "right", position: { x: 0, y }, local_position: { x: 50, y } }, cx);
  const n0 = log.overlays.length;
  press("a1", 8 + yOf("echo two"));
  const o = log.overlays.at(-1);
  assert.equal(log.overlays.length, n0 + 1);
  assert.deepEqual(o.props.items.map((i) => i.label), ["Copy code", "Copy message"]);
  fake.props = { view: "menu", items: o.props.items, overlay: "o1" };
  let copied = null;
  const m = new MenuView(fakeCx()).render(fakeCx());
  byId(m, "menu-0")._h.on_click({}, { ...fakeCx(), write_to_clipboard: (s) => { copied = s; } });
  assert.equal(copied, "echo two");
  press("a1", 8 + yOf("  tailscale serve 8787"));
  assert.equal(log.overlays.at(-1).props.items[0].args.text, "  tailscale serve 8787");
  // On the prose, or on a user bubble without code: only Copy message. On an unknown row: no menu.
  press("a1", 8 + 4);
  assert.deepEqual(log.overlays.at(-1).props.items.map((i) => i.label), ["Copy message"]);
  press("u1", BAR_H + 20);
  assert.deepEqual(log.overlays.at(-1).props.items.map((i) => i.label), ["Copy message"]);
  const n1 = log.overlays.length;
  press("gone", 10);
  assert.equal(log.overlays.length, n1);
});

test("Copy message copies the row's markdown as written, for user, assistant and note rows; not for others", async () => {
  const { MenuView } = await import("../mods/herdr-contr/menu.js");
  const reply = "**Done.**\n\n- one\n- two\n\n```sh\nmake\n```";
  const note = { type: "row", id: "n1", k: "user", from: "t2 docs-worker", text: "Notes updated.\nSee notes.md" };
  fake.exec = () => jl(header, logLine("c1"), { type: "splice", at: 0, count: 4 }, user("u1", "Fix `a.rs`, please"), asst("a1", reply),
    note, { type: "row", id: "s1", k: "status", text: "Interrupted" }, end);
  const cx = fakeCx(); const v = new ChatView(cx); await cx.settle(); v.width = 900;
  v.render(cx);
  const copy = (key) => {
    const n = log.overlays.length;
    lastList._h.on_item_secondary_click(key, { button: "right", position: { x: 0, y: 200 }, local_position: { x: 50, y: 200 } }, cx);
    if (log.overlays.length === n) return null;
    const items = log.overlays.at(-1).props.items;
    fake.props = { view: "menu", items, overlay: "o1" };
    let copied = null;
    const m = new MenuView(fakeCx()).render(fakeCx());
    byId(m, `menu-${items.findIndex((i) => i.label === "Copy message")}`)._h.on_click({}, { ...fakeCx(), write_to_clipboard: (s) => { copied = s; } });
    return copied;
  };
  assert.equal(copy("u1"), "Fix `a.rs`, please");
  assert.equal(copy("a1"), reply);
  assert.equal(copy("n1"), "Notes updated.\nSee notes.md");
  assert.equal(copy("s1"), null, "a status row has no menu");
});

test("echoes: a sent text shows after the rows until the log has it; a repeat waits for its own row; old ones go", () => {
  const f = emptyFeed("t1");
  applyFeed(f, parseAnswer(jl(header, logLine("c1"), { type: "splice", at: 0, count: 1 }, user("u1", "go on"), end).stdout));
  const a = addEcho(f, "go  on\n", 1000), b = addEcho(f, "and test", 1000);
  assert.deepEqual(shownRows(f).map((r) => r.id), ["u1", a.id, b.id]);
  assert.equal(settleEchoes(f, 2000), false, "the old \"go on\" is not this one");
  applyFeed(f, parseAnswer(jl(header, logLine("c2"), { type: "splice", at: 1, count: 1 }, user("u2", "go on"), end).stdout));
  assert.equal(settleEchoes(f, 2000), true);
  assert.deepEqual(shownRows(f).map((r) => r.id), ["u1", "u2", b.id]);
  assert.equal(settleEchoes(f, 1000 + ECHO_MS), true, "an echo the log never shows goes after ECHO_MS");
  assert.deepEqual(shownRows(f).map((r) => r.id), ["u1", "u2"]);
  const c = addEcho(f, "x", 0);
  assert.equal(dropEcho(f, c), true); assert.equal(dropEcho(f, c), false);
});

test("an echo with a pasted image path goes when the log shows the path as [Image #N]", () => {
  const f = emptyFeed("t1");
  const path = "~/.config/herdr-app/herdr-contr/paste/paste-20261112-120834-E3B9D8.png";
  const a = addEcho(f, path, 1000), b = addEcho(f, `look at  this ${path}`, 1000), c = addEcho(f, "plain text", 1000);
  applyFeed(f, parseAnswer(jl(header, logLine("c1"), { type: "splice", at: 0, count: 0 }, user("u1", "[Image #2]"), user("u2", "look at this [Image #1]"), user("u3", "plain  text?"), end).stdout));
  assert.equal(settleEchoes(f, 2000), true);
  assert.deepEqual(shownRows(f).map((r) => r.id), ["u1", "u2", "u3", c.id], "normal text still needs the same text");
  const d = addEcho(f, `see ${path}`, 3000);
  applyFeed(f, parseAnswer(jl(header, logLine("c2"), { type: "splice", at: 3, count: 0 }, user("u4", "look [Image #3]"), end).stdout));
  assert.equal(settleEchoes(f, 4000), false, "the rest of the text must be equal");
  assert.ok(shownRows(f).includes(d)); void a; void b;
});

test("a sent message shows at once, dimmed, and the chat reads the feed again and fast until the log has it", async () => {
  let logged = false, feeds = 0, sends = 0, result = "sent";
  fake.exec = (_c, args) => {
    if (args[0] === "send") { sends++; return jl({ ...header, command: "send" }, { type: "done", result }, end); }
    if (args[0] !== "feed") return jl({ ...header, command: args[0] }, end);
    feeds++;
    return jl(header, logLine("c1"), { type: "splice", at: 0, count: 2 }, user("u1", "hi"), ...(logged ? [user("u2", "Fix the tests")] : []), end);
  };
  const cx = fakeCx(); const v = new ChatView(cx); await cx.settle(); v.width = 900;
  const b = v.composer.box(fake.agents[0]);
  b.input.set_value("Fix the tests");
  const before = feeds;
  await v.composer.submit(cx); await cx.settle();
  assert.equal(sends, 1);
  assert.ok(feeds > before, "the feed is read again when the send is done");
  v.render(cx);
  assert.equal(lastList._args.n, 2);
  const echo = lastList._args.render({ start: 1, end: 2 })[0];
  assert.match(textOf(echo), /Fix the tests/);
  // While it waits, the next tick reads the feed after CHECK_MS, not POLL_MS.
  v.lastPoll = Date.now() - 500; const n = feeds;
  await v.tick(cx); await cx.settle();
  assert.equal(feeds, n + 1);
  // The log has it now: the echo goes and the logged row takes its place.
  logged = true; v.lastPoll = 0;
  await v.tick(cx); await cx.settle();
  v.render(cx);
  assert.equal(lastList._args.n, 2);
  assert.deepEqual(lastList._args.render({ start: 0, end: 2 }).map(textOf).map((s) => /Fix the tests/.test(s)), [false, true]);
  assert.equal(v.feed().echoes.length, 0);
  // A failed send: the echo goes and the text is back in the box.
  result = "failed";
  b.input.set_value("Nope");
  await v.composer.submit(cx); await cx.settle();
  assert.equal(v.feed().echoes.length, 0);
  assert.equal(b.input.value(), "Nope");
});

test("while the agent works, its working line is above the input, not in the feed; it goes when the agent stops or the helper has none", async () => {
  forgetCommands();
  let line = { type: "working", text: "Improvising…", detail: "1m 16s · ↓ 6.0k tokens · thought for 1s" }, version = ["feed", "working"];
  fake.exec = (_c, args) => {
    if (args[0] === "version") return jl({ ...header, command: "version" }, { type: "version", commands: version }, end);
    if (args[0] === "working") return jl({ ...header, command: "working" }, line, end);
    if (args[0] === "feed") return jl(header, logLine("c1"), { type: "splice", at: 0, count: 1 }, user("u1", "hi"), end);
    return jl({ ...header, command: args[0] }, end);
  };
  const cx = fakeCx(); const v = new ChatView(cx); await cx.settle(); v.width = 900;
  await v.tick(cx); await cx.settle();
  const work = () => byId(v.render(cx), "working");
  assert.match(textOf(work()), /Improvising….*\(1m 16s · ↓ 6\.0k tokens · thought for 1s\)/);
  // Read again after WORK_MS: the new time shows.
  line = { ...line, detail: "1m 17s" }; v.lastWork = 0;
  await v.tick(cx); await cx.settle();
  assert.match(textOf(work()), /\(1m 17s\)/);
  assert.equal(lastList._args.n, 1, "the feed has no working row (#47)");
  // No line on screen: the row goes.
  line = { type: "working", text: null, detail: null }; v.lastWork = 0;
  await v.tick(cx); await cx.settle(); 
  assert.equal(work(), null);
  // The agent stops: no row, and the helper is not asked.
  line = { type: "working", text: "Brewing…", detail: "" }; v.lastWork = 0;
  await v.tick(cx); await cx.settle(); 
  assert.ok(work());
  fake.agents = [agent("t1", "idle")];
  const asked = log.exec.filter(([, a]) => a[0] === "working").length;
  v.lastWork = 0; await v.tick(cx); await cx.settle(); 
  assert.equal(work(), null);
  assert.equal(log.exec.filter(([, a]) => a[0] === "working").length, asked + 1, "one read when the agent goes idle (#61)");
  // Idle with no background note: not asked again.
  v.lastWork = 0; await v.tick(cx); await cx.settle();
  assert.equal(log.exec.filter(([, a]) => a[0] === "working").length, asked + 1);
  // An older helper without the command: never asked.
  forgetCommands(); version = ["feed"]; fake.agents = [agent("t1", "working")];
  const before = log.exec.filter(([, a]) => a[0] === "working").length;
  v.lastWork = 0; await v.tick(cx); await cx.settle(); 
  assert.equal(work(), null);
  assert.equal(log.exec.filter(([, a]) => a[0] === "working").length, before);
  forgetCommands();
});

test("Claude Code's background note shows while the agent is idle; it is read slowly and only while it shows (#61)", async () => {
  forgetCommands();
  let lines = [{ type: "working", text: null, detail: null }, { type: "background", text: "1 shell still running" }];
  fake.exec = (_c, args) => {
    if (args[0] === "version") return jl({ ...header, command: "version" }, { type: "version", commands: ["feed", "working"] }, end);
    if (args[0] === "working") return jl({ ...header, command: "working" }, ...lines, end);
    if (args[0] === "feed") return jl(header, logLine("c1"), { type: "splice", at: 0, count: 1 }, user("u1", "hi"), end);
    return jl({ ...header, command: args[0] }, end);
  };
  fake.agents = [agent("t1", "idle")];
  const reads = () => log.exec.filter(([, a]) => a[0] === "working").length;
  const cx = fakeCx(); const v = new ChatView(cx); await cx.settle(); v.width = 900;
  const start = reads();
  await v.tick(cx); await cx.settle();
  const bg = () => byId(v.render(cx), "background");
  assert.match(textOf(bg()), /1 shell still running/);
  assert.equal(reads(), start + 1, "one read when the agent is shown idle");
  // Not again before BACKGROUND_MS.
  await v.tick(cx); await cx.settle();
  assert.equal(reads(), start + 1);
  // After it: read again while the note shows.
  lines = [lines[0], { type: "background", text: "2 shells still running" }]; v.lastWork = 0;
  await v.tick(cx); await cx.settle();
  assert.equal(reads(), start + 2);
  assert.match(textOf(bg()), /2 shells still running/);
  // The note goes: it is not drawn, and the reads stop.
  lines = [lines[0]]; v.lastWork = 0;
  await v.tick(cx); await cx.settle();
  assert.equal(bg(), null);
  v.lastWork = 0; await v.tick(cx); await cx.settle();
  assert.equal(reads(), start + 3, "no reads while nothing runs");
  forgetCommands();
});

test("a done agent is read for its background note like an idle one, and the note stays (#61)", async () => {
  forgetCommands();
  let lines = [{ type: "working", text: "Brewing…", detail: "" }];
  fake.exec = (_c, args) => {
    if (args[0] === "version") return jl({ ...header, command: "version" }, { type: "version", commands: ["feed", "working"] }, end);
    if (args[0] === "working") return jl({ ...header, command: "working" }, ...lines, end);
    if (args[0] === "feed") return jl(header, logLine("c1"), { type: "splice", at: 0, count: 1 }, user("u1", "hi"), end);
    return jl({ ...header, command: args[0] }, end);
  };
  fake.agents = [agent("t1", "working")];
  const reads = () => log.exec.filter(([, a]) => a[0] === "working").length;
  const cx = fakeCx(); const v = new ChatView(cx); await cx.settle(); v.width = 900;
  await v.tick(cx); await cx.settle();
  const bg = () => byId(v.render(cx), "background");
  // The turn ends with a shell still running: one read on done.
  lines = [{ type: "working", text: null, detail: null }, { type: "background", text: "1 shell still running" }];
  fake.agents = [agent("t1", "done")];
  const start = reads();
  await v.tick(cx); await cx.settle();
  assert.equal(reads(), start + 1, "one read when the agent is done");
  assert.match(textOf(bg()), /1 shell still running/);
  // Still done: read again every BACKGROUND_MS while the note shows, and the note is not cleared.
  v.lastWork = 0; await v.tick(cx); await cx.settle();
  assert.equal(reads(), start + 2);
  assert.ok(bg());
  // Done to idle is no new read by itself.
  fake.agents = [agent("t1", "idle")]; lines = [lines[0]];
  await v.tick(cx); await cx.settle();
  assert.equal(reads(), start + 2);
  forgetCommands();
});

test("the reply box is as wide as the chat's column", async () => {
  fake.exec = (_c, args) => jl(header, logLine("c1"), { type: "splice", at: 0, count: 1 }, user("u1", "hi"), end);
  const cx = fakeCx(); const v = new ChatView(cx); await cx.settle();
  for (const w of [1400, 500]) {
    v.width = w;
    const el = v.render(cx);
    assert.equal(byId(el, "box")._a.max_w, v.colWidth(), `pane ${w}`);
    assert.equal(byId(el, "tags-row")._a.max_w, v.colWidth());
    // A card above the box: as wide as the box.
    v.cards.entry(v.address).card = { kind: "approval", request: "toolu_1", title: "Bash command", command: "ls", tool: "Bash" };
    assert.equal(byId(v.render(cx), "chat-card")?._a.max_w, v.colWidth(), `card, pane ${w}`);
    v.cards.entry(v.address).card = null;
  }
});
