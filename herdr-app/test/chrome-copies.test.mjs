// Our copies of Herdr's agents and tabs mods: the working mark is herdr-contr's braille loader.
import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fake, reset, theme } from "./stub/herdr.mjs";
import { byId } from "./stub/ui.mjs";
import { BRAILLE } from "../mods/herdr-contr/state.js";
import Agents from "../mods/herdr-contr-agents/main.js";
import Tabs from "../mods/herdr-contr-tabs/main.js";

beforeEach(() => reset());

const isLoader = (el) => {
  assert.deepEqual(el._a.frames, [BRAILLE, 80]);
  assert.equal(el._a.font_family, "Menlo");
  assert.equal(el._a.line_height, 1);
  assert.equal(el._a.text_size, 12);
  assert.equal(el.kids[0], BRAILLE[0]);
  assert.equal(el._a.spin, undefined);
};

test("agents: a working agent shows the braille loader, not the arc", () => {
  const el = new Agents().status(theme(), { status: "working" });
  assert.equal(el.tag, "div");
  isLoader(el);
  assert.equal(el._a.text_color, "#working");
});

test("tabs: a working tab shows the braille loader, not the arc", () => {
  fake.spaces = [{ id: "s1", active: true, tabs: [{ id: "t1", name: "work", active: true, status: "working" }] }];
  const v = new Tabs();
  v.scroll = { scroll_to_item() {} };
  const tab = byId(v.render(), "tab-t1");
  const marks = tab.kids.filter((k) => k?._a?.frames);
  assert.equal(marks.length, 1);
  isLoader(marks[0]);
  assert.ok(!tab.kids.some((k) => k?._a?.path === "mods/workspace-chrome/spinner.svg"));
});

test("the copies keep no arc spinner, and tabs' window icons point at the copy", () => {
  for (const m of ["agents", "tabs"]) {
    const src = readFileSync(new URL(`../mods/herdr-contr-${m}/main.js`, import.meta.url), "utf8");
    assert.ok(!src.includes("spinner.svg") && !src.includes(".spin("), m);
    assert.match(src, new RegExp(`^// Adapted from Herdr's ${m} mod \\(Apache-2.0\\); changes: braille loader for the working mark\\.`));
  }
  const tabs = readFileSync(new URL("../mods/herdr-contr-tabs/main.js", import.meta.url), "utf8");
  assert.ok(!tabs.includes("mods/tabs/"));
  for (const n of ["win-min", "win-max", "win-restore", "win-close"]) assert.ok(tabs.includes(n));
});
