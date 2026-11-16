// The reply box takes the keyboard when the chat shows (#204): on another agent, on the chat screen shown again,
// on the window made active. Not on a redraw or a new message, and not while the owner is busy in the chat.
import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { fake, reset } from "./stub/herdr.mjs";
import { calls } from "./stub/ui.mjs";
import { fakeCx } from "./fake-cx.mjs";
import { agent, space } from "./fixtures.mjs";
import { ChatView, PRESS_MS } from "../mods/herdr-contr/chat.js";
import { Composer } from "../mods/herdr-contr/composer.js";

const jl = (...lines) => ({ code: 0, stdout: lines.map((l) => JSON.stringify(l)).join("\n") + "\n", stderr: "" });
const header = { type: "header", protocol: 1, sidecar: "0.1.0", command: "feed" };
const end = { type: "end", ok: true };
const logLine = (cursor) => ({ type: "log", agent: "claude", path: "/logs/s.jsonl", start: 0, size: 10, cursor, model: "claude-opus-5-5" });
const row = (id, text) => ({ type: "row", id, k: "assistant", text, ts: 2 });

// A cx whose focus handles remember a focus() until another handle takes it.
function focusCx() {
  const cx = fakeCx(), handles = [];
  cx.focus_handle = () => {
    const h = { on: false, focus() { handles.forEach((o) => { o.on = false; }); h.on = true; }, is_focused() { return h.on; } };
    handles.push(h);
    return h;
  };
  return cx;
}

let n = 0;
beforeEach(() => {
  reset(); calls.length = 0;
  fake.window = { ...fake.window, active: true };
  fake.spaces = [space()];
  fake.agents = [agent("t1", "idle", { title: "fix login" }), agent("t2", "idle", { title: "write docs" })];
  n = 0;
  fake.exec = (_c, args) => { n++; return jl(header, logLine("c" + n), { type: "splice", at: 0, count: n }, row("a" + n, "message " + n), end); };
});

// A chat that has had its first tick on the chat screen (that tick is a "chat shows" change too).
async function shownChat() {
  const cx = focusCx(); const v = new ChatView(cx); await cx.settle(); v.width = 900;
  await v.tick(cx); await cx.settle();
  return { cx, v };
}
const focusCalls = (v) => v.composer.input.focusCalls;

// Makes the window inactive and active again, with `setup` run between: the activation is the change.
async function activate(v, cx, setup) {
  fake.window = { ...fake.window, active: false }; await v.tick(cx); await cx.settle();
  setup?.();
  const before = focusCalls(v);
  fake.window = { ...fake.window, active: true }; await v.tick(cx);
  return focusCalls(v) - before;
}

test("the first tick on the chat screen puts the keyboard in the reply box", async () => {
  const { v } = await shownChat();
  assert.equal(focusCalls(v), 1);
});

test("another agent's chat puts the keyboard in the reply box, in the text state made for its name", async () => {
  const { cx, v } = await shownChat();
  const old = v.composer.input;
  fake.shared["herdr-contr.agent"] = "t2"; await v.tick(cx); await cx.settle();
  assert.equal(v.address, "t2");
  assert.notEqual(v.composer.input, old, "the new name made a new text state");
  assert.equal(focusCalls(v), 1, "the keyboard goes to the new state, not the released one");
});

test("Terminal → Chat puts the keyboard in the reply box; Chat → Terminal and an agent switch on Terminal do not", async () => {
  const { cx, v } = await shownChat();
  const at = focusCalls(v);
  fake.shared["herdr-contr.screen"] = "tab"; await v.tick(cx); await cx.settle();
  assert.equal(focusCalls(v), at, "Chat → Terminal gives no focus");
  fake.shared["herdr-contr.agent"] = "t2"; await v.tick(cx); await cx.settle();
  assert.equal(focusCalls(v), 0, "the new name's text state got no focus while the terminal shows");
  fake.shared["herdr-contr.screen"] = "chat"; await v.tick(cx); await cx.settle();
  assert.equal(focusCalls(v), 1);
});

