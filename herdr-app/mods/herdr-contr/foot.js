// The sidebar's foot in the herdr-contr layout (#97), in place of Herdr's footer: the Claude and Codex usage
// bars (usage.js), then one row: Settings, which fills the width, and the add machine button at its right.
// A press on Settings opens a small menu above the row (#120): "Herdr settings" and "herdr-contr settings". The
// runtime has no way for a mod to add a section to Herdr's settings window (`app.settings` takes only Herdr's own
// sections), so the menu is the way to both. A right-click still opens the herdr-contr settings (also ⌘⇧,).
import { div, svg } from "gpui-kit";
import { h_flex, v_flex, Popover } from "gpui-base";
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

export const SETTINGS_TIP = "Herdr settings (⌘,) and herdr-contr settings (⌘⇧,)";

// The menu's panel: the look of the model menu (modelmenu.js): raised panel, border, 12 px corners.
export function settingsMenu(t, pick) {
  const row = (id, text, tip) => h_flex().id(id).w_full().min_h(32).px(10).py(6).gap(10).items_center().rounded(8)
    .cursor_pointer().hover((s) => s.bg(t.hover))
    .on_click((_e, cx) => pick(id, cx))
    .child(div().flex_1().min_w(0).truncate().text_size(15).text_color(t.foreground).child(text))
    .child(div().flex_none().text_size(13).text_color(t.muted).child(tip));
  return v_flex().id("settings-menu").w(240).p(4).gap(0).rounded(12).bg(t.raised).border(1).border_color(t.border).shadow_lg()
    .child(row("settings-menu-herdr", "Herdr settings", "⌘,"))
    .child(row("settings-menu-contr", "herdr-contr settings", "⌘⇧,"));
}

export class FootView {
  constructor(cx) { startUsage(cx); this.menu = false; }
  // The Popover's open state changed (a press on Settings, a press outside, Escape).
  openMenu(open, cx) { this.menu = Boolean(open); cx?.notify(); }
  pick(id, cx) {
    this.menu = false;
    cx?.notify();
    if (id === "settings-menu-herdr") run("app.settings");
    else showSettings();
  }
  render() {
    const t = theme();
    const collapsed = narrow();
    const ready = Boolean(updates()?.app?.ready);
    const dot = div().id("foot-update").size(6).flex_none().rounded_full().bg(t.accent);
    const settings = h_flex().id("foot-settings").h(30).min_w(0).px(10).gap(8).items_center().rounded(8)
      .text_size(13).text_color(t.muted).cursor_pointer().hover((s) => s.bg(t.hover).text_color(t.foreground))
      .when(!this.menu, (el) => el.tooltip(SETTINGS_TIP)).when(this.menu, (el) => el.bg(t.hover))
      .on_mouse_down("right", () => showSettings())
      .child(svg(`${ICON}settings.svg`).size(16).flex_none())
      .when(collapsed, (el) => el.relative().w(32).px(0).justify_center()
        .when(ready, (el2) => el2.child(dot.absolute().top(6).right(6))))
      .when(!collapsed, (el) => el.flex_1().child(div().flex_1().min_w(0).truncate().child("Settings"))
        .when(ready, (el2) => el2.child(dot)));
    // The Popover owns the press, a press outside and Escape (as the model menu, #112).
    const settingsEl = Popover.new("settings-popover").anchor("bottom_left").open(this.menu)
      .on_open_change((open, cx) => this.openMenu(open, cx))
      .trigger(settings)
      .content(div().pb(6).child(settingsMenu(t, (id, cx) => this.pick(id, cx))));
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
        .when(collapsed, (el) => el.px(0).justify_center().child(settingsEl))
        // The Popover does not grow by itself: a box that fills the row holds it, so the + stays at the right edge.
        .when(!collapsed, (el) => el.child(div().id("foot-settings-fill").flex_1().min_w(0).child(settingsEl)).child(add)));
  }
}
