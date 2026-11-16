// The Health section of Settings (#217): the helper's checks, a row each, and Re-check.
import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { fake, log, reset } from "./stub/herdr.mjs";
import { textOf, byId } from "./stub/ui.mjs";
import { fakeCx } from "./fake-cx.mjs";
import { SettingsView } from "../mods/herdr-contr/settings.js";
import { checkHealth, healthRows, healthError, forgetHealth } from "../mods/herdr-contr/health.js";

const jl = (...lines) => ({ code: 0, stdout: lines.map((l) => JSON.stringify(l)).join("\n") + "\n", stderr: "" });
const header = { type: "header", protocol: 1, sidecar: "0.1.0", cmd: "health" };
const end = { type: "end", ok: true };
const check = (id, state, message, fix = "") => ({ type: "check", id, state, message, fix });
const health = () => log.exec.filter(([, a]) => a[0] === "health");

beforeEach(() => {
  reset(); forgetHealth();
  fake.shared = { layout: "herdr-contr" };
  fake.updates = { app: { version: "0.1.16" }, core: { version: "0.5.0" } };
  fake.exec = (_c, args) => args[0] === "health"
    ? jl(header, check("mods", "ok", "Mods are installed.", "Nothing."), check("layout", "warn", "The layout in use is main.", "Switch the layout."), end)
    : jl({ ...header, cmd: args[0] }, end);
});

test("health: passes the layout, Herdr's and hc's versions to the helper, and keeps its rows", async () => {
  await checkHealth();
  const [, args] = health()[0];
  assert.deepEqual(args.slice(0, 7), ["health", "--layout", "herdr-contr", "--herdr", "0.1.16", "--hc", "0.5.0"]);
  assert.deepEqual(healthRows().map((r) => r.state), ["ok", "warn"]);
  assert.equal(healthError(), null);
});

test("health: an unknown layout or version is left out", async () => {
  fake.shared = {}; fake.updates = { app: {} };
  await checkHealth();
  assert.deepEqual(health()[0][1].slice(0, 2), ["health", "--home"]);
});

test("health: a helper that fails or gives no rows shows why", async () => {
  fake.exec = () => ({ code: 127, stdout: "", stderr: "not found" });
  await checkHealth();
  assert.match(healthError(), /did not answer/);
  fake.exec = (_c, args) => jl({ ...header, cmd: args[0] }, end);
  await checkHealth();
  assert.match(healthError(), /older than this mod/);
});

test("settings: the Health section shows a row a check, the fix of a warning, and Re-check runs the checks again", async () => {
  const v = new SettingsView(fakeCx());
  await new Promise((r) => setTimeout(r, 5));
  assert.equal(health().length, 1, "checked when Settings opens");
  v.section = "health";
  const el = v.render(fakeCx());
  assert.ok(byId(el, "set-nav-health"));
  assert.match(textOf(byId(el, "set-health-mods")), /✓.*Mods are installed\./);
  assert.doesNotMatch(textOf(byId(el, "set-health-mods")), /Fix:/);
  assert.match(textOf(byId(el, "set-health-layout")), /!.*main\..*Fix: Switch the layout\./);
  byId(el, "set-health-recheck")._h.on_click({}, fakeCx());
  await new Promise((r) => setTimeout(r, 5));
  assert.equal(health().length, 2);
});
