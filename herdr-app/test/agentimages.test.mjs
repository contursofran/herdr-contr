// An agent's linked images (#192): a line that is only "![alt](path)" stays text until the helper copied the file
// under the app home, then it is an image at the height the estimate counts; asked from the tick, never from render.
import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { fake, log, reset } from "./stub/herdr.mjs";
import { markdownView } from "../mods/herdr-contr/mdblocks.js";
import { markdownHeight, rowHeight } from "../mods/herdr-contr/estimate.js";
import { IMAGE_LINE, imageLines, splitBlocks } from "../mods/herdr-contr/blocks.js";
import { forgetCommands } from "../mods/herdr-contr/sidecar.js";
import { ChatView } from "../mods/herdr-contr/chat.js";
import { chatPictures } from "../mods/herdr-contr/pictures.js";
import {
  agentImageOf, agentImageSize, agentImages, agentImagesTick, agentImagesVersion, forgetAgentImages, noteImages, IMG_BOX, IMG_MAX_H,
} from "../mods/herdr-contr/agentimages.js";

const T = { background: "#1c1d20", foreground: "#e6e6e6", raised: "#26272b", border: "#3a3b40", chrome: "#202125" };
const HOME = "/home/test/.config/herdr-app";
const LOG = "/home/test/.claude/projects/p/s.jsonl";
const MSG = "The chart:\n\n![chart](shots/chart.png)\n\nDone.";

// A fake helper: `version` names agent-images; each src is a 400 x 200 copy, but for "gone" (path null).
let sizeOf, fail;
const line = (o) => JSON.stringify(o);
const helper = (_c, args) => {
  const cmd = args[0];
  const out = [line({ type: "header", protocol: 1, cmd })];
  if (cmd === "version") out.push(line({ type: "version", commands: ["version", "agent-images"] }));
  if (cmd === "agent-images") {
    if (fail) out.push(line({ type: "error", code: "write", message: "no" }));
    else JSON.parse(args[args.indexOf("--srcs") + 1]).forEach((src, index) => out.push(line(src.includes("gone")
      ? { type: "agent-image", index, path: null }
      : { type: "agent-image", index, path: `${HOME}/herdr-contr/images/img-${index}.png`, ...sizeOf(src) })));
  }
  out.push(line({ type: "end", ok: !fail }));
  return { code: 0, stdout: out.join("\n") };
};
const asked = () => log.exec.filter(([, a]) => a[0] === "agent-images");
const images = (el) => {
  const out = [];
  const walk = (e) => { if (!e || typeof e !== "object") return; if (e.tag === "image") out.push(e); for (const k of e.kids ?? []) walk(k); };
  walk(el);
  return out;
};

beforeEach(() => {
  reset(); forgetAgentImages(); forgetCommands();
  sizeOf = () => ({ width: 400, height: 200 }); fail = false;
  fake.exec = helper;
});

test("an image line: one link alone on its line, outside code", () => {
  for (const l of ["![a](x.png)", "  ![a b](/abs/x.svg)", '![](x.jpg "title")', "![a](<my shot.png>)", "![a](x.png)\r"]) assert.ok(IMAGE_LINE.test(l), l);
  for (const l of ["see ![a](x.png)", "![a](x.png) and more", "[a](x.png)", "    ![a](x.png)", "> ![a](x.png)"]) assert.ok(!IMAGE_LINE.test(l), l);
  const text = "![one](a.png)\n\n```\n![code](b.png)\n```\n\n| h |\n| -- |\n| ![t](c.png) |\n![two](<d e.png>)";
  assert.deepEqual(imageLines(text), [{ alt: "one", src: "a.png" }, { alt: "two", src: "d e.png" }]);
  // Without an image for the src, the line stays in the markdown.
  assert.deepEqual(splitBlocks("a\n![x](a.png)\nb", () => null), [{ k: "md", text: "a\n![x](a.png)\nb" }]);
  assert.deepEqual(splitBlocks("a\n![x](a.png)\nb", () => ({ rel: "r" })).map((b) => b.k), ["md", "img", "md"]);
});

test("render asks nothing; after the tick the line is the image, at the height the estimate counts", async () => {
  const el = markdownView("r1", MSG, T, 600, 1, null, LOG);
  assert.equal(log.exec.length, 0, "no helper call while drawing");
  assert.equal(images(el).length, 0, "the link as text until then");
  const v = agentImagesVersion();
  assert.equal(await agentImagesTick(), true);
  assert.ok(agentImagesVersion() > v, "the row heights are measured again");
  const [, args] = asked()[0];
  assert.deepEqual(args.slice(0, 7), ["agent-images", "--log", LOG, "--dir", `${HOME}/herdr-contr/images`, "--srcs", '["shots/chart.png"]']);
  assert.equal(args.at(-2), "--home", "--home goes last");

  const [img] = images(markdownView("r1", MSG, T, 600, 1, null, LOG));
  assert.equal(img._a.path, "herdr-contr/images/img-0.png", "a path from the app home");
  assert.deepEqual([img._a.w, img._a.h], [400, 200]);
  // The image counts as a code box does (no paragraph gap next to it), with its own box and height.
  const box = markdownHeight("The chart:\n\n```\n```\n\nDone.", 600);
  assert.equal(markdownHeight(MSG, 600, null, 1, 1, LOG), box - 34 + IMG_BOX + 200);
  assert.equal(rowHeight({ k: "assistant", text: MSG }, 600, false, 1, 1, null, LOG) - rowHeight({ k: "assistant", text: MSG }, 600), markdownHeight(MSG, 600, null, 1, 1, LOG) - markdownHeight(MSG, 600));
  // Nothing new: nothing asked.
  markdownView("r1", MSG, T, 600, 1, null, LOG);
  assert.equal(await agentImagesTick(), false);
  assert.equal(asked().length, 1);
});

