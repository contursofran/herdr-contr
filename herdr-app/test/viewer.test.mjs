import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { fake, reset } from "./stub/herdr.mjs";
import { textOf, byId, lastList } from "./stub/ui.mjs";
import { fakeCx } from "./fake-cx.mjs";
import { agent, space } from "./fixtures.mjs";
import { ChatView } from "../mods/herdr-contr/chat.js";
import { forgetCommands } from "../mods/herdr-contr/sidecar.js";
import { chatPictures, addPictureSource, forgetPictures } from "../mods/herdr-contr/pictures.js";
import { PictureViewer, fitPicture } from "../mods/herdr-contr/viewer.js";

const jl = (...lines) => ({ code: 0, stdout: lines.map((l) => JSON.stringify(l)).join("\n") + "\n", stderr: "" });
const header = { type: "header", protocol: 1, sidecar: "0.1.0", command: "feed" };
const end = { type: "end", ok: true };
const PASTE = "/home/test/.config/herdr-app/herdr-contr/paste";
const paste = (n) => `${PASTE}/paste-20261115-10000${n}-A1B2C${n}.png`;
const rel = (n) => `herdr-contr/paste/paste-20261115-10000${n}-A1B2C${n}.png`;
// Three user messages: one picture, none, two pictures; and an agent's answer between them.
const rows = [
  { type: "row", id: "u1", k: "user", text: `first ${paste(1)}`, ts: 1 },
  { type: "row", id: "a1", k: "assistant", text: "Seen.", ts: 2 },
  { type: "row", id: "u2", k: "user", text: "only text", ts: 3 },
  { type: "row", id: "u3", k: "user", text: `two ${paste(2)} ${paste(3)}`, ts: 4 },
];

beforeEach(() => {
  reset(); forgetCommands(); forgetPictures();
  fake.spaces = [space()];
  fake.agents = [agent("t1", "idle")];
  fake.exec = (cmd, args) => {
    if (args[0] === "feed") return jl(header, { type: "log", agent: "claude", path: "/logs/s.jsonl", start: 0, size: 10, cursor: "c1" }, { type: "splice", at: 0, count: rows.length }, ...rows, end);
    return jl(header, end);
  };
});

// The chat with the rows above, drawn once; `root()` draws it again and gives the whole tree.
async function chat() {
  const cx = fakeCx(); const v = new ChatView(cx); await cx.settle(); v.width = 900; v.heightPx = 700;
  const root = () => v.render(cx);
  root();
  return { cx, v, root };
}
const list = [{ rel: "a", row: "r1" }, { rel: "b", row: "r2" }, { rel: "c", row: "r2" }, { rel: "d", row: "r4" }];

test("chatPictures: each row's pictures in chat order, with the id of their row", () => {
  assert.deepEqual(chatPictures(rows, "/logs/s.jsonl"), [
    { rel: rel(1), width: null, height: null, row: "u1" },
    { rel: rel(2), width: null, height: null, row: "u3" },
    { rel: rel(3), width: null, height: null, row: "u3" },
  ]);
  assert.deepEqual(chatPictures([{ id: "n", k: "user", from: "t2", text: paste(1) }], "/l"), [], "a note from another agent has none");
  assert.deepEqual(chatPictures(null, "/l"), []);
});

test("another kind of picture joins the list with one call, in chat order", () => {
  const agentImages = (row) => (row.k === "assistant" && row.img ? [{ rel: row.img, width: 40, height: 20 }] : []);
  addPictureSource(agentImages);
  addPictureSource(agentImages);   // the same source twice counts once
  const mixed = [rows[0], { id: "a9", k: "assistant", text: "", img: "herdr-contr/pics/x.png" }, rows[3]];
  assert.deepEqual(chatPictures(mixed, "/l").map((p) => [p.row, p.rel, p.width]),
    [["u1", rel(1), null], ["a9", "herdr-contr/pics/x.png", 40], ["u3", rel(2), null], ["u3", rel(3), null]]);
});

test("open: at the row's first picture, \"2 / 4\"; a row without pictures opens nothing", () => {
  let focused = 0;
  const v = new PictureViewer({ focus_handle: () => ({ focus: () => { focused++; } }) });
  assert.equal(v.open(list, "r3"), false);
  assert.equal(v.shown, false);
  assert.equal(focused, 0);
  assert.equal(v.open(list, "r2"), true);
  assert.deepEqual([v.shown, v.current.rel, v.label()], [true, "b", "2 / 4"]);
  assert.equal(focused, 1, "the viewer takes the keys");
});

test("moves: next and previous step one; at the ends the viewer stays", () => {
  const v = new PictureViewer();
  assert.equal(v.move(1), false, "closed: no move");
  v.open(list, "r4");
  assert.equal(v.move(1), false, "the last picture: next does nothing");
  assert.equal(v.label(), "4 / 4");
  assert.equal(v.move(-1), true);
  assert.equal(v.current.rel, "c");
  v.move(-1); v.move(-1);
  assert.equal(v.label(), "1 / 4");
  assert.equal(v.move(-1), false, "the first picture: previous does nothing");
  assert.equal(v.label(), "1 / 4");
});

