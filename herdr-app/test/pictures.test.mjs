import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { fake, log, reset } from "./stub/herdr.mjs";
import { textOf, calls, lastList } from "./stub/ui.mjs";
import { fakeCx } from "./fake-cx.mjs";
import { agent, space } from "./fixtures.mjs";
import { ChatView } from "../mods/herdr-contr/chat.js";
import { forgetCommands } from "../mods/herdr-contr/sidecar.js";
import { picturesTick, picturesKey, picturesOf, forgetPictures } from "../mods/herdr-contr/pictures.js";
import { picturesLayout, PIC_H, PIC_GAP } from "../mods/herdr-contr/metrics.js";
import { rowHeight } from "../mods/herdr-contr/estimate.js";

const jl = (...lines) => ({ code: 0, stdout: lines.map((l) => JSON.stringify(l)).join("\n") + "\n", stderr: "" });
const header = { type: "header", protocol: 1, sidecar: "0.1.0", command: "x" };
const end = { type: "end", ok: true };
const LOG = "/logs/s.jsonl";
const PICS = "/home/test/.config/herdr-app/herdr-contr/pics";
const pic = (digest, width, height) => ({ format: "png", width, height, offset: 10, index: 0, digest });
const row = { type: "row", id: "u1", k: "user", text: "Look at these", ts: 1, images: 2, pics: [pic("00000000000000aa", 300, 200), pic("00000000000000bb", 40, 80)] };
const answer = (paths) => jl(header, ...paths.map((p, i) => p ? { type: "picture", index: i, path: p.path, width: p.width, height: p.height } : { type: "picture", index: i, path: null }), end);

// The sidecar: `feed` gives `row`, `version` the commands, `pictures` what `pictures` returns.
let pictures, version;
beforeEach(() => {
  reset(); calls.length = 0; forgetCommands(); forgetPictures();
  fake.spaces = [space()];
  fake.agents = [agent("t1", "idle")];
  version = ["feed", "pictures"];
  pictures = () => answer([{ path: `${PICS}/pic-00000000000000aa.png`, width: 300, height: 200 }, { path: `${PICS}/pic-00000000000000bb.png`, width: 40, height: 80 }]);
  fake.exec = (cmd, args) => {
    if (args[0] === "version") return jl(header, { type: "version", commands: version }, end);
    if (args[0] === "pictures") return pictures(args);
    if (args[0] === "feed") return jl(header, { type: "log", agent: "claude", path: LOG, start: 0, size: 10, cursor: "c1" }, { type: "splice", at: 0, count: 1 }, row, end);
    return jl(header, end);
  };
});

const images = (el) => { const out = []; const walk = (e) => { if (!e || typeof e !== "object") return; if (e.tag === "image") out.push(e); (e.kids || []).forEach(walk); }; walk(el); return out; };

test("layout: 120 px high, width from the shape, at most the text width, side by side and wrapping", () => {
  const one = picturesLayout([{ width: 300, height: 200 }], 500);
  assert.deepEqual(one, { lines: [[{ i: 0, w: 180, h: PIC_H }]], width: 180, height: PIC_H });
  const wide = picturesLayout([{ width: 2000, height: 100 }], 400);
  assert.deepEqual(wide.lines[0][0], { i: 0, w: 400, h: 20 }, "a wide picture is cut to the text width, its shape kept");
  const three = picturesLayout([{ width: 300, height: 200 }, { width: 300, height: 200 }, { width: 40, height: 80 }], 400);
  assert.deepEqual(three.lines.map((l) => l.map((p) => p.i)), [[0, 1], [2]], "the third goes on a new line");
  assert.equal(three.width, 180 + PIC_GAP + 180);
  assert.equal(three.height, PIC_H + PIC_GAP + PIC_H);
});

test("the row height counts the pictures in place of the \"N images\" line", () => {
  const r = { k: "user", text: "hi", images: 2 };
  const text = rowHeight(r, 800), pics = rowHeight(r, 800, false, 1, 1, 248);
  assert.equal(pics - text, 248 - 19);
});

test("the chat shows \"N images\" first, then the pictures; the row grows", async () => {
  const cx = fakeCx();
  const v = new ChatView(cx); await cx.settle(); v.width = 900;
  const feed = log.exec.find(([, a]) => a[0] === "feed")[1];
  assert.ok(feed.includes("--pictures"), "the feed asks where the pictures are");
  v.render(cx);
  const before = lastList._args.sizes[0];
  assert.match(textOf(lastList._args.render({ start: 0, end: 1 })[0]), /2 images/);

  v.lastPoll = Date.now(); const n = cx.notified;
  v.tick(cx); await cx.settle();
  const ask = log.exec.find(([, a]) => a[0] === "pictures")[1];
  assert.deepEqual(ask.slice(0, 5), ["pictures", "--path", LOG, "--dir", PICS]);
  assert.deepEqual(JSON.parse(ask[6]), row.pics);
  assert.ok(cx.notified > n, "the chat draws again");

  v.render(cx);
  const el = lastList._args.render({ start: 0, end: 1 })[0];
  assert.doesNotMatch(textOf(el), /images/);
  assert.deepEqual(images(el).map((e) => e._a.path), ["herdr-contr/pics/pic-00000000000000aa.png", "herdr-contr/pics/pic-00000000000000bb.png"]);
  assert.equal(lastList._args.sizes[0] - before, PIC_H - 19, "one line of pictures in place of the text line");

  // Asked once: the next tick has the answer.
  v.tick(cx); await cx.settle();
  assert.equal(log.exec.filter(([, a]) => a[0] === "pictures").length, 1);
});

test("a failed ask keeps the text, and is asked again after a minute", async () => {
  pictures = () => ({ code: 1, stdout: "", stderr: "boom" });
  const r = { k: "user", pics: row.pics, images: 2 };
  assert.equal(await picturesTick([r], LOG, 1000), false);
  assert.equal(picturesOf(r, LOG), null);
  assert.equal(await picturesTick([r], LOG, 2000), false);
  assert.equal(log.exec.filter(([, a]) => a[0] === "pictures").length, 1, "not again at once");
  pictures = () => answer([{ path: `${PICS}/a.png`, width: 10, height: 10 }, null]);
  assert.equal(await picturesTick([r], LOG, 62_000), true);
  assert.equal(picturesOf(r, LOG).list.length, 1, "the picture that came; the one not in the log any more is left out");
});

test("no pictures to draw: the text stays", async () => {
  const r = { k: "user", pics: row.pics, images: 2 };
  pictures = () => answer([null, { path: "/elsewhere/b.png", width: 10, height: 10 }]);
  assert.equal(await picturesTick([r], LOG), false, "null, and a path outside the app home");
  assert.equal(picturesOf(r, LOG), null);
});

test("an older sidecar without `pictures`, a remote row and a note from another agent: nothing is asked", async () => {
  version = ["feed"];
  assert.equal(await picturesTick([{ k: "user", pics: row.pics }], LOG), false);
  forgetCommands(); version = ["feed", "pictures"];
  assert.equal(picturesKey({ k: "user", images: 2 }, LOG), null, "a row from another machine has no pics");
  assert.equal(picturesKey({ k: "user", from: "t2 worker", pics: row.pics }, LOG), null);
  assert.equal(picturesKey({ k: "tools", pics: row.pics }, LOG), null);
  assert.equal(picturesKey({ k: "user", pics: row.pics }, null), null);
  assert.equal(await picturesTick([{ k: "user", images: 1 }], LOG), false);
  assert.equal(log.exec.filter(([, a]) => a[0] === "pictures").length, 0);
});
