// The chat's rows for one agent, kept in step with the sidecar's `feed` answers.
export function emptyFeed(address) {
  return { address, rows: [], tail: [], older: [], start: null, olderStart: null, first: false,
    cursor: null, path: null, agent: null, model: null, error: null, loaded: false };
}

// `rows` is what the chat draws: the older rows read on scroll-up, then the tail the feed keeps current.
function join(state) { state.rows = state.older.concat(state.tail); }
function dropOlder(state) { state.older = []; state.olderStart = null; state.first = false; }

// Where the next `feed --before` reads up to; null when the log's first row is already shown.
export function olderFrom(state) {
  if (state.first || !state.path || state.start === null || state.start === 0) return null;
  return state.olderStart ?? state.start;
}

// Applies a `feed --before` answer. Returns how many rows went on top (0 for none).
export function applyOlder(state, answer, before) {
  if (answer.error) return 0;
  const head = answer.lines.find((l) => l.type === "older");
  if (!head || head.path !== state.path || before !== olderFrom(state)) return 0;
  const have = new Set(state.rows.map((r) => r.id));
  const add = answer.lines.filter((l) => l.type === "row" && !have.has(l.id));
  state.older = add.concat(state.older);
  state.olderStart = head.start; state.first = head.start === 0;
  join(state);
  return add.length;
}

// Applies one `feed` answer (parsed by sidecar.parseAnswer). Returns the index of the first row that changed,
// or -1 when nothing changed.
export function applyFeed(state, answer) {
  if (answer.error) {
    const changed = !state.error || state.error.code !== answer.error.code;
    state.error = answer.error; state.loaded = true;
    if (answer.error.code === "protocol" || answer.error.code === "no-sidecar") { state.tail = []; dropOlder(state); join(state); state.cursor = null; }
    return changed ? 0 : -1;
  }
  const hadError = Boolean(state.error);
  state.error = null; state.loaded = true;
  let changedAt = -1, splice = null;
  const rows = [];
  for (const l of answer.lines) {
    if (l.type === "log") {
      if (l.path !== state.path || (l.start ?? null) !== state.start) {
        if (l.path !== state.path) { state.tail = []; changedAt = 0; }
        if (state.older.length) changedAt = 0;
        dropOlder(state);
      }
      state.start = l.start ?? null;
      state.path = l.path ?? null; state.agent = l.agent ?? state.agent; state.cursor = l.cursor ?? null;
      if (l.model) state.model = l.model;
    } else if (l.type === "splice") splice = l;
    else if (l.type === "row") rows.push(l);
  }
  if (splice) {
    const at = Math.max(0, Math.min(splice.at, state.tail.length));
    state.tail = state.tail.slice(0, at).concat(rows);
    const shown = state.older.length + at;
    changedAt = changedAt === -1 ? shown : Math.min(changedAt, shown);
  }
  join(state);
  return hadError && changedAt === -1 ? 0 : changedAt;
}

// A message sent from the composer shows at once, dimmed, as an echo at the end of the rows. It goes away when
// the agent's log has a user row with the same text, when the send fails, or after ECHO_MS.
export const ECHO_MS = 120000;
const norm = (s) => (s || "").replace(/\s+/g, " ").trim();
const logged = (state, text) => state.tail.filter((r) => r.k === "user" && r.from == null && norm(r.text) === norm(text)).length;

// Adds an echo of `text` and returns it. `seen` counts the same text already in the log, so a repeat waits for its own row.
export function addEcho(state, text, at = Date.now()) {
  const echoes = state.echoes ?? [];
  const echo = { id: `echo-${at}-${echoes.length}`, k: "user", text, echo: true, at, seen: logged(state, text) + echoes.filter((e) => norm(e.text) === norm(text)).length };
  state.echoes = echoes.concat(echo);
  return echo;
}

export function dropEcho(state, echo) {
  const before = state.echoes?.length ?? 0;
  state.echoes = (state.echoes ?? []).filter((e) => e !== echo);
  return state.echoes.length !== before;
}

// Drops the echoes the log has now, and the old ones. Returns true when one went away.
export function settleEchoes(state, at = Date.now()) {
  const before = state.echoes?.length ?? 0;
  if (!before) return false;
  state.echoes = state.echoes.filter((e) => at - e.at < ECHO_MS && logged(state, e.text) <= e.seen);
  return state.echoes.length !== before;
}

// The rows the chat draws: the feed's rows, then the echoes that wait.
export function shownRows(state) {
  return state.echoes?.length ? state.rows.concat(state.echoes) : state.rows;
}
