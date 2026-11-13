import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import HerdrContr from "../mods/herdr-contr/main.js";
import { textOf } from "./stub/ui.mjs";
import { fakeCx } from "./fake-cx.mjs";
import { fake } from "./stub/herdr.mjs";

const layout = JSON.parse(readFileSync(new URL("../layouts/herdr-contr-own.json", import.meta.url)));
function blocks(n, out = []) { if (n.type === "block") out.push(n); (n.children || []).forEach((c) => blocks(c, out)); return out; }

test("every layout block shows the herdr-contr mod with a known view, or the shown tab", () => {
  const mods = blocks(layout).filter((b) => b.content.mod);
  const views = mods.map((b) => { assert.equal(b.content.mod, "herdr-contr"); return b.content.props.view; });
  assert.deepEqual(views.sort(), ["chat", "needs", "overview", "side", "top"], "settings is a dialog, not a block");
  assert.deepEqual(blocks(layout).filter((b) => !b.content.mod).map((b) => [b.id, b.content.tab]), [["tab-panes", "active"]]);
});

test("keys.json binds only commands that exist, in the herdr-contr layout's section only", () => {
  const keys = JSON.parse(readFileSync(new URL("../keys.json", import.meta.url)));
  // Not mod:herdr-contr: with it, no key worked at all.
  assert.deepEqual(Object.keys(keys), ["layout:herdr-contr-own", "layout:herdr-contr"]);
  const mine = ["terminal", "view", ...Array.from({ length: 9 }, (_, i) => `main-${i + 1}`), "needs", "move-left", "move-right", "sidebar", "overview", "settings", "text-smaller", "text-larger", "text-default"];
  for (const [key, action] of Object.entries(keys["layout:herdr-contr-own"])) {
    if (action.startsWith("herdr-contr.")) assert.ok(mine.includes(action.slice("herdr-contr.".length)), `${key}: ${action}`);
  }
});

test("each view renders, and an unknown view says so", () => {
  for (const view of ["top", "side", "chat", "needs", "overview", "settings", "usage"]) {
    fake.props = { view }; const m = new HerdrContr(); m.init(undefined, fakeCx());
    assert.doesNotMatch(textOf(m.render(fakeCx())), /unknown view/, view);
  }
  fake.props = { view: "nope" }; const m = new HerdrContr(); m.init(undefined, fakeCx());
  assert.match(textOf(m.render(fakeCx())), /unknown view "nope"/);
});
