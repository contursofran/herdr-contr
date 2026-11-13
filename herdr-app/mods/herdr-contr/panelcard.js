// A panel that the agent's terminal shows in place of its input box (the screen of /model, /config, /agents,
// /resume) as a card above the reply box: the panel's plain text in a mono box, and the keys (#141). Read with the
// helper's `panel`, at a calm rate and only while the agent does not work; keys go out with `keys`, only on the
// owner's key or press. No parsing of each kind of panel: hc gives no text styles, so a selected row shows only when
// the panel marks it with a character. The Terminal switch stays the way out for anything else.
import { div } from "gpui-kit";
import { v_flex, h_flex } from "gpui-base";
import * as sidecar from "./sidecar.js";

export const PANEL_MS = 1500;   // while a panel shows
export const LOOK_MS = 5000;    // while none shows and the agent does not work

// The helper's key name for a key event, or null. Arrows, Enter, Tab, Space, Escape and the digits only.
const NAMES = { up: "Up", down: "Down", left: "Left", right: "Right", enter: "Enter", tab: "Tab", space: "Space", escape: "Escape" };
export function keyName(e) {
  const m = e?.modifiers ?? {};
  if (m.platform || m.cmd || m.control || m.alt || m.shift) return null;
  const k = e?.key ?? "";
  if (/^[0-9]$/.test(k)) return k;
  return NAMES[k] ?? null;
}

// The key that a press on a row sends: "1. Default" sends "1"; the row the pointer marks ("❯ …") sends Enter.
export function rowKey(line) {
  const s = String(line ?? "").trim();
  const n = /^(?:[❯›>]\s*)?([0-9])[.)]\s/.exec(s);
  if (n) return n[1];
  return /^[❯›>]\s/.test(s) ? "Enter" : null;
}

// The buttons under the box: label and key.
export const BUTTONS = [["↑", "Up"], ["↓", "Down"], ["←", "Left"], ["→", "Right"], ["Enter", "Enter"], ["Tab", "Tab"], ["Space", "Space"], ["Esc", "Escape"]];

export class PanelCard {
  constructor(cx) {
    this.focus = cx?.focus_handle?.() ?? null;
    this.canFocus = () => true;
    this.byAgent = new Map();   // address -> { lines, at, busy, sending, note }
    this.shown = null;
  }

  entry(address) {
    let e = this.byAgent.get(address);
    if (!e) { e = { lines: null, at: 0, busy: false, sending: false, note: null }; this.byAgent.set(address, e); }
    return e;
  }

  lines(address) { return this.byAgent.get(address)?.lines ?? null; }

  // Reads the panel when `want` (the agent does not work and no other card shows): every PANEL_MS while one shows,
  // every LOOK_MS while none does. Not wanted: the card goes. An older helper without `panel`: never a card.
  async poll(cx, address, want, force = false) {
    if (!address) return;
    const e = this.entry(address);
    if (!want) { if (e.lines) { e.lines = null; e.note = null; cx.notify(); } return; }
    if (e.busy || e.sending || (!force && Date.now() - e.at < (e.lines ? PANEL_MS : LOOK_MS))) return;
    e.busy = true; e.at = Date.now();
    try {
      if (!(await sidecar.hasCommand("panel"))) return;
      const r = await sidecar.panel(address);
      const l = r.error ? null : r.lines.find((x) => x.type === "panel");
      const lines = Array.isArray(l?.lines) && l.lines.length ? l.lines : null;
      const fresh = lines && !e.lines;
      if (JSON.stringify(lines) !== JSON.stringify(e.lines)) { e.lines = lines; if (!lines) e.note = null; cx.notify(); }
      if (fresh && address === this.shown && this.focus && this.canFocus()) this.focus.focus();
    } finally { e.busy = false; }
  }

  // Sends one key to the panel, then reads it again at once.
  async press(cx, address, key) {
    const e = this.entry(address);
    if (e.sending || !e.lines) return;
    e.sending = true; e.note = null; cx.notify();
    try {
      const r = await sidecar.keys(address, [key]);
      const done = r.error ? null : r.lines.find((x) => x.type === "done");
      if (r.error) e.note = r.error.message;
      else if (done?.result !== "sent") e.note = done?.message || "The key did not go out. Use the terminal.";
    } finally { e.sending = false; cx.notify(); }
    await this.poll(cx, address, true, true);
  }

  // A key while the card has the focus. True when it took the key.
  key_down(ev, cx, address) {
    const k = keyName(ev);
    if (!k || !this.lines(address)) return false;
    cx.spawn((cx) => this.press(cx, address, k));
    return true;
  }

  view(address, t, id = "panel-card", width = 800) {
    const e = this.byAgent.get(address);
    if (!e?.lines) return null;
    const box = v_flex().id(id).w_full().min_w(0).max_w(width).p(14).gap(10).rounded(14).bg(t.raised);
    if (this.focus) box.track_focus(this.focus)
      .on_mouse_down("left", () => this.focus.focus())
      .on_key_down((ev, cx) => { if (this.key_down(ev, cx, address)) cx.stop_propagation(); });
    box.child(h_flex().gap(8).items_center()
      .child(div().size(8).flex_none().rounded(4).bg(t.blocked))
      .child(div().text_size(13.5).font_semibold().text_color(t.foreground).child("The terminal shows a panel")));
    box.child(v_flex().id(`${id}-text`).max_h(320).overflow_y_scrollbar().px(10).py(8).rounded(8).bg(t.background)
      .children(e.lines.map((line, i) => {
        const k = rowKey(line);
        return div().id(`${id}-row-${i}`).min_h(16).text_size(12.5).font_family("Menlo").text_color(t.foreground)
          .when(Boolean(k) && !e.sending, (el) => el.rounded(4).hover((s) => s.bg(t.chrome)).on_click((_ev, cx) => cx.spawn((cx) => this.press(cx, address, k))))
          .child(line || " ");
      })));
    box.child(h_flex().gap(6).items_center().flex_wrap()
      .children(BUTTONS.map(([label, k]) => div().id(`${id}-key-${k}`).h(26).px(10).flex().items_center().rounded(6).text_size(12)
        .bg(t.chrome).text_color(e.sending ? t.dim : t.foreground).border(1).border_color(t.border)
        .when(!e.sending, (el) => el.hover((s) => s.opacity(0.9)).on_click((_ev, cx) => cx.spawn((cx) => this.press(cx, address, k))))
        .child(label)))
      .child(div().text_size(11.5).text_color(t.muted).child("Or use the terminal.")));
    if (e.note) box.child(div().id(`${id}-note`).text_size(12).text_color(t.blocked).child(e.note));
    return box;
  }
}
