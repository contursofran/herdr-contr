// The sidebar's agent rows: the agent's model and effort, from the helper's `model` command.
import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { fake, reset, log, theme } from "./stub/herdr.mjs";
import { byId } from "./stub/ui.mjs";
import Agents, { modelText, forgetHelper } from "../mods/herdr-contr-agents/main.js";

const store = new Map();
Object.defineProperty(globalThis, "localStorage", { configurable: true,
  value: { getItem: (k) => store.get(k) ?? null, setItem: (k, v) => store.set(k, v), removeItem: (k) => store.delete(k) } });
beforeEach(() => { reset(); store.clear(); forgetHelper(); });
const make = (cx) => { const v = new Agents(); v.init({}, cx); return v; };

const settle = () => new Promise((r) => setTimeout(r, 0));
const all = (el) => (el && typeof el === "object" ? [el, ...(el.kids ?? []).flatMap(all)] : []);
const modelEls = (row) => all(row).filter((e) => e._a?.text_size === 11 && e._a?.text_color === "#muted");
const text = (el) => (typeof el === "string" ? el : (el?.kids ?? []).map(text).join(""));

test("modelText: family and version, then the effort; unknown parts left out", () => {
  assert.equal(modelText("claude-opus-5-5", "low"), "Opus 5.5 · low");
  assert.equal(modelText("claude-sonnet-5-5-20261001", "medium"), "Sonnet 5.5 · medium");
  assert.equal(modelText("claude-haiku-4", null), "Haiku 4");
  assert.equal(modelText("claude-3-5-sonnet-20241022", null), "Sonnet 3.5");
  assert.equal(modelText("gpt-6", "high"), "GPT-6 · high");
  assert.equal(modelText("gpt-5.1-codex-max", "xhigh"), "GPT-5.1 · xhigh");
  assert.equal(modelText("opus", null), "Opus");
  assert.equal(modelText(null, "low"), "low");
  assert.equal(modelText(null, null), "");
  assert.equal(modelText("kimi-k2", ""), "kimi-k2");
});

function setup(answer) {
  fake.agents = [{ address: "t1", pane: "p1", tab: "tb1", space_name: "proj", status: "idle", seq: 1, program: { name: "claude" } }];
  fake.exec = (_cmd, args) => {
    if (args[0] === "version") return { code: 0, stdout: JSON.stringify({ type: "version", commands: answer ? ["model"] : [] }) + "\n" };
    if (args[0] === "model") return { code: 0, stdout: JSON.stringify(answer) + "\n" };
    return { code: 1, stdout: "" };
  };
}

test("a row shows the model and effort on a second line, once the helper answers", async () => {
  setup({ type: "model", agent: "claude", model: "claude-opus-5-5", effort: "low" });
  let notified = 0;
  const cx = { notify: () => notified++, timer: { every() {} } };
  const v = make(cx);
  v.render(cx);
  await settle(); await settle();
  assert.equal(notified, 1);
  const r = byId(v.render(cx), "ag-t1");
  assert.equal(r.kids.length, 2, "line 1 and line 2");
  assert.equal(r.kids[0]._a.h, 26);
  assert.ok(!text(r.kids[0]).includes("Opus 5.5"), "not on line 1");
  assert.ok(text(r.kids[1]).includes("Opus 5.5 · low"));
  const calls = log.exec.filter(([, a]) => a[0] === "model");
  assert.deepEqual(calls[0][1].slice(0, 3), ["model", "--terminal", "t1"]);
  assert.equal(calls.length, 1, "same status: not asked again");
});

for (const [why, answer] of [["both parts unknown", { type: "model", model: null, effort: null }], ["no model command", null]]) {
  test(`nothing shows when ${why}`, async () => {
    setup(answer);
    const v = make({ notify() {}, timer: { every() {} } });
    v.render({ notify() {} }); await settle(); await settle();
    const r = byId(v.render({ notify() {} }), "ag-t1");
    assert.deepEqual(modelEls(r), []);
    assert.equal(r.kids.length, 1, "one line");
    assert.equal(r.kids[0]._a.h, 30, "today's height");
    if (!answer) assert.equal(log.exec.filter(([, a]) => a[0] === "model").length, 0, "checks hasCommand first");
  });
}

