// The plan usage rows at the foot of the sidebar: one per plan (Claude, Codex) with data. The helper's `usage`
// command reads them (docs/protocol.md); the mod asks it every USAGE_MS, off the UI thread, once for all the
// views that show them. A plan with no data has no row.
// Two places draw the plans: the mod's own sidebar (side.js, usageRows), and the `foot` block of the herdr-contr
// layout (foot.js, footRows), which is as tall as the plans with data need.
import { div } from "gpui-kit";
import { h_flex } from "gpui-base";
import { layout, run } from "herdr";
import * as sidecar from "./sidecar.js";

// The id of the foot block in layouts/herdr-contr.json, and its height for n plan rows: a 1 px border, then
// (with plans) 12 px, the 16 px rows 10 px apart and 6 px, then the 44 px Settings row (4 + 30 + 10).
export const FOOT_BLOCK = "foot";
export const FOOT_ROW_H = 16;
export const footHeight = (n) => 1 + (n ? 12 + n * FOOT_ROW_H + (n - 1) * 10 + 6 : 0) + 44;

export const USAGE_MS = 5 * 60_000;
const NAMES = { claude: "Claude", codex: "Codex" };
// The window kinds: Claude's five_hour and seven_day, Codex's primary (5 h) and secondary (week).
const SHORT = { five_hour: "5h", primary: "5h", seven_day: "wk", secondary: "wk" };

const state = { plans: [], at: 0, busy: false, px: null };
// For tests.
export const resetUsage = () => Object.assign(state, { plans: [], at: 0, busy: false, px: null });

function find(node, id) {
  if (node?.id === id) return node;
  for (const c of node?.children ?? []) { const f = find(c, id); if (f) return f; }
  return null;
}

// The layout's foot block takes the height of its rows. The block's own height in the layout decides (#133): an
// install can load the layout again (the block is back at its file height) and keep this module, so a height
// remembered here is not enough. A layout that gives no height: `again` (each read) sends the height again.
export const FIT_MS = 30_000;
export function fitFoot(again = false) {
  const px = footHeight(state.plans.length);
  const node = find(layout(), FOOT_BLOCK);
  if (!node) return;
  const now = node.size?.px;
  if (now != null ? now === px : px === state.px && !again) return;
  state.px = px;
  run("layout.resize", { id: FOOT_BLOCK, px });
}
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
    fitFoot(true);
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

// The foot's value for a plan: its most used window as "41% wk · 4d" (percent used, kind, time to reset);
// no time when the reset is not known or past.
export function planValue(p, nowSecs) {
  let top = null;
  for (const w of p.windows) {
    const past = w.resets_at != null && w.resets_at <= nowSecs;
    const used = past ? 0 : Math.round(Math.min(Math.max(w.used_percent, 0), 100));
    if (!top || used > top.used) top = { used, kind: SHORT[w.kind] ?? w.kind, reset: !past && w.resets_at != null ? untilText(w.resets_at - nowSecs) : null };
  }
  return { name: NAMES[p.plan], used: top.used, text: `${top.used}% ${top.kind}${top.reset ? ` · ${top.reset}` : ""}` };
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

// The foot's rows: the name in a 52 px column (12 px, muted), a 4 px bar (a faint track, the used part in
// mid grey), and the value (Menlo 11, muted), 10 px apart.
// `rail`: the bar only, with the name and value in a tooltip.
export function footRows(t, rail = false, nowSecs = Date.now() / 1000) {
  return state.plans.map((p) => {
    const r = planValue(p, nowSecs);
    return h_flex().id(`foot-usage-${p.plan}`).h(FOOT_ROW_H).flex_none().gap(10).items_center()
      .when(rail, (el) => el.tooltip(`${r.name} ${r.text}`))
      .when(!rail, (el) => el.child(div().w(52).flex_none().text_size(12).text_color(t.muted).child(r.name)))
      .child(div().id(`foot-usage-${p.plan}-bar`).flex_1().min_w(0).h(4).rounded(2).bg(t.hover)
        .child(div().h(4).rounded(2).bg(t.dim).w(`${r.used}%`)))
      .when(!rail, (el) => el.child(div().flex_none().font_family("Menlo").text_size(11).text_color(t.muted).child(r.text)));
  });
}

// Starts the reads: one now and one every USAGE_MS. The foot block's height is checked every FIT_MS, with no read.
export function startUsage(cx) {
  cx?.spawn?.((cx2) => pollUsage(cx2));
  cx?.timer?.every?.(USAGE_MS, (cx2) => cx2?.spawn?.((cx3) => pollUsage(cx3)));
  cx?.timer?.every?.(FIT_MS, () => fitFoot());
}
