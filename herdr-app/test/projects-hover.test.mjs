// The Projects list's agent rows on hover (Stop, "Open in a new tab") and the model tooltip of agent and
// worker rows, from herdr-contr's helper (#98).
import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { fake, log, reset, theme } from "./stub/herdr.mjs";
import { byId } from "./stub/ui.mjs";
import { fakeCx } from "./fake-cx.mjs";
import Spaces, { forgetHelper } from "../mods/herdr-contr-spaces/main.js";
import { modelName, modelTip, modelDue, MODEL_MS } from "../mods/herdr-contr-spaces/projects.js";

const store = new Map();
Object.defineProperty(globalThis, "localStorage", { configurable: true,
  value: { getItem: (k) => store.get(k) ?? null, setItem: (k, v) => store.set(k, v), removeItem: (k) => store.delete(k) } });

const claude = { name: "claude", key: "claude", agent: true, icon: "mods/x/claude.svg", tint: true };
const agent = (address, o = {}) => ({ address, pane: `p-${address}`, tab: "tb-s1", space: "s1", space_name: "s1", status: "idle", seq: 1, program: claude, title: address, ...o });
const all = (el) => (el && typeof el === "object" ? [el, ...(el.kids ?? []).flatMap(all)] : []);
const ids = (el) => all(el).map((e) => e._a?.id).filter(Boolean);
const settle = () => new Promise((r) => setTimeout(r, 0));
const modelCalls = () => log.exec.filter(([, a]) => a[0] === "model");

beforeEach(() => {
  reset();
  store.clear();
  forgetHelper();
  fake.spaces = [{ id: "s1", name: "contr", folder: "/p/contr", active: true, tabs: [{ id: "tb-s1", name: "1", named: false }] }];
  fake.agents = [agent("a1", { status: "working" }), agent("a2", { status: "blocked" })];
  fake.terminals = [{ address: "a1", state: "live" }, { address: "a2", state: "live" },
    { address: "w1", parent: "a1", state: "live", name: "builder", status: "working", program: claude }];
  store.set("changed", JSON.stringify({ a2: { s: "blocked", at: Date.now() - 120000 } }));
});

const make = () => { const cx = fakeCx(); const v = new Spaces(); v.init({}, cx); return { v, cx }; };
const ev = () => Object.assign(fakeCx(), { stop_propagation() { this.stopped = true; } });

function helperAnswers(byTerminal, has = true) {
  fake.exec = (_cmd, args) => {
    if (args[0] === "version") return { code: 0, stdout: JSON.stringify({ type: "version", commands: has ? ["model"] : [] }) + "\n" };
    if (args[0] === "model") {
      const a = byTerminal[args[2]];
      return { code: 0, stdout: a ? JSON.stringify({ type: "model", ...a }) + "\n" : "" };
    }
    return { code: 1, stdout: "" };
  };
}

test("modelName and modelTip: 'Opus 5.5 · medium effort'; no model, no tooltip", () => {
  assert.equal(modelName("claude-opus-5-5"), "Opus 5.5");
  assert.equal(modelName("claude-sonnet-5-5-20261001"), "Sonnet 5.5");
  assert.equal(modelName("claude-3-5-sonnet-20241022"), "Sonnet 3.5");
  assert.equal(modelName("gpt-6"), "GPT-6");
  assert.equal(modelName("opus"), "Opus");
  assert.equal(modelName("kimi-k2"), "kimi-k2");
  assert.equal(modelTip("claude-opus-5-5", "medium"), "Opus 5.5 · medium effort");
  assert.equal(modelTip("gpt-6", null), "GPT-6");
  assert.equal(modelTip(null, "low"), "");
  assert.equal(modelTip("", ""), "");
});

test("modelDue: a new terminal, then a changed status, never twice in MODEL_MS, not while asking", () => {
  assert.equal(modelDue(undefined, "idle", 0), true);
  const m = { status: "idle", at: 1000, busy: false };
  assert.equal(modelDue(m, "idle", 1000 + 10 * MODEL_MS), false, "same status");
  assert.equal(modelDue(m, "working", 1000 + MODEL_MS - 1), false, "too soon");
  assert.equal(modelDue(m, "working", 1000 + MODEL_MS), true);
  assert.equal(modelDue({ ...m, busy: true }, "working", 1000 + MODEL_MS), false);
});

