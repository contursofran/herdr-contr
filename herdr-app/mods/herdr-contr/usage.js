// The plan usage rows at the foot of the sidebar: one per plan (Claude, Codex) with data. The helper's `usage`
// command reads them (docs/protocol.md); the mod asks it every USAGE_MS, off the UI thread, once for both copies
// of the sidebar (docked and floating). A plan with no data has no row.
import { div } from "gpui-kit";
import { h_flex } from "gpui-base";
import * as sidecar from "./sidecar.js";

export const USAGE_MS = 5 * 60_000;
const NAMES = { claude: "Claude", codex: "Codex" };
// The window kinds: Claude's five_hour and seven_day, Codex's primary (5 h) and secondary (week).
const SHORT = { five_hour: "5h", primary: "5h", seven_day: "wk", secondary: "wk" };

const state = { plans: [], at: 0, busy: false };
// For tests.
export const resetUsage = () => Object.assign(state, { plans: [], at: 0, busy: false });
export const usagePlans = () => state.plans;

// Reads the plans again when the last read is USAGE_MS old. An error or an older helper keeps what was shown.
export async function pollUsage(cx, now = Date.now()) {
  if (state.busy || (state.at && now - state.at < USAGE_MS)) return;
  state.busy = true; state.at = now;
  try {
    if (!(await sidecar.hasCommand("usage"))) return;
    const r = await sidecar.usage();
    const l = r.error ? null : r.lines.find((x) => x.type === "usage");
    if (!l) return;
    state.plans = (l.plans ?? []).filter((p) => NAMES[p.plan] && p.windows?.length);
    cx?.notify?.();
  } finally { state.busy = false; }
}

// "45m", "3h", "2d": the time to a reset.
export function untilText(secs) {
  if (secs < 3600) return `${Math.max(1, Math.round(secs / 60))}m`;
  if (secs < 86400) return `${Math.round(secs / 3600)}h`;
  return `${Math.round(secs / 86400)}d`;
}

// One plan's row text and meter: each window as "5h 38% 2h" (kind, whole percent used, time to reset); a
// window past its reset is 0% with no time. The meter shows the most used window.
export function planRow(p, nowSecs) {
  const parts = p.windows.map((w) => {
    const past = w.resets_at != null && w.resets_at <= nowSecs;
    const used = past ? 0 : Math.round(Math.min(Math.max(w.used_percent, 0), 100));
    const reset = !past && w.resets_at != null ? ` ${untilText(w.resets_at - nowSecs)}` : "";
    return { used, text: `${SHORT[w.kind] ?? w.kind} ${used}%${reset}` };
  });
  return { name: NAMES[p.plan], used: Math.max(...parts.map((x) => x.used)), text: parts.map((x) => x.text).join(" · ") };
}

// The rows: a 22 px column with a 16 px meter (as the icons of the rows above), the plan's name, then its windows.
export function usageRows(t, nowSecs = Date.now() / 1000) {
  return state.plans.map((p) => {
    const r = planRow(p, nowSecs);
    return h_flex().id(`side-usage-${p.plan}`).h(24).px(10).gap(9).items_center().text_size(11.5).text_color(t.muted)
      .child(h_flex().w(22).flex_none().justify_center()
        .child(div().id(`side-usage-${p.plan}-meter`).w(18).h(4).rounded(2).bg(t.border)
          .child(div().h(4).rounded(2).bg(t.muted).w(Math.round(18 * r.used / 100)))))
      .child(div().flex_none().child(r.name))
      .child(div().flex_1().min_w(0).truncate().font_family("Menlo").text_size(10.5).text_color(t.dim).child(r.text));
  });
}
