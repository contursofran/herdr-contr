// A code block with the language mermaid draws as a diagram (#87). The helper draws the text to an SVG in
// <app home>/herdr-contr/mermaid (`mermaid`, docs/protocol.md) and gives its size. Render only reads the
// answers here (a Map by text) and notes the blocks it drew; mermaidTick (the chat's timer) asks the helper
// for them. Until the answer is there, or when the helper cannot draw the text, the block is a code box.
import { appHome } from "herdr";
import * as sidecar from "./sidecar.js";

// The box around the diagram is the code box's: 14 px padding and a 1 px border on each side.
export const BOX_X = 30;
const MAX_ASK = 2;

const drawn = new Map();   // text -> { look, path (from the app home), w, h } or { look, failed: true }
const wanted = new Map();  // text -> colors: the blocks render drew since the last tick
const asking = new Set();
let version = 0;

export const isMermaid = (b) => b?.k === "code" && /^mermaid$/i.test(String(b.lang ?? "").trim());

// The theme's colors for the helper. Dark: the background's luminance is under half.
export function mermaidColors(t) {
  const m = /^#([0-9a-f]{6})/i.exec(String(t?.background ?? ""));
  const lum = m ? [0, 2, 4].map((i) => parseInt(m[1].slice(i, i + 2), 16)).reduce((a, c, i) => a + c * [0.299, 0.587, 0.114][i], 0) / 255 : 0;
  return { dark: lum < 0.5, fg: t?.foreground, bg: t?.raised, fill: t?.surface ?? t?.background, border: t?.border, line: t?.muted };
}
const lookOf = (c) => [c.dark ? "dark" : "light", c.fg, c.bg, c.fill, c.border, c.line].join("|");

// The diagram of a block's text: { path, w, h } once the helper drew it (in any theme: the new theme's one
// replaces it when it is there), else null. `t`: the theme, given by render: the block is noted for the tick.
export function mermaidOf(text, t = null) {
  if (t) wanted.set(text, mermaidColors(t));
  const e = drawn.get(text);
  return e && !e.failed ? e : null;
}
// The drawn size at a text width of `width` (the box's inside, BOX_X less): no wider than that, scaled down.
export function mermaidSize(e, width) {
  const w = Math.max(1, Math.min(e.w, width - BOX_X));
  return { w, h: Math.round((e.h * w) / e.w) };
}
// Goes up each time an answer changes a diagram, so the chat measures its rows again.
export const mermaidVersion = () => version;

// The path under the app home, or null.
function rel(path) {
  const root = appHome().replace(/\/+$/, "") + "/";
  return typeof path === "string" && path.startsWith(root) ? path.slice(root.length) : null;
}
const hex = (s) => (/^#[0-9a-f]{3,8}$/i.test(String(s ?? "")) ? [String(s)] : []);
async function ask(text, c) {
  const args = ["--dir", `${appHome()}/herdr-contr/mermaid`, ...(c.dark ? ["--dark"] : [])];
  for (const k of ["fg", "bg", "fill", "border", "line"]) for (const v of hex(c[k])) args.push(`--${k}`, v);
  const r = await sidecar.call("mermaid", [...args, "--text", text]);
  const l = r.error ? null : r.lines.find((x) => x.type === "mermaid");
  const path = rel(l?.path);
  return path && l.width > 0 && l.height > 0 ? { path, w: l.width, h: l.height } : null;
}

// Asks the helper for the blocks drawn since the last tick that have no diagram in the theme yet, MAX_ASK at
// once. Run from a timer or event, never from render. true when the chat should draw again.
export async function mermaidTick() {
  const todo = [...wanted].filter(([text, c]) => drawn.get(text)?.look !== lookOf(c) && !asking.has(text));
  wanted.clear();
  if (!todo.length || !(await sidecar.hasCommand("mermaid"))) return false;
  const run = todo.slice(0, Math.max(0, MAX_ASK - asking.size));
  for (const [text] of run) asking.add(text);
  const got = await Promise.all(run.map(async ([text, c]) => {
    let e = null;
    try { e = await ask(text, c); } catch { e = null; }
    asking.delete(text);
    const was = drawn.get(text);
    // A failed draw keeps the diagram of another theme, if there is one, and is not asked again in this one.
    if (!e) { drawn.set(text, was && !was.failed ? { ...was, look: lookOf(c) } : { look: lookOf(c), failed: true }); return false; }
    drawn.set(text, { look: lookOf(c), ...e });
    if (drawn.size > 500) drawn.delete(drawn.keys().next().value);
    return !was || was.failed || was.path !== e.path || was.w !== e.w || was.h !== e.h;
  }));
  if (!got.some(Boolean)) return false;
  version++;
  return true;
}
export function forgetMermaid() { drawn.clear(); wanted.clear(); asking.clear(); version++; }
