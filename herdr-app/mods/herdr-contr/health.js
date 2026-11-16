// The Health section of Settings (#217): the helper's `health` checks of this install, a row a check with
// its state (ok, warn, fail), what it found and the fix, and a Re-check button. The mod gives the helper
// what only Herdr knows: the layout in use, Herdr's version and hc's version.
import { shared, updates } from "herdr";
import { call } from "./sidecar.js";

let rows = null;     // null: not checked yet
let error = null;    // why the helper gave no rows
let busy = false;

export const healthRows = () => rows;
export const healthError = () => error;
export const forgetHealth = () => { rows = null; error = null; busy = false; };

// Runs the checks once; a second call while one runs does nothing.
export async function checkHealth() {
  if (busy) return;
  busy = true;
  const u = updates() ?? {};
  const args = [];
  const add = (flag, v) => { if (v) args.push(flag, String(v)); };
  add("--layout", shared("layout"));
  add("--herdr", u.app?.version);
  add("--hc", u.core?.version);
  const a = await call("health", args);
  busy = false;
  if (a.error) { rows = []; error = a.error.message; return; }
  rows = a.lines.filter((l) => l.type === "check");
  error = rows.length ? null : "The helper did not check anything: it may be older than this mod. Install again.";
}

const MARK = { ok: "✓", warn: "!", fail: "✕" };

// The section's body, from the settings view's parts (settings.js).
export function healthBody({ t, small, heading, div, h_flex, v_flex }) {
  const color = { ok: t.done, warn: t.working, fail: t.blocked };
  const recheck = small("set-health-recheck", busy ? "Checking…" : "Re-check",
    (cx) => cx.spawn(async (cx) => { const p = checkHealth(); cx.notify(); await p; cx.notify(); }));
  const box = v_flex().id("set-health").w_full().gap(0)
    .child(h_flex().w_full().items_center().child(div().flex_1().child(heading("INSTALL"))).child(recheck));
  if (error) box.child(div().id("set-health-error").mt(12).p(10).rounded(8).bg(t.raised).border(1).border_color(t.blocked)
    .text_size(12.5).text_color(t.foreground).child(error));
  if (rows === null) box.child(div().py(14).text_size(12.5).text_color(t.muted).child("Checking…"));
  for (const r of rows ?? []) {
    box.child(h_flex().id(`set-health-${r.id}`).w_full().py(12).gap(12).items_start().border_b(1).border_color(t.border)
      .child(div().w(16).flex_none().text_size(13.5).font_semibold().text_color(color[r.state] ?? t.muted).child(MARK[r.state] ?? "?"))
      .child(v_flex().flex_1().min_w(0).gap(3)
        .child(div().text_size(13).text_color(t.foreground).child(r.message))
        .when(Boolean(r.fix) && r.state !== "ok", (el) => el.child(div().text_size(12).text_color(t.muted).child(`Fix: ${r.fix}`)))));
  }
  return box;
}
