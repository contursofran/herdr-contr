// The owner's questions (#131). contr keeps the questions that a coordinator asked with ask_owner (and
// the ones its workers raised through it) in one owner file (contr's internal/owner). Each open
// question shows in the chat of the coordinator that asked it (the item's `coordinator`: its hc
// terminal) as the question card of cards.js, one at a time, oldest first. A press answers it with
// `herdr-contr-orch owner answer <id> <answer>`: the tool writes the file under its lock and sends the coordinator
// "Owner answered <id>: <answer>". The mod reads the open questions with `herdr-contr-orch owner open
// --json` every OWNER_MS (#186); it never reads the file. An answered or decided item leaves.
// The same list gives the owner's open to-dos (`kind: "todo"`, pinned with pin_for_owner; #195). Each shows
// as the to-do card in the chat of its coordinator, one at a time, oldest first: its text, where it came
// from, a copy button for each code span and code block, and Done (`herdr-contr-orch owner done <id>`).
import { div, svg } from "gpui-kit";
import { v_flex, h_flex, TextView } from "gpui-base";
import { runOrch } from "./orch.js";
import { Cards, answersFor } from "./cards.js";
import { machineOf, icon } from "./state.js";
import { splitBlocks, codeCopyText } from "./mdblocks.js";

export const OWNER_MS = 3000;     // the open questions are read at most this often
export const NOTE_MS = 15000;     // a note without a card stays this long
export const COPY_MS = 1500;      // a copy button shows a check this long

const str = (v) => (typeof v === "string" ? v : "");
// "hc:t2" → "t2": the key that a question's `coordinator` and an agent's address share.
const local = (address) => String(address ?? "").replace(/^hc:/, "");

// One open question as a card of cards.js. The recommended option goes first; its impact is its detail.
export function cardOf(q) {
  const options = (Array.isArray(q.options) ? q.options : []).map((label, i) => ({
    label: str(label), detail: str(q.impacts?.[i]), recommended: q.recommended === i,
  }));
  const rec = options.findIndex((o) => o.recommended);
  if (rec > 0) options.unshift(...options.splice(rec, 1));
  const from = str(q.from);
  return {
    kind: "question", request: q.id, title: q.id, unread: 0, owner: true,
    refs: (Array.isArray(q.refs) ? q.refs : []).map(str).filter(Boolean),
    question: { questions: [{ id: q.id, header: [str(q.title), from && `from ${from}`].filter(Boolean).join(" · "),
      text: str(q.question), multi: false, options }] },
  };
}

// The open questions that `owner open --json` gives, by coordinator, oldest first. A question that no
// coordinator asked, or one with no id or text, gives nothing.
export function openByCoordinator(list) {
  const out = new Map();
  for (const q of Array.isArray(list) ? list : []) {
    if (q?.kind === "todo") continue;
    const who = local(q?.coordinator);
    if (!who || !str(q.id) || !str(q.question) || str(q.answer)) continue;
    if (!out.has(who)) out.set(who, []);
    out.get(who).push(cardOf(q));
  }
  return out;
}

// The open to-dos that `owner open --json` gives, by coordinator, oldest first. A to-do that no coordinator
// pinned, or one with no id or text, gives nothing.
export function todosByCoordinator(list) {
  const out = new Map();
  for (const t of Array.isArray(list) ? list : []) {
    const who = local(t?.coordinator);
    if (t?.kind !== "todo" || !who || !str(t.id) || !str(t.text)) continue;
    if (!out.has(who)) out.set(who, []);
    out.get(who).push({ id: t.id, from: str(t.from), text: t.text, know: t.todo_kind === "know" });
  }
  return out;
}

