// The Chat | Terminal switch for a tab with an agent: over the chat, and in its own block over the tab's
// terminal (layout block "view-switch"). A press shows the other screen and keeps the pick for the tab.
import { div } from "gpui-kit";
import { h_flex } from "gpui-base";
import { theme } from "herdr";
import { setView, shownScreen, activeTab, tabAgents } from "./state.js";
import { startTop } from "./top.js";

export const SWITCH_H = 36;
export const KEY_HINT = "⌘J";

export function viewSwitch(t, shown = shownScreen()) {
  const seg = (child, label) => {
    const on = shown === child;
    return div().id(`view-${child}`).h(22).px(10).flex().items_center().rounded(6).text_size(12)
      .bg(on ? t.raised : t.chrome).text_color(on ? t.foreground : t.muted)
      .when(on, (el) => el.border(1).border_color(t.border))
      .when(!on, (el) => el.hover((s) => s.text_color(t.foreground)))
      .tooltip(`${label} (${KEY_HINT})`)
      .on_mouse_down("left", (_e, cx) => cx.stop_propagation())
      .on_click(() => { if (!on) setView(child); })
      .child(label);
  };
  return h_flex().id("view-switch").p(2).gap(2).items_center().rounded(8).bg(t.chrome).border(1).border_color(t.border)
    .child(seg("chat", "Chat")).child(seg("tab", "Terminal"));
}

// The strip that holds the switch, on the right.
export const switchStrip = (t, shown) => h_flex().w_full().h(SWITCH_H).flex_none().px(10).items_center().justify_end()
  .child(viewSwitch(t, shown));

// The "view-switch" block over the tab's terminal. Empty for a tab without an agent (the sync hides the block).
// It is the only herdr-contr view on that screen, so it runs the key commands and the screen sync (#171):
// without them, the key did not go back from the terminal to the chat.
export class SwitchView {
  constructor(cx) { this.top = {}; startTop(cx, this.top); }
  render(_cx) {
    const t = theme(), tab = activeTab();
    if (!tab || !tabAgents(tab.id).length) return div().size_full().bg(t.background);
    return div().size_full().bg(t.background).child(switchStrip(t, "tab"));
  }
}
