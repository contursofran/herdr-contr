// The Orchestration section: the tool's settings (`agents config --json`), changed with `agents set` (#186).
import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { fake, log, reset } from "./stub/herdr.mjs";
import { textOf, byId } from "./stub/ui.mjs";
import { fakeCx } from "./fake-cx.mjs";
import { agent, space } from "./fixtures.mjs";
import { SettingsView } from "../mods/herdr-contr/settings.js";
import { loadAgents, flowValue, flowChoices, roleValue, setFlow, setRole, useRecommended, recommendedInUse } from "../mods/herdr-contr/orchestration.js";

const HOME = "/home/test/.config/herdr-app";
const TOOL = ["HERDR_APP_HOME=" + HOME, HOME + "/herdr-contr/bin/herdr-contr-orch"];
const role = (name, model, effort, claude = true) => ({ name, argv: claude ? ["claude", "--model", model, "--effort", effort] : ["codex", "-m", "x"], claude, model: claude ? model : "", effort: claude ? effort : "" });
// As `agents config --json` writes it (xplat/herdr-contr-orch/src/settings.rs).
const config = () => ({
  path: HOME + "/herdr-contr/agents.json", bad: false,
  flow: { plan: "when-needed", review: "code", merge: "ask", debugger_after: 2, workers: 4 },
  fields: { plan: { default: "when-needed", values: ["never", "when-needed", "always"] }, review: { default: "code", values: ["none", "code", "every"] },
    merge: { default: "ask", values: ["ask", "auto"] }, debugger_after: { default: 2, min: 1 }, workers: { default: 4, min: 1 } },
  roles: [role("orchestrator", "opus", "medium"), role("builder", "sonnet", "medium"), role("planner", "opus", "low"), role("reviewer", "opus", "low"), role("debugger", "opus", "xhigh")],
});
let cfg, sets, setResult, configResult;
beforeEach(() => {
  reset(); cfg = config(); sets = []; setResult = null; configResult = null;
  fake.window = { platform: "macos" };
  fake.exec = (cmd, args) => {
    if (cmd !== "env") return { code: 0, stdout: [{ type: "header", protocol: 1, sidecar: "0.1.0", cmd: args[0] }, { type: "end", ok: true }].map((l) => JSON.stringify(l)).join("\n") + "\n", stderr: "" };
    assert.deepEqual(args.slice(0, 2), TOOL);
    const a = args.slice(2);
    if (a.join(" ") === "agents config --json") return configResult ?? { code: 0, stdout: JSON.stringify(cfg), stderr: "" };
    if (a[0] === "agents" && a[1] === "set") {
      sets.push(a.slice(2));
      if (setResult) return setResult;
      for (let i = 2; i < a.length; i += 2) {
        const [k, v] = [a[i], a[i + 1]], p = k.split(".");
        if (p[0] === "flow") cfg.flow[p[1]] = cfg.fields[p[1]].min ? Number(v) : v;
        else { const r = cfg.roles.find((x) => x.name === p[1]); if (r.claude) r[p[2]] = v; }
      }
      return { code: 0, stdout: "", stderr: "" };
    }
    return { code: 2, stdout: "", stderr: "unknown" };
  };
  fake.spaces = [space("sp1")];
  fake.agents = [agent("t1", "idle")];
});

const open = async () => {
  const v = new SettingsView(fakeCx()); await loadAgents();
  v.section = "orchestration"; return { el: v.render(fakeCx()) };
};
const click = async (el) => { const cx = fakeCx(); el._h.on_click({}, cx); await cx.settle(); };
// The stub elements answer any method, `then` too, so open() wraps the tree in { el }.

test("the section shows the tool's values, choices from its fields with the UI's labels", async () => {
  cfg.fields.plan.values.push("later");
  const { el } = await open();
  assert.ok(byId(el, "set-nav-orchestration"));
  assert.equal(flowValue("merge"), "ask");
  assert.deepEqual(flowChoices("plan").map(([v, l]) => [v, l]), [["never", "Never"], ["when-needed", "When needed"], ["always", "Always"], ["later", "later"]]);
  assert.equal(roleValue("builder", "model"), "sonnet");
  assert.equal(roleValue("debugger", "effort"), "xhigh");
  assert.match(textOf(byId(el, "set-flow-workers-value")), /4 workers/);
  assert.match(textOf(el), /Saved in \/home\/test\/.config\/herdr-app\/herdr-contr\/agents.json/);
  assert.equal(log.writes.length, 0, "the mod writes no file");
});

test("a pick runs agents set with one pair, and the section reads the tool again", async () => {
  let { el } = await open();
  await click(byId(el, "set-flow-merge-auto"));
  ({ el } = await open());
  await click(byId(el, "set-role-builder-model-haiku"));
  await click(byId(el, "set-role-builder-effort-max"));
  assert.deepEqual(sets, [["flow.merge", "auto"], ["profile.builder.model", "haiku"], ["profile.builder.effort", "max"]]);
  assert.equal(flowValue("merge"), "auto");
  assert.equal(roleValue("builder", "model"), "haiku");
});