test("the window made active with the chat shown puts the keyboard in the reply box; inactive does not", async () => {
  const { cx, v } = await shownChat();
  const at = focusCalls(v);
  fake.window = { ...fake.window, active: false }; await v.tick(cx); await cx.settle();
  assert.equal(focusCalls(v), at, "going inactive gives no focus");
  fake.window = { ...fake.window, active: true }; await v.tick(cx); await cx.settle();
  assert.equal(focusCalls(v), at + 1);
});

test("the window made active on the terminal screen gives no focus", async () => {
  const { cx, v } = await shownChat();
  fake.shared["herdr-contr.screen"] = "tab"; await v.tick(cx); await cx.settle();
  assert.equal(await activate(v, cx), 0);
});

test("redraws, ticks and new messages do not take the keyboard", async () => {
  const { cx, v } = await shownChat();
  const at = focusCalls(v);
  for (let i = 0; i < 4; i++) { v.lastPoll = 0; await v.tick(cx); await cx.settle(); v.render(cx); }
  assert.ok(n >= 4, "new messages came in");
  assert.equal(focusCalls(v), at);
});

test("no focus steal while a question or approval card of the shown agent has the keyboard", async () => {
  const { cx, v } = await shownChat();
  assert.equal(await activate(v, cx, () => { v.cards.byAgent.set("t1", { card: { request: "r1" } }); v.cards.focus.focus(); }), 0);
});

test("no focus steal while the owner types in a card's own-answer box", async () => {
  const { cx, v } = await shownChat();
  assert.equal(await activate(v, cx, () => { v.cards.byAgent.set("t1", { card: { request: "r1" } }); v.cards.typing = v.cards.others[0]; }), 0);
});

test("no focus steal while a terminal panel card has the keyboard", async () => {
  const { cx, v } = await shownChat();
  assert.equal(await activate(v, cx, () => { v.panel.byAgent.set("t1", { lines: ["1. Yes"] }); v.panel.focus.focus(); }), 0);
});

test("no focus steal while a question of the coordinator has the keyboard", async () => {
  const { cx, v } = await shownChat();
  assert.equal(await activate(v, cx, () => { v.owner.byAgent.set("t1", { card: { request: "q1" } }); v.owner.focus.focus(); }), 0);
});

test("a focused card handle with no card for the shown agent does not hold the keyboard", async () => {
  const { cx, v } = await shownChat();
  assert.equal(await activate(v, cx, () => { v.cards.focus.focus(); }), 1, "the question card's handle");
  assert.equal(await activate(v, cx, () => { v.panel.focus.focus(); }), 1, "the panel card's handle");
  assert.equal(await activate(v, cx, () => { v.owner.focus.focus(); }), 1, "the coordinator question's handle");
});

test("no focus steal while the picture viewer is open", async () => {
  const { cx, v } = await shownChat();
  assert.equal(await activate(v, cx, () => { v.viewer.list = [{ row: "a1" }]; v.viewer.at = 0; }), 0);
});

test("no focus steal while a left press in the chat (a text selection) is under way; an old press no longer counts", async () => {
  const { cx, v } = await shownChat();
  assert.equal(await activate(v, cx, () => { v.press = { x: 0, y: 0, moved: true, at: Date.now() }; }), 0);
  assert.equal(await activate(v, cx, () => { v.press = { x: 0, y: 0, moved: true, at: Date.now() - PRESS_MS - 1 }; }), 1);
});

test("Composer.focus: true when the box took the keyboard, false when the input can not take it", () => {
  const c = new Composer(fakeCx());
  assert.equal(c.focus(), true);
  assert.equal(c.input.focusCalls, 1);
  c.input = { value: () => "" };
  assert.equal(c.focus(), false);
});
