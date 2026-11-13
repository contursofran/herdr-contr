// The chat's row sizes, in px: one set for the drawing (chat.js), the row height guess (estimate.js) and
// the right-click menu's offsets (chat.js rowMenu). Change a size here and all three follow.
// Pure (no gpui): the node tests import it as it is.

// Above and below a user or assistant row's content.
export const ROW_PAD_Y = 8;

// The user's bubble: at most BUBBLE_W of the column, padded inside.
export const BUBBLE_W = 0.78;
export const BUBBLE_PAD_X = 14;
export const BUBBLE_PAD_Y = 10;
// The bubble's outer width and its text's width, for a column `width` px wide.
export const bubbleWidth = (width) => Math.floor(width * BUBBLE_W);
export const bubbleTextWidth = (width) => bubbleWidth(width) - 2 * BUBBLE_PAD_X;
// Under the text, BUBBLE_GAP below it: "2 images" at IMAGES_FS px, one line (gpui's line height is about 1.618 ×
// the size: 18.6 px, counted high).
export const BUBBLE_GAP = 4;
export const IMAGES_FS = 11.5;
export const IMAGES_LINE_H = Math.ceil(IMAGES_FS * 1.618);   // 19

// A row of chips (tool calls, a note from another agent): the chip and the row's padding above and below.
export const CHIP_H = 28;
export const CHIP_PAD_Y = 6;
export const CHIP_ROW = CHIP_H + 2 * CHIP_PAD_Y;   // 40
// Open: what shows under the chips, CHIP_GAP below them.
export const CHIP_GAP = 4;
// What an open chip shows (a note's text, the calls): this far in from the chips.
export const OPEN_INDENT = 12;
// An open tool chip: one line a call, and 8 px more under the calls (the gap above them and 4 px spare).
export const CALL_H = 26;
export const CALLS_PAD = 8;

// Rows of one fixed height.
export const SEP_H = 40;
export const FOOTER_H = 30;
export const STATUS_H = 34;
export const CARD_ROW_H = 44;   // a running call (perm) or a question
export const OTHER_H = 30;

// A user row's pictures (#86), under its text in place of the "N images" line: PIC_H high (the width from the
// picture's shape), at most the bubble's text width, PIC_GAP apart, side by side and wrapping. Each box counts its
// 1 px border. The layout of `pics` ([{ width, height }]) in `maxW` px: { lines: [[{ i, w, h }]], width, height }.
export const PIC_H = 120;
export const PIC_GAP = 8;
export function picturesLayout(pics, maxW) {
  const lines = [];
  let line = [], lineW = 0;
  pics.forEach((p, i) => {
    const r = p.width > 0 && p.height > 0 ? p.width / p.height : 1;
    let w = Math.max(8, Math.round(PIC_H * r)), h = PIC_H;
    if (w > maxW) { w = Math.max(8, Math.floor(maxW)); h = Math.max(8, Math.round(w / r)); }
    if (line.length && lineW + PIC_GAP + w > maxW) { lines.push(line); line = []; lineW = 0; }
    lineW += (line.length ? PIC_GAP : 0) + w;
    line.push({ i, w, h });
  });
  if (line.length) lines.push(line);
  const lineW2 = (l) => l.reduce((s, p) => s + p.w, 0) + PIC_GAP * (l.length - 1);
  const lineH = (l) => Math.max(...l.map((p) => p.h));
  return { lines, width: Math.max(0, ...lines.map(lineW2)), height: lines.reduce((s, l) => s + lineH(l), 0) + PIC_GAP * Math.max(0, lines.length - 1) };
}
