// Mermaid diagrams (#87): a ```mermaid block is a code box until the helper drew it, then an image at the
// height the estimate counts; asked from the tick, never from render.
import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { fake, log, reset } from "./stub/herdr.mjs";
import { markdownView, codeBlockAt } from "../mods/herdr-contr/mdblocks.js";
import { markdownHeight } from "../mods/herdr-contr/estimate.js";
import { forgetCommands } from "../mods/herdr-contr/sidecar.js";
import { isMermaid, mermaidColors, mermaidOf, mermaidSize, mermaidTick, mermaidVersion, forgetMermaid, BOX_X } from "../mods/herdr-contr/mermaid.js";

const DARK = { background: "#1c1d20", foreground: "#e6e6e6", raised: "#26272b", surface: "#202125", border: "#3a3b40", muted: "#9a9ba0" };
const LIGHT = { background: "#ffffff", foreground: "#1d1d1f", raised: "#f2f2f4", surface: "#fafafa", border: "#d4d4d8", muted: "#6b6b70" };
const FLOW = "flowchart TD\n  A --> B";
const MSG = "Here:\n\n```mermaid\n" + FLOW + "\n```\n\nDone.";
const HOME = "/home/test/.config/herdr-app";

// A fake helper: `version` names mermaid; `mermaid` answers a 400 x 200 SVG, or an error for "bad" text.
let size, sizeOf;
const line = (o) => JSON.stringify(o);
const helper = (_c, args) => {
  const cmd = args[0];
  const out = [line({ type: "header", protocol: 1, cmd })];
  if (cmd === "version") out.push(line({ type: "version", commands: ["version", "mermaid"] }));
  if (cmd === "mermaid") {
    const text = args[args.indexOf("--text") + 1];
    if (text.includes("bad")) out.push(line({ type: "error", code: "parse", message: "no" }));
    else out.push(line({ type: "mermaid", path: `${HOME}/herdr-contr/mermaid/${args.includes("--dark") ? "d" : "l"}.svg`, ...sizeOf(text) }));
  }
  out.push(line({ type: "end", ok: true }));
  return { code: 0, stdout: out.join("\n") };
};
const asked = () => log.exec.filter(([, a]) => a[0] === "mermaid");
const boxOf = (el) => el.kids.find((k) => k._a.rounded === 12);  // the code box: rounded 12

beforeEach(() => {
  reset(); forgetMermaid(); forgetCommands();
  size = { width: 400, height: 200 }; sizeOf = () => size;
  fake.exec = helper;
});

test("only a code block with the language mermaid", () => {
  assert.ok(isMermaid({ k: "code", lang: "mermaid" }));
  assert.ok(isMermaid({ k: "code", lang: "Mermaid " }));
  for (const b of [{ k: "code", lang: "js" }, { k: "code", lang: "" }, { k: "md", text: "mermaid" }]) assert.ok(!isMermaid(b));
});

test("the theme's colors and whether it is dark", () => {
  assert.deepEqual(mermaidColors(DARK), { dark: true, fg: "#e6e6e6", bg: "#26272b", fill: "#202125", border: "#3a3b40", line: "#9a9ba0" });
  assert.equal(mermaidColors(LIGHT).dark, false);
});

