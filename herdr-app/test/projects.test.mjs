// The Projects list (the herdr-contr-spaces copy): each project with its agents, an orchestrator's
// workers under it, and the projects with no agent in one "N more projects" row.
import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { fake, log, reset, theme } from "./stub/herdr.mjs";
import { byId, textOf } from "./stub/ui.mjs";
import { fakeCx } from "./fake-cx.mjs";
import Spaces from "../mods/herdr-contr-spaces/main.js";
import { split, countLine, moreLine, agentLabel, bySpace, workers, workerWork } from "../mods/herdr-contr-spaces/projects.js";

const store = new Map();
Object.defineProperty(globalThis, "localStorage", { configurable: true,
  value: { getItem: (k) => store.get(k) ?? null, setItem: (k, v) => store.set(k, v), removeItem: (k) => store.delete(k) } });

const claude = { name: "claude", key: "claude", agent: true, icon: "mods/x/claude.svg", tint: true };
const sp = (id, name, o = {}) => ({ id, name, folder: `/p/${name}`, active: false, tabs: [{ id: `tb-${id}`, name: "1", named: false }], ...o });
const agent = (address, space, o = {}) => ({ address, pane: `p-${address}`, tab: `tb-${space}`, space, space_name: space, status: "idle", seq: 1, program: claude, title: address, ...o });

beforeEach(() => {
  reset();
  store.clear();
  fake.spaces = [sp("s1", "contr", { active: true }), sp("s2", "play"), sp("s3", "docs"), sp("s4", "site")];
  fake.agents = [
    agent("a1", "s1", { title: "Flockbook herdr-app", status: "working", focused: true }),
    agent("w1", "s1", { title: "w1 own pane" }),
    agent("a2", "s1", { title: "Old folders", status: "done" }),
    agent("a3", "s2", { title: "KZ trainer" }),
  ];
  fake.terminals = [
    { address: "a1", state: "live" },
    { address: "w1", parent: "a1", state: "live", name: "Question card", program: claude },
    { address: "w2", parent: "a1", state: "live", name: "PR links in chat", status: "working", program: claude },
    { address: "w3", parent: "w2", state: "live", template: "builder", status: "done" },
    { address: "w4", parent: "a1", state: "exited", name: "gone" },
  ];
  fake.focused = { agent: "w2" };
});

const make = () => { const cx = fakeCx(); const v = new Spaces(); v.init({}, cx); return v; };
const ids = (el, out = []) => { if (el && typeof el === "object") { if (el._a?.id) out.push(el._a.id); (el.kids ?? []).forEach((k) => ids(k, out)); } return out; };
const cx = () => ({ ...fakeCx(), stop_propagation() { this.stopped = true; } });

test("grouping: each project, then its agents; an orchestrator's workers under it, depth first", () => {
  const el = make().render();
  const order = ids(el).filter((i) => /^(space-s|ag-|sub-)/.test(i));
  assert.deepEqual(order, ["space-s1", "ag-a1", "sub-w1", "sub-w2", "sub-w3", "ag-a2", "space-s2", "ag-a3"]);
  assert.ok(!ids(el).includes("ag-w1"), "a worker with a pane is not an agent row of its own");
  assert.ok(!ids(el).includes("sub-w4"), "an exited worker is not shown");
  assert.equal(textOf(byId(el, "sub-w1").kids[1]), "w1 own pane", "a worker with a pane: its own title");
  assert.equal(textOf(byId(el, "sub-w1").kids[2]), "Question card", "and its name at the right");
  assert.equal(byId(el, "sub-w1")._a.tooltip, "Question card · w1 own pane");
  assert.equal(textOf(byId(el, "sub-w2").kids[1]), "PR links in chat", "no title: the name alone");
  assert.equal(byId(el, "sub-w2").kids.length, 3, "and no name at the right");
  assert.equal(textOf(byId(el, "sub-w3").kids[1]), "builder");
  assert.equal(textOf(el.kids[0]), "Projects");
});

test("the count line: only an agent with live workers has 'Orchestrating · N workers'", () => {
  const el = make().render();
  const a1 = byId(el, "ag-a1");
  assert.equal(a1.kids.length, 2);
  assert.equal(textOf(a1.kids[1]), "Orchestrating · 3 workers");
  assert.equal(byId(el, "ag-a2").kids.length, 1);
  assert.equal(countLine(1), "Orchestrating · 1 worker");
  assert.equal(countLine(4), "Orchestrating · 4 workers");
});

