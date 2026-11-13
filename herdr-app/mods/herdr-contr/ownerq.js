// The owner's questions (#131). contr keeps the questions that a coordinator asked with ask_owner (and
// the ones its workers raised through it) in one owner file (contr's internal/owner). Each open
// question shows in the chat of the coordinator that asked it (the item's `coordinator`: its hc
// terminal) as the question card of cards.js, one at a time, oldest first. A press answers it with
// `contr owner answer <id> <answer>`: contr writes the file under its lock and sends the coordinator
// "Owner answered <id>: <answer>". The mod only reads the file. An answered or decided item leaves.
// The path: CONTR_OWNER_FILE when set, else the user config folder's contr/owner.json (macOS:
// ~/Library/Application Support; other systems: $XDG_CONFIG_HOME or ~/.config).
import { div } from "gpui-kit";
import { home, window, exec, readTextFile } from "herdr";
import { Cards, answersFor } from "./cards.js";
import { machineOf } from "./state.js";

export const OWNER_MS = 3000;     // the file is read at most this often
export const NOTE_MS = 15000;     // a note without a card stays this long

// The file's path, asked of the user's shell once (the mod has no environment of its own).
let pathOnce = null;
export const resetOwnerPath = () => { pathOnce = null; };
export function ownerPath() {
  return (pathOnce ??= (async () => {
    let over = "", xdg = "";
    try {
      const r = await exec("sh", ["-c", 'printf "%s\\n%s" "$CONTR_OWNER_FILE" "$XDG_CONFIG_HOME"']);
      if (r?.code === 0) [over = "", xdg = ""] = String(r.stdout).split("\n");
    } catch { /* the defaults below */ }
    if (over) return over;
    if ((window()?.platform ?? "macos") === "macos") return `${home()}/Library/Application Support/contr/owner.json`;
    return `${xdg || `${home()}/.config`}/contr/owner.json`;
  })());
}

const str = (v) => (typeof v === "string" ? v : "");
// "hc:t2" → "t2": the key that the file's `coordinator` and an agent's address share.
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

// The open questions of the file's text, by coordinator, oldest first. A file that is not JSON, an
// answered question and one that no coordinator asked give nothing.
export function openByCoordinator(text) {
  const out = new Map();
  let st; try { st = JSON.parse(text ?? "null"); } catch { return out; }
  for (const q of Array.isArray(st?.questions) ? st.questions : []) {
    const who = local(q?.coordinator);
    if (!who || !str(q.id) || !str(q.question) || str(q.answer)) continue;
    if (!out.has(who)) out.set(who, []);
    out.get(who).push(cardOf(q));
  }
  return out;
}

// Runs `contr owner answer`. The answer has the sidecar's shape, for Cards.act.
export async function answerOwner(id, text) {
  let r;
  try { r = await exec("contr", ["owner", "answer", id, text], { timeout: 30000 }); }
  catch (e) { return { lines: [], error: { message: `contr could not start (${e?.message ?? e}).` } }; }
  if (r?.code === 0) return { lines: [{ type: "done", result: "sent" }], error: null };
  const why = String(r?.stderr ?? "").trim();
  if (/coordinator not reached/.test(why)) {
    return { lines: [{ type: "done", result: "saved", message: `The answer is saved, but the coordinator did not get it: ${why}` }], error: null };
  }
  return { lines: [], error: { message: why || `contr owner answer failed (exit ${r?.code ?? "?"}).` } };
}

export class OwnerCards extends Cards {
  constructor(cx) {
    super(cx);
    this.text = undefined;   // the file's text at the last read
    this.open = new Map();   // coordinator → cards
    this.readAt = 0; this.reading = false;
  }

  // Reads the file at most every OWNER_MS unless `force`. True when its text changed.
  async refresh(force = false) {
    if (this.reading || (!force && Date.now() - this.readAt < OWNER_MS)) return false;
    this.reading = true; this.readAt = Date.now();
    try {
      let text;
      try { text = await readTextFile(await ownerPath()); } catch { return false; }   // keep what was read
      if (text === this.text) return false;
      this.text = text; this.open = openByCoordinator(text);
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
}
