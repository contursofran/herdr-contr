// The Version row of the settings (#219): the installed and the newest release build (helper `update check`),
// and Update, which runs `update install` (docs/protocol.md, update). Nothing installs without a press.
import { call } from "./sidecar.js";

export const UPDATE_MS = 300000;

let state = { line: null, error: null, busy: false, done: null };
export const updateState = () => state;
export const resetUpdate = () => { state = { line: null, error: null, busy: false, done: null }; };

// When the settings open: the helper keeps the newest build a day (update.json).
export async function checkUpdate() {
  const r = await call("update", ["check"]);
  state = { ...state, line: r.lines.find((l) => l.type === "update") ?? null, error: r.error?.message ?? null };
}

// After the press only. On success the row says to restart Herdr, so the new mod loads.
export async function runUpdate() {
  if (state.busy) return;
  state = { ...state, busy: true, done: null, error: null };
  // A download and an install take longer than exec's 30 s default: give it 5 minutes.
  const r = await call("update", ["install"], { timeout: UPDATE_MS });
  const done = r.lines.find((l) => l.type === "update-done");
  state = { ...state, busy: false, error: r.error?.message ?? null, done: done ?? null,
    line: done && state.line ? { ...state.line, installed: done.installed, differs: false } : state.line };
}

export function updateBody(t, owner, { small, about, div, h_flex, v_flex }) {
  const s = state, l = s.line;
  const title = `herdr-contr ${l?.installed ?? "…"}`;
  const line = !l ? (s.error ? "The newest build is not known." : "Looking for the newest build…")
    : l.differs ? `Newest: ${l.newest}` : "This is the newest build.";
  const go = (cx) => cx.spawn(async (cx) => { await runUpdate(); cx.notify(); });
  const result = s.busy ? "Updating…" : s.done ? `Installed ${s.done.installed}. Restart Herdr to load it.` : s.error;
  return v_flex().id("set-update").w_full().py(14).gap(8).border_b(1).border_color(t.border)
    .child(h_flex().w_full().gap(24).items_center()
      .child(about(title, line))
      .when(Boolean(l?.differs) && !s.busy, (el) => el.child(small("set-update-go", "Update", go))))
    .when(Boolean(result), (el) => el.child(div().id("set-update-result").text_size(12)
      .text_color(s.error ? t.blocked : t.muted).child(result)));
}
