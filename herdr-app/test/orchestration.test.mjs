// The Orchestration section: contr's agents.json, edited in the herdr-contr settings.
import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { fake, log, reset } from "./stub/herdr.mjs";
import { textOf, byId } from "./stub/ui.mjs";
import { fakeCx } from "./fake-cx.mjs";
import { agent, space } from "./fixtures.mjs";
import { SettingsView } from "../mods/herdr-contr/settings.js";
import { loadAgents, agentsPath, resetAgentsPath, flowValue, roleValue, setFlow, setRole, setFlag, DEFAULT_FLOW } from "../mods/herdr-contr/orchestration.js";

const MAC = "/home/test/Library/Application Support/contr/agents.json";
let file, env, path;
beforeEach(() => {
  reset(); resetAgentsPath(); file = null; env = ["", ""]; path = MAC;
  fake.window = { platform: "macos" };
  fake.readText = (p) => (p === path ? file : null);
  fake.writeText = (p, text, expected) => {
    if (p !== path) throw new Error("wrong path");
    if (expected !== file) throw new Error("stale");
    file = text; return null;
  };
  fake.exec = (cmd, args) => (cmd === "sh" ? { code: 0, stdout: env.join("\n"), stderr: "" }
    : cmd === "mkdir" ? { code: 0, stdout: "", stderr: "" }
    : { code: 0, stdout: [{ type: "header", protocol: 1, sidecar: "0.1.0", cmd: args[0] }, { type: "end", ok: true }].map((l) => JSON.stringify(l)).join("\n") + "\n", stderr: "" });
  fake.spaces = [space("sp1")];
  fake.agents = [agent("t1", "idle")];
});

const open = async () => {
  const v = new SettingsView(fakeCx()); await loadAgents();
  v.section = "orchestration"; return { el: v.render(fakeCx()) };
};
const click = async (el) => { const cx = fakeCx(); el._h.on_click({}, cx); await cx.settle(); };
// The stub elements answer any method, `then` too, so open() wraps the tree in { el }.

test("the path: macOS user config folder, XDG on Linux, and CONTR_AGENTS_CONFIG over both", async () => {
  assert.equal(await agentsPath(), MAC);
  resetAgentsPath(); fake.window = { platform: "linux" };
  assert.equal(await agentsPath(), "/home/test/.config/contr/agents.json");
  resetAgentsPath(); env = ["", "/x/cfg"];
  assert.equal(await agentsPath(), "/x/cfg/contr/agents.json");
  resetAgentsPath(); env = ["/y/agents.json", "/x/cfg"];
  assert.equal(await agentsPath(), "/y/agents.json");
});

test("no file: contr's defaults show, nothing is written; the first change creates the file", async () => {
  let { el } = await open();
  assert.ok(byId(el, "set-nav-orchestration"));
  assert.equal(log.writes.length, 0);
  assert.deepEqual(Object.fromEntries(Object.keys(DEFAULT_FLOW).map((k) => [k, flowValue(k)])), DEFAULT_FLOW);
  assert.equal(roleValue("builder", "model"), "sonnet");
  assert.equal(roleValue("debugger", "effort"), "xhigh");
  assert.match(textOf(byId(el, "set-flow-workers-value")), /4 workers/);
  await click(byId(el, "set-flow-merge-auto"));
  assert.ok(log.exec.some(([c, a]) => c === "mkdir" && a[1] === "/home/test/Library/Application Support/contr"));
  assert.deepEqual(JSON.parse(file), { flow: { ...DEFAULT_FLOW, merge: "auto" } });
});

test("a change keeps every key the section does not know", async () => {
  file = JSON.stringify({ kit: "/k", setup: { a: "b" }, x: 1, flow: { plan: "always", extra: true },
    profiles: { builder: { argv: ["claude", "--model", "opus", "--effort=low", "--x"], agent: "b2" }, mine: { argv: ["sh"] } } });
  let { el } = await open();
  assert.equal(flowValue("plan"), "always");
  await click(byId(el, "set-flow-review-every"));
  ({ el } = await open());
  await click(byId(el, "set-role-builder-model-haiku"));
  await click(byId(el, "set-role-builder-effort-max"));
  const f = JSON.parse(file);
  assert.equal(f.kit, "/k"); assert.equal(f.x, 1); assert.deepEqual(f.setup, { a: "b" });
  assert.deepEqual(f.flow, { ...DEFAULT_FLOW, plan: "always", review: "every", extra: true });
  assert.deepEqual(f.profiles.builder, { argv: ["claude", "--model", "haiku", "--effort=max", "--x"], agent: "b2" });
  assert.deepEqual(f.profiles.mine, { argv: ["sh"] });
  assert.equal(f.profiles.planner, undefined, "only the changed role is written");
});