// The code spans of a markdown text, outside code blocks, as written (CommonMark: line ends as spaces, one
// space cut at each end when both ends have one). Each text once, in order.
export function codeSpans(md) {
  const out = [];
  for (const b of splitBlocks(md)) {
    if (b.k !== "md") continue;
    for (const m of b.text.matchAll(/(?<!`)(`+)(?!`)([\s\S]*?[^`])\1(?!`)/g)) {
      let code = m[2].replace(/\r?\n/g, " ");
      if (/^ [\s\S]* $/.test(code) && code.trim()) code = code.slice(1, -1);
      if (code.trim() && !out.includes(code)) out.push(code);
    }
  }
  return out;
}

// Runs `herdr-contr-orch owner answer`.
// The answer has the sidecar's shape, for Cards.act.
export async function answerOwner(id, text) {
  const r = await runOrch(["owner", "answer", id, text]);
  if (r.ok) return { lines: [{ type: "done", result: "sent" }], error: null };
  const why = r.error;
  if (r.code != null && /coordinator not reached/.test(why)) {
    return { lines: [{ type: "done", result: "saved", message: `The answer is saved, but the coordinator did not get it: ${why}` }], error: null };
  }
  return { lines: [], error: { message: why } };
}

// Runs `herdr-contr-orch owner done`. { ok, note }: a note when the tool failed, or when the to-do is
// marked done but the coordinator did not get it.
export async function doneOwner(id) {
  const r = await runOrch(["owner", "done", id]);
  if (r.ok) return { ok: true, note: null };
  if (r.code != null && /coordinator not reached/.test(r.error)) return { ok: true, note: `The to-do is marked done, but the coordinator did not get it: ${r.error}` };
  return { ok: false, note: r.error };
}

export class OwnerCards extends Cards {
  constructor(cx) {
    super(cx);
    this.text = undefined;   // the tool's output at the last read
    this.open = new Map();   // coordinator → cards
    this.todos = new Map();  // coordinator → to-dos
    this.todoBy = new Map(); // address → { sending, note, noteAt }: the to-do card's state
    this.copied = null;      // { key, at }: the copy button that shows a check
    this.readAt = 0; this.reading = false;
  }

  // Runs `owner open --json` at most every OWNER_MS unless `force`. True when its output changed. A tool
  // that is missing or fails gives no cards.
  async refresh(force = false) {
    if (this.reading || (!force && Date.now() - this.readAt < OWNER_MS)) return false;
    this.reading = true; this.readAt = Date.now();
    try {
      const r = await runOrch(["owner", "open", "--json"]);
      let list = null;
      if (r.ok) { try { list = JSON.parse(r.stdout); } catch { list = null; } }
      const text = Array.isArray(list) ? r.stdout : null;
      if (text === this.text) return false;
      this.text = text; this.open = openByCoordinator(list); this.todos = todosByCoordinator(list);
      return true;
    } finally { this.reading = false; }
  }

  // The card of `address`: its coordinator's oldest open question. Only local agents: the file is this
  // machine's. `want` false drops it.
  async poll(cx, address, want = true, force = false) {
    if (!address) return;
    await this.refresh(force);
    const e = this.entry(address);
    if (e.sending) return;
    const list = want && !machineOf(address) ? this.open.get(local(address)) ?? [] : [];
    const card = list.length ? { ...list[0], more: list.length - 1 } : null;
    if (e.noteAt && Date.now() - e.noteAt > NOTE_MS) { e.note = null; e.noteAt = 0; cx.notify(); }
    const d = this.todoBy.get(address);
    if (d?.noteAt && Date.now() - d.noteAt > NOTE_MS) { d.note = null; d.noteAt = 0; cx.notify(); }
    if (JSON.stringify(card) === JSON.stringify(e.card)) return;
    const fresh = card && card.request !== e.card?.request;
    if (card?.request !== e.card?.request) {
      e.picks = new Map(); e.others = new Map(); e.step = 0;
      if (this.address === address) this.others.forEach((i) => i.set_value(""));
    }
    e.card = card; cx.notify();
    if (fresh && address === this.shown && this.focus && this.canFocus()) this.focus.focus();
  }

  // The pick or the own answer, as text, to contr.
  async answer(cx, address) {
    const e = this.entry(address), c = e.card;
    const a = answersFor(c?.question, e.picks, e.others)?.[0];
    if (!a) return;
    const text = a.other || c.question.questions[0].options[a.picks[0]]?.label;
    if (!text) return;
    await this.act(cx, address, (card) => answerOwner(card.request, text));
    if (e.note) e.noteAt = Date.now();
  }

  // The card, or its note alone when the card left with one (the answer is saved, the send failed).
  view(address, t, id = "owner-card", width = 800) {
    const c = this.card(address);
    if (c) return super.view(address, t, id, width);
    const note = this.byAgent.get(address)?.note;
    return note ? div().id(`${id}-note`).w_full().max_w(width).p(12).rounded(14).bg(t.raised).text_size(12).text_color(t.blocked).child(note) : null;
  }

  // The oldest open to-do of `address`'s coordinator, with how many more wait. Only local agents.
  todo(address) {
    const list = address && !machineOf(address) ? this.todos.get(local(address)) ?? [] : [];
    return list.length ? { ...list[0], more: list.length - 1 } : null;
  }

  todoKey(address) { return this.todo(address)?.id ?? ""; }

  // Done: marks the shown to-do done. It leaves at once when the tool saved it; a failure keeps it with the
  // tool's message.
  async done(cx, address) {
    const t = this.todo(address);
    if (!t) return;
    let d = this.todoBy.get(address);
    if (!d) { d = { sending: false, note: null, noteAt: 0 }; this.todoBy.set(address, d); }
    if (d.sending) return;
    d.sending = true; d.note = null; cx.notify();
    try {
      const r = await doneOwner(t.id);
      if (r.ok) this.todos.set(local(address), (this.todos.get(local(address)) ?? []).filter((x) => x.id !== t.id));
      d.note = r.note; d.noteAt = r.ok && r.note ? Date.now() : 0;
    } finally { d.sending = false; }
    await this.refresh(true);
    cx.notify();
  }

  // Copies `text` and shows a check on the button `key` for COPY_MS.
  copy(cx, key, text) {
    cx.write_to_clipboard?.(text);
    const press = { key, at: Date.now() };   // a later press's check outlives this press's timer
    this.copied = press;
    cx.notify();
    cx.timer?.after?.(COPY_MS, (cx2) => { if (this.copied === press) { this.copied = null; (cx2 ?? cx).notify?.(); } });
  }

  // The to-do card of `address`, or its note alone when the to-do left with one.
  todoView(address, t, id = "owner-todo", width = 800) {
    const c = this.todo(address), d = this.todoBy.get(address);
    const note = d?.note ? div().id(`${id}-note`).text_size(12).text_color(t.blocked).child(d.note) : null;
    if (!c) return note ? div().id(id).w_full().max_w(width).p(12).rounded(14).bg(t.raised).child(note) : null;
    let n = 0;
    const copyButton = (text) => {
      const key = `${c.id}:${n}`, on = this.copied?.key === key && Date.now() - this.copied.at < COPY_MS;
      return div().id(`${id}-copy-${n++}`).size(24).flex_none().flex().items_center().justify_center().rounded(6)
        .hover((s) => s.bg(t.hover)).on_click((_e, cx) => this.copy(cx, key, text))
        .child(svg(icon(on ? "check" : "copy")).size(14).text_color(on ? t.accent : t.muted));
    };
    const code = (text, block) => h_flex().w_full().gap(8).items_start().px(10).py(block ? 8 : 5).rounded(8).bg(t.background)
      .border(1).border_color(t.border)
      .child(div().flex_1().min_w(0).text_size(12.5).font_family("Menlo").text_color(t.foreground).child(text))
      .child(copyButton(text));
    const body = v_flex().gap(6).text_size(13.5).text_color(t.foreground);
    const shown = new Set();   // a span that the text has twice gets one copy row
    splitBlocks(c.text).forEach((b, i) => {
      if (b.k === "code") { body.child(code(codeCopyText(b), true)); return; }
      const md = b.k === "table" ? [b.head, b.head.map(() => "---"), ...b.rows].map((r) => `| ${r.join(" | ")} |`).join("\n") : b.text;
      body.child(TextView.markdown(`${id}-${c.id}-md${i}`, md).selectable(true));
      codeSpans(md).filter((s) => !shown.has(s)).forEach((s) => { shown.add(s); body.child(code(s, false)); });
    });
    const sending = Boolean(d?.sending);
    return v_flex().id(id).w_full().min_w(0).max_w(width).p(14).gap(10).rounded(14).bg(t.raised)
      .child(h_flex().gap(8).items_center()
        .child(div().size(8).flex_none().rounded(4).bg(t.blocked))
        .child(div().text_size(13.5).font_semibold().text_color(t.foreground)
          .child(`${c.id} · ${c.know ? "To know" : "To do"}${c.more > 0 ? ` · ${c.more} more after this` : ""}`)))
      .when(Boolean(c.from), (el) => el.child(div().text_size(11.5).text_color(t.muted).child(`from ${c.from}`)))
      .child(body)
      .child(h_flex().gap(10).items_center().child(div().flex_1())
        .child(div().id(`${id}-done`).h(30).px(14).flex().items_center().rounded(8).text_size(13)
          .bg(sending ? t.chrome : t.accent).text_color(sending ? t.dim : t.background).border(1).border_color(sending ? t.border : t.accent)
          .when(!sending, (el) => el.hover((s) => s.opacity(0.9)).on_click((_e, cx) => cx.spawn((cx) => this.done(cx, address))))
          .child(sending ? "Marking…" : "Done")))
      .when(Boolean(note), (el) => el.child(note));
  }
}