test("the size: no wider than the text, no higher than IMG_MAX_H, never scaled up", () => {
  assert.deepEqual(agentImageSize({ width: 1200, height: 600 }, 602), { w: 600, h: 300 });
  assert.deepEqual(agentImageSize({ width: 300, height: 3000 }, 600), { w: 48, h: IMG_MAX_H - 2 });
  assert.deepEqual(agentImageSize({ width: 40, height: 20 }, 600), { w: 40, h: 20 });
});

test("a URL is never asked for; a missing file stays text and is not asked again", async () => {
  const text = "![web](https://example.com/a.png)\n![data](data:image/png;base64,AA)\n![gone](gone.png)";
  markdownView("r1", text, T, 600, 1, null, LOG);
  assert.equal(await agentImagesTick(), false);
  assert.equal(JSON.parse(asked()[0][1][6]).join(), "gone.png", "only the local path");
  markdownView("r1", text, T, 600, 1, null, LOG);
  assert.equal(await agentImagesTick(), false);
  assert.equal(asked().length, 1);
  assert.equal(images(markdownView("r1", text, T, 600, 1, null, LOG)).length, 0);
  assert.equal(markdownHeight(text, 600, null, 1, 1, LOG), markdownHeight(text, 600));
});

test("no log (a user row, an agent on another machine): nothing noted, the line stays text", async () => {
  noteImages(null, MSG);
  markdownView("r1", MSG, T, 600);
  assert.equal(await agentImagesTick(), false);
  assert.equal(asked().length, 0);
  assert.equal(agentImageOf(null, "shots/chart.png"), null);
  assert.equal(ChatView.prototype.imageLog.call({ address: "mini/t1", feed: () => ({ path: LOG }) }), null);
  assert.equal(ChatView.prototype.imageLog.call({ address: "t1", feed: () => ({ path: LOG }) }), LOG);
  assert.equal(ChatView.prototype.imageLog.call({ address: "t1", feed: () => null }), null);
});

test("a failed ask is asked again after a minute; until then the line is text", async () => {
  fail = true;
  noteImages(LOG, MSG);
  assert.equal(await agentImagesTick(1000), false);
  noteImages(LOG, MSG);
  assert.equal(await agentImagesTick(2000), false);
  assert.equal(asked().length, 1, "not again at once");
  fail = false;
  noteImages(LOG, MSG);
  assert.equal(await agentImagesTick(62_000), true);
  assert.equal(asked().length, 2);
  assert.ok(agentImageOf(LOG, "shots/chart.png"));
});

test("the same src in another agent's log is asked for again (a relative path is from its cwd)", async () => {
  noteImages(LOG, MSG);
  await agentImagesTick();
  assert.equal(agentImageOf("/other.jsonl", "shots/chart.png"), null);
  noteImages("/other.jsonl", MSG);
  assert.equal(await agentImagesTick(), true);
  assert.equal(asked()[1][1][2], "/other.jsonl");
});

test("the drawn images of a message as a list, for a viewer", async () => {
  const text = "![a](a.png)\n\n![g](gone.png)\n\n![b](/abs/b.svg)";
  assert.deepEqual(agentImages(LOG, text), []);
  noteImages(LOG, text);
  await agentImagesTick();
  assert.deepEqual(agentImages(LOG, text), [
    { src: "a.png", alt: "a", rel: "herdr-contr/images/img-0.png", width: 400, height: 200 },
    { src: "/abs/b.svg", alt: "b", rel: "herdr-contr/images/img-2.png", width: 400, height: 200 },
  ]);
  assert.deepEqual(agentImages(null, text), []);
});

test("an older helper without the command: text, and no agent-images call", async () => {
  fake.exec = (c, a) => (a[0] === "version"
    ? { code: 0, stdout: [line({ type: "header", protocol: 1 }), line({ type: "version", commands: ["version"] }), line({ type: "end", ok: true })].join("\n") }
    : helper(c, a));
  noteImages(LOG, MSG);
  assert.equal(await agentImagesTick(), false);
  assert.equal(asked().length, 0);
});

test("the viewer's list: user and agent pictures in chat order; an image not drawn is not in it", async () => {
  const paste = (n) => `${HOME}/herdr-contr/paste/paste-${n}.png`;
  const rows = [
    { id: "u1", k: "user", text: `look ${paste(1)}` },
    { id: "a2", k: "assistant", text: MSG },
    { id: "a3", k: "assistant", text: "![web](https://x.dev/a.png)" },
    { id: "u4", k: "user", text: paste(2) },
  ];
  const order = () => chatPictures(rows, LOG).map((p) => [p.row, p.rel, p.width]);
  assert.deepEqual(order(), [["u1", "herdr-contr/paste/paste-1.png", null], ["u4", "herdr-contr/paste/paste-2.png", null]]);
  noteImages(LOG, MSG);
  await agentImagesTick();
  assert.deepEqual(order(), [["u1", "herdr-contr/paste/paste-1.png", null], ["a2", "herdr-contr/images/img-0.png", 400], ["u4", "herdr-contr/paste/paste-2.png", null]]);
  assert.deepEqual(chatPictures(rows, "/other/machine.jsonl").map((p) => p.row), ["u1", "u4"], "another log: none of these images");
});
