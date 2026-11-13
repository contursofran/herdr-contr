// The shortcuts of #117: Control+1 … Control+9 show a main agent in the sidebar's order, Command+J switches
// Chat and Terminal. The order is in mainagents.js, one copy in each mod that needs it.
import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fake, log, reset } from "./stub/herdr.mjs";
import { fakeCx } from "./fake-cx.mjs";
import { agent, space } from "./fixtures.mjs";
import { TopView } from "../mods/herdr-contr/top.js";
import { KEY_AGENT } from "../mods/herdr-contr/state.js";
import { mainAgents, mainKeys, mainKeyHint } from "../mods/herdr-contr/mainagents.js";
import { viewSwitch } from "../mods/herdr-contr/viewswitch.js";
import { byId } from "./stub/ui.mjs";

const live = (address, parent) => ({ address, parent, state: "live" });

beforeEach(() => {
  reset();
  // spaces() order: a remote space first, two local spaces after it. The sidebar lists this computer first.
  fake.spaces = [space("r1", { machine: "box" }), space("s1"), space("s2", { active: false }), space("r0", { machine: "alpha" })];
  fake.agents = [agent("box/t1", "idle", { space: "r1" }), agent("t2", "idle", { space: "s2" }), agent("t1", "working", { space: "s1" }),
    agent("t3", "idle", { space: "s1" }), agent("alpha/t1", "idle", { space: "r0" }), agent("w1", "working", { space: "s1" })];
  // t1 orchestrates w1 (it has a pane, so it is in agents() too), and w1 has a worker of its own.
  fake.terminals = [live("t1"), live("t2"), live("t3"), live("w1", "t1"), live("w2", "w1"), { address: "w3", parent: "t3", state: "exited" }];
});

test("main agents: this computer first, then other machines by name; spaces in order; workers left out", () => {
  const order = mainAgents(fake.spaces, fake.agents, fake.terminals).map((a) => a.address);
  assert.deepEqual(order, ["t1", "t3", "t2", "alpha/t1", "box/t1"]);
});

test("mainKeys: numbers 1 to 9 only; mainKeyHint names the key", () => {
  const many = Array.from({ length: 11 }, (_, i) => agent(`a${i}`, "idle"));
  const keys = mainKeys([space("sp1")], many, []);
  assert.equal(keys.size, 9);
  assert.equal(keys.get("a0"), 1);
  assert.equal(keys.get("a8"), 9);
  assert.equal(keys.get("a9"), undefined);
  assert.equal(mainKeyHint(3), "⌃3");
});

test("an exited sub-agent is not a worker: its agent keeps its number", () => {
  fake.terminals.push({ address: "t2", parent: "t3", state: "exited" });
  assert.ok(mainAgents(fake.spaces, fake.agents, fake.terminals).some((a) => a.address === "t2"));
});

test("Control+N shows the Nth main agent; a number with no agent does nothing", () => {
  const top = new TopView(fakeCx());
  top.command("main-3");
  assert.deepEqual(log.run.at(-1), ["agent.focus", { terminal: "t2" }]);
  assert.equal(fake.shared[KEY_AGENT], "t2");
  top.command("main-5");
  assert.deepEqual(log.run.at(-1), ["agent.focus", { terminal: "box/t1" }]);
  log.run = [];
  top.command("main-6");
  top.command("main-9");
  assert.deepEqual(log.run, [], "no 6th or 9th main agent");
  assert.equal(fake.shared[KEY_AGENT], "box/t1");
});

test("the two mods' copies of mainagents.js are equal", () => {
  const read = (mod) => readFileSync(new URL(`../mods/${mod}/mainagents.js`, import.meta.url), "utf8");
  assert.equal(read("herdr-contr-spaces"), read("herdr-contr"));
});

test("keys.json: Control+1 … 9 show a main agent and Command+J switches Chat and Terminal, in both layouts", () => {
  const keys = JSON.parse(readFileSync(new URL("../keys.json", import.meta.url)));
  for (const section of ["layout:herdr-contr", "layout:herdr-contr-own"]) {
    for (let n = 1; n <= 9; n++) assert.equal(keys[section][`ctrl-${n}`], `herdr-contr.main-${n}`, `${section} ctrl-${n}`);
    assert.equal(keys[section]["cmd-j"], "herdr-contr.view", section);
  }
});

test("the Chat | Terminal switch names its key in the tooltip", () => {
  const t = new Proxy({}, { get: (_t, k) => `#${String(k)}` });
  const el = viewSwitch(t, "chat");
  assert.equal(byId(el, "view-chat")._a.tooltip, "Chat (⌘J)");
  assert.equal(byId(el, "view-tab")._a.tooltip, "Terminal (⌘J)");
});