test("the title's ink and weight: shown 500 full, done and not seen 600 full, else muted 400", () => {
  const el = make().render();
  const title = (id) => byId(el, id).kids[0].kids[1]._a;
  assert.ok("font_medium" in title("ag-a1") && title("ag-a1").text_color === "#foreground");
  assert.ok("font_semibold" in title("ag-a2") && title("ag-a2").text_color === "#foreground");
  assert.ok(!("font_medium" in title("ag-a3")) && !("font_semibold" in title("ag-a3")) && title("ag-a3").text_color === "#muted");
  assert.equal(byId(el, "ag-a3").kids[0].kids[0]._a.opacity, 0.45, "the icon is dim off the shown row");
  assert.equal(byId(el, "ag-a1").kids[0].kids[0]._a.opacity, undefined);
});

test("the shown agent's row is raised; marks: loader working, a pulsing ⠿ waiting, ✓ done, ⠶ idle", () => {
  fake.agents[3].status = "blocked";
  store.set("changed", JSON.stringify({ a1: { s: "working", at: Date.now() - 600000 } }));
  const el = make().render();
  const a1 = byId(el, "ag-a1");
  assert.equal(a1._a.bg, "#raised");
  assert.equal(a1._a.border_color, "#border");
  assert.equal(byId(el, "ag-a2")._a.bg, undefined);
  assert.equal(byId(el, "sub-w2")._a.bg, "#raised", "the focused worker");
  const line = a1.kids[0];
  assert.deepEqual(line.kids.at(-1)._a.frames[1], 80);
  assert.equal(line.kids.at(-1)._a.text_color, "#foreground");
  assert.ok(!textOf(line).includes("10m"), "no age while it works");
  assert.equal(line.kids[0]._a.text_color, "#working", "the icon is tinted by the state");
  const a3 = byId(el, "ag-a3").kids[0].kids.at(-1);
  assert.equal(textOf(a3), "⠿");
  assert.deepEqual(a3._a.loop, ["opacity", { from: 0.35, to: 1, period: 1400 }]);
  assert.equal(a3._a.text_color, "#blocked");
  assert.ok(!["✓", "⠶"].includes(textOf(byId(el, "ag-a2").kids[0].kids.at(-1))), "an agent row shows no done or idle mark");
  assert.equal(textOf(byId(el, "sub-w3").kids.at(-1)), "✓");
  assert.equal(textOf(byId(el, "sub-w1").kids.at(-1)), "⠶");
});

test("a shut project shows its agents' state marks before the commits behind", () => {
  const v = make();
  v.closed.add("s1");
  v.git = {};
  const el = v.render();
  const tail = byId(el, "space-s1").kids[0].kids.at(-1);
  assert.deepEqual(tail.kids.slice(1).map((k) => k._a.frames ? "loader" : textOf(k)), ["loader", "✓"]);
});

test("projects with no agent: one closed row at the end; open, it lists them as project rows", () => {
  const v = make();
  let el = v.render();
  const more = byId(el, "more-local");
  assert.equal(textOf(more), moreLine(2));
  assert.equal(moreLine(2), "2 more projects, no agents");
  assert.equal(moreLine(1), "1 more project, no agents");
  assert.ok(!byId(el, "space-s3") && !byId(el, "space-s4"));
  assert.equal(ids(el).filter((i) => /^(space-s|more-)/.test(i)).at(-1), "more-local");
  more._h.on_click(null, cx());
  el = v.render();
  assert.deepEqual(ids(el).filter((i) => /^(space-s|more-)/.test(i)), ["space-s1", "space-s2", "more-local", "space-s3", "space-s4"]);
});

test("the shown project never hides in the 'more' row", () => {
  fake.spaces = [sp("s1", "contr"), sp("s2", "play"), sp("s3", "docs", { active: true })];
  fake.agents = [agent("a1", "s1")];
  fake.terminals = [];
  const el = make().render();
  assert.ok(byId(el, "space-s3"));
  assert.equal(textOf(byId(el, "more-local")), "1 more project, no agents");
});

test("no project without agents: no 'more' row", () => {
  fake.spaces = [sp("s1", "contr", { active: true })];
  assert.equal(byId(make().render(), "more-local"), null);
});

