// The Version row of the herdr-contr settings: the helper's `update check|install` (docs/protocol.md, update).
import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { fake, log, reset } from "./stub/herdr.mjs";
import { textOf, byId } from "./stub/ui.mjs";
import { fakeCx } from "./fake-cx.mjs";
import { SettingsView } from "../mods/herdr-contr/settings.js";
import { resetUpdate } from "../mods/herdr-contr/update.js";

const jl = (...lines) => ({ code: 0, stdout: lines.map((l) => JSON.stringify(l)).join("\n") + "\n", stderr: "" });
const header = (cmd) => ({ type: "header", protocol: 1, sidecar: "0.1.0", cmd });
const end = (ok = true) => ({ type: "end", ok });
const NEW = "abc1234 2026-11-16";
let installed, fail;
const updates = () => log.exec.filter(([, a]) => a[0] === "update").map(([, a]) => a[1]);

beforeEach(() => {
  reset(); resetUpdate(); installed = "1111111 2026-11-10"; fail = null;
  fake.exec = (_c, args) => {
    if (args[0] !== "update") return jl(header(args[0]), end());
    if (args[1] === "check") return jl(header("update"), { type: "update", installed, newest: NEW, tag: "build-2026-11-16-abc1234", differs: installed !== NEW }, end());
    if (fail) return jl(header("update"), { type: "error", code: "check", message: fail }, end(false));
    installed = NEW;
    return jl(header("update"), { type: "update-plan", text: "Dry run" }, { type: "update-done", installed: NEW, text: "Installed" }, end());
  };
});

async function open() {
  const cx = fakeCx();
  const v = new SettingsView(cx);
  await cx.settle();
  return () => byId(v.render(fakeCx()), "set-update");
}
const press = async (row) => { const cx = fakeCx(); byId(row, "set-update-go")._h.on_click({}, cx); await cx.settle(); };

test("opening the settings checks once; an older build shows Newest and Update", async () => {
  const row = await open();
  assert.deepEqual(updates(), ["check"]);
  assert.match(textOf(row()), /herdr-contr 1111111 2026-11-10/);
  assert.match(textOf(row()), /Newest: abc1234 2026-11-16/);
  assert.ok(byId(row(), "set-update-go"));
});

test("the newest build shows no Update button", async () => {
  installed = NEW;
  const row = await open();
  assert.match(textOf(row()), /This is the newest build/);
  assert.equal(byId(row(), "set-update-go"), null);
});

test("Update installs and says to restart Herdr", async () => {
  const row = await open();
  await press(row());
  assert.deepEqual(updates(), ["check", "install"]);
  assert.match(textOf(byId(row(), "set-update-result")), /Installed abc1234 2026-11-16\. Restart Herdr/);
  assert.match(textOf(row()), /herdr-contr abc1234 2026-11-16/);
  assert.equal(byId(row(), "set-update-go"), null);
});

test("a failed update shows the error and keeps the button", async () => {
  fail = "remote/x does not match SHA256SUMS. Nothing was installed.";
  const row = await open();
  await press(row());
  assert.match(textOf(byId(row(), "set-update-result")), /does not match SHA256SUMS/);
  assert.ok(byId(row(), "set-update-go"));
});

test("a failed check says the newest build is not known", async () => {
  fake.exec = () => jl(header("update"), { type: "error", code: "no-gh", message: "gh could not start" }, end(false));
  const row = await open();
  assert.match(textOf(row()), /newest build is not known/);
  assert.match(textOf(byId(row(), "set-update-result")), /gh could not start/);
});
