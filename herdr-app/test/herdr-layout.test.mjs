// The layout with Herdr's own sidebar: herdr-contr.
import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fake, log, reset, theme } from "./stub/herdr.mjs";
import { byId } from "./stub/ui.mjs";
import { peekAtEdge, loadSidebar, sidebarShown, toggleSidebar, showScreen } from "../mods/herdr-contr/state.js";
import { lightsRoom, herdrTabBar, barH, overBar, BAR_H } from "../mods/herdr-contr/top.js";

const read = (n) => JSON.parse(readFileSync(new URL(`../${n}`, import.meta.url)));
const layout = read("layouts/herdr-contr.json");
const find = (n, id) => n.id === id ? n : (n.children || []).map((c) => find(c, id)).find(Boolean);
const mods = (n, out = []) => { if (n.type === "block" && n.content.mod) out.push(n.content.mod); (n.children || []).forEach((c) => mods(c, out)); return out; };

beforeEach(() => { reset(); fake.shared.layout = "herdr-contr"; });

test("the sidebar is Herdr's, shown, and the work area is the mod's screen stack", () => {
  const side = find(layout, "sidebar");
  assert.equal(side.hidden, undefined);
  assert.deepEqual(mods(side), ["workspace-chrome", "herdr-contr-spaces", "herdr-contr-agents", "herdr-contr", "workspace-chrome"]);
  // The plan usage: between the lists and Herdr's footer, hidden until a plan has data.
  assert.deepEqual(side.children.map((c) => c.id).slice(-3), ["sidebar-lists", "usage", "sidebar-footer"]);
  const usage = find(layout, "usage");
  assert.deepEqual([usage.content.props.view, usage.size.px, usage.hidden], ["usage", 56, true]);
  const screen = find(layout, "screen");
  assert.equal(screen.type, "stack");
  assert.deepEqual(screen.children.map((c) => c.id), ["chat", "needs", "overview", "tab"]);
  assert.ok(find(layout, "tab-panes"));
});

test("the top bar is Herdr's tabs block, with Herdr's Files and Changes panel beside the screens", () => {
  const work = find(layout, "work");
  assert.deepEqual(work.children.map((c) => c.id), ["tabs", "work-body"]);
  assert.equal(find(layout, "tabs").content.mod, "herdr-contr-tabs");
  assert.deepEqual(find(layout, "work-body").children.map((c) => c.id), ["screen", "panel"]);
  const panel = find(layout, "panel");
  assert.equal(panel.hidden, true);
  assert.deepEqual(mods(panel), ["files", "changes"]);
  assert.ok(!find(layout, "top"), "no bar of the mod's own");
  assert.ok(!JSON.stringify(layout).includes('"view":"top"'));
});

test("with Herdr's tab bar the screens draw no bar and leave no room for one", () => {
  fake.layout = layout;
  assert.equal(herdrTabBar(), true);
  assert.equal(barH(), 0);
  assert.ok(byId(overBar(theme()), "no-bar"));
  assert.equal(byId(overBar(theme()), "over-bar"), null);
  fake.layout = read("layouts/herdr-contr-own.json");
  assert.equal(herdrTabBar(), false);
  assert.equal(barH(), BAR_H);
  assert.ok(byId(overBar(theme()), "over-bar"));
});

test("keys.json has the same keys for this layout", () => {
  const keys = read("keys.json");
  assert.deepEqual(keys["layout:herdr-contr"], keys["layout:herdr-contr-own"]);
});

test("the left edge does not float the mod's sidebar over Herdr's", () => {
  fake.shared["herdr-contr.sidebar"] = false;
  peekAtEdge({ position: { x: 0, y: 5 } });
  assert.equal(log.overlays.length, 0);
});

test("the sidebar shows unless this layout's saved state hides it", async () => {
  await loadSidebar();
  assert.equal(sidebarShown(), true);
  reset(); fake.shared.layout = "herdr-contr";
  fake.readText = (p) => (p.endsWith("/layouts/.state/herdr-contr.json") ? JSON.stringify({ nodes: { sidebar: { hidden: true } } }) : null);
  await loadSidebar();
  assert.equal(sidebarShown(), false);
});

test("⌘B hides and shows Herdr's sidebar column; screens switch in the stack", () => {
  fake.shared["herdr-contr.sidebar"] = true;
  toggleSidebar();
  assert.deepEqual(log.run.at(-1), ["layout.hide", { id: "sidebar" }]);
  showScreen("needs");
  assert.deepEqual(log.run.at(-1), ["layout.select", { stack: "screen", child: "needs" }]);
});

test("the bar leaves no room for the window buttons while Herdr's sidebar shows", () => {
  fake.layout = layout;
  fake.shared["herdr-contr.sidebar"] = true;
  assert.equal(lightsRoom(), 12);
  fake.shared["herdr-contr.sidebar"] = false;
  assert.ok(lightsRoom() > 12);
});

test("Herdr's own tabs mod in the tabs block also counts as the tab bar", () => {
  fake.layout = JSON.parse(JSON.stringify(layout));
  find(fake.layout, "tabs").content.mod = "tabs";
  assert.equal(herdrTabBar(), true);
  find(fake.layout, "tabs").content.mod = "other";
  assert.equal(herdrTabBar(), false);
});
