// The tabs' short names (#127), and the copies of the sidebar's title cleaning.
import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fake, reset } from "./stub/herdr.mjs";
import { byId, textOf } from "./stub/ui.mjs";
import { tabLabel } from "../mods/herdr-contr-tabs/label.js";
import Tabs from "../mods/herdr-contr-tabs/main.js";

beforeEach(() => reset());

const claude = { name: "Claude Code", key: "claude", agent: true };
const zsh = { name: "zsh", key: "zsh" };

test("a named tab shows its name", () => {
  assert.equal(tabLabel({ name: "api", named: true }, { title: "✳ Fix the login", program: claude }), "api");
});

test("an unnamed tab shows its lead agent's cleaned title", () => {
  assert.equal(tabLabel({ name: "1", named: false }, { title: "✳ Fix the login", program: claude }), "Fix the login");
  assert.equal(tabLabel({ name: "2", named: true }, { title: "⠋ Fix the login", program: claude }), "Fix the login");
  assert.equal(tabLabel({ name: "tb3" }, { title: "Fix the login", program: claude }), "Fix the login");
});

test("a title that tells nothing gives the program's name", () => {
  assert.equal(tabLabel({ name: "1" }, { title: "~/code/app", program: zsh }), "zsh");
  assert.equal(tabLabel({ name: "1" }, { title: "me@mac", program: zsh }), "zsh");
  assert.equal(tabLabel({ name: "1" }, { title: "zsh", program: zsh }), "zsh");
});

test("the number only when there is nothing else", () => {
  assert.equal(tabLabel({ name: "4" }, undefined), "4");
  assert.equal(tabLabel({ name: "4" }, { title: "" }), "4");
});

test("the tab draws the short name, truncated at 140 px, with the full text as tooltip", () => {
  const long = "Fix the login page and the session refresh";
  fake.spaces = [{ id: "s1", active: true, tabs: [{ id: "t1", name: "1", active: true }] }];
  fake.panes = [{ id: "p1", tab: "t1", terminal: true, title: `✳ ${long}`, program: claude }];
  const v = new Tabs();
  v.scroll = { scroll_to_item() {} };
  const tab = byId(v.render(), "tab-t1");
  assert.equal(tab._a.tooltip, long);
  const box = tab.kids.find((k) => k?._a?.max_w === 140);
  assert.ok(box && "truncate" in box._a);
  assert.match(textOf(box), /Fix the login/);
});

test("the copies of clean, leadPanes and title equal the sidebar's", () => {
  const src = (p) => readFileSync(new URL(p, import.meta.url), "utf8");
  const a = src("../mods/herdr-contr-spaces/projects.js"), b = src("../mods/herdr-contr-tabs/label.js");
  const part = (s, start) => { const i = s.indexOf(start); assert.ok(i >= 0, start); return s.slice(i, s.indexOf("\n}\n", i) + 3); };
  const cleanLine = (s) => s.split("\n").find((l) => l.startsWith("const clean ="));
  assert.equal(cleanLine(b), cleanLine(a));
  for (const f of ["export function leadPanes(", "function title("]) assert.equal(part(b, f), part(a, f));
});
