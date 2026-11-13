// The main agents in the order of the sidebar's Projects list, for the Control+1 … Control+9 keys.
// herdr-contr and herdr-contr-spaces each have a copy of this file (a mod cannot import another mod's
// files). A test keeps the two copies equal: change both.
//
// A main agent is an agent that is not a worker of another (an orchestrator, or an agent with no
// parent among the listed agents). A worker is a live sub-agent of a listed agent, at any depth: it has
// no number. The order: this computer's projects first, then the projects of each other machine by
// machine name; in a machine, the spaces in spaces() order (the Projects list moves the spaces into its
// order); in a space, the agents in agents() order.

export const MAIN_KEYS = 9;

// Adds the live sub-agents of `addr` to `out`, at any depth.
function under(all, addr, out) {
  for (const k of all) {
    if (k.parent !== addr || k.state !== "live" || out.has(k.address)) continue;
    out.add(k.address);
    under(all, k.address, out);
  }
  return out;
}

// `spaceList`: spaces(); `list`: agents(); `all`: terminals().
export function mainAgents(spaceList, list, all) {
  const nested = new Set();
  for (const a of list) under(all, a.address, nested);
  const m = (s) => s.machine ?? "";
  const rank = (x, y) => (m(x.s) === m(y.s) ? x.i - y.i : !m(x.s) ? -1 : !m(y.s) ? 1 : m(x.s) < m(y.s) ? -1 : 1);
  const order = spaceList.map((s, i) => ({ s, i })).sort(rank);
  return order.flatMap(({ s }) => list.filter((a) => a.space === s.id && !nested.has(a.address)));
}

// Agent address -> its key number (1 … 9). The 10th main agent and later have none.
export function mainKeys(spaceList, list, all) {
  return new Map(mainAgents(spaceList, list, all).slice(0, MAIN_KEYS).map((a, i) => [a.address, i + 1]));
}

// The key hint of a number: "⌃1".
export const mainKeyHint = (n) => `⌃${n}`;
