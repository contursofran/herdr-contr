// The Orchestration section of the herdr-contr settings: the flow and the profiles of the orchestration
// tool (xplat/herdr-contr-orch). The tool is the one reader and writer of its agents.json (#186): the
// section reads it with `agents config --json` and changes it with `agents set`. The defaults and the
// valid values come from the tool; the mod keeps only the labels and the lists that it shows.
import { shared, setShared } from "herdr";
import { runOrch, orchJson } from "./orch.js";

export const KEY_AGENTS = "herdr-contr.agents";   // { path, bad, flow, fields, roles, error, missing }

// The label that the UI shows for each flow value. The valid values come from the tool's `fields`.
export const FLOW_LABELS = {
  plan: { never: "Never", "when-needed": "When needed", always: "Always" },
  review: { none: "None", code: "Code PRs", every: "Every PR" },
  merge: { ask: "Ask me", auto: "When CI and review pass" },
};
export const STEP_MAX = 99;
export const MODELS = [["opus", "Opus 5.5", "opus"], ["sonnet", "Sonnet 5.5", "sonnet"], ["haiku", "Haiku 5.5", "haiku"], ["claude-fable-5-2", "Fable 5.2", "fable"]];
export const EFFORTS = ["low", "medium", "high", "xhigh", "max"].map((e) => [e, e, e]);
// The owner's recommended setup (#90): [model, effort] per role.
export const RECOMMENDED = {
  orchestrator: ["claude-fable-5-2", "medium"], builder: ["opus", "low"], reviewer: ["claude-fable-5-2", "low"],
  planner: ["claude-fable-5-2", "low"], debugger: ["claude-fable-5-2", "high"],
};
export const ROLES = [
  ["orchestrator", "Orchestrator", "Plans and lands work."], ["planner", "Planner", "Writes PLAN.md."],
  ["builder", "Builder", "Builds, opens the PR."], ["reviewer", "Reviewer", "Reviews the PR head."],
  ["debugger", "Debugger", "After failed fixes."],
];

// Reads the settings with `herdr-contr-orch agents config --json`. A tool that is missing or fails gives
// `missing` and its message; the section then shows the message and its switches are off.
export async function loadAgents() {
  const r = await orchJson(["agents", "config", "--json"]);
  const v = r.value;
  const c = v && typeof v === "object" && !Array.isArray(v)
    ? { path: v.path ?? null, bad: v.bad === true, flow: v.flow ?? {}, fields: v.fields ?? {}, roles: Array.isArray(v.roles) ? v.roles : [], error: null, missing: false }
    : { path: null, bad: false, flow: {}, fields: {}, roles: [], error: `The settings could not be read: ${r.error ?? "no JSON object"}`, missing: true };
  if (JSON.stringify(c) !== JSON.stringify(shared(KEY_AGENTS))) setShared(KEY_AGENTS, c);
  return c;
}

const agents = () => shared(KEY_AGENTS) ?? { path: null, bad: false, flow: {}, fields: {}, roles: [], error: null, missing: true };
export const agentsBad = () => agents().bad === true;
export const agentsError = () => agents().error ?? null;

export const flowValue = (key) => agents().flow?.[key];
// The choices of a flow key, as [value, label, slug]: the tool's values with the UI's labels.
export const flowChoices = (key) => (agents().fields?.[key]?.values ?? []).map((v) => [v, FLOW_LABELS[key]?.[v] ?? v, v]);
const flowMin = (key) => agents().fields?.[key]?.min ?? 1;

const roleOf = (name) => agents().roles.find((r) => r?.name === name);
export const roleIsClaude = (name) => roleOf(name)?.claude === true;
// The model or effort that a role starts with; "" when it is not one of the choices (or a custom command).
export function roleValue(name, what) {
  const r = roleOf(name);
  if (r?.claude !== true) return "";
  const v = r[what];
  return (what === "model" ? MODELS : EFFORTS).some(([x]) => x === v) ? v : "";
}

// Runs `herdr-contr-orch agents set` with the pairs, then reads the settings again. False when the tool
// refuses or fails; `error` then has its message.
async function set(pairs) {
  const r = await runOrch(["agents", "set", ...pairs.flat().map(String)]);
  if (!r.ok) {
    await loadAgents();
    setShared(KEY_AGENTS, { ...agents(), error: r.error });
    return false;
  }
  await loadAgents();
  return true;
}

export const setFlow = (key, value) => set([[`flow.${key}`, value]]);
// One role's model or effort. The tool changes only that flag, and never a command that is not claude.
export const setRole = (name, what, value) => set([[`profile.${name}.${what}`, value]]);

// True when each role is claude and has the recommended model and effort.
export const recommendedInUse = () => Object.entries(RECOMMENDED)
  .every(([name, [m, e]]) => roleValue(name, "model") === m && roleValue(name, "effort") === e);

