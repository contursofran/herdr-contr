// The sidebar's head (#97), in place of Herdr's brand block: "Herdr", the machine switch, a spacer, search and
// the sidebar toggle. The switch picks which machine's projects the Projects list shows; a press on it opens
// the machines menu (another copy of this mod, view "machines"). The Projects list writes the machines it
// knows to shared MACHINES; the pick is shared PICKED (null: this computer).
import { div, svg } from "gpui-kit";
import { v_flex, h_flex } from "gpui-base";
import { theme, run, props, window, shared, setShared, layout, openOverlay, closeOverlay } from "herdr";

export const PICKED = "projects.machine";
export const MACHINES = "projects.machines";
export const LOCAL = "This computer";
const ICON = "mods/workspace-chrome/";
const ROW_H = 30, PAD = 6, MENU_W = 240;

function find(node, id) {
  if (node?.id === id) return node;
  for (const child of node?.children ?? []) {
    const found = find(child, id);
    if (found) return found;
  }
  return null;
}
const narrow = () => (find(layout(), "sidebar")?.size?.px ?? Infinity) < 160;

// The machines other than this computer: [{ name, state }]. state: "connected", "needs login",
// "rebuilt", "unreachable", "asleep" and the other words hc gives.
export const knownMachines = () => (Array.isArray(shared(MACHINES)) ? shared(MACHINES) : []);

// The picked machine's name, or null for this computer (also when the picked one is not known any more).
export function pickedMachine(names = knownMachines().map((m) => m.name)) {
  const p = shared(PICKED);
  return typeof p === "string" && names.includes(p) ? p : null;
}

// As Herdr's workspace-chrome: to the rail (on macOS wide enough for the traffic lights), or back.
function toggleSidebar() {
  const width = find(layout(), "sidebar")?.size?.px;
  if (!narrow()) setShared("sidebar.width", width);
  setShared("sidebar.collapsed", !narrow());
  run("layout.resize", { id: "sidebar", px: narrow() ? (shared("sidebar.width") ?? 268) : window()?.platform === "macos" ? 76 : 44 });
}

export function openMachines() {
  const n = knownMachines().length + 1;
  return openOverlay({ mod: "herdr-contr-spaces", props: { view: "machines" }, kind: "popup",
    width: MENU_W, height: PAD * 2 + (n + 1) * ROW_H + 9 + 2 });
}

// A press in the head is no window drag: the block is `window_drag`.
const keep = (_e, cx) => cx?.stop_propagation?.();

export class HeadView {
  button(id, icon, tip, action, t) {
    return div().id(id).size(28).flex_none().flex().items_center().justify_center().rounded(8).text_color(t.muted)
      .hover((s) => s.bg(t.hover).text_color(t.foreground)).tooltip(tip)
      .on_mouse_down("left", keep).on_click(action).child(svg(`${ICON}${icon}.svg`).size(16));
  }

  render() {
    const t = theme();
    const collapsed = narrow();
    const mac = window()?.platform === "macos";
    const picked = pickedMachine();
    return h_flex().id("head").size_full().bg(t.chrome).pl(collapsed ? 6 : 20).pr(10).gap(4).items_center()
      .when(mac && !window()?.fullscreen && !collapsed, (el) => el.pl(76))
      .when(!collapsed, (el) => el
        .child(div().flex_none().text_size(14).font_semibold().text_color(t.foreground).child("Herdr"))
        .child(h_flex().id("machine-switch").h(26).min_w(0).px(6).gap(4).items_center().rounded(6)
          .text_size(12).text_color(t.muted).cursor_pointer().hover((s) => s.bg(t.hover).text_color(t.foreground))
          .tooltip("Which machine's projects show")
          .on_mouse_down("left", keep).on_click(() => openMachines())
          .child(div().min_w(0).truncate().child(picked ?? LOCAL))
          .child(svg(`${ICON}chevron-down.svg`).size(12).flex_none()))
        .child(div().flex_1())
        .child(this.button("search", "search", "Search", () => run("mod.open", { name: "picker" }), t)))
      // On the macOS rail the traffic lights fill the row.
      .when(!(collapsed && mac), (el) => el.child(this.button("sidebar-toggle", "sidebar", "Hide sidebar", toggleSidebar, t)));
  }
}

// The machines menu: this computer, then each machine (an unreachable one says why, dim), a check at the
// picked one; then "Add machine…". A press picks the machine and closes the menu.
export class MachinesMenu {
  render() {
    const t = theme();
    const list = knownMachines();
    const picked = pickedMachine(list.map((m) => m.name));
    const close = () => closeOverlay(props()?.overlay);
    const row = (id, icon, label, note, on, action) => h_flex().id(id).h(ROW_H).flex_none().px(8).gap(8).items_center().rounded(6)
      .text_size(13).text_color(note ? t.muted : t.foreground).cursor_pointer().hover((s) => s.bg(t.hover))
      .on_click(() => { action(); close(); })
      .child(svg(`${ICON}${icon}.svg`).size(13).flex_none().text_color(t.muted))
      .child(div().flex_1().min_w(0).truncate().child(label))
      .when(Boolean(note), (el) => el.child(div().flex_none().text_size(11.5).text_color(t.dim).child(note)))
      .child(div().w(13).flex_none().when(on, (el) => el.child(svg(`${ICON}check.svg`).size(13).text_color(t.foreground))));
    const pick = (name) => () => setShared(PICKED, name);
    return v_flex().id("machines-menu").size_full().p(PAD).gap(0).bg(t.chrome)
      .child(row("pick-local", "monitor", LOCAL, null, picked === null, pick(null)))
      .children(list.map((m) => row(`pick-${m.name}`, "server", m.name, m.state === "connected" ? null : m.state, picked === m.name, pick(m.name))))
      .child(div().h(1).my(4).flex_none().bg(t.border))
      .child(row("machines-add", "plus", "Add machine…", null, false, () => run("mod.open", { name: "machine-add" })));
  }
}
