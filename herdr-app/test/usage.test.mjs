import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { fake, log, reset } from "./stub/herdr.mjs";
import { textOf, byId } from "./stub/ui.mjs";
import { fakeCx } from "./fake-cx.mjs";
import { SideView } from "../mods/herdr-contr/side.js";
import { forgetCommands } from "../mods/herdr-contr/sidecar.js";
import { pollUsage, resetUsage, usagePlans, planRow, untilText, USAGE_MS } from "../mods/herdr-contr/usage.js";

const NOW = Math.floor(Date.now() / 1000);
const answer = (...lines) => ({ code: 0, stdout: [{ type: "header", protocol: 1 }, ...lines, { type: "end", ok: true }].map((l) => JSON.stringify(l)).join("\n") });
const PLANS = [
  { plan: "claude", windows: [{ kind: "five_hour", used_percent: 38.4, resets_at: NOW + 7200 }, { kind: "seven_day", used_percent: 12, resets_at: NOW + 3 * 86400 }], read_at: NOW },
  { plan: "codex", windows: [{ kind: "primary", used_percent: 90, resets_at: NOW - 5 }, { kind: "secondary", used_percent: 20, resets_at: null }], read_at: NOW },
];
let commands, plans, usageCalls;

beforeEach(() => {
  reset(); forgetCommands(); resetUsage();
  commands = ["version", "usage"]; plans = PLANS; usageCalls = 0;
  fake.exec = (_cmd, args) => {
    if (args[0] === "version") return answer({ type: "version", commands });
    if (args[0] === "usage") { usageCalls++; return answer({ type: "usage", plans }); }
    return { code: 1, stdout: "" };
  };
});

const rowIds = (el) => el.kids.map((k) => k?._a?.id);

test("the sidebar reads the usage off the UI thread and shows Claude, then Codex, above herdr-contr and Settings", async () => {
  const cx = fakeCx();
  new SideView(cx);
  await cx.settle();
  assert.equal(usageCalls, 1);
  assert.equal(cx.notified, 1);
  assert.ok(cx.timers.some((x) => x.ms === USAGE_MS && !x.once), "read again every USAGE_MS");
  const el = new SideView(fakeCx()).render(fakeCx());
  const ids = rowIds(el);
  const [a, b, c, d] = ["side-usage-claude", "side-usage-codex", "side-herdr-contr", "side-settings"].map((id) => ids.indexOf(id));
  assert.ok(a > 0 && a < b && b < c && c < d, ids.join(","));
  assert.equal(textOf(byId(el, "side-usage-claude")), "Claude 5h 38% 2h · wk 12% 3d");
});

test("a read within USAGE_MS is not repeated; after it, it is", async () => {
  await pollUsage(null, 1000);
  await pollUsage(null, 1000 + USAGE_MS - 1);
  assert.equal(usageCalls, 1);
  await pollUsage(null, 1000 + USAGE_MS);
  assert.equal(usageCalls, 2);
});

test("a plan with no data has no row; none at all: no usage rows", async () => {
  plans = [{ plan: "codex", windows: [{ kind: "primary", used_percent: 5, resets_at: null }] }, { plan: "claude", windows: [] }, { plan: "other", windows: [{ kind: "x", used_percent: 1 }] }];
  await pollUsage(null);
  assert.deepEqual(usagePlans().map((p) => p.plan), ["codex"]);
  resetUsage(); plans = [];
  await pollUsage(null);
  const ids = rowIds(new SideView(fakeCx()).render(fakeCx()));
  assert.ok(!ids.some((id) => String(id).startsWith("side-usage")), ids.join(","));
});

test("an older helper without usage, or an error, does not call it or keeps what was shown", async () => {
  commands = ["version"];
  await pollUsage(null, 1);
  assert.equal(usageCalls, 0);
  assert.deepEqual(usagePlans(), []);
  forgetCommands(); commands = ["version", "usage"];
  await pollUsage(null, 1 + USAGE_MS);
  assert.equal(usagePlans().length, 2);
  fake.exec = (_c, args) => args[0] === "version" ? answer({ type: "version", commands }) : { code: 1, stdout: "", stderr: "boom" };
  await pollUsage(null, 1 + 2 * USAGE_MS);
  assert.equal(usagePlans().length, 2, "kept");
  assert.ok(log.exec.length > 0);
});

test("row text: whole percents, the time to reset, a window past its reset is 0%; the meter is the most used", () => {
  assert.deepEqual(planRow(PLANS[1], NOW), { name: "Codex", used: 20, text: "5h 0% · wk 20%" });
  assert.equal(planRow(PLANS[0], NOW).used, 38);
  assert.deepEqual([untilText(30), untilText(45 * 60), untilText(5 * 3600), untilText(2 * 86400)], ["1m", "45m", "5h", "2d"]);
});
