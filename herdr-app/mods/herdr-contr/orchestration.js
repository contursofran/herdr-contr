// The Orchestration section of the herdr-contr settings: it edits contr's agents.json, the file that
// contr's coordinator reads (contr's internal/agents: Flow, DefaultConfig, LoadConfig). The path:
// CONTR_AGENTS_CONFIG when set, else the user config folder's contr/agents.json (macOS:
// ~/Library/Application Support; other systems: $XDG_CONFIG_HOME or ~/.config).
// The choices are the ones of contr's Settings, Agents tab. A write reads the file, changes one field
// and keeps all other keys. A file that is not a JSON object is never written. A missing file is
// contr's defaults, and the first change creates it.
import { home, window, exec, readTextFile, writeTextFile, shared, setShared } from "herdr";

export const KEY_AGENTS = "herdr-contr.agents";   // { path, file, bad, text, error }

export const DEFAULT_FLOW = { plan: "when-needed", review: "code", debugger_after: 2, workers: 4, merge: "ask" };
const claude = (model, effort) => ["claude", "--model", model, "--effort", effort, "--permission-mode", "auto"];
export const DEFAULT_PROFILES = {
  orchestrator: claude("opus", "medium"), builder: claude("sonnet", "medium"), planner: claude("opus", "low"),
  reviewer: claude("opus", "low"), debugger: claude("opus", "xhigh"),
};
// [value, label, slug], as the settings' choices.
export const FLOW_CHOICES = {
  plan: [["never", "Never", "never"], ["when-needed", "When needed", "when-needed"], ["always", "Always", "always"]],
  review: [["none", "None", "none"], ["code", "Code PRs", "code"], ["every", "Every PR", "every"]],
  merge: [["ask", "Ask me", "ask"], ["auto", "When CI and review pass", "auto"]],
};
export const STEP_MAX = 99;
export const MODELS = [["opus", "Opus 5.5", "opus"], ["sonnet", "Sonnet 5.5", "sonnet"], ["haiku", "Haiku 5.5", "haiku"], ["claude-fable-5-2", "Fable 5.2", "fable"]];
export const EFFORTS = ["low", "medium", "high", "xhigh", "max"].map((e) => [e, e, e]);
export const ROLES = [
  ["orchestrator", "Orchestrator", "Plans and lands work."], ["planner", "Planner", "Writes PLAN.md."],
  ["builder", "Builder", "Builds, opens the PR."], ["reviewer", "Reviewer", "Reviews the PR head."],
  ["debugger", "Debugger", "After failed fixes."],
];

// The file's path, asked of the user's shell once (the mod has no environment of its own).
let pathOnce = null;
export const resetAgentsPath = () => { pathOnce = null; };
export function agentsPath() {
  return (pathOnce ??= (async () => {
    let over = "", xdg = "";
    try {
      const r = await exec("sh", ["-c", 'printf "%s\\n%s" "$CONTR_AGENTS_CONFIG" "$XDG_CONFIG_HOME"']);
      if (r?.code === 0) [over = "", xdg = ""] = String(r.stdout).split("\n");
    } catch { /* the defaults below */ }
    if (over) return over;
    if ((window()?.platform ?? "macos") === "macos") return `${home()}/Library/Application Support/contr/agents.json`;
    return `${xdg || `${home()}/.config`}/contr/agents.json`;
  })());
}

export function parseAgents(text) {
  if (text == null) return { file: {}, bad: false, text: null };
  try {
    const v = JSON.parse(text);
    if (v && typeof v === "object" && !Array.isArray(v)) return { file: v, bad: false, text };
  } catch { /* not JSON */ }
  return { file: {}, bad: true, text };
}

export async function loadAgents() {
  const path = await agentsPath();
  let text = null;
  try { text = await readTextFile(path); } catch { text = null; }
  const c = { path, ...parseAgents(text), error: null };
  const was = shared(KEY_AGENTS);
  if (!was || was.text !== c.text || was.path !== c.path) setShared(KEY_AGENTS, c);
  return c;
}

const agents = () => shared(KEY_AGENTS) ?? { path: null, file: {}, bad: false, text: null };
export const agentsBad = () => agents().bad === true;
export const agentsError = () => agents().error ?? null;

// A flow value: the file's, when contr accepts it, else contr's default.
export function flowValue(key) {
  const v = agents().file?.flow?.[key];
  if (FLOW_CHOICES[key]) return FLOW_CHOICES[key].some(([x]) => x === v) ? v : DEFAULT_FLOW[key];
  return Number.isInteger(v) && v >= 1 ? v : DEFAULT_FLOW[key];
}

const flagOf = (argv, flag) => {
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === flag && i + 1 < argv.length) return argv[i + 1];
    if (typeof argv[i] === "string" && argv[i].startsWith(flag + "=")) return argv[i].slice(flag.length + 1);
  }
  return "";
};
const argvOf = (name) => {
  const p = agents().file?.profiles?.[name];
  return Array.isArray(p?.argv) && p.argv.length ? p.argv : DEFAULT_PROFILES[name];
};
// Where claude is in a command: the first argument whose base name is "claude", at the start or after
// "env" and its NAME=value arguments (#80). -1 when the command is not claude.
export function claudeAt(argv) {
  let i = 0;
  if (argv[0] === "env") { i = 1; while (i < argv.length && /^[A-Za-z_][A-Za-z0-9_]*=/.test(String(argv[i]))) i++; }
  return String(argv[i] ?? "").split("/").pop() === "claude" ? i : -1;
}
export const roleIsClaude = (name) => claudeAt(argvOf(name)) >= 0;
// The model or effort that a role starts with; "" when it is not one of the choices (a custom command).
export function roleValue(name, what) {
  const argv = argvOf(name);
  if (claudeAt(argv) < 0) return "";
  const v = flagOf(argv, what === "model" ? "--model" : "--effort");
  return (what === "model" ? MODELS : EFFORTS).some(([x]) => x === v) ? v : "";
}