test("select: a project row, an agent row, a worker in a pane and a worker without one", () => {
  const v = make();
  let el = v.render();
  const row = byId(el, "space-s2");
  row._h.on_mouse_down(null, cx());
  row._h.on_mouse_up(null, cx());
  byId(el, "ag-a3")._h.on_click(null, cx());
  byId(el, "sub-w1")._h.on_click(null, cx());
  byId(el, "sub-w2")._h.on_click(null, cx());
  assert.deepEqual(log.run, [
    ["space.select", { id: "s2" }],
    ["pane.focus", { pane: "p-a3" }],
    ["pane.focus", { terminal: "w1" }],
    ["terminal.peek", { terminal: "w2" }],
  ]);
});

test("the chevron shuts a project's agents, is no select, and is kept", () => {
  const v = make();
  let el = v.render();
  assert.equal(byId(el, "fold-s3"), null, "only in rows that show");
  assert.equal(byId(el, "fold-s1").kids[0]._a.path, "mods/workspace-chrome/chevron-down.svg");
  const c = cx();
  byId(el, "fold-s1")._h.on_mouse_down(null, c);
  assert.ok(c.stopped);
  el = v.render();
  assert.equal(byId(el, "ag-a1"), null);
  assert.equal(byId(el, "fold-s1").kids[0]._a.path, "mods/workspace-chrome/chevron-right.svg");
  assert.deepEqual(JSON.parse(store.get("projects.closed")), ["s1"]);
  assert.deepEqual(log.run, []);
  assert.ok(byId(make().render(), "space-s1") && !byId(make().render(), "ag-a1"), "shut after a restart");
});

test("Herdr's agent order follows the projects, shut ones included", () => {
  fake.spaces = [sp("s2", "play"), sp("s1", "contr", { active: true })];
  const v = make();
  v.closed.add("s2");
  v.render();
  assert.deepEqual(log.order, ["a3", "a1", "a2"]);
});

test("the time since the state changed shows at the right", () => {
  store.set("changed", JSON.stringify({ a2: { s: "idle", at: Date.now() - 3 * 3600000 } }));
  const el = make().render();
  assert.ok(textOf(byId(el, "ag-a2")).includes("3h"));
});

test("split: a main and its worktrees stay together", () => {
  const r = (id, o = {}) => ({ w: { id, active: false }, ...o });
  const rows = [r("m1"), r("wt1", { tree: "└─" }), r("m2"), r("wt2", { tree: "└─" }), r("m3")];
  const { shown, idle } = split(rows, (id) => id === "wt1");
  assert.deepEqual(shown.map((x) => x.w.id), ["m1", "wt1"]);
  assert.deepEqual(idle.map((x) => x.w.id), ["m2", "wt2", "m3"]);
});

test("bySpace and workers: nested sub-agents are left out of the top rows", () => {
  const all = fake.terminals;
  assert.deepEqual(workers(all, "a1").map((s) => [s.t.address, s.depth]), [["w1", 0], ["w2", 0], ["w3", 1]]);
  const m = bySpace(fake.agents, all);
  assert.deepEqual(m.get("s1").map((x) => x.a.address), ["a1", "a2"]);
});

test("agentLabel: a named tab, the agent's own title, never an automatic tab number", () => {
  const tabs = new Map([["t1", { id: "t1", name: "3", named: false }], ["t2", { id: "t2", name: "review", named: true }]]);
  const a = { tab: "t1", title: "✳ Fix the bar", program: claude };
  assert.equal(agentLabel(a, tabs, new Map([["t1", { title: "✳ Fix the bar", program: claude }]]), false), "Fix the bar");
  assert.equal(agentLabel(a, tabs, new Map(), false), "claude");
  assert.equal(agentLabel({ ...a, tab: "t2" }, tabs, new Map(), false), "review");
});

test("workerWork: the worker's own title, cleaned; not its program, a path, user@host or its name", () => {
  const k = { address: "t9", name: "b-h118", program: claude };
  assert.equal(workerWork(k, "\u2733 Worker row title"), "Worker row title");
  assert.equal(workerWork(k, "claude"), "");
  assert.equal(workerWork(k, "~/code/x"), "");
  assert.equal(workerWork(k, "\u2733 /tmp"), "");
  assert.equal(workerWork(k, "me@mac"), "");
  assert.equal(workerWork(k, "b-h118"), "");
  assert.equal(workerWork(k, "t9"), "");
  assert.equal(workerWork(k, ""), "");
  assert.equal(workerWork(k, undefined), "");
});
