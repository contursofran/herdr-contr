// The local images that an agent links in its message (#192): a line that is only "![alt](path)" shows the
// image. image() reads only files under the app home, so the helper copies the file to
// <app home>/herdr-contr/images (`agent-images`, docs/protocol.md) and gives its size. A relative path is from the
// session's cwd, which the helper reads from the log; so each answer is kept by log and src. Render only reads the
// answers here and notes the lines it drew; agentImagesTick (the chat's timer) asks the helper. Until an answer
// with a file is there (a URL, a missing file, another format: never), the line stays text.
import { appHome } from "herdr";
import * as sidecar from "./sidecar.js";
import { appRel } from "./phone.js";
import { imageLines } from "./blocks.js";

// The box around an image: 6 px margin above and below, a 1 px border on each side.
export const IMG_BOX = 14;
// The highest an image is drawn, its border included.
export const IMG_MAX_H = 480;
const MAX_ASK = 16, MAX_LOGS = 2, RETRY_MS = 60_000, MAX_KEPT = 500;

const answers = new Map();  // key -> { rel, width, height } | { none: true } | { failed: true, at }
const wanted = new Map();   // log -> Set of srcs: the image lines render drew since the last tick
const asking = new Set();   // keys
const lines = new Map();    // text -> imageLines(text)
let version = 0;

const keyOf = (log, src) => `${log}\n${src}`;
// A URL is never asked for ("https:", "data:"); "file://" is a local path.
const isUrl = (src) => /^[a-z][a-z0-9+.-]+:/i.test(src) && !/^file:\/\//i.test(src);
export const imagesDir = () => `${appHome()}/herdr-contr/images`;

function linesOf(text) {
  let l = lines.get(text);
  if (!l) {
    l = imageLines(text);
    if (lines.size >= MAX_KEPT) lines.clear();
    lines.set(text, l);
  }
  return l;
}

// The drawn image of `src` in the message of `log`'s agent: { rel (from the app home), width, height }, or null.
export function agentImageOf(log, src) {
  const a = log ? answers.get(keyOf(log, src)) : null;
  return a?.rel ? a : null;
}

// Render notes the image lines of a text for the tick. `log`: the agent's log on this machine, else nothing.
export function noteImages(log, text) {
  if (!log) return;
  for (const { src } of linesOf(String(text ?? ""))) {
    if (isUrl(src) || answers.get(keyOf(log, src))?.rel) continue;
    let s = wanted.get(log);
    if (!s) wanted.set(log, (s = new Set()));
    s.add(src);
  }
}

// The drawn size of the image inside its border at a text width of `width`: no wider than the text, no higher
// than IMG_MAX_H with the border, scaled down, never up.
export function agentImageSize(e, width) {
  const r = e.height / e.width;
  let w = Math.max(1, Math.min(e.width, width - 2));
  let h = w * r;
  if (h > IMG_MAX_H - 2) { h = IMG_MAX_H - 2; w = h / r; }
  return { w: Math.max(1, Math.round(w)), h: Math.max(1, Math.round(h)) };
}

// The drawn images of a message, in order, for a viewer (#193): [{ src, alt, rel, width, height }].
export function agentImages(log, text) {
  if (!log) return [];
  return linesOf(String(text ?? "")).flatMap(({ src, alt }) => {
    const a = agentImageOf(log, src);
    return a ? [{ src, alt, rel: a.rel, width: a.width, height: a.height }] : [];
  });
}

// Goes up each time an answer adds an image, so the chat measures its rows again.
export const agentImagesVersion = () => version;

// Asks the helper for the image lines drawn since the last tick that have no answer: MAX_ASK srcs of MAX_LOGS
// logs at once; a failed ask again after RETRY_MS. An older helper without `agent-images`: nothing. Run from a
// timer, never from render. true when the chat should draw again.
export async function agentImagesTick(now = Date.now()) {
  const todo = [];
  for (const [log, srcs] of wanted) {
    const run = [...srcs].filter((src) => {
      const k = keyOf(log, src), a = answers.get(k);
      return !asking.has(k) && (!a || (a.failed && now - a.at >= RETRY_MS));
    }).slice(0, MAX_ASK);
    if (run.length) todo.push([log, run]);
  }
  wanted.clear();
  if (!todo.length || !(await sidecar.hasCommand("agent-images"))) return false;
  const got = await Promise.all(todo.slice(0, MAX_LOGS).map(async ([log, srcs]) => {
    for (const src of srcs) asking.add(keyOf(log, src));
    let r;
    try { r = await sidecar.call("agent-images", ["--log", log, "--dir", imagesDir(), "--srcs", JSON.stringify(srcs)]); }
    catch (e) { r = { error: { message: String(e) }, lines: [] }; }
    let any = false;
    for (const [i, src] of srcs.entries()) {
      const k = keyOf(log, src);
      asking.delete(k);
      const l = r.error ? null : r.lines.find((x) => x.type === "agent-image" && x.index === i);
      const rel = l ? appRel(l.path) : null;
      if (answers.size >= MAX_KEPT) answers.clear();
      if (r.error) answers.set(k, { failed: true, at: now });
      else if (rel && l.width > 0 && l.height > 0) { answers.set(k, { rel, width: l.width, height: l.height }); any = true; }
      else answers.set(k, { none: true });
    }
    return any;
  }));
  if (!got.some(Boolean)) return false;
  version++;
  return true;
}

export function forgetAgentImages() { answers.clear(); wanted.clear(); asking.clear(); lines.clear(); version++; }