// Sets --flag to v and keeps the other arguments; a missing --effort goes after --model (as contr does).
export function setFlag(argv, flag, v) {
  const out = [...argv];
  for (let i = 0; i < out.length; i++) {
    if (out[i] === flag && i + 1 < out.length) { out[i + 1] = v; return out; }
    if (typeof out[i] === "string" && out[i].startsWith(flag + "=")) { out[i] = `${flag}=${v}`; return out; }
  }
  let at = claudeAt(out) + 1 || 1;
  if (flag === "--effort") out.forEach((a, i) => { if (a === "--model" && i + 1 < out.length) at = i + 2; else if (String(a).startsWith("--model=")) at = i + 1; });
  out.splice(Math.min(at, out.length), 0, flag, v);
  return out;
}

// Reads the file, applies `change(file)` to a copy, and writes it against the text just read.
// False when the file is not JSON (left alone) or the write failed; `error` says why.
async function update(change) {
  for (let i = 0; i < 2; i++) {
    const c = await loadAgents();
    if (c.bad) return false;
    const file = JSON.parse(JSON.stringify(c.file));   // Herdr's runtime has no structuredClone (#80)
    change(file);
    const text = JSON.stringify(file, null, 2) + "\n";
    if (c.text == null) {
      const dir = c.path.slice(0, c.path.lastIndexOf("/"));
      try { await exec("mkdir", ["-p", dir]); } catch { /* the write says it */ }
    }
    try { await writeTextFile(c.path, text, c.text); }
    catch (e) { if (i === 1) setShared(KEY_AGENTS, { ...c, error: `agents.json could not be written: ${e?.message ?? e}` }); continue; }
    setShared(KEY_AGENTS, { path: c.path, file, bad: false, text, error: null });
    return true;
  }
  return false;
}

// One flow key. The flow is written whole (as contr's Save does), the flow's other keys kept.
export function setFlow(key, value) {
  return update((f) => {
    const was = f.flow && typeof f.flow === "object" && !Array.isArray(f.flow) ? f.flow : {};
    f.flow = { ...DEFAULT_FLOW, ...was, [key]: value };
  });
}

// One role's model or effort: only that flag changes in the command as it is; the profile's other keys
// stay. A command that is not claude is never replaced (#80): no change.
export function setRole(name, what, value) {
  return update((f) => {
    if (!f.profiles || typeof f.profiles !== "object" || Array.isArray(f.profiles)) f.profiles = {};
    const p = f.profiles[name] && typeof f.profiles[name] === "object" ? f.profiles[name] : {};
    let argv = Array.isArray(p.argv) && p.argv.length ? p.argv : DEFAULT_PROFILES[name];
    if (claudeAt(argv) < 0) return;
    argv = setFlag(argv, what === "model" ? "--model" : "--effort", value);
    f.profiles[name] = { ...p, argv };
  });
}

// The section's body. `ui`: { seg, small, heading, about, t } from settings.js.
export function orchestrationBody({ t, seg, small, heading, about, div, h_flex, v_flex }, view) {
  const bad = agentsBad();
  // A change that throws shows its error; it is not dropped (#80).
  const run = (p, cx) => cx.spawn(async (cx) => {
    try { await p(); } catch (e) { setShared(KEY_AGENTS, { ...agents(), error: `The change failed: ${e?.message ?? e}` }); }
    cx.notify();
  });
  const body = v_flex().w_full();
  const st = agents();
  if (bad) body.child(div().id("set-agents-bad").mt(12).p(10).rounded(8).bg(t.raised).border(1).border_color(t.blocked)
    .text_size(12.5).text_color(t.foreground)
    .child("agents.json could not be read (it is not valid JSON), so contr's defaults show. Fix the file to change these here."));
  if (st.error) body.child(div().id("set-agents-error").mt(12).p(10).rounded(8).bg(t.raised).border(1).border_color(t.blocked)
    .text_size(12.5).text_color(t.foreground).child(st.error));
  const line = (id, title, text, control) => h_flex().id(id).w_full().py(14).gap(24).items_center().border_b(1).border_color(t.border)
    .child(about(title, text)).child(control);
  const pick = (key) => seg(`flow-${key}`, FLOW_CHOICES[key], (v, cx) => run(() => setFlow(key, v), cx), flowValue(key), bad);
  const step = (key, unit) => {
    const n = flowValue(key);
    const go = (d) => (cx) => { if (!bad && n + d >= 1 && n + d <= STEP_MAX) run(() => setFlow(key, n + d), cx); };
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
  for (const [name, title, does] of ROLES) {
    // A command that is not claude: its text shows and the switches are off; it is never replaced (#80).
    const custom = !roleIsClaude(name);
    body.child(v_flex().id(`set-role-${name}`).w_full().py(14).gap(10).border_b(1).border_color(t.border)
      .child(about(title, custom ? `${does} Custom: ${argvOf(name).join(" ")}` : does))
      .child(h_flex().w_full().flex_wrap().gap(12).items_center()
        .child(seg(`role-${name}-model`, MODELS, (v, cx) => run(() => setRole(name, "model", v), cx), roleValue(name, "model"), bad || custom))
        .child(seg(`role-${name}-effort`, EFFORTS, (v, cx) => run(() => setRole(name, "effort", v), cx), roleValue(name, "effort"), bad || custom))));
  }
  body.child(div().pt(12).text_size(11).text_color(t.dim).child(`Saved in ${st.path ?? "contr's agents.json"}. contr's coordinator reads it.`));
  return body;
}
