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

// A worker's text: what it works on (its name), else its template or program.
export const workerText = (k) => k.name || k.template || k.program?.name || k.address;

const clean = (s) => (s ?? "").replace(/^[^\p{L}\p{N}]+/u, "").trim();

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
  const prog = l?.program?.name;
  const live = clean(l?.title);
  // clean() would strip a path's leading ~ or /: test the title with only its status glyph gone.
  const path = /^[~/]|^[A-Za-z]:\\/.test((l?.title ?? "").replace(/^[^\p{L}\p{N}~/]+/u, ""));
  const telling = live && live !== prog && !path && !live.includes("@") && live.toLowerCase() !== l?.program?.key;
  return (tab.name === prog || !tab.named) && telling ? live : tab.name;
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
