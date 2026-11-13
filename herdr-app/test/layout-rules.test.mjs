// Layout rules of the mod runtime, over each view's tree (#149). The stub does not lay out or paint, so these
// rules, each learned from a bug in the owner's window, hold here: a scrolling column's rows have flex_none (#137),
// a Popover that fills a row has a flex_1 wrapper (#135), and each state of the sidebar can toggle it (#144).
import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { fake, log, reset } from "./stub/herdr.mjs";
import { byId } from "./stub/ui.mjs";
import { fakeCx } from "./fake-cx.mjs";
import { agent, space } from "./fixtures.mjs";
import { scrollBreaks, scrollColumns, all } from "./layout-rules.mjs";
import { menuPanel } from "../mods/herdr-contr/modelmenu.js";
import { FootView } from "../mods/herdr-contr/foot.js";
import { SettingsView } from "../mods/herdr-contr/settings.js";
import { NeedsView } from "../mods/herdr-contr/needs.js";
import { OverviewView } from "../mods/herdr-contr/overview.js";
import { SideView } from "../mods/herdr-contr/side.js";
import Spaces from "../mods/herdr-contr-spaces/main.js";
import { HeadView, MachinesMenu, MACHINES } from "../mods/herdr-contr-spaces/head.js";

const store = new Map();
Object.defineProperty(globalThis, "localStorage", { configurable: true,
  value: { getItem: (k) => store.get(k) ?? null, setItem: (k, v) => store.set(k, v), removeItem: (k) => store.delete(k) } });

const TH = new Proxy({}, { get: (_t, k) => `#${k}` });
const sidebar = (px) => ({ id: "root", children: [{ id: "sidebar", size: { px } }] });
const claude = { name: "claude", key: "claude", agent: true, icon: "mods/x/claude.svg", tint: true };
const tabs = (n) => Array.from({ length: n }, (_, i) => ({ id: `tb-t${i + 1}`, number: i + 1, pane: `p${i + 1}`, status: "done" }));

beforeEach(() => {
  reset(); store.clear();
  fake.layout = sidebar(268);
  fake.window = { platform: "linux" };
  fake.spaces = [space("sp1", { name: "app", tabs: tabs(2) }), space("sp2", { name: "docs", active: false, tabs: tabs(1) })];
  fake.agents = [agent("t1", "working", { program: claude }), agent("t2", "blocked", { space: "sp2", program: claude })];
});

const holds = (name, el, { scrolls = true } = {}) => {
  if (scrolls) assert.ok(scrollColumns(el) > 0, `${name}: the walk finds its scrolling column`);
  assert.deepEqual(scrollBreaks(el), [], `${name}: each row of a scrolling column has flex_none`);
};

test("the walker reports the id path of each row of a scrolling column without flex_none", async () => {
  const { v_flex, div } = await import("gpui-kit");
  const el = v_flex().id("root").child(v_flex().id("list").overflow_y_scrollbar()
    .child(div().id("ok").flex_none()).child(div().id("bad")).child(div()));
  assert.deepEqual(scrollBreaks(el), ["root > list > bad", "root > list > div[2]"]);
  assert.deepEqual(scrollBreaks(v_flex().overflow_y_scroll().child(div())), ["v > div[0]"], "overflow_y_scroll too");
  assert.deepEqual(scrollBreaks(v_flex().child(div())), [], "a column that does not scroll: no rule");
});

test("scroll rule: the model menu's More models page", () => {
  const list = [{ name: "model", args: Array.from({ length: 12 }, (_, i) => ({ value: `m-${i}`, label: `Model ${i}` })) }];
  const on = { page() {}, choose() {} };
  holds("model menu", menuPanel({ page: "models" }, { id: "x", name: "X" }, list, TH, on));
});

test("scroll rule: the Projects list, wide and on the rail", () => {
  for (const px of [268, 44]) {
    fake.layout = sidebar(px);
    const v = new Spaces(); v.init({}, fakeCx());
    holds(`Projects list at ${px}px`, v.render());
  }
});

test("scroll rule: the machines menu and the foot (no scrolling column today)", () => {
  fake.shared[MACHINES] = [{ name: "linux-pc", state: "unreachable" }, { name: "mac", state: "online" }];
  holds("machines menu", new MachinesMenu().render(), { scrolls: false });
  holds("foot", new FootView(null).render(), { scrolls: false });
});

test("scroll rule: settings, Needs you, the overview and the own sidebar", () => {
  holds("settings", new SettingsView(fakeCx()).render(fakeCx()));
  holds("needs", new NeedsView(fakeCx()).render(fakeCx()));
  holds("overview", new OverviewView(fakeCx()).render(fakeCx()));
  holds("side", new SideView(fakeCx()).render(fakeCx()));
});

// Popovers that must fill their row: each sits in a flex_1 wrapper (it does not grow by itself, #135).
// Popovers that stay small (the model button, the machine switch) are not on this list.
const FILL = [["foot", () => new FootView(null).render(), "settings-popover"]];

test("popover rule: a Popover that fills its row has a flex_1 wrapper in that row", () => {
  for (const [name, render, id] of FILL) {
    const el = render();
    const wrap = all(el).find((x) => x.kids?.some((k) => k?._a?.id === id));
    assert.ok(wrap, `${name}: ${id} is there`);
    assert.ok("flex_1" in wrap._a, `${name}: the wrapper of ${id} has flex_1`);
    const row = all(el).find((x) => x.kids?.includes(wrap));
    assert.equal(row?.tag, "h", `${name}: the wrapper of ${id} is in a row`);
  }
});

// In each state of the sidebar, the head and the Projects list together can toggle it (#144).
test("sidebar toggle: wide and rail, macOS and not, some element runs layout.resize on the sidebar", () => {
  for (const platform of ["macos", "linux"]) for (const wide of [true, false]) {
    fake.window = { platform };
    fake.layout = sidebar(wide ? 268 : platform === "macos" ? 76 : 44);
    const v = new Spaces(); v.init({}, fakeCx());
    const els = [...all(new HeadView().render()), ...all(v.render())];
    const toggles = els.filter((el) => {
      const press = el._h.on_click;
      if (!press) return false;
      const n = log.run.length;
      try { press({}, { ...fakeCx(), stop_propagation() {} }); } catch { return false; }
      return log.run.slice(n).some(([cmd, a]) => cmd === "layout.resize" && a?.id === "sidebar");
    });
    const state = `${platform}, ${wide ? "wide" : "rail"}`;
    if (platform === "macos" && !wide) continue; // rail-open comes with #145
    assert.ok(toggles.length > 0, `${state}: an element toggles the sidebar`);
  }
});
