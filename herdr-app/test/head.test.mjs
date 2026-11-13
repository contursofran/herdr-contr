// The sidebar's head (#97): "Herdr", the machine switch, search and the sidebar toggle; the machines menu;
// and the Projects list, which shows the picked machine's projects only.
import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { fake, log, reset } from "./stub/herdr.mjs";
import { byId, textOf } from "./stub/ui.mjs";
import { fakeCx } from "./fake-cx.mjs";
import Spaces from "../mods/herdr-contr-spaces/main.js";
import { HeadView, MachinesMenu, PICKED, MACHINES, pickedMachine } from "../mods/herdr-contr-spaces/head.js";

const store = new Map();
Object.defineProperty(globalThis, "localStorage", { configurable: true,
  value: { getItem: (k) => store.get(k) ?? null, setItem: (k, v) => store.set(k, v), removeItem: (k) => store.delete(k) } });

const sidebar = (px) => ({ id: "root", children: [{ id: "sidebar", size: { px } }] });
const sp = (id, name, o = {}) => ({ id, name, folder: `/p/${name}`, active: false, tabs: [{ id: `tb-${id}` }], ...o });
const ids = (el, out = []) => { if (el && typeof el === "object") { if (el._a?.id) out.push(el._a.id); (el.kids ?? []).forEach((k) => ids(k, out)); } return out; };

// hc's machine list, as `hc machine ls --json` gives it.
const hcMachines = (list) => { fake.exec = (cmd, args) => (cmd === "hc" && args[0] === "machine" ? { code: 0, stdout: JSON.stringify({ machines: list }) } : { code: 1, stdout: "", stderr: "not a git repository" }); };

beforeEach(() => {
  reset(); store.clear();
  fake.layout = sidebar(268);
  fake.window = { platform: "linux" };
});

test("the head: Herdr, the machine switch, a spacer, search and the sidebar toggle", () => {
  const el = new HeadView().render();
  assert.equal(textOf(el.kids[0]), "Herdr");
  assert.deepEqual([el.kids[0]._a.text_size, "font_semibold" in el.kids[0]._a], [14, true]);
  assert.deepEqual(ids(el).filter((i) => i !== "head"), ["machine-switch", "search", "sidebar-toggle"]);
  assert.equal(textOf(byId(el, "machine-switch")), "This computer");
  assert.equal(byId(el, "machine-switch").kids[1]._a.path, "mods/workspace-chrome/chevron-down.svg");
  for (const id of ["search", "sidebar-toggle"]) assert.deepEqual([byId(el, id)._a.size, byId(el, id)._a.rounded], [28, 8]);
  byId(el, "search")._h.on_click();
  assert.deepEqual(log.run.at(-1), ["mod.open", { name: "picker" }]);
  byId(el, "sidebar-toggle")._h.on_click();
  assert.deepEqual(log.run.at(-1), ["layout.resize", { id: "sidebar", px: 44 }]);
});

test("a press on a head control is no window drag", () => {
  const el = new HeadView().render();
  const cx = { stop_propagation() { this.stopped = true; } };
  byId(el, "machine-switch")._h.on_mouse_down(null, cx);
  assert.ok(cx.stopped);
});

test("the switch names the picked machine; one not known any more is this computer", () => {
  fake.shared[MACHINES] = [{ name: "linux-pc", state: "unreachable" }];
  fake.shared[PICKED] = "linux-pc";
  assert.equal(textOf(byId(new HeadView().render(), "machine-switch")), "linux-pc");
  fake.shared[PICKED] = "gone";
  assert.equal(pickedMachine(), null);
  assert.equal(textOf(byId(new HeadView().render(), "machine-switch")), "This computer");
});

test("a press on the switch opens the machines menu, a popup of this mod", () => {
  fake.shared[MACHINES] = [{ name: "linux-pc", state: "unreachable" }];
  byId(new HeadView().render(), "machine-switch")._h.on_click();
  const o = log.overlays.at(-1);
  assert.deepEqual([o.mod, o.kind, o.props.view], ["herdr-contr-spaces", "popup", "machines"]);
});

test("on the rail: the toggle only; on the macOS rail nothing (the traffic lights fill it)", () => {
  fake.layout = sidebar(44);
  assert.deepEqual(ids(new HeadView().render()).filter((i) => i !== "head"), ["sidebar-toggle"]);
  fake.window = { platform: "macos" };
  assert.deepEqual(ids(new HeadView().render()).filter((i) => i !== "head"), []);
});

test("the menu: this computer, each machine with its state when not connected, a check at the picked one, Add machine", () => {
  fake.shared[MACHINES] = [{ name: "linux-pc", state: "unreachable" }, { name: "mini", state: "connected" }];
  fake.shared[PICKED] = "mini";
  fake.props = { view: "machines", overlay: "o9" };
  const el = new MachinesMenu().render();
  assert.deepEqual(ids(el).filter((i) => i !== "machines-menu"), ["pick-local", "pick-linux-pc", "pick-mini", "machines-add"]);
  assert.equal(textOf(byId(el, "pick-linux-pc")), "linux-pc unreachable");
  assert.equal(textOf(byId(el, "pick-mini")), "mini");
  assert.equal(byId(el, "pick-mini").kids[2].kids.length, 1, "checked");
  assert.equal(byId(el, "pick-local").kids[2].kids.length, 0);
  byId(el, "pick-linux-pc")._h.on_click();
  assert.equal(fake.shared[PICKED], "linux-pc");
  assert.deepEqual(log.closed, ["o9"]);
  byId(el, "pick-local")._h.on_click();
  assert.equal(fake.shared[PICKED], null);
  byId(el, "machines-add")._h.on_click();
  assert.deepEqual(log.run.at(-1), ["mod.open", { name: "machine-add" }]);
});

