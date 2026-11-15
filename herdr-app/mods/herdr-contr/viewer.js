// The picture viewer (#193): over the chat, one of the chat's pictures large and fitted, "3 / 7" under it,
// previous and next as buttons and as ← and →, Esc or a click outside the picture closes it. The list is the
// chat's pictures in chat order (pictures.js chatPictures), taken when it opens. It is drawn in the chat view,
// not in the list's rows, so its handlers are allowed (#70).
import { div, svg, image } from "gpui-kit";
import { v_flex, h_flex } from "gpui-base";
import { icon } from "./state.js";

export const SCRIM = "#000000c0";
export const VIEW_PAD = 48;      // between the picture and the chat's edges, with room for the buttons
export const LABEL_H = 28;       // the "3 / 7" line under the picture
export const BTN = 36;

// The picture's size in a box of w × h: its shape kept, never larger than its own size. Without a size
// (a pasted picture): the whole box, and the image fits itself in it.
export function fitPicture(p, w, h) {
  const bw = Math.max(1, Math.floor(w)), bh = Math.max(1, Math.floor(h));
  if (!(p?.width > 0 && p?.height > 0)) return { w: bw, h: bh };
  const k = Math.min(1, bw / p.width, bh / p.height);
  return { w: Math.max(1, Math.floor(p.width * k)), h: Math.max(1, Math.floor(p.height * k)) };
}

export class PictureViewer {
  constructor(cx) {
    this.list = null; this.at = 0;
    this.focus = cx?.focus_handle?.() ?? null;
  }

  get shown() { return Boolean(this.list); }
  get current() { return this.list ? this.list[this.at] : null; }
  label() { return this.list ? `${this.at + 1} / ${this.list.length}` : ""; }

  // Opens at the first picture of row `rowId`. false (and stays closed) when the row has none.
  open(list, rowId) {
    const at = (list ?? []).findIndex((p) => p.row === rowId);
    if (at < 0) return false;
    this.list = list; this.at = at;
    this.focus?.focus?.();
    return true;
  }

  // One step back (-1) or on (1). At the first or the last picture it stays: false.
  move(step) {
    if (!this.list) return false;
    const to = this.at + step;
    if (to < 0 || to >= this.list.length) return false;
    this.at = to;
    return true;
  }

  close() { const was = this.shown; this.list = null; this.at = 0; return was; }

  // Esc closes, ← and → move. true when the key was the viewer's.
  key(e) {
    const k = e?.keystroke ?? e?.key;
    if (k === "escape") { this.close(); return true; }
    if (k === "left") { this.move(-1); return true; }
    if (k === "right") { this.move(1); return true; }
    return false;
  }

  button(t, id, name, can, step) {
    return div().id(id).size(BTN).flex_none().flex().items_center().justify_center().rounded(BTN / 2)
      .bg(t.raised).border(1).border_color(t.border).text_color(can ? t.foreground : t.dim).opacity(can ? 1 : 0.4)
      .when(can, (el) => el.hover((s) => s.bg(t.hover)))
      .on_click((_e, cx) => { cx.stop_propagation?.(); if (this.move(step)) cx.notify(); })
      .child(svg(icon(name)).size(16));
  }

  // The layer over the chat, w × h px. null when closed.
  view(t, w, h) {
    const p = this.current;
    if (!p) return null;
    const fit = fitPicture(p, w - 2 * (VIEW_PAD + BTN), h - 2 * VIEW_PAD - LABEL_H);
    const layer = div().id("viewer").absolute().inset_0().bg(SCRIM)
      .on_click((_e, cx) => { if (this.close()) cx.notify(); })
      .on_key_down((e, cx) => { if (this.key(e)) { cx.stop_propagation?.(); cx.notify(); } });
    if (this.focus) layer.track_focus(this.focus);
    return layer.child(h_flex().size_full().items_center().justify_center().gap(VIEW_PAD / 2)
      .child(this.button(t, "viewer-prev", "chevron-left", this.at > 0, -1))
      .child(v_flex().flex_none().items_center().gap(0)
        .child(div().id("viewer-picture").flex_none().w(fit.w).h(fit.h)
          .on_click((_e, cx) => cx.stop_propagation?.())
          .child(image(p.rel).w(fit.w).h(fit.h).rounded(6)))
        .child(div().id("viewer-count").h(LABEL_H).pt(8).text_size(12.5).text_color("#ffffffcc").child(this.label())))
      .child(this.button(t, "viewer-next", "chevron-right", this.at < this.list.length - 1, 1)));
  }
}
