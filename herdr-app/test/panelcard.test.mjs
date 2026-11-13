import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { fake, log, reset } from "./stub/herdr.mjs";
import { textOf, byId } from "./stub/ui.mjs";
import { fakeCx } from "./fake-cx.mjs";
import { PanelCard, keyName, rowKey } from "../mods/herdr-contr/panelcard.js";
import { forgetCommands } from "../mods/herdr-contr/sidecar.js";

const jl = (...lines) => ({ code: 0, stdout: lines.map((l) => JSON.stringify(l)).join("\n") + "\n", stderr: "" });
const header = (command) => ({ type: "header", protocol: 1, sidecar: "0.1.0", command });
const end = { type: "end", ok: true };
const T = new Proxy({}, { get: (_t, k) => `#${k}` });
const MODEL = [" Select model", "", " ❯ 1. Default (Opus)  ✔", "   2. Sonnet", "", " Enter to confirm · Esc to exit"];

let panelNow, sent, commands, result;
beforeEach(() => {
  reset(); forgetCommands(); sent = []; panelNow = MODEL; commands = ["panel", "keys"]; result = "sent";
  fake.exec = (_c, args) => {
    if (args[0] === "version") return jl(header("version"), { type: "version", commands }, end);
    if (args[0] === "panel") return jl(header("panel"), { type: "panel", lines: panelNow }, end);
    if (args[0] === "keys") { sent.push(JSON.parse(args[args.indexOf("--keys") + 1])); if (result === "sent" && sent.at(-1)[0] === "Escape") panelNow = null;
      return jl(header("keys"), { type: "done", result, message: result === "sent" ? null : "no panel on screen now; nothing was sent" }, end); }
    return jl(header(args[0]), end);
  };
});

test("keyName: arrows, Enter, Tab, Space, Escape, digits; nothing with a modifier", () => {
  assert.equal(keyName({ key: "down" }), "Down");
  assert.equal(keyName({ key: "escape" }), "Escape");
  assert.equal(keyName({ key: "space" }), "Space");
  assert.equal(keyName({ key: "0" }), "0");
  assert.equal(keyName({ key: "a" }), null);
  assert.equal(keyName({ key: "1", modifiers: { platform: true } }), null);
  assert.equal(keyName({ key: "tab", modifiers: { shift: true } }), null);
});

test("rowKey: a numbered row sends its digit, the pointed row Enter, other rows nothing", () => {
  assert.equal(rowKey(" ❯ 1. Default (Opus)  ✔"), "1");
  assert.equal(rowKey("   2. Sonnet"), "2");
  assert.equal(rowKey(" ❯ fix the tests"), "Enter");
  assert.equal(rowKey(" Select model"), null);
  assert.equal(rowKey("2024 was a year"), null);
});

test("the card shows the panel's text in a mono box; it leaves when the panel closes", async () => {
  const cx = fakeCx(); const p = new PanelCard();
  await p.poll(cx, "t1", true);
  const el = p.view("t1", T);
  assert.match(textOf(el), /The terminal shows a panel.*Select model.*Default \(Opus\).*Sonnet/s);
  panelNow = null; await p.poll(cx, "t1", true, true);
  assert.equal(p.view("t1", T), null);
});

test("not wanted (the agent works, another card shows): the card goes and panel is not read", async () => {
  const cx = fakeCx(); const p = new PanelCard();
  await p.poll(cx, "t1", true);
  const before = log.exec.length;
  await p.poll(cx, "t1", false);
  assert.equal(p.view("t1", T), null);
  assert.equal(log.exec.length, before);
});

test("an older helper without panel: no card and no panel read", async () => {
  commands = ["feed"];
  const cx = fakeCx(); const p = new PanelCard();
  await p.poll(cx, "t1", true);
  assert.equal(p.view("t1", T), null);
  assert.ok(!log.exec.some(([, a]) => a[0] === "panel"));
});

test("a calm rate: no second read before PANEL_MS while it shows", async () => {
  const cx = fakeCx(); const p = new PanelCard();
  await p.poll(cx, "t1", true);
  const n = log.exec.length;
  await p.poll(cx, "t1", true);
  assert.equal(log.exec.length, n);
});

test("keys go out only on a key or a press: a key, a row press, a button", async () => {
  const cx = fakeCx(); const p = new PanelCard();
  await p.poll(cx, "t1", true);
  assert.deepEqual(sent, [], "a read sends no key");
  assert.equal(p.key_down({ key: "down" }, cx, "t1"), true); await cx.settle();
  assert.equal(p.key_down({ key: "x" }, cx, "t1"), false);
  byId(p.view("t1", T), "panel-card-row-3")._h.on_click({}, cx); await cx.settle();
  byId(p.view("t1", T), "panel-card-key-Escape")._h.on_click({}, cx); await cx.settle();
  assert.deepEqual(sent, [["Down"], ["2"], ["Escape"]]);
  assert.equal(p.view("t1", T), null, "Escape closed the panel; the card read again and left");
});

test("a key that did not go out shows the helper's note", async () => {
  result = "changed";
  const cx = fakeCx(); const p = new PanelCard();
  await p.poll(cx, "t1", true);
  p.key_down({ key: "enter" }, cx, "t1"); await cx.settle();
  assert.match(textOf(p.view("t1", T)), /no panel on screen now/);
});
