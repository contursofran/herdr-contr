// The herdr-contr row of the herdr-contr layout: a block between the usage rows and Herdr's sidebar footer
// (Settings, Add machine). It looks like Herdr's footer rows and opens the herdr-contr settings dialog.
// The mod's own sidebar draws its own row (side.js).
import { div, svg } from "gpui-kit";
import { h_flex, v_flex } from "gpui-base";
import { theme, layout } from "herdr";
import { showSettings, icon } from "./state.js";

export const SETTINGS_KEY = "⌘⇧,";

function find(node, id) {
  if (node?.id === id) return node;
  for (const c of node?.children ?? []) { const f = find(c, id); if (f) return f; }
  return null;
}
// As Herdr's footer: the rail goes by the sidebar's width.
const narrow = () => (find(layout(), "sidebar")?.size?.px ?? Infinity) < 160;

export class SettingsRowView {
  constructor() { this.hovered = false; }
  render(cx) {
    const t = theme();
    const collapsed = narrow();
    const row = h_flex().id("herdr-contr-settings").h(28).px(8).gap(9).items_center().rounded(8)
      .when(collapsed, (el) => el.w(32).px(0).justify_center().tooltip("herdr-contr"))
      .text_color(t.muted).hover((s) => s.bg(t.hover).text_color(t.foreground)).cursor_pointer()
      .on_hover((h, cx2) => { if (this.hovered !== h) { this.hovered = h; (cx2 ?? cx)?.notify?.(); } })
      .on_mouse_down("left", (_e, cx2) => cx2?.stop_propagation?.())
      .on_click(() => showSettings())
      .child(svg(icon("sliders")).size(14))
      .when(!collapsed, (el) => el.child(div().flex_1().min_w(0).truncate().child("herdr-contr"))
        .when(this.hovered, (el2) => el2.child(div().id("herdr-contr-settings-hint").flex_none().text_size(11.5).text_color(t.dim).child(SETTINGS_KEY))));
    return v_flex().id("settings-row-block").size_full().bg(t.chrome).px(8).pt(4).text_size(13)
      .when(collapsed, (el) => el.px(0).items_center())
      .child(row);
  }
}