test("steppers: +1 and −1, and never below 1", async () => {
  file = JSON.stringify({ flow: { debugger_after: 1, workers: 2 } });
  const { el } = await open();
  await click(byId(el, "set-flow-debugger_after-less"));
  assert.equal(JSON.parse(file).flow.debugger_after, 1, "stays at 1");
  assert.equal(log.writes.length, 0);
  await click(byId(el, "set-flow-workers-more"));
  assert.equal(JSON.parse(file).flow.workers, 3);
});

test("a role that is not claude: its command shows, a pick changes nothing (#80)", async () => {
  file = JSON.stringify({ profiles: { reviewer: { argv: ["codex", "-m", "x"] } } });
  const { el } = await open();
  assert.equal(roleValue("reviewer", "model"), "");
  assert.match(textOf(byId(el, "set-role-reviewer")), /Custom: codex -m x/);
  await click(byId(el, "set-role-reviewer-model-sonnet"));
  assert.deepEqual(JSON.parse(file).profiles.reviewer.argv, ["codex", "-m", "x"]);
  assert.equal(log.writes.length, 0);
});

test("claude after env, or by its path, is claude: a pick sets only its flag (#80)", async () => {
  const orch = ["env", "CLAUDE_CODE_PLUGIN_DIRS=", "claude", "--plugin-dir", "/k", "--model", "opus", "--effort", "medium", "--permission-mode", "auto"];
  const build = ["/home/u/.local/bin/claude", "--model", "opus", "--permission-mode", "auto"];
  file = JSON.stringify({ kit: "/k", profiles: { orchestrator: { argv: orch }, builder: { argv: build, note: "x" } } });
  const { el } = await open();
  assert.equal(roleValue("orchestrator", "model"), "opus");
  assert.equal(roleValue("orchestrator", "effort"), "medium");
  assert.doesNotMatch(textOf(byId(el, "set-role-orchestrator")), /Custom/);
  await click(byId(el, "set-role-orchestrator-model-sonnet"));
  await click(byId(el, "set-role-builder-effort-high"));
  const f = JSON.parse(file);
  assert.deepEqual(f.profiles.orchestrator.argv, orch.map((a) => a === "opus" ? "sonnet" : a));
  assert.deepEqual(f.profiles.builder.argv, ["/home/u/.local/bin/claude", "--model", "opus", "--effort", "high", "--permission-mode", "auto"]);
  assert.equal(f.profiles.builder.note, "x"); assert.equal(f.kit, "/k");
});

test("a change works without structuredClone, which Herdr's runtime does not have (#80)", async () => {
  const was = globalThis.structuredClone; globalThis.structuredClone = undefined;
  try {
    file = JSON.stringify({ flow: { plan: "never" } });
    const { el } = await open();
    await click(byId(el, "set-flow-workers-more"));
    assert.equal(JSON.parse(file).flow.workers, 5);
  } finally { globalThis.structuredClone = was; }
});

test("setFlag puts a missing --effort after --model", () => {
  assert.deepEqual(setFlag(["claude", "--model", "opus", "-p"], "--effort", "high"), ["claude", "--model", "opus", "--effort", "high", "-p"]);
  assert.deepEqual(setFlag(["claude"], "--model", "opus"), ["claude", "--model", "opus"]);
});

test("a file that is not JSON: an error, contr's defaults, and the file is never written", async () => {
  file = "{ flow: ";
  const { el } = await open();
  assert.match(textOf(byId(el, "set-agents-bad")), /could not be read/);
  await click(byId(el, "set-flow-plan-always"));
  assert.equal(await setFlow("plan", "always"), false);
  assert.equal(await setRole("builder", "model", "opus"), false);
  assert.equal(file, "{ flow: ");
  assert.equal(log.writes.length, 0);
});

test("a write that fails shows why", async () => {
  fake.writeText = () => { throw new Error("denied"); };
  const { el } = await open();
  await click(byId(el, "set-flow-merge-auto"));
  assert.match(textOf(byId((await open()).el, "set-agents-error")), /could not be written: denied/);
});

test("loadAgents reads the file again on each open", async () => {
  await loadAgents(); file = JSON.stringify({ flow: { workers: 7 } }); await loadAgents();
  assert.equal(flowValue("workers"), 7);
});