test("a new status asks again, after MODEL_MS", async () => {
  setup({ type: "model", agent: "claude", model: "gpt-6", effort: "high" });
  const v = make({ notify() {}, timer: { every() {} } });
  const cx = { notify() {} };
  v.pollModel(cx, "t1", "idle", 1000); await settle(); await settle();
  v.pollModel(cx, "t1", "working", 2000); await settle();
  assert.equal(log.exec.filter(([, a]) => a[0] === "model").length, 1, "too soon");
  v.pollModel(cx, "t1", "working", 20000); await settle(); await settle();
  assert.equal(log.exec.filter(([, a]) => a[0] === "model").length, 2);
  assert.equal(v.models.t1.text, "GPT-6 \u00b7 high");
});

test("a sub-agent row shows its model too", async () => {
  setup({ type: "model", agent: "claude", model: "claude-opus-5-5", effort: "low" });
  const v = make({ notify() {}, timer: { every() {} } });
  v.listed = new Map([["t2", "idle"]]);
  const sub = () => v.subRow(theme(), { t: { address: "t2", name: "b-h47", status: "idle" }, lines: [] }, null, false, { notify() {} });
  sub(); await settle(); await settle();
  assert.equal(text(modelEls(sub())[0]), "Opus 5.5 \u00b7 low");
});

test("a sub-agent in its own terminal, without a pane, shows its model (#65)", async () => {
  setup({ type: "model", agent: "claude", model: "claude-opus-5-5", effort: "low" });
  const v = make({ notify() {}, timer: { every() {} } });
  v.listed = new Map();
  const sub = () => v.subRow(theme(), { t: { address: "t9", name: "b-1", status: "working", parent: "t1" }, lines: ["└"] }, null, false, { notify() {} });
  sub(); await settle(); await settle();
  assert.deepEqual(log.exec.filter(([, a]) => a[0] === "model").map(([, a]) => a.slice(0, 3)), [["model", "--terminal", "t9"]]);
  assert.equal(text(modelEls(sub())[0]), "Opus 5.5 \u00b7 low");
  sub(); sub(); await settle();
  assert.equal(log.exec.filter(([, a]) => a[0] === "model").length, 1);
});

test("a sub-agent row with a model has two lines, and the tree line goes on through line 2", async () => {
  setup({ type: "model", agent: "claude", model: "claude-opus-5-5", effort: "low" });
  const v = make({ notify() {}, timer: { every() {} } });
  v.models = { t1: { text: "Opus 5.5 \u00b7 low", status: "idle", at: Date.now(), busy: false } };
  v.listed = new Map([["t1", "idle"]]);
  const r = v.subRow(theme(), { t: { address: "t1", name: "w", status: "idle" }, lines: ["├"] }, null, false, { notify() {} });
  assert.equal(r.kids.length, 2, "line 1 and line 2");
  assert.equal(r.kids[0]._a.h, 26);
  assert.equal(r.kids[1]._a.h, 14);
  assert.ok(text(r.kids[1]).includes("Opus 5.5"));
  assert.ok(!text(r.kids[0]).includes("Opus 5.5"), "not on line 1");
  // The tree line goes on through line 2 under a "├"; it stops under a "└".
  assert.equal(r.kids[1].kids[0]._a.border_l, 1.5);
  const last = v.subRow(theme(), { t: { address: "t1", name: "w", status: "idle" }, lines: ["└"] }, null, false, { notify() {} });
  assert.equal(last.kids[1].kids[0]._a.border_l, undefined);
  v.models.t1.text = "";
  const one = v.subRow(theme(), { t: { address: "t1", name: "w", status: "idle" }, lines: ["├"] }, null, false, { notify() {} });
  assert.equal(one.kids.length, 1);
  assert.equal(one.kids[0]._a.h, 26);
});
