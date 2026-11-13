// The model menu (#112): a press on the reply box's model button opens it above the button. Row 1 is the current
// model with a short line and a check mark; then "Effort" with its value (its values on a press); then "More models"
// (the other models on a press). A press on a value sends "/model <id>" or "/effort <level>" to the agent through
// the helper's `send`, the same path as a typed command. The values are the helper's `commands` answer: the values
// of the agent's `/model` and `/effort` (docs/protocol.md, commands). An agent whose `/model` has no values gets
// no menu.
import { div, svg } from "gpui-kit";
import { v_flex, h_flex } from "gpui-base";
import { icon } from "./state.js";

// A model id as people say it: "claude-opus-5-5" → "Opus 5.5", "claude-fable-5-2" → "Fable 5.2", "gpt-6" → "GPT-6".
// A name that is short already ("Opus 5.5", from the status line) stays. Kept by id: each draw asks.
const CLAUDE_ID = /^claude-(?:(\d+)-(\d+)-)?([a-z]+)(?:-(\d+)(?:-(\d{1,2}))?)?(?:-\d{8})?$/;
const names = new Map();
export function modelName(id) {
  const m = String(id ?? "").trim();
  if (names.has(m)) return names.get(m);
  let out = m;
  const c = m.match(CLAUDE_ID);
  if (c) {
    const ver = c[1] ? `${c[1]}.${c[2]}` : c[4] ? (c[5] ? `${c[4]}.${c[5]}` : c[4]) : "";
    out = [c[3][0].toUpperCase() + c[3].slice(1), ver].filter(Boolean).join(" ");
  } else if (/^gpt-\d/i.test(m)) out = `GPT-${m.split("-")[1]}`;
  else if (/^(opus|sonnet|haiku|fable)$/i.test(m)) out = m[0].toUpperCase() + m.slice(1).toLowerCase();
  if (names.size >= 100) names.clear();
  names.set(m, out);
  return out;
}

// One fixed line for each known model family, under the model's name. An unknown family has none.
export const FAMILY_LINES = {
  fable: "Most capable, for the hardest work",
  opus: "Deep reasoning for complex work",
  sonnet: "Smart and fast for everyday work",
  haiku: "Fastest, for quick tasks",
};
export function modelLine(id) {
  const family = modelName(id).split(" ")[0].toLowerCase();
  return Object.prototype.hasOwnProperty.call(FAMILY_LINES, family) ? FAMILY_LINES[family] : "";
}

// "medium" → "Medium".
export const titleCase = (s) => (s ? String(s)[0].toUpperCase() + String(s).slice(1) : "");

// The values of the built-in `name` in the agent's commands list, as { value, label }; none: [].
function argsOf(list, name) {
  const c = (list ?? []).find((x) => x.name === name && Array.isArray(x.args) && x.args.length);
  return c ? c.args.map((x) => (x && typeof x === "object" ? { value: String(x.value ?? ""), label: String(x.label ?? "") } : { value: String(x), label: "" }))
    .filter((x) => x.value) : [];
}

// Whether the agent's `/model` takes values the helper knows: then it has a menu.
export const hasModelMenu = (list) => argsOf(list, "model").length > 0;

// The efforts `/effort` takes ("low" … "max"), or [] (the agent has no such command: the row has no sub menu).
export const effortValues = (list) => argsOf(list, "effort").map((x) => x.value);

// The other models: { value, name, line } for each `/model` value, in the helper's order, without the current
// model (by id or by name) and with each name once. Kept by list and current name: the menu draws it on each frame.
const others = new Map();
export function otherModels(list, current) {
  const cur = modelName(current);
  const key = others.get(list);
  if (key && key.cur === cur) return key.items;
  const seen = new Set([cur]);
  const items = [];
  for (const a of argsOf(list, "model")) {
    const name = a.label || modelName(a.value);
    if (a.value === current || seen.has(name) || modelName(a.value) === cur) continue;
    seen.add(name);
    items.push({ value: a.value, name, line: modelLine(a.value) });
  }
  if (others.size >= 20) others.clear();
  others.set(list, { cur, items });
  return items;
}

const ROW_PX = 34;

// The panel. `m`: { page: "main" | "effort" | "models" }; `model`: modelInfo's answer; `list`: the agent's commands;
// `on`: { page(p, cx), choose(text, cx) }.
export function menuPanel(m, model, list, t, on) {
  const check = () => svg(icon("check")).size(14).flex_none().text_color(t.accent);
  const chevron = (dir) => svg(icon(`chevron-${dir}`)).size(12).flex_none().text_color(t.muted);
  const row = (id) => h_flex().id(id).w_full().min_h(ROW_PX).px(10).py(6).gap(10).items_center().rounded(8).hover((s) => s.bg(t.hover));
  const label = (text) => div().flex_1().min_w(0).truncate().text_size(15).text_color(t.foreground).child(text);
  const divider = (id) => div().id(id).w_full().h(1).my(4).bg(t.border);
  const named = (id, name, line, checked) => row(id)
    .child(v_flex().flex_1().min_w(0).gap(2)
      .child(div().min_w(0).truncate().text_size(15).text_color(t.foreground).child(name))
      .when(Boolean(line), (el) => el.child(div().min_w(0).truncate().text_size(13).text_color(t.muted).child(line))))
    .when(checked, (el) => el.child(check()));
  const back = (title) => row("menu-back").on_click((_e, cx) => on.page("main", cx)).child(chevron("left")).child(label(title));
  const panel = v_flex().id("model-menu").w(280).p(4).gap(0).rounded(12).bg(t.raised).border(1).border_color(t.border).shadow_lg();
  const efforts = effortValues(list);
  const more = otherModels(list, model.id || model.name);

  if (m.page === "effort" && efforts.length) {
    return panel.child(back("Effort")).child(divider("menu-div-back"))
      .children(efforts.map((v) => row(`menu-effort-${v}`).on_click((_e, cx) => on.choose(`/effort ${v}`, cx))
        .child(label(titleCase(v))).when(v === String(model.effort).toLowerCase(), (el) => el.child(check()))));
  }
  if (m.page === "models" && more.length) {
    return panel.child(back("More models")).child(divider("menu-div-back"))
      .child(v_flex().id("menu-models").w_full().max_h(320).overflow_y_scrollbar()
        .children(more.map((x, i) => named(`menu-model-${i}`, x.name, x.line, false)
          .on_click((_e, cx) => on.choose(`/model ${x.value}`, cx)))));
  }
  return panel
    .child(named("menu-current", model.name, modelLine(model.id || model.name), true).on_click((_e, cx) => on.page(null, cx)))
    .when(Boolean(model.effort) || efforts.length > 0, (el) => el.child(divider("menu-div-effort"))
      .child(row("menu-effort").when(efforts.length > 0, (r) => r.on_click((_e, cx) => on.page("effort", cx)))
        .child(label("Effort"))
        .when(Boolean(model.effort), (r) => r.child(div().flex_none().text_size(15).text_color(t.muted).child(titleCase(model.effort))))
        .when(efforts.length > 0, (r) => r.child(chevron("right")))))
    .when(more.length > 0, (el) => el.child(divider("menu-div-more"))
      .child(row("menu-more").on_click((_e, cx) => on.page("models", cx)).child(label("More models")).child(chevron("right"))));
}
