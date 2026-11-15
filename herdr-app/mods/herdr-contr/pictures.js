// The pictures of a user message (#86), drawn in its bubble under the text. The rows of an agent on this
// machine say where their pictures are in the log (`feed --pictures`, the rows' `pics`); the sidecar's
// `pictures` writes them as files under <app home>/herdr-contr/pics. image() reads a path relative to the app
// home (phone.js appRel). picturesTick (the chat's timer) asks for them, never the draw; the answers are kept
// here by log and pictures. Until an answer is there, or when it has no picture to draw, the "N images" line shows.
// The chat's list renderer refuses handlers and state (#70): the pictures are plain elements. A click on the row
// opens the viewer (chat.js click, viewer.js).
import { div, image } from "gpui-kit";
import { v_flex, h_flex } from "gpui-base";
import { appHome } from "herdr";
import * as sidecar from "./sidecar.js";
import { appRel } from "./phone.js";
import { picturesLayout, PIC_H, PIC_GAP } from "./metrics.js";

const RETRY_MS = 60_000, MAX_ASKS = 2, MAX_KEPT = 500;
const answers = new Map();   // key -> { list: [{ rel, width, height }], at, failed?, layouts: Map }
const asking = new Set();
const keys = new WeakMap();  // row -> key

export const picsDir = () => `${appHome()}/herdr-contr/pics`;

// The key of a row's pictures in `path`'s log, or null (no pictures, or a note from another agent).
export function picturesKey(row, path) {
  if (!path || row?.k !== "user" || row.from != null || !Array.isArray(row.pics) || !row.pics.length) return null;
  const c = keys.get(row);
  if (c && c.path === path) return c.key;
  const key = `${path}|${row.pics.map((p) => `${p.digest}:${p.offset}:${p.index}`).join(",")}`;
  keys.set(row, { path, key });
  return key;
}

// The answer for a row with one picture or more to draw, or null.
export function picturesOf(row, path) {
  const key = picturesKey(row, path);
  const a = key ? answers.get(key) : null;
  return a?.list.length ? a : null;
}

// The pictures' layout in `maxW` px, kept by width (each draw asks).
export function layoutOf(a, maxW) {
  let l = a.layouts.get(maxW);
  if (!l) { l = picturesLayout(a.list, maxW); a.layouts.set(maxW, l); }
  return l;
}

// The height of a row's pictures in `maxW` px, or null (the "N images" line counts then).
export function picturesHeight(row, path, maxW) {
  const a = picturesOf(row, path);
  return a ? layoutOf(a, maxW).height : null;
}

// The pictures as plain elements: a column of lines, each a row of boxes with a border from the theme.
export function picturesView(a, t, maxW) {
  const l = layoutOf(a, maxW);
  return v_flex().flex_none().w(l.width).h(l.height).gap(PIC_GAP).children(l.lines.map((line) =>
    h_flex().flex_none().gap(PIC_GAP).items_start().children(line.map((p) =>
      div().flex_none().w(p.w).h(p.h).rounded(8).border(1).border_color(t.border).bg(t.chrome).overflow_hidden()
        .child(image(a.list[p.i].rel).w(p.w - 2).h(p.h - 2).rounded(7))))));
}

