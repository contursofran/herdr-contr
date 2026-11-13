// The sidebar's foot in the herdr-contr layout (#97): the usage bars, then Settings and the add machine button.
import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { fake, log, reset } from "./stub/herdr.mjs";
import { textOf, byId } from "./stub/ui.mjs";
import { fakeCx } from "./fake-cx.mjs";
import { forgetCommands } from "../mods/herdr-contr/sidecar.js";
import { resetUsage, USAGE_MS } from "../mods/herdr-contr/usage.js";
import { FootView, SETTINGS_TIP } from "../mods/herdr-contr/foot.js";

const NOW = Math.floor(Date.now() / 1000);
const answer = (...lines) => ({ code: 0, stdout: [{ type: "header", protocol: 1 }, ...lines, { type: "end", ok: true }].map((l) => JSON.stringify(l)).join("\n") });
const PLANS = [
  { plan: "claude", windows: [{ kind: "five_hour", used_percent: 12, resets_at: NOW + 7200 }, { kind: "seven_day", used_percent: 41, resets_at: NOW + 4 * 86400 }] },
  { plan: "codex", windows: [{ kind: "secondary", used_percent: 7, resets_at: null }] },
];
let plans;
const sidebar = (px) => ({ id: "root", children: [{ id: "sidebar", size: { px } }] });

beforeEach(() => {
  reset(); forgetCommands(); resetUsage(); plans = PLANS;
  fake.layout = sidebar(268);
  fake.exec = (_cmd, args) => {
    if (args[0] === "version") return answer({ type: "version", commands: ["version", "usage"] });
    if (args[0] === "usage") return answer({ type: "usage", plans });
    return { code: 1, stdout: "" };
  };
});

const loaded = async () => { const cx = fakeCx(); const v = new FootView(cx); await cx.settle(); return { v, cx }; };

test("the foot starts the reads and shows one bar row per plan: name, bar, value", async () => {
  const { v, cx } = await loaded();
  assert.ok(cx.timers.some((x) => x.ms === USAGE_MS && !x.once));
  const el = v.render();
  assert.equal(textOf(byId(el, "foot-usage-claude")), "Claude 41% wk · 4d");
  assert.equal(textOf(byId(el, "foot-usage-codex")), "Codex 7% wk");
  const bar = byId(el, "foot-usage-claude-bar");
  assert.deepEqual([bar._a.h, bar._a.rounded, bar._a.bg], [4, 2, "#hover"]);
  assert.deepEqual([bar.kids[0]._a.w, bar.kids[0]._a.bg], ["41%", "#dim"]);
  assert.equal(byId(el, "foot-usage-claude").kids[0]._a.w, 52);
});

test("no plan with data: no usage rows, only Settings and the add machine button", () => {
  const el = new FootView(null).render();
  assert.equal(byId(el, "foot-usage-claude"), null);
  assert.equal(el.kids.length, 1);
  assert.deepEqual(el.kids[0].kids.map((k) => k._a.id), ["foot-settings", "foot-add-machine"]);
});

test("Settings fills the width and opens Herdr's settings; a right-click opens the herdr-contr settings", () => {
  const s = byId(new FootView(null).render(), "foot-settings");
  assert.ok("flex_1" in s._a, "fills the width");
  assert.equal(textOf(s), "Settings");
  assert.equal(s._a.tooltip, SETTINGS_TIP);
  s._h.on_click();
  assert.deepEqual(log.run.at(-1), ["app.settings", undefined]);
  s._h.on_mouse_down_right();
  assert.equal(log.overlays.at(-1).props.view, "settings");
});

test("the add machine button: an icon with a tooltip that opens Herdr's add machine dialog", () => {
  const b = byId(new FootView(null).render(), "foot-add-machine");
  assert.equal(b._a.tooltip, "Add machine");
  assert.equal(textOf(b), "");
  b._h.on_click();
  assert.deepEqual(log.run.at(-1), ["mod.open", { name: "machine-add" }]);
});

test("an update that is ready shows a dot on Settings", () => {
  assert.equal(byId(new FootView(null).render(), "foot-update"), null);
  fake.updates = { app: { ready: true } };
  assert.ok(byId(new FootView(null).render(), "foot-update"));
});

test("on the rail: the bars only, with the name and value in a tooltip, and Settings only", async () => {
  const { v } = await loaded();
  fake.layout = sidebar(76);
  const el = v.render();
  const row = byId(el, "foot-usage-claude");
  assert.equal(row._a.tooltip, "Claude 41% wk · 4d");
  assert.equal(textOf(row), "");
  assert.equal(byId(el, "foot-add-machine"), null);
  assert.ok(byId(el, "foot-settings"));
});
