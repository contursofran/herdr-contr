// The Shortcuts table (#165): every key that the mods bind is in it, and each of its keys is bound.
import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { fake, reset } from "./stub/herdr.mjs";
import { fakeCx } from "./fake-cx.mjs";
import { textOf, byId } from "./stub/ui.mjs";
import { SHORTCUTS, SHORTCUT_GROUPS, expand, caps, keyCaps } from "../mods/herdr-contr/shortcuts.js";
import { ChatView, spaceKey } from "../mods/herdr-contr/chat.js";
import { keyName } from "../mods/herdr-contr/panelcard.js";
import { digit } from "../mods/herdr-contr/cards.js";
import { SettingsView } from "../mods/herdr-contr/settings.js";

beforeEach(() => reset());

const rows = (bound) => SHORTCUTS.filter((s) => s.bound === bound);
const strokes = (list) => new Set(list.flatMap((s) => s.keys.flatMap(expand)));
const keysJson = JSON.parse(readFileSync(new URL("../keys.json", import.meta.url)));

test("keys.json: each herdr-contr key and the project keys are in the table, with the same command, in both layouts", () => {
  for (const [layout, map] of Object.entries(keysJson)) {
    const ours = Object.entries(map).filter(([, a]) => a.startsWith("herdr-contr.") || a === "space.prev" || a === "space.next");
    for (const [k, a] of ours) {
      const row = rows("keys.json").find((s) => s.keys.flatMap(expand).includes(k));
      assert.ok(row, `${layout}: ${k} (${a}) is not in the table`);
      assert.ok(a.startsWith(row.action), `${layout}: ${k} is ${a}, the table says ${row.action}`);
    }
    for (const s of rows("keys.json")) for (const k of s.keys.flatMap(expand)) assert.ok(map[k]?.startsWith(s.action), `${layout}: ${k} is not bound`);
  }
});

test("bind_keys: the reply box binds exactly the table's keys, in their contexts", async () => {
  const cx = fakeCx(); const bound = [];
  cx.bind_keys = (b) => { bound.push(...b); return b.length; };
  new ChatView(cx); await cx.settle();
  const want = rows("bind_keys").flatMap((s) => s.keys.map((k) => `${k} ${s.context} > Input`)).sort();
  assert.deepEqual(bound.map((b) => `${b.keystroke} ${b.context}`).sort(), want);
});

test("no mod binds keys in a place that the table does not know", () => {
  const known = new Set(["composer.js"]);
  for (const s of SHORTCUTS) if (s.file) known.add(s.file);
  for (const mod of ["herdr-contr", "herdr-contr-spaces", "herdr-contr-tabs"]) {
    const dir = new URL(`../mods/${mod}/`, import.meta.url);
    for (const f of readdirSync(dir).filter((x) => x.endsWith(".js"))) {
      const src = readFileSync(new URL(f, dir), "utf8");
      // cards.js: the `keep` handler only stops the press, it binds no key.
      const binds = /bind_keys\(|on_key_down\(\(/.test(src);
      if (binds) assert.ok(mod === "herdr-contr" && known.has(f), `${mod}/${f} binds keys that the table does not list`);
    }
  }
  const manifests = ["herdr-contr", "herdr-contr-spaces", "herdr-contr-tabs"].map((m) => JSON.parse(readFileSync(new URL(`../mods/${m}/mod.json`, import.meta.url))));
  for (const m of manifests) assert.deepEqual(Object.keys(m).sort(), ["place", "title"], "a manifest binds no keys");
});

// Every key press that a handler could take: the named keys and the characters, with and without modifiers.
const NAMED = ["up", "down", "left", "right", "enter", "tab", "space", "escape", "backspace", " "];
const CHARS = [..."0123456789abcdefghijklmnopqrstuvwxyz-=,./`"];
const MODS = [{}, { shift: true }, { alt: true }, { control: true }, { platform: true }, { platform: true, alt: true }];
const stroke = (key, m) => [m.control && "ctrl", m.platform && "cmd", m.alt && "alt", m.shift && "shift", key === " " ? "space" : key].filter(Boolean).join("-");
const takes = (fn) => {
  const out = new Set();
  for (const key of [...NAMED, ...CHARS]) for (const m of MODS) if (fn({ key, modifiers: m })) out.add(stroke(key, m));
  return out;
};
const sub = (got, want, what) => { for (const k of got) assert.ok(want.has(k), `${what} takes ${k}, which the table does not list`); };

test("handlers: the panel card, the question and approval cards, the chat and the settings take only the table's keys", () => {
  const file = (f) => strokes(SHORTCUTS.filter((s) => s.file === f));
  sub(takes((e) => keyName(e) !== null), file("panelcard.js"), "panel card");
  assert.deepEqual([...takes((e) => keyName(e) !== null)].sort(), [...file("panelcard.js")].sort());
  // cards.js: digit() and Enter. digit() lets Shift through: the table lists the plain digit.
  const cards = file("cards.js");
  sub(takes((e) => digit(e) !== null && !e.modifiers.shift), cards, "cards");
  assert.ok(cards.has("enter"));
  sub(takes((e) => spaceKey(e) !== null), file("chat.js"), "chat");
  sub(takes((e) => e?.key === "enter" || e?.key === "space" || e?.key === " "), new Set([...file("settings.js"), ...[...file("settings.js")].flatMap((k) => MODS.map((m) => stroke(k, m)))]), "settings");
});

test("caps: ⌘ on macOS, Ctrl elsewhere; ranges show their ends", () => {
  assert.deepEqual(caps("cmd-shift-t", "macos"), ["⌘", "⇧", "T"]);
  assert.deepEqual(caps("cmd-shift-t", "linux"), ["Ctrl", "Shift", "T"]);
  assert.deepEqual(caps("secondary-v", "windows"), ["Ctrl", "V"]);
  assert.deepEqual(caps("cmd-alt--", "macos"), ["⌘", "⌥", "-"]);
  assert.deepEqual(caps("ctrl-`", "macos"), ["⌃", "`"]);
  assert.deepEqual(caps("cmd-ctrl-shift-left", "linux"), ["Ctrl", "Shift", "←"]);
  assert.deepEqual(keyCaps("ctrl-1..9", "macos"), [["⌃", "1"], "…", ["⌃", "9"]]);
  assert.deepEqual(expand("0..2"), ["0", "1", "2"]);
});

test("settings: the Shortcuts section lists each row under its group, with the platform's caps, and nothing to change", () => {
  const v = new SettingsView(fakeCx()); v.section = "shortcuts";
  let el = v.render(fakeCx());
  const text = textOf(el);
  for (const g of SHORTCUT_GROUPS) assert.ok(text.includes(g.toUpperCase()), g);
  for (const s of SHORTCUTS) assert.ok(text.includes(s.what), s.what);
  assert.ok(byId(el, "set-key-window-0"));
  assert.match(textOf(byId(el, "set-key-window-0")), /⌘.*J.*or.*⌘.*⇧.*T/);
  fake.window = { ...fake.window, platform: "linux" };
  el = v.render(fakeCx());
  assert.match(textOf(byId(el, "set-key-window-0")), /Ctrl.*J.*or.*Ctrl.*Shift.*T/);
  assert.doesNotMatch(textOf(byId(el, "settings-root")), /⌘/);
});
