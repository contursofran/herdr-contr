// The Chat | Terminal switch: the per-tab pick, its key, the block over the terminal, the strip over the chat.
import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fake, log, reset, theme } from "./stub/herdr.mjs";
import { textOf, byId } from "./stub/ui.mjs";
import { fakeCx } from "./fake-cx.mjs";
import { agent, space } from "./fixtures.mjs";
import { screenFor, toggleView, setView } from "../mods/herdr-contr/state.js";
import { command, syncScreen } from "../mods/herdr-contr/top.js";
import { viewSwitch, SwitchView } from "../mods/herdr-contr/viewswitch.js";
import { ChatView } from "../mods/herdr-contr/chat.js";

const read = (n) => JSON.parse(readFileSync(new URL(`../${n}`, import.meta.url)));
const tab = (id, index, extra = {}) => ({ id, index, name: String(index), named: false, active: false, status: null, panes: 1, settled: false, ...extra });
const screen = () => fake.shared["herdr-contr.screen"];

beforeEach(() => {
  reset();
  fake.agents = [agent("t1", "idle", { tab: "tb1" })];
  fake.spaces = [space("sp1", { tabs: [tab("tb1", 1, { active: true }), tab("tb2", 2)] })];
});

test("the key switches the agent's tab between chat and terminal, focuses the terminal, and the tab keeps the pick", () => {
  const [tb1, tb2] = fake.spaces[0].tabs;
  assert.equal(screenFor(tb1), "chat");
  command("view");
  assert.equal(screen(), "tab");
  assert.deepEqual(log.run.at(-1), ["agent.focus", { terminal: "t1" }]);
  assert.equal(screenFor(tb1), "tab", "the tab keeps the terminal");
  assert.equal(screenFor(tb2), "tab", "a tab without an agent is always its terminals");
  toggleView();
  assert.equal(screen(), "chat"); assert.equal(screenFor(tb1), "chat");
  // From Needs you or Overview, the key goes to the chat.
  fake.shared["herdr-contr.screen"] = "needs"; toggleView();
  assert.equal(screen(), "chat");
  // A split tab with an agent can show the chat too.
  setView("chat", tab("tb1", 1, { panes: 2 }));
  assert.equal(screenFor(tab("tb1", 1, { panes: 2 })), "chat");
});

test("the key does nothing on a tab without an agent", () => {
  fake.agents = [];
  command("view");
  assert.equal(screen(), undefined);
  assert.equal(log.run.length, 0);
});

test("the switch shows the current screen; a press on the other side switches", () => {
  const el = viewSwitch(theme(), "chat");
  assert.match(textOf(el), /Chat.*Terminal/);
  byId(el, "view-tab")._h.on_click({}, fakeCx());
  assert.equal(screen(), "tab");
  byId(viewSwitch(theme(), "tab"), "view-tab")._h.on_click({}, fakeCx());
  assert.equal(screen(), "tab", "the shown side does nothing");
});

test("the herdr-contr layout has the switch block over the tab's terminal; the sync hides it for a shell tab", () => {
  const layout = read("layouts/herdr-contr.json");
  fake.layout = layout;
  const col = JSON.stringify(layout).indexOf('"view-switch"'), panes = JSON.stringify(layout).indexOf('"tab-panes"');
  assert.ok(col > 0 && col < panes, "the switch is above the panes");
  syncScreen({});
  assert.deepEqual(log.run.filter(([c]) => c.startsWith("layout.s") || c === "layout.hide").map(([c, a]) => [c, a.id ?? a.child]), [["layout.show", "view-switch"], ["layout.select", "chat"]]);
  log.run.length = 0;
  fake.spaces[0].tabs[0].active = false; fake.spaces[0].tabs[1].active = true;
  syncScreen({ seen: "tb1:chat" });
  assert.deepEqual(log.run.map(([c, a]) => [c, a.id ?? a.child]), [["layout.hide", "view-switch"], ["layout.select", "tab"]]);
  // The block draws the switch for an agent's tab, nothing for a shell tab.
  assert.equal(textOf(new SwitchView().render()), "");
  fake.spaces[0].tabs[0].active = true; fake.spaces[0].tabs[1].active = false;
  assert.match(textOf(new SwitchView().render()), /Chat.*Terminal/);
});

test("the chat draws the switch only under Herdr's tab bar", async () => {
  fake.exec = () => ({ code: 127, stdout: "", stderr: "none" });
  const cx = fakeCx();
  fake.layout = read("layouts/herdr-contr.json");
  const v = new ChatView(cx); await cx.settle(); v.width = 900;
  assert.ok(byId(v.render(cx), "view-switch"));
  fake.layout = read("layouts/herdr-contr-own.json");
  assert.equal(byId(v.render(cx), "view-switch"), null);
});
