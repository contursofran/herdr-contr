// The Projects list's data: each space's agents, each agent's workers, and the spaces with no agent.
// Pure functions: main.js draws them. The labels and the worker tree come from our copy of Herdr's agents mod.

// Live sub-agents of `addr` (hc gives each terminal its parent), depth first, with their depth.
export function workers(all, addr, depth = 0) {
  const kids = all.filter((k) => k.parent === addr && k.state === "live");
  return kids.flatMap((k) => [{ t: k, depth }, ...workers(all, k.address, depth + 1)]);
}

// agents() by space id: [{ a, workers }] in agents() order. A sub-agent with a pane is in agents() too;
// it shows only under its parent. An agent with workers is an orchestrator.
export function bySpace(list, all) {
  const trees = new Map(list.map((a) => [a.address, workers(all, a.address)]));
  const nested = new Set([...trees.values()].flat().map((s) => s.t.address));
  const out = new Map();
  for (const a of list) {
    if (nested.has(a.address)) continue;
    if (!out.has(a.space)) out.set(a.space, []);
    out.get(a.space).push({ a, workers: trees.get(a.address) });
  }
  return out;
}

export const countLine = (n) => `Orchestrating · ${n} worker${n === 1 ? "" : "s"}`;
export const moreLine = (n) => `${n} more project${n === 1 ? "" : "s"}, no agents`;

// Rows in list order (a worktree row after its main checkout) split into the rows shown and the rows of
// the "N more projects" row. A main and its worktrees go together: there only when none of them has an
// agent and none is the shown space.
export function split(rows, has) {
  const units = [];
  for (const r of rows) {
    if (!r.tree || !units.length) units.push([]);
    units.at(-1).push(r);
  }
  const shown = [], idle = [];
  for (const u of units) (u.some((r) => r.w.active || has(r.w.id)) ? shown : idle).push(...u);
  return { shown, idle };
}

// A worker's name: its name, else its template or program, else its address.
export const workerText = (k) => k.name || k.template || k.program?.name || k.address;

const clean = (s) => (s ?? "").replace(/^[^\p{L}\p{N}]+/u, "").trim();

// A title that says something: not empty, not the program's name or key, not a path, not user@host.
function telling(raw, program) {
  const live = clean(raw);
  // clean() would strip a path's leading ~ or /: test the title with only its status glyph gone.
  const path = /^[~/]|^[A-Za-z]:\\/.test((raw ?? "").replace(/^[^\p{L}\p{N}~/]+/u, ""));
  return live && live !== program?.name && !path && !live.includes("@") && live.toLowerCase() !== program?.key ? live : "";
}

// What a worker works on: its session's own title (`title`: the agent's terminal title), when it says
// something and is not the worker's name; else "" (the row shows the name alone).
export function workerWork(k, title) {
  const w = telling(title, k.program);
  return w && w !== k.name && w !== k.address ? w : "";
}

const oneLine = (s) => String(s ?? "").replace(/\s+/g, " ").trim();

// A worker row's texts. `work`: what it works on: the title that the orchestration tool keeps as the hc label
// `title` (spawn --title, `agents title`), else workerWork(); "" when neither says something. `name`: the
// worker's name, shown muted at the right when there is work. `tip`: the tooltip's first line, the name,
// the issue ("#194", from the `issue` label) and the work; "" when it would be the name alone.
export function workerRow(k, title) {
  const label = oneLine(k.labels?.title);
  const work = label && label !== k.name && label !== k.address ? label : workerWork(k, title);
  const name = workerText(k);
  const issue = oneLine(k.labels?.issue);
  const parts = [name, /^\d+$/.test(issue) ? `#${issue}` : issue, work].filter(Boolean);
  return { work, name, tip: parts.length > 1 ? parts.join(" · ") : "" };
}

// Each tab's lead pane: its first agent, else its first focused terminal, else its first terminal.
export function leadPanes(all) {
  const leads = new Map();
  for (const p of all) {
    if (!p.terminal) continue;
    const current = leads.get(p.tab);
    if (!current || (!current.program?.agent && (p.program?.agent || (!current.focused && p.focused)))) leads.set(p.tab, p);
  }
  return leads;
}

function title(tab, l) {
  const live = telling(l?.title, l?.program);
  return (tab.name === l?.program?.name || !tab.named) && live ? live : tab.name;
}

// An agent's title: a tab someone named, or the agent's own title; never an automatic tab number.
// `tabs`: tab id -> tab; `leads`: from leadPanes(); `many`: the tab has more than one agent.
export function agentLabel(a, tabs, leads, many) {
  const tab = tabs.get(a.tab);
  const l = tab ? title(many ? { ...tab, named: false } : tab, many ? a : leads.get(a.tab)) : (a.title || a.program?.name || "");
  return /^\d+$/.test(l) ? a.program?.name || l : l;
}

export function duration(ms) {
  const m = Math.floor(ms / 60000);
  if (m < 1) return "<1m";
  if (m < 60) return `${m}m`;
  const h = Math.floor(m / 60);
  return h < 24 ? `${h}h` : `${Math.floor(h / 24)}d`;
}

const CLAUDE = /^claude-(?:(\d+)-(\d+)-)?([a-z]+)(?:-(\d+)(?:-(\d{1,2}))?)?(?:-\d{8})?$/;
// "claude-opus-5-5" -> "Opus 5.5", "gpt-6" -> "GPT-6"; another name as it is.
export function modelName(model) {
  const m = (model ?? "").trim();
  const c = m.match(CLAUDE);
  if (c) {
    const ver = c[1] ? `${c[1]}.${c[2]}` : c[4] ? (c[5] ? `${c[4]}.${c[5]}` : c[4]) : "";
    return [c[3][0].toUpperCase() + c[3].slice(1), ver].filter(Boolean).join(" ");
  }
  if (/^gpt-/i.test(m)) return `GPT-${m.split("-")[1]}`;
  if (/^(opus|sonnet|haiku|fable)$/i.test(m)) return m[0].toUpperCase() + m.slice(1).toLowerCase();
  return m;
}

// The tooltip of an agent or worker row: "Opus 5.5 · medium effort". No model: no tooltip ("").
export function modelTip(model, effort) {
  const name = modelName(model);
  if (!name) return "";
  const e = (effort ?? "").trim();
  return e ? `${name} · ${e} effort` : name;
}

// How often the helper is asked for one terminal's model, at most.
export const MODEL_MS = 15000;
// Whether to ask the helper for a terminal's model now. `m`: what was asked last ({ status, at, busy }), or
// undefined. Asked when the terminal is new and when its status changed (a turn can change the model or
// the effort), never twice in MODEL_MS.
export const modelDue = (m, status, now) => !m || (!m.busy && m.status !== status && now - m.at >= MODEL_MS);