test("steppers: +1 and −1, and never below the field's min", async () => {
  cfg.flow.debugger_after = 1; cfg.flow.workers = 2;
  const { el } = await open();
  await click(byId(el, "set-flow-debugger_after-less"));
  assert.equal(sets.length, 0, "stays at 1");
  await click(byId(el, "set-flow-workers-more"));
  assert.deepEqual(sets, [["flow.workers", "3"]]);
  assert.equal(flowValue("workers"), 3);
});

test("a model or effort that is not in the lists shows as none", async () => {
  cfg.roles[1] = { ...cfg.roles[1], model: "gpt-x", effort: "turbo" };
  await open();
  assert.equal(roleValue("builder", "model"), "");
  assert.equal(roleValue("builder", "effort"), "");
});

test("a role that is not claude: its command shows, its switches are off (#80)", async () => {
  cfg.roles[3] = role("reviewer", "", "", false);
  const { el } = await open();
  assert.equal(roleValue("reviewer", "model"), "");
  assert.match(textOf(byId(el, "set-role-reviewer")), /Custom: codex -m x/);
  await click(byId(el, "set-role-reviewer-model-sonnet"));
  assert.equal(sets.length, 0);
});

test("a file that is not JSON: the tool says bad, a note shows and the switches are off", async () => {
  cfg.bad = true;
  const { el } = await open();
  assert.match(textOf(byId(el, "set-agents-bad")), /could not be read/);
  await click(byId(el, "set-flow-plan-always"));
  assert.equal(sets.length, 0);
});

test("a change that the tool refuses shows its stderr", async () => {
  setResult = { code: 1, stdout: "", stderr: "agents: flow.workers: 0 is not valid\n" };
  const { el } = await open();
  await click(byId(el, "set-flow-merge-auto"));
  assert.equal(await setFlow("merge", "auto"), false);
  assert.match(textOf(byId((await openKeep()).el, "set-agents-error")), /agents: flow.workers: 0 is not valid/);
});
// A render that keeps the error: open() reads the tool again, which clears it.
const openKeep = async () => { const v = new SettingsView(fakeCx()); v.section = "orchestration"; return { el: v.render(fakeCx()) }; };

test("a missing tool: the section says so, the switches are off, nothing throws", async () => {
  fake.exec = () => { throw new Error("ENOENT"); };
  const { el } = await open();
  assert.match(textOf(byId(el, "set-agents-error")), /settings could not be read: herdr-contr-orch could not start.*ENOENT/);
  assert.equal(byId(el, "set-flow-merge-auto"), null, "no choices without the tool"); await click(byId(el, "set-flow-workers-more"));
  assert.equal(byId(el, "set-recommended-use")._h?.on_click, undefined);
  assert.equal(log.exec.filter(([c, a]) => c === "env" && a.includes("set")).length, 0);
});

test("a tool that fails or gives text that is not JSON: the section says so", async () => {
  configResult = { code: 1, stdout: "", stderr: "agents: lock: busy" };
  let { el } = await open();
  assert.match(textOf(byId(el, "set-agents-error")), /lock: busy/);
  configResult = { code: 0, stdout: "nope", stderr: "" };
  ({ el } = await open());
  assert.match(textOf(byId(el, "set-agents-error")), /not JSON/);
});

test("Use recommended setup: one agents set call with all pairs (#90)", async () => {
  const { el } = await open();
  assert.equal(byId(el, "set-recommended-note"), null);
  await click(byId(el, "set-recommended-use"));
  assert.deepEqual(sets, [["profile.orchestrator.model", "claude-fable-5-2", "profile.orchestrator.effort", "medium",
    "profile.builder.model", "opus", "profile.builder.effort", "low", "profile.reviewer.model", "claude-fable-5-2", "profile.reviewer.effort", "low",
    "profile.planner.model", "claude-fable-5-2", "profile.planner.effort", "low", "profile.debugger.model", "claude-fable-5-2", "profile.debugger.effort", "high"]]);
  assert.equal(recommendedInUse(), true);
});

test("all roles at the recommended setup: the button is off and says so; a custom role keeps it on (#90)", async () => {
  await loadAgents(); await useRecommended();
  let { el } = await open();
  assert.match(textOf(byId(el, "set-recommended-note")), /Recommended setup in use/);
  assert.equal(byId(el, "set-recommended-use")._h?.on_click, undefined);
  cfg.roles[3] = role("reviewer", "", "", false);
  await loadAgents();
  assert.equal(recommendedInUse(), false);
  assert.equal(await setRole("reviewer", "model", "opus"), true);
});
