// The sidebar's foot in the herdr-contr layout (#97), in place of Herdr's footer: the Claude and Codex usage
// bars (usage.js), then one row: Settings, which fills the width, and the add machine button at its right.
// Settings opens Herdr's settings; a right-click on it opens the herdr-contr settings (also ⌘⇧,).
import { div, svg } from "gpui-kit";
import { h_flex, v_flex } from "gpui-base";
import { theme, layout, run, updates } from "herdr";
import { showSettings } from "./state.js";
import { startUsage, footRows } from "./usage.js";

const ICON = "mods/workspace-chrome/";

function find(node, id) {
  if (node?.id === id) return node;
  for (const c of node?.children ?? []) { const f = find(c, id); if (f) return f; }
  return null;
}
// As Herdr's footer: the rail goes by the sidebar's width.
const narrow = () => (find(layout(), "sidebar")?.size?.px ?? Infinity) < 160;

export const SETTINGS_TIP = "Settings (⌘,). Right-click: herdr-contr settings (⌘⇧,)";

export class FootView {
  constructor(cx) { startUsage(cx); }
  render() {
    const t = theme();
    const collapsed = narrow();
    const ready = Boolean(updates()?.app?.ready);
    const dot = div().id("foot-update").size(6).flex_none().rounded_full().bg(t.accent);
    const settings = h_flex().id("foot-settings").h(30).min_w(0).px(10).gap(8).items_center().rounded(8)
      .text_size(13).text_color(t.muted).cursor_pointer().hover((s) => s.bg(t.hover).text_color(t.foreground))
      .tooltip(SETTINGS_TIP)
      .on_click(() => run("app.settings"))
      .on_mouse_down("right", () => showSettings())
      .child(svg(`${ICON}settings.svg`).size(16).flex_none())
      .when(collapsed, (el) => el.relative().w(32).px(0).justify_center()
        .when(ready, (el2) => el2.child(dot.absolute().top(6).right(6))))
      .when(!collapsed, (el) => el.flex_1().child(div().flex_1().min_w(0).truncate().child("Settings"))
        .when(ready, (el2) => el2.child(dot)));
    const add = div().id("foot-add-machine").size(28).flex_none().flex().items_center().justify_center().rounded(8)
      .text_color(t.muted).hover((s) => s.bg(t.hover).text_color(t.foreground)).tooltip("Add machine")
      .on_click(() => run("mod.open", { name: "machine-add" }))
      .child(svg(`${ICON}plus.svg`).size(16));
    // On the rail: the bars only (the name and value in a tooltip), and Settings only (the machines menu has
    // "Add machine…" too); the block keeps the height of the full foot.
    const plans = footRows(t, collapsed);
    return v_flex().id("foot").size_full().bg(t.chrome).border_t(1).border_color(t.border)
      .when(plans.length > 0, (el) => el.child(v_flex().flex_none().pt(12).px(collapsed ? 8 : 20).pb(6).gap(10).children(plans)))
      .child(h_flex().flex_none().pt(4).px(10).pb(10).gap(4).items_center()
        .when(collapsed, (el) => el.px(0).justify_center().child(settings))
        .when(!collapsed, (el) => el.child(settings).child(add)));
  }
}