// A sent message that names a pasted image (composer.js, herdr-contr/paste under the app home) by its path: the
// path is taken out of the text and the picture is drawn. The file is there, so nothing is asked. A path that is
// not under the app home stays text. { text, rels }, kept by text (each draw asks).
const PASTE_PATH = /(['"]?)(\S*\/herdr-contr\/paste\/paste-[\w-]+\.(?:png|jpe?g))\1/gi;
const pastes = new Map();
export function pastedOf(text) {
  const src = String(text ?? "");
  if (pastes.has(src)) return pastes.get(src);
  const rels = [];
  const out = src.replace(PASTE_PATH, (m, _q, path) => {
    const rel = appRel(path);
    if (!rel) return m;
    rels.push(rel);
    return "";
  });
  const r = rels.length ? { text: out.split("\n").map((l) => l.replace(/[ \t]{2,}/g, " ").trim()).join("\n").trim(), rels } : { text: src, rels };
  if (pastes.size >= 500) pastes.clear();
  pastes.set(src, r);
  return r;
}

// What a user row's bubble shows: its text (pasted paths out), and its pictures: the log's when they are known,
// else the pasted ones (one line PIC_H high, each as wide as its image). `h`: their height, or null (the
// "N images" line, if any). For estimate.js rowHeight and the draw.
export function userPictures(row, path, maxW) {
  const p = pastedOf(row.text);
  const log = picturesOf(row, path);
  if (log) return { text: p.text, log, pasted: [], h: layoutOf(log, maxW).height };
  return { text: p.text, log: null, pasted: p.rels, h: p.rels.length ? PIC_H : null };
}

// The pasted pictures: one line, PIC_H high, cut at `maxW`.
export function pastedView(rels, t, maxW) {
  return h_flex().flex_none().h(PIC_H).max_w(maxW).gap(PIC_GAP).overflow_hidden().children(rels.map((rel) =>
    div().flex_none().h(PIC_H).rounded(8).border(1).border_color(t.border).bg(t.chrome).overflow_hidden()
      .child(image(rel).h(PIC_H - 2).rounded(7))));
}

// Asks the sidecar for the pictures of the shown rows that have no answer yet: at most MAX_ASKS at once,
// a failed ask again after RETRY_MS. An older sidecar without `pictures`: nothing. Run from a timer, never
// from render. true when an answer came that changes what the chat draws.
export async function picturesTick(rows, path, now = Date.now()) {
  const todo = [];
  for (const row of rows ?? []) {
    const key = picturesKey(row, path);
    if (!key || asking.has(key) || todo.some((x) => x.key === key)) continue;
    const a = answers.get(key);
    if (a && !(a.failed && now - a.at >= RETRY_MS)) continue;
    todo.push({ key, pics: row.pics });
  }
  const run = todo.slice(0, Math.max(0, MAX_ASKS - asking.size));
  if (!run.length || !(await sidecar.hasCommand("pictures"))) return false;
  for (const x of run) asking.add(x.key);
  const got = await Promise.all(run.map(async (x) => {
    let r;
    try { r = await sidecar.pictures(path, x.pics, picsDir()); } catch (e) { r = { error: { message: String(e) }, lines: [] }; }
    asking.delete(x.key);
    const list = r.error ? [] : r.lines.filter((l) => l.type === "picture" && l.path && l.width > 0 && l.height > 0)
      .map((l) => ({ rel: appRel(l.path), width: l.width, height: l.height })).filter((p) => p.rel);
    if (answers.size >= MAX_KEPT) answers.clear();
    answers.set(x.key, { list, at: now, failed: Boolean(r.error), layouts: new Map() });
    return list.length > 0;
  }));
  return got.some(Boolean);
}

// The chat's pictures for the viewer (viewer.js, #193), in chat order: each row's pictures from each source, in
// the order the sources were added. A source is (row, path) => [{ rel, width?, height? }] (width and height in
// px, when known). Another kind of picture (the agent's images, #192) joins with one addPictureSource call.
const sources = [];
export function addPictureSource(fn) { if (!sources.includes(fn)) sources.push(fn); }

// The pictures of a user message: the log's when they are known, else the pasted ones (no size).
export function userPictureList(row, path) {
  if (row?.k !== "user" || row.from != null) return [];
  const log = picturesOf(row, path);
  if (log) return log.list;
  return pastedOf(row.text).rels.map((rel) => ({ rel }));
}
addPictureSource(userPictureList);

// [{ rel, width, height, row }]: `row` is the id of the row that has the picture.
export function chatPictures(rows, path) {
  const out = [];
  for (const row of rows ?? []) for (const src of sources) for (const p of src(row, path) ?? []) {
    if (p?.rel) out.push({ rel: p.rel, width: p.width ?? null, height: p.height ?? null, row: row.id });
  }
  return out;
}

export function forgetPictures() { answers.clear(); asking.clear(); pastes.clear(); }