test("keys: ← and → move, Esc closes, other keys are not the viewer's", () => {
  const v = new PictureViewer();
  v.open(list, "r2");
  assert.equal(v.key({ keystroke: "right" }), true);
  assert.equal(v.label(), "3 / 4");
  assert.equal(v.key({ keystroke: "left" }), true);
  assert.equal(v.label(), "2 / 4");
  assert.equal(v.key({ keystroke: "a" }), false);
  assert.equal(v.key({ keystroke: "escape" }), true);
  assert.equal(v.shown, false);
  assert.equal(v.label(), "");
  assert.equal(v.close(), false, "closing a closed viewer changes nothing");
});

test("fitPicture: the shape kept, never larger than the picture; without a size, the whole box", () => {
  assert.deepEqual(fitPicture({ width: 2000, height: 1000 }, 800, 600), { w: 800, h: 400 });
  assert.deepEqual(fitPicture({ width: 1000, height: 2000 }, 800, 600), { w: 300, h: 600 });
  assert.deepEqual(fitPicture({ width: 100, height: 50 }, 800, 600), { w: 100, h: 50 });
  assert.deepEqual(fitPicture({ rel: "x" }, 800.5, 600.7), { w: 800, h: 600 });
});

test("the chat: a click on a row with pictures opens the viewer over the chat; buttons move; the ends stay", async () => {
  const { cx, v, root } = await chat();
  assert.equal(byId(root(), "viewer"), null, "closed at first");
  lastList._h.on_item_click("u2", cx);
  assert.equal(byId(root(), "viewer"), null, "a row without pictures opens nothing");
  const n = cx.notified;
  lastList._h.on_item_click("u3", cx);
  assert.ok(cx.notified > n);
  let el = root();
  assert.match(textOf(byId(el, "viewer-count")), /^2 \/ 3$/);
  assert.equal(byId(el, "viewer-picture").kids[0]._a.path, rel(2));
  byId(el, "viewer-next")._h.on_click({}, cx);
  el = root();
  assert.match(textOf(byId(el, "viewer-count")), /^3 \/ 3$/);
  byId(el, "viewer-next")._h.on_click({}, cx);
  el = root();
  assert.match(textOf(byId(el, "viewer-count")), /^3 \/ 3$/, "the last picture: next stays");
  for (let k = 0; k < 3; k++) { byId(el, "viewer-prev")._h.on_click({}, cx); el = root(); }
  assert.match(textOf(byId(el, "viewer-count")), /^1 \/ 3$/, "the first picture: previous stays");
  assert.equal(byId(el, "viewer-picture").kids[0]._a.path, rel(1));
  assert.ok(v.viewer.shown);
});

test("the chat: a click on the picture keeps the viewer, a click outside or Esc closes it", async () => {
  const { cx, v, root } = await chat();
  lastList._h.on_item_click("u1", cx);
  let el = root();
  let stopped = 0;
  byId(el, "viewer-picture")._h.on_click({}, { ...cx, stop_propagation: () => { stopped++; } });
  assert.equal(stopped, 1, "the press stays on the picture");
  assert.ok(byId(root(), "viewer"));
  byId(root(), "viewer")._h.on_click({}, cx);
  assert.equal(byId(root(), "viewer"), null, "a click outside the picture closes it");
  lastList._h.on_item_click("u1", cx);
  byId(root(), "viewer")._h.on_key_down({ keystroke: "escape" }, cx);
  assert.equal(byId(root(), "viewer"), null, "Esc closes it");
  lastList._h.on_item_click("u3", cx);
  v.switchTo("other");
  assert.equal(v.viewer.shown, false, "another agent's chat closes it");
});

test("the chat: a drag that selects text in a row with pictures does not open the viewer", async () => {
  const { cx, v, root } = await chat();
  const box = (function find(e) { if (!e || typeof e !== "object") return null; if ((e.kids || []).includes(lastList)) return e; for (const k of e.kids || []) { const f = find(k); if (f) return f; } return null; })(root());
  box._h.on_mouse_down({ position: { x: 100, y: 100 } });
  box._h.on_mouse_move({ position: { x: 160, y: 104 } });
  lastList._h.on_item_click("u3", cx);
  box._h.on_mouse_up({});
  assert.equal(v.viewer.shown, false);
  box._h.on_mouse_down({ position: { x: 100, y: 100 } });
  box._h.on_mouse_move({ position: { x: 102, y: 101 } });
  lastList._h.on_item_click("u3", cx);
  assert.equal(v.viewer.shown, true, "a small move is still a click");
});