// Sets the model and effort of each role to RECOMMENDED in one `agents set` call.
export const useRecommended = () => set(Object.entries(RECOMMENDED)
  .flatMap(([name, [m, e]]) => [[`profile.${name}.model`, m], [`profile.${name}.effort`, e]]));

// The section's body. `ui`: { seg, small, heading, about, t } from settings.js.
export function orchestrationBody({ t, seg, small, heading, about, div, h_flex, v_flex }, view) {
  const bad = agentsBad() || agents().missing === true;
  // A change that throws shows its error; it is not dropped (#80).
  const run = (p, cx) => cx.spawn(async (cx) => {
    try { await p(); } catch (e) { setShared(KEY_AGENTS, { ...agents(), error: `The change failed: ${e?.message ?? e}` }); }
    cx.notify();
  });
  const body = v_flex().w_full();
  const st = agents();
  if (agentsBad()) body.child(div().id("set-agents-bad").mt(12).p(10).rounded(8).bg(t.raised).border(1).border_color(t.blocked)
    .text_size(12.5).text_color(t.foreground)
    .child("agents.json could not be read (it is not valid JSON), so contr's defaults show. Fix the file to change these here."));
  if (st.error) body.child(div().id("set-agents-error").mt(12).p(10).rounded(8).bg(t.raised).border(1).border_color(t.blocked)
    .text_size(12.5).text_color(t.foreground).child(st.error));
  const line = (id, title, text, control) => h_flex().id(id).w_full().py(14).gap(24).items_center().border_b(1).border_color(t.border)
    .child(about(title, text)).child(control);
  const pick = (key) => seg(`flow-${key}`, flowChoices(key), (v, cx) => run(() => setFlow(key, v), cx), flowValue(key) ?? "", bad);
  const step = (key, unit) => {
    const n = Number(flowValue(key)) || flowMin(key);
    const go = (d) => (cx) => { if (!bad && n + d >= flowMin(key) && n + d <= STEP_MAX) run(() => setFlow(key, n + d), cx); };
    return h_flex().flex_none().gap(8).items_center()
      .child(small(`set-flow-${key}-less`, "−", go(-1)))
      .child(div().id(`set-flow-${key}-value`).min_w(64).text_size(12.5).text_color(t.foreground).child(`${n} ${unit}`))
      .child(small(`set-flow-${key}-more`, "+", go(1)));
  };
  body.child(heading("FLOW"))
    .child(line("set-flow-plan", "Plan first", "When a planner runs before the builder.", pick("plan")))
    .child(line("set-flow-review", "Review", "Which PRs get a reviewer.", pick("review")))
    .child(line("set-flow-debugger_after", "Debugger after", "Failed fix rounds before a debugger.", step("debugger_after", "rounds")))
    .child(line("set-flow-workers", "Workers at once", "The most workers at once, reviewers included.", step("workers", "workers")))
    .child(line("set-flow-merge", "Merge", "Ask each time, or merge when CI and review pass.", pick("merge")));
  body.child(heading("ROLES"));
  const inUse = recommendedInUse();
  body.child(h_flex().id("set-recommended").w_full().pt(12).gap(12).items_center()
    .child(inUse || bad
      ? div().id("set-recommended-use").h(24).px(9).flex().items_center().rounded(6).text_size(12)
        .text_color(t.dim).bg(t.chrome).border(1).border_color(t.border).child("Use recommended setup")
      : small("set-recommended-use", "Use recommended setup", (cx) => run(() => useRecommended(), cx)))
    .child(inUse ? div().id("set-recommended-note").text_size(12).text_color(t.dim).child("Recommended setup in use") : div()));
  for (const [name, title, does] of ROLES) {
    // A command that is not claude: its text shows and the switches are off; it is never replaced (#80).
    const custom = !bad && !roleIsClaude(name);
    body.child(v_flex().id(`set-role-${name}`).w_full().py(14).gap(10).border_b(1).border_color(t.border)
      .child(about(title, custom ? `${does} Custom: ${(roleOf(name)?.argv ?? []).join(" ")}` : does))
      .child(h_flex().w_full().flex_wrap().gap(12).items_center()
        .child(seg(`role-${name}-model`, MODELS, (v, cx) => run(() => setRole(name, "model", v), cx), roleValue(name, "model"), bad || custom))
        .child(seg(`role-${name}-effort`, EFFORTS, (v, cx) => run(() => setRole(name, "effort", v), cx), roleValue(name, "effort"), bad || custom))));
  }
  body.child(div().pt(12).text_size(11).text_color(t.dim).child(`Saved in ${st.path ?? "the tool's agents.json"} by herdr-contr-orch. The coordinator reads it.`));
  return body;
}