test("render asks nothing; until the tick has the answer, the block is the code box", async () => {
  const el = markdownView("r1", MSG, DARK, 600);
  assert.equal(log.exec.length, 0, "no helper call while drawing");
  assert.equal(boxOf(el).kids[0].tag, "md", "the code as text");
  const before = markdownHeight(MSG, 600);
  const v = mermaidVersion();
  assert.equal(await mermaidTick(), true);
  assert.ok(mermaidVersion() > v, "the row heights are measured again");
  const [, args] = asked()[0];
  assert.deepEqual(args.slice(0, 4), ["mermaid", "--dir", `${HOME}/herdr-contr/mermaid`, "--dark"]);
  assert.deepEqual(args.slice(args.indexOf("--fg"), args.indexOf("--fg") + 2), ["--fg", "#e6e6e6"]);
  assert.equal(args[args.indexOf("--text") + 1], FLOW);
  assert.equal(args.at(-2), "--home", "--home goes last");

  const img = boxOf(markdownView("r1", MSG, DARK, 600)).kids[0];
  assert.equal(img.tag, "image");
  assert.equal(img._a.path, "herdr-contr/mermaid/d.svg", "a path from the app home");
  assert.deepEqual([img._a.w, img._a.h], [400, 200]);
  // The estimate counts the image, not the code lines: 34 px of box around 200 px.
  const lines = markdownHeight("```js\n" + FLOW + "\n```", 600);
  assert.equal(markdownHeight(MSG, 600) - before, 200 - (lines - 34));
  // A second tick with nothing new asks nothing.
  markdownView("r1", MSG, DARK, 600);
  assert.equal(await mermaidTick(), false);
  assert.equal(asked().length, 1);
});

test("a diagram wider than the text column is scaled down to it", async () => {
  size = { width: 1200, height: 600 };
  markdownView("r1", MSG, DARK, 330);
  await mermaidTick();
  const img = boxOf(markdownView("r1", MSG, DARK, 330)).kids[0];
  assert.deepEqual([img._a.w, img._a.h], [300, 150]);
  assert.deepEqual(mermaidSize(mermaidOf(FLOW), 330), { w: 330 - BOX_X, h: 150 });
  assert.equal(markdownHeight("```mermaid\n" + FLOW + "\n```", 330), 34 + 150 + 0);
});

test("the code box under the pointer still copies the diagram's source", async () => {
  markdownView("r1", MSG, DARK, 600);
  await mermaidTick();
  const top = markdownHeight("Here:\n\n", 600);
  assert.equal(codeBlockAt(MSG, 600, top + 50), FLOW);
});

test("a text the helper cannot draw stays a code box and is not asked again", async () => {
  const bad = "```mermaid\nbad ((\n```";
  markdownView("r1", bad, DARK, 600);
  assert.equal(await mermaidTick(), false);
  markdownView("r1", bad, DARK, 600);
  assert.equal(await mermaidTick(), false);
  assert.equal(asked().length, 1);
  assert.equal(boxOf(markdownView("r1", bad, DARK, 600)).kids[0].tag, "md");
});

test("another theme: drawn again in its colors; the old diagram shows until then", async () => {
  markdownView("r1", MSG, DARK, 600);
  await mermaidTick();
  const el = markdownView("r1", MSG, LIGHT, 600);
  assert.equal(boxOf(el).kids[0]._a.path, "herdr-contr/mermaid/d.svg");
  assert.equal(await mermaidTick(), true);
  assert.ok(!asked()[1][1].includes("--dark"));
  assert.equal(boxOf(markdownView("r1", MSG, LIGHT, 600)).kids[0]._a.path, "herdr-contr/mermaid/l.svg");
});

test("an older helper without the command: code boxes, and no mermaid call", async () => {
  fake.exec = (c, a) => (a[0] === "version"
    ? { code: 0, stdout: [line({ type: "header", protocol: 1 }), line({ type: "version", commands: ["version"] }), line({ type: "end", ok: true })].join("\n") }
    : helper(c, a));
  markdownView("r1", MSG, DARK, 600);
  assert.equal(await mermaidTick(), false);
  assert.equal(asked().length, 0);
});

test("at most two diagrams are asked at once", async () => {
  let live = 0, most = 0;
  fake.exec = (c, a) => {
    if (a[0] !== "mermaid") return helper(c, a);
    live++; most = Math.max(most, live);
    return new Promise((ok) => setTimeout(() => { live--; ok(helper(c, a)); }, 1));
  };
  for (const n of [1, 2, 3, 4]) markdownView(`r${n}`, "```mermaid\nflowchart LR\n  A --> N" + n + "\n```", DARK, 600);
  await mermaidTick();
  assert.equal(most, 2);
  assert.equal(asked().length, 2);
});