test("hover: the age and the mark hide, Stop and 'Open in a new tab' show, the icon is at full opacity", () => {
  const { v } = make();
  let row = byId(v.render(), "ag-a2");
  const line = () => row.kids[0];
  assert.equal(line().kids[0]._a.opacity, 0.45);
  assert.ok(line().kids.some((k) => k._a?.font_family === "Menlo" && k._a?.text_size === 11), "the age");
  assert.ok(line().kids.some((k) => k._a?.loop), "the waiting mark");
  assert.ok(!ids(row).includes("stop-a2"));

  const cx = ev();
  row._h.on_hover(true, cx);
  assert.equal(cx.notified, 1);
  row = byId(v.render(), "ag-a2");
  assert.equal(line().kids[0]._a.opacity, undefined, "full opacity");
  assert.ok(!line().kids.some((k) => k._a?.font_family === "Menlo"), "no age, no mark");
  const box = row.kids.at(-1);
  assert.ok("absolute" in box._a);
  assert.ok("relative" in row._a);
  assert.deepEqual([box._a.top, box._a.right, box._a.gap], [4, 6, 2]);
  const stop = byId(row, "stop-a2"), open = byId(row, "open-tab-a2");
  for (const b of [stop, open]) {
    assert.deepEqual([b._a.size, b._a.rounded, b._a.text_color], [22, 6, "#muted"]);
    assert.equal(b.kids[0]._a.size, 12);
  }
  // The title line keeps the controls' place free (#123): 2 × 22 + 2 px, less the 4 px nearer to the edge.
  assert.deepEqual([byId(row, "ctl-space-a2")._a.w, "flex_none" in byId(row, "ctl-space-a2")._a], [42, true]);
  assert.ok(!ids(byId(v.render(), "ag-a1")).includes("ctl-space-a1"));
  assert.equal(stop._a.tooltip, "Stop");
  assert.equal(open._a.tooltip, "Open in a new tab");
  assert.ok(!ids(byId(v.render(), "ag-a1")).includes("stop-a1"), "only the row under the pointer");

  row._h.on_hover(false, cx);
  assert.ok(!ids(byId(v.render(), "ag-a2")).includes("stop-a2"));
});

test("Stop kills the agent's terminal; 'Open in a new tab' gives it a pane in a tab; neither focuses the row", () => {
  const { v } = make();
  byId(v.render(), "ag-a1")._h.on_hover(true, ev());
  const row = byId(v.render(), "ag-a1");
  const e1 = ev();
  byId(row, "stop-a1")._h.on_click(null, e1);
  assert.ok(e1.stopped);
  assert.deepEqual(log.run.at(-1), ["terminal.kill", { terminal: "a1" }]);
  assert.equal(v.hovered, null, "the row can go from under the pointer");
  v.hovered = "a1";
  const e2 = ev();
  byId(row, "open-tab-a1")._h.on_click(null, e2);
  assert.ok(e2.stopped);
  assert.deepEqual(log.run.at(-1), ["terminal.keep", { terminal: "a1", place: "tab" }]);
  assert.ok(!log.run.some(([n]) => n === "pane.focus"));
});

test("a hover-out of another row does not clear the hovered row", () => {
  const { v } = make();
  const el = v.render();
  const cx = ev();
  byId(el, "ag-a1")._h.on_hover(true, cx);
  byId(el, "ag-a2")._h.on_hover(false, cx);
  assert.equal(v.hovered, "a1");
  assert.equal(cx.notified, 1);
});

test("the model tooltip on agent and worker rows, asked from the timer and never from a draw", async () => {
  helperAnswers({ a1: { model: "claude-opus-5-5", effort: "medium" }, w1: { model: "claude-sonnet-5-5", effort: "low" } });
  const { v, cx } = make();
  v.render(); v.render();
  assert.equal(modelCalls().length, 0, "a draw asks nothing");
  const first = cx.timers.find((x) => x.once && x.ms === 0);
  const every = cx.timers.find((x) => !x.once && x.ms === MODEL_MS);
  assert.ok(first && every, "a first ask, then every 15 s");
  await first.f(cx);
  assert.deepEqual(modelCalls().map(([, a]) => a.slice(0, 3)).sort(), [["model", "--terminal", "a1"], ["model", "--terminal", "a2"], ["model", "--terminal", "w1"]]);
  assert.ok(cx.notified >= 1);
  const el = v.render();
  assert.equal(byId(el, "ag-a1")._a.tooltip, "Opus 5.5 · medium effort · ⌃1", "a main agent: its key at the end");
  assert.equal(byId(el, "sub-w1")._a.tooltip, "Sonnet 5.5 · low effort", "a worker has no key");
  assert.equal(byId(el, "ag-a2")._a.tooltip, "⌃2", "no model known: only the key");

  // The same status: not asked again. A new status: asked again, once MODEL_MS went by.
  await v.askModels(cx, Date.now() + MODEL_MS);
  assert.equal(modelCalls().length, 3);
  fake.agents[0].status = "idle";
  await v.askModels(cx, Date.now() + 1000);
  assert.equal(modelCalls().length, 3, "too soon");
  await v.askModels(cx, Date.now() + MODEL_MS + 1);
  assert.equal(modelCalls().length, 4);
});

test("a helper without the model command is not asked for models; a terminal that went is forgotten", async () => {
  helperAnswers({}, false);
  const { v, cx } = make();
  await v.askModels(cx);
  assert.equal(modelCalls().length, 0);
  assert.equal(byId(v.render(), "ag-a1")._a.tooltip, "⌃1");
  assert.equal(byId(v.render(), "sub-w1")._a.tooltip, undefined, "a worker with no model: no tooltip");
  fake.agents = [fake.agents[0]];
  fake.terminals = [{ address: "a1", state: "live" }];
  await v.askModels(cx);
  assert.deepEqual([...v.models.keys()], ["a1"]);
});
