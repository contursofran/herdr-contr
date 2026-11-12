// The herdr-contr row of the herdr-contr layout, above Herdr's Settings row.
import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fake, log, reset } from "./stub/herdr.mjs";
import { textOf, byId } from "./stub/ui.mjs";
import { fakeCx } from "./fake-cx.mjs";
import { SettingsRowView, SETTINGS_KEY } from "../mods/herdr-contr/settingsrow.js";

const read = (n) => JSON.parse(readFileSync(new URL(`../${n}`, import.meta.url)));
const find = (n, id) => n.id === id ? n : (n.children || []).map((c) => find(c, id)).find(Boolean);
const sidebar = (px) => ({ id: "root", children: [{ id: "sidebar", size: { px } }] });

beforeEach(() => { reset(); fake.layout = sidebar(268); });

test("the herdr-contr layout has the row between the usage block and Herdr's footer; the own-sidebar layout does not", () => {
  const side = find(read("layouts/herdr-contr.json"), "sidebar");
  assert.deepEqual(side.children.map((c) => c.id).slice(-3), ["usage", "settings-row", "sidebar-footer"]);
  const b = find(side, "settings-row");
  assert.deepEqual([b.content.mod, b.content.props.view, b.size.px, b.hidden], ["herdr-contr", "settings-row", 32, undefined]);
  assert.equal(find(read("layouts/herdr-contr-own.json"), "settings-row"), undefined);
});

test("a click on the row opens the herdr-contr settings dialog", () => {
  const row = byId(new SettingsRowView().render(fakeCx()), "herdr-contr-settings");
  assert.match(textOf(row), /herdr-contr/);
  row._h.on_click();
  assert.equal(log.overlays.at(-1).props.view, "settings");
  assert.equal(log.overlays.at(-1).kind, "dialog");
});

test("the key hint shows only on hover", () => {
  const v = new SettingsRowView();
  const cx = fakeCx();
  assert.equal(byId(v.render(cx), "herdr-contr-settings-hint"), null);
  byId(v.render(cx), "herdr-contr-settings")._h.on_hover(true, cx);
  assert.equal(cx.notified, 1);
  assert.equal(textOf(byId(v.render(cx), "herdr-contr-settings-hint")), SETTINGS_KEY);
  byId(v.render(cx), "herdr-contr-settings")._h.on_hover(false, cx);
  assert.equal(byId(v.render(cx), "herdr-contr-settings-hint"), null);
});

test("on the rail the row is an icon with a tooltip and no label", () => {
  fake.layout = sidebar(76);
  const v = new SettingsRowView();
  const row = byId(v.render(fakeCx()), "herdr-contr-settings");
  assert.equal(row._a.tooltip, "herdr-contr");
  assert.doesNotMatch(textOf(row), /herdr-contr/);
});
