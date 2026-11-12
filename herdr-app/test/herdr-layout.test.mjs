// The layout with Herdr's own sidebar: flockbook-herdr.
import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fake, log, reset } from "./stub/herdr.mjs";
import { peekAtEdge, loadSidebar, sidebarShown, toggleSidebar, showScreen } from "../mods/flockbook/state.js";
import { lightsRoom } from "../mods/flockbook/top.js";

const read = (n) => JSON.parse(readFileSync(new URL(`../${n}`, import.meta.url)));
const layout = read("layouts/flockbook-herdr.json");
const find = (n, id) => n.id === id ? n : (n.children || []).map((c) => find(c, id)).find(Boolean);
const mods = (n, out = []) => { if (n.type === "block" && n.content.mod) out.push(n.content.mod); (n.children || []).forEach((c) => mods(c, out)); return out; };

beforeEach(() => { reset(); fake.shared.layout = "flockbook-herdr"; });

test("the sidebar is Herdr's, shown, and the work area is the mod's screen stack", () => {
  const side = find(layout, "sidebar");
  assert.equal(side.hidden, undefined);
  assert.deepEqual(mods(side), ["workspace-chrome", "spaces", "agents", "workspace-chrome"]);
  const screen = find(layout, "screen");
  assert.equal(screen.type, "stack");
  assert.deepEqual(screen.children.map((c) => c.id), ["chat", "needs", "overview", "tab"]);
  assert.ok(!mods(layout).includes("tabs"), "the mod's own bar, not Herdr's tabs block");
  assert.ok(find(layout, "tab-panes"));
});

test("keys.json has the same keys for this layout", () => {
  const keys = read("keys.json");
  assert.deepEqual(keys["layout:flockbook-herdr"], keys["layout:flockbook"]);
});

test("the left edge does not float the mod's sidebar over Herdr's", () => {
  fake.shared["flockbook.sidebar"] = false;
  peekAtEdge({ position: { x: 0, y: 5 } });
  assert.equal(log.overlays.length, 0);
});

test("the sidebar shows unless this layout's saved state hides it", async () => {
  await loadSidebar();
  assert.equal(sidebarShown(), true);
  reset(); fake.shared.layout = "flockbook-herdr";
  fake.readText = (p) => (p.endsWith("/layouts/.state/flockbook-herdr.json") ? JSON.stringify({ nodes: { sidebar: { hidden: true } } }) : null);
  await loadSidebar();
  assert.equal(sidebarShown(), false);
});

test("⌘B hides and shows Herdr's sidebar column; screens switch in the stack", () => {
  fake.shared["flockbook.sidebar"] = true;
  toggleSidebar();
  assert.deepEqual(log.run.at(-1), ["layout.hide", { id: "sidebar" }]);
  showScreen("needs");
  assert.deepEqual(log.run.at(-1), ["layout.select", { stack: "screen", child: "needs" }]);
});

test("the bar leaves no room for the window buttons while Herdr's sidebar shows", () => {
  fake.layout = layout;
  fake.shared["flockbook.sidebar"] = true;
  assert.equal(lightsRoom(), 12);
  fake.shared["flockbook.sidebar"] = false;
  assert.ok(lightsRoom() > 12);
});