test("the spaces mod draws the head or the menu for props.view, without the list's timers", () => {
  fake.props = { view: "head" };
  const cx = fakeCx();
  const v = new Spaces(); v.init({}, cx);
  assert.equal(cx.timers.length, 0);
  assert.ok(byId(v.render(cx), "machine-switch"));
  fake.props = { view: "machines" };
  const m = new Spaces(); m.init({}, fakeCx());
  assert.ok(byId(m.render(fakeCx()), "machines-menu"));
});

const list = async () => { const cx = fakeCx(); const v = new Spaces(); v.init({}, cx); await cx.settle(); return v; };

test("the list shows the picked machine's projects only, with no machine rows; it writes the machines for the switch", async () => {
  fake.spaces = [sp("s1", "here"), sp("s2", "there", { machine: "linux-pc" })];
  hcMachines([{ name: "linux-pc", state: "connected" }]);
  const v = await list();
  // Projects with no agent sit in the "N more projects" row of their machine.
  let el = v.render();
  assert.ok(byId(el, "more-local"));
  assert.equal(byId(el, "more-linux-pc"), null);
  assert.ok(!ids(el).some((i) => /^machine-/.test(i)), ids(el).join(","));
  assert.deepEqual(fake.shared[MACHINES], [{ name: "linux-pc", state: "connected" }]);
  const writes = log.shared.filter(([k]) => k === MACHINES).length;
  v.render();
  assert.equal(log.shared.filter(([k]) => k === MACHINES).length, writes, "written only on a change");
  fake.shared[PICKED] = "linux-pc";
  el = v.render();
  assert.ok(byId(el, "more-linux-pc"));
  assert.equal(byId(el, "more-local"), null);
  assert.equal(byId(el, "machine-linux-pc"), null, "a connected machine has no state line");
});

test("+ makes a space on the picked machine", async () => {
  fake.spaces = [sp("s1", "here")];
  hcMachines([{ name: "linux-pc", state: "connected" }]);
  const v = await list();
  byId(v.render(), "space-new")._h.on_click();
  assert.deepEqual(log.run.at(-1), ["space.new", {}]);
  fake.shared[PICKED] = "linux-pc";
  byId(v.render(), "space-new")._h.on_click();
  assert.deepEqual(log.run.at(-1), ["space.new", { machine: "linux-pc" }]);
});

test("a picked machine that is unreachable says so, and offers a new space there", async () => {
  hcMachines([{ name: "linux-pc", state: "unreachable" }]);
  const v = await list();
  fake.shared[PICKED] = "linux-pc";
  const el = v.render();
  assert.match(textOf(byId(el, "machine-linux-pc")), /linux-pc · unreachable/);
  assert.deepEqual(fake.shared[MACHINES], [{ name: "linux-pc", state: "unreachable" }]);
  byId(el, "machine-new-linux-pc")._h.on_click();
  assert.deepEqual(log.run.at(-1), ["space.new", { machine: "linux-pc" }]);
});

test("a picked machine that needs a login shows the command to run, with Copy", async () => {
  hcMachines([{ name: "linux-pc", state: "needs_login" }]);
  const v = await list();
  fake.shared[PICKED] = "linux-pc";
  const el = v.render();
  assert.match(textOf(el), /hc machine login linux-pc/);
  assert.ok(byId(el, "machine-login-copy-linux-pc"));
  assert.deepEqual(fake.shared[MACHINES], [{ name: "linux-pc", state: "needs login" }]);
});

test("a picked machine that was rebuilt offers Connect, which adds it again", async () => {
  hcMachines([{ name: "linux-pc", state: "unreachable", why: 'a different core answers there ("x")', target: "pc" }]);
  const v = await list();
  fake.shared[PICKED] = "linux-pc";
  const el = v.render();
  assert.deepEqual(fake.shared[MACHINES], [{ name: "linux-pc", state: "rebuilt" }]);
  const cx = { ...fakeCx(), stop_propagation() {} };
  byId(el, "machine-connect-linux-pc")._h.on_click(null, cx);
  await cx.settle();
  const calls = log.exec.filter(([c]) => c === "hc").map(([, a]) => a.slice(0, 2).join(" "));
  assert.ok(calls.includes("machine rm") && calls.includes("machine add"), calls.join(","));
});

test("on the macOS rail the list shows the toggle, and a press opens the sidebar again (#144)", async () => {
  fake.spaces = [sp("s1", "here")];
  hcMachines([]);
  const v = await list();
  assert.equal(byId(v.render(), "rail-open"), null, "the wide sidebar has its toggle in the head");
  fake.layout = sidebar(76);
  assert.equal(byId(v.render(), "rail-open"), null, "off macOS the head keeps the toggle");
  fake.window = { platform: "macos" };
  fake.shared["sidebar.width"] = 300;
  byId(v.render(), "rail-open")._h.on_click();
  assert.deepEqual(log.run.at(-1), ["layout.resize", { id: "sidebar", px: 300 }]);
});
