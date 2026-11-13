// Under the chat: a quiet status line (the working verb, contr's orchestration) and the box to reply in, with its
// bottom row (+, the model button, the context ring, the branch, send). Enter sends through the sidecar's `send`;
// "+" or a drop attaches files.
import { div, svg, image } from "gpui-kit";
import { v_flex, h_flex, Textarea, TextareaState, Popover } from "gpui-base";
import { theme, panes, home, exec, window } from "herdr";
import { agentName, icon, loader, orchestration } from "./state.js";
import * as sidecar from "./sidecar.js";
import { setting } from "./config.js";
import { appRel } from "./phone.js";
import { modelName, hasModelMenu, menuPanel } from "./modelmenu.js";

export { modelName };

export const STATUS_MS = 5000;
export const MODEL_MS = 15000;   // the helper's `model` is asked again after this long

// The pane of a terminal address: its folder and branch.
export function paneOf(address) {
  return panes().find((p) => p.terminal === address) ?? null;
}

// A folder's last part ("app"), or "~" for home: the full path is too long for a tag.
export function shortPath(path) {
  if (!path) return "";
  if (path === home()) return "~";
  return path.replace(/\/+$/, "").split("/").pop() || "/";
}

// The tags to show, in order: the sidecar's (model, effort, context, cost), then folder, branch, agent.
// The sidecar's model, effort, context and cost (its other tags repeat what is added here),
// then the folder, the branch and the agent. A text shown twice is shown once.
const FROM_SIDECAR = ["model", "effort", "context", "cost"];
export function tagsFor(agent, status) {
  const out = (status?.tags ?? []).filter((t) => FROM_SIDECAR.includes(t.kind))
    .map((t) => ({ kind: t.kind, text: t.text, warn: Boolean(t.warn), ...(Number.isFinite(t.percent) ? { percent: t.percent } : {}) }));
  const pane = agent ? paneOf(agent.address) : null;
  if (pane?.cwd) out.push({ kind: "cwd", text: shortPath(pane.cwd), warn: false });
  if (pane?.branch) out.push({ kind: "branch", text: pane.branch, warn: false });
  if (agent) out.push({ kind: "agent", text: agentName(agent), warn: false });
  const seen = new Set();
  return out.filter((t) => t.text && !seen.has(t.text) && seen.add(t.text));
}

// The model button's text: { name, effort }. The helper's `model` line first (the session log, the settings), then
// the status line's tags. A model not known yet: the app's name alone ("Claude Code").
export function modelInfo(agent, status, line) {
  const tag = (k) => (status?.tags ?? []).find((t) => t.kind === k)?.text || "";
  const id = String(line?.model || tag("model") || "").trim();
  const name = modelName(id);
  const effort = String(line?.effort || tag("effort") || "").trim();
  if (!name) return { name: agent?.program?.name || agent?.app || "Agent", effort: "", known: false };
  return { id, name, effort, known: true };
}

// The model button's tooltip: "Opus 5.5 · medium effort · Claude Code", and the folder on a second line.
export function modelTip(agent, model) {
  const app = agent?.program?.name || agent?.app || "";
  const first = model.known ? [model.name, model.effort && `${model.effort} effort`, app].filter(Boolean).join(" · ") : app;
  const cwd = agent ? paneOf(agent.address)?.cwd : null;
  const folder = cwd ? (cwd === home() || cwd.startsWith(home() + "/") ? "~" + cwd.slice(home().length) : cwd) : "";
  return [first, folder].filter(Boolean).join("\n");
}

// The context tag's percent (0–100), or null: the helper's `percent`, else a "ctx 37%" text.
export function contextPercent(tag) {
  const p = Number.isFinite(tag?.percent) ? tag.percent : Number(/(\d+(?:\.\d+)?)\s*%/.exec(tag?.text ?? "")?.[1]);
  return Number.isFinite(p) ? Math.max(0, Math.min(100, Math.round(p))) : null;
}

// "ctx 149k / 250k" → "149k of 250k tokens"; else "37% of the context".
export function contextTip(tag) {
  const m = /([\d.]+[kKmM]?)\s*\/\s*([\d.]+[kKmM]?)/.exec(tag?.text ?? "");
  if (m) return `${m[1]} of ${m[2]} tokens`;
  const p = contextPercent(tag);
  return p === null ? "" : `${p}% of the context`;
}

// The ring: a track and an arc file for each 5% (icons/ring), so a draw only picks a path. 0% has no arc;
// a small use shows the first step.
export const RING_TRACK = icon("ring/track");
export function ringIcon(percent) {
  const p = Number(percent);
  if (!Number.isFinite(p) || p <= 0) return null;
  const step = Math.max(5, Math.min(100, Math.round(p / 5) * 5));
  return icon(`ring/arc-${String(step).padStart(3, "0")}`);
}

// The agent's mark from the app's program table, in the agent's color: a one-color glyph in its tile's color (the
// table's brand color; `color` is the glyph's color on that tile), else the foreground; or a picture.
export function agentMark(agent, size, t) {
  const p = agent?.program;
  if (!p?.icon) return null;
  return p.tint ? svg(p.icon).size(size).flex_none().text_color(p.tile || t.foreground) : image(p.icon).w(size).h(size).flex_none();
}

// The box's placeholder follows the agent's name.
// The agent's name for the placeholder: a tab someone named, else the agent's own title (its status glyph
// dropped; not a path, not the program's name), else the program's name.
export function replyName(agent) {
  const n = agentName(agent);
  if (!agent) return n;
  const prog = agent.program?.name ?? agent.app ?? "Agent";
  if (n && n !== prog) return n;
  const raw = String(agent.title ?? "").replace(/^[^\p{L}\p{N}~/]+/u, "").trim();
  return raw && raw !== prog && !/^[~/]|^[A-Za-z]:\\/.test(raw) && !raw.includes("@") ? raw : n;
}
export const placeholderFor = (agent) => `Reply to ${replyName(agent) || "the agent"}`;

// The "+" button opens the macOS file dialog through osascript (a mod has no file picker of its own), so it
// shows on macOS only. Elsewhere, drop files on the chat or paste an image.
export const canPick = () => (window()?.platform ?? "macos") === "macos";

// The macOS file dialog, through osascript. exec stops after 30 s.
export async function chooseFiles() {
  const script = ['set l to choose file with prompt "Attach files" with multiple selections allowed',
    'set o to ""', "repeat with f in l", "set o to o & POSIX path of f & linefeed", "end repeat", "return o"];
  try {
    const r = await exec("osascript", script.flatMap((s) => ["-e", s]));
    if (r.code !== 0) return { files: [], error: /-128/.test(r.stderr) ? null : "The file dialog did not open. Drop the files on the chat instead." };
    return { files: r.stdout.split("\n").map((s) => s.trim()).filter(Boolean), error: null };
  } catch {
    return { files: [], error: "The file dialog closes after 30 seconds. Drop the files on the chat instead." };
  }
}

const base = (p) => p.split("/").pop();

// An attached file that shows as a picture: a PNG, JPEG or SVG under the app home (pasted images are in
// herdr-contr/paste there). image() reads a path relative to the app home, and may refuse one outside it,
// so a file from another folder keeps its chip. The path for image(), or null. Kept by path: each draw asks.
export const THUMB_PX = 56;
const IMAGE_EXT = /\.(png|jpe?g|svg)$/i;
const thumbs = new Map();
export function thumbPath(path) {
  if (thumbs.has(path)) return thumbs.get(path);
  const rel = typeof path === "string" && IMAGE_EXT.test(path) ? appRel(path) : null;
  if (thumbs.size >= 200) thumbs.clear();
  thumbs.set(path, rel);
  return rel;
}

// A path as the agent reads it: in single quotes when it has a space or a shell character.
export function quoted(path) {
  return /^[A-Za-z0-9_\/.,:@%+=~-]+$/.test(path) ? path : "'" + path.replace(/'/g, "'\\''") + "'";
}

// The text to send: the typed text, then each attached file's path. The sidecar's `send --file`
// expects the text to name every file (docs/protocol.md).
export function withFiles(text, files) {
  if (!files.length) return text;
  const paths = files.map(quoted).join(" ");
  return text ? `${text} ${paths}` : paths;
}

// ⌘V in the box (Ctrl+V on Linux). The box's own paste runs before any key handler and types the clipboard's
// text, so a file copied in Finder came in as its name. The mod binds the chord under its own key context
// instead (the keymap is the app's; the context keeps it to this box) and asks the sidecar first.
export const PASTE_ACTION = "herdr_contr_paste";
export const PASTE_CONTEXT = "HerdrContrComposer";
let pasteBound = false;
function bindPaste(cx) {
  if (pasteBound || typeof cx?.bind_keys !== "function") return;
  try {
    cx.bind_keys([{ keystroke: "secondary-v", action: PASTE_ACTION, context: `${PASTE_CONTEXT} > Input` }]);
    pasteBound = true;
  } catch { /* the box's own paste stays, and paste_other still attaches an image */ }
}

// "/" commands (docs/protocol.md, commands; the helper lists them for Claude Code, Codex, pi and agy, and none for
// other agents): while the text is "/" and a name with no space, a
// list of the agent's commands shows above the box; after "/model " (a built-in with values) its values. All the
// matches are in the list; SLASH_ROWS show at a time, and the window follows the pick (↑ ↓ stop at the ends, no
// wrap) or the wheel. Tab or Enter completes, Esc closes.
// The box takes ↑ ↓ and Tab itself (as ⌘V), so they are bound under a key context the box's wrapper has only while
// the list shows: without the list they move the cursor as before. Only full names go out: Claude Code's own "/"
// menu takes the Enter of a part of a name and runs its first match.
export const SLASH_CONTEXT = "HerdrContrSlash";
export const SLASH = { up: "herdr_contr_slash_up", down: "herdr_contr_slash_down", complete: "herdr_contr_slash_complete", close: "herdr_contr_slash_close" };
export const SLASH_ROWS = 8;
const CMDS_MS = 30000;   // the list is asked again after this long
let slashBound = false;
function bindSlash(cx) {
  if (slashBound || typeof cx?.bind_keys !== "function") return;
  try {
    cx.bind_keys([["up", SLASH.up], ["down", SLASH.down], ["tab", SLASH.complete], ["escape", SLASH.close]]
      .map(([keystroke, action]) => ({ keystroke, action, context: `${SLASH_CONTEXT} > Input` })));
    slashBound = true;
  } catch { /* no keys for the list: a click on a row and Enter still complete */ }
}

// The name being typed after a leading "/", or null: the text has a space (or is not a "/" name).
export function slashQuery(text) {
  const m = /^\/([^\s]*)$/.exec(String(text ?? ""));
  return m ? m[1] : null;
}

// The commands whose name starts with `query` (any case), in the list's order. All of them: the list scrolls.
export function matchCommands(list, query) {
  const q = String(query ?? "").toLowerCase();
  return (list ?? []).filter((c) => String(c.name).toLowerCase().startsWith(q));
}

// A command's values as list items: { name: value, description: label }. A value is a string, or
// { value, label } (the helper's /model lists).
export function argItems(args) {
  return (Array.isArray(args) ? args : []).map((x) => (x && typeof x === "object"
    ? { name: String(x.value ?? ""), description: x.label ? String(x.label) : "" }
    : { name: String(x) })).filter((x) => x.name);
}

// The values that have `query` in their value or label (any case), in the list's order: a model can be found by a
// part of its name ("sonnet", "flash").
export function matchArgs(items, query) {
  const q = String(query ?? "").toLowerCase();
  return items.filter((x) => x.name.toLowerCase().includes(q) || (x.description ?? "").toLowerCase().includes(q));
}

// "/<name> <part>" (one space, no other): the command and the part of a value typed after it; else null.
export function argQuery(text) {
  const m = /^\/([^\s]+) ([^\s]*)$/.exec(String(text ?? ""));
  return m ? { name: m[1], query: m[2] } : null;
}

// A screen command sent bare ("/model", spaces trimmed) opens a screen in the terminal the chat cannot show: why it is
// not sent, or null. `list`: the agent's commands (a listed screen takes a value); `screens`: the names the helper
// left out (they take none). The helper's `send` checks the same and builds the same text (commands.rs).
export function screenNote(list, screens, text) {
  const m = /^\/(\S+)$/.exec(String(text ?? "").trim());
  if (!m) return null;
  const name = m[1];
  if ((screens ?? []).includes(name)) return `/${name} opens a screen in the terminal. Use it there.`;
  const c = (list ?? []).find((x) => x.name === name && x.screen);
  if (!c) return null;
  const example = argItems(c.args)[0]?.name || c.hint || "";
  return `/${name} opens a menu in the terminal. Add a value, like /${name} ${example}.`;
}

// The first shown row so that `pick` is shown, from the window's last top. Keeps the window still while it can.
export function windowTop(top, pick, n, rows = SLASH_ROWS) {
  let t = Math.max(0, Math.min(top, Math.max(0, n - rows)));
  if (pick < t) t = pick;
  else if (pick >= t + rows) t = pick - rows + 1;
  return t;
}
const ROW_PX = 30;   // a list row; the wheel moves the window a row per ROW_PX


// What `paste-image` found to attach: copied files, else the saved image. Empty: paste the text.
export function pastedFiles(lines) {
  const files = lines.find((l) => l.type === "files")?.paths;
  if (Array.isArray(files) && files.some(Boolean)) return files.filter((p) => typeof p === "string" && p);
  const image = lines.find((l) => l.type === "image")?.path;
  return image ? [image] : [];
}

// Enter in the box puts a new line at the cursor (over a selection) before it reports the submit, so in the
// middle of the text that line went out. `before` is the text just before the key: when `now` is `before` with
// one span turned into "\n", the text to send is `before`. Anything else is sent as it is.
export function withoutEnter(now, before) {
  if (typeof before !== "string" || now === before) return now;
  let i = 0;
  while (i < now.length && i < before.length && now[i] === before[i]) i++;
  let j = 0;
  while (j < now.length - i && j < before.length - i && now[now.length - 1 - j] === before[before.length - 1 - j]) j++;
  return now.slice(i, now.length - j) === "\n" ? before : now;
}

export class Composer {
  // One input box, made here: herdr-app does not let a view make input state while it draws.
  // Each agent's unsent text and files are kept in `drafts` and swapped in when the agent changes.
  constructor(_cx) {
    this.placeholder = placeholderFor(null);
    this.focused = false;
    this.input = this.makeInput(this.placeholder);
    bindPaste(_cx);
    bindSlash(_cx);
    this.empty = true;
    // The last two texts the box reported, so a submit knows the text before Enter's new line (the box
    // reports that line's change before or after the submit).
    this.typed = ["", ""];
    this.models = new Map();   // address → { at, line } of the helper's `model` answer
    this.modelBusy = false;
    this.cmds = new Map();      // address → { at, list } of a Claude Code agent's "/" commands
    this.cmdsBusy = false;
    this.menu = null;           // the model menu: { address, page } while open
    this.agent = null;          // the agent the box is for (its kind decides the list)
    this.pick = 0;              // the chosen match of the list
    this.top = 0;               // the first shown match
    this.wheel = 0;             // wheel pixels not yet a whole row
    this.slashClosed = null;    // the text Esc closed the list at
    this.drafts = new Map();   // address → { text, files } of agents not shown
    this.address = null;
    // address → { agent, path } the chat showed last (set by the chat); send is bound to it.
    this.seen = null;
    // (address, cx): the send found a new conversation of the same agent; the chat reads its feed again (set by the chat).
    this.onNewConversation = null;
    this.files = [];
    this.sending = false;
    this.note = null;          // { text, bad } under the box, after a send
    this.status = new Map();   // address → status line
    this.statusAt = 0;
    this.statusBusy = false;
  }

  // One text state with its handlers. A state's placeholder is fixed when it is made (TextareaState has no
  // setter), so syncPlaceholder makes a new one when the agent's name changes.
  makeInput(placeholder) {
    const input = TextareaState.new({ placeholder, rows: 1 });
    input.set_auto_grow?.(1, 8);
    // A paste that found no text, when the binding above is missing: the clipboard's image, if any.
    input.on("paste_other", (_e, cx) => cx.spawn((cx) => this.paste(cx, undefined)));
    // Enter sends. On Shift+Enter the box puts the new line in itself (at the cursor, over a selection);
    // the submit with shift only reports it, so do nothing here. Adding "\n" here gives two new lines.
    input.on("submit", (e, cx) => {
      if (e?.shift) return;
      cx.spawn((cx) => this.submit(cx));
    });
    input.on("focus", () => { this.focused = true; });
    input.on("blur", () => { this.focused = false; });
    input.on("change", (_e, cx) => {
      if (input !== this.input) return;
      const v = this.input.value();
      if (v !== this.typed[1]) this.typed = [this.typed[1], v];
      const e = !v.trim(); if (e !== this.empty) { this.empty = e; cx.notify(); }
      if (this.slashClosed !== null && v !== this.slashClosed) this.slashClosed = null;
      this.pick = 0; this.top = 0; this.wheel = 0;
      if (slashQuery(v) !== null || argQuery(v) !== null) cx.spawn((cx) => this.loadCommands(cx));
      cx.notify();
    });
    return input;
  }

  // "Reply to <agent>": called from the chat's tick, not while drawing (a text state can only be made outside a
  // draw). A new name gets a new state with the same text; the keyboard stays in the box when it was there.
  syncPlaceholder(agent, cx) {
    const want = placeholderFor(agent);
    if (want === this.placeholder) return false;
    const old = this.input, text = old.value();
    this.input = this.makeInput(want);
    this.placeholder = want;
    if (text) this.input.set_value(text);
    if (this.focused) this.input.focus?.();
    old.release?.();
    cx?.notify?.();
    return true;
  }

  // The model menu (#112) for the shown agent: { address, page } while it is open, else null. `page`: "main",
  // "effort" or "models". Another agent shown closes it.
  menuOpen(address) { return Boolean(address) && this.menu?.address === address; }

  // Whether the model button opens the menu: the agent's `/model` takes values (the helper's commands), or a
  // Claude Code session whose commands are not read yet (it always has them).
  hasMenu(agent) {
    const had = agent && this.cmds.get(agent.address);
    return had ? hasModelMenu(had.list) : agent?.app === "claude-code";
  }

  // The Popover's open state changed (a press on the button, a press outside, Escape).
  openMenu(open, cx) {
    this.menu = open && this.address ? { address: this.address, page: "main" } : null;
    if (this.menu) cx.spawn((cx) => this.loadCommands(cx));
    cx.notify();
  }

  // A row with a sub menu shows that page; null closes the menu.
  menuPage(page, cx) {
    this.menu = page && this.menu ? { ...this.menu, page } : null;
    cx.notify();
  }

  // A press on a model or an effort: the menu closes and `text` ("/model <id>", "/effort <level>") goes to the agent
  // through the helper's `send`, as a typed command does. The box keeps its text. After it, the model and the status
  // are read again at the next poll, so the button shows the new value.
  choose(text, cx) {
    this.menu = null;
    cx.notify();
    cx.spawn((cx) => this.sendCommand(text, cx));
  }

  async sendCommand(text, cx) {
    const a = this.address;
    if (!a || this.sending) return;
    this.sending = true; this.note = null;
    cx.notify();
    try {
      const r = await sidecar.send(a, text, [], this.seen?.(a) ?? null);
      const done = r.lines.find((l) => l.type === "done");
      const moved = sidecar.agentChanged(done), fresh = sidecar.newConversation(done);
      if (r.error || !done || (done.result !== "sent" && done.result !== "unconfirmed")) {
        this.note = { text: fresh ? sidecar.NEW_CONVERSATION : moved ? sidecar.AGENT_CHANGED : (r.error?.message ?? done?.message) || `${text} was not sent.`, bad: true };
        if (fresh) this.onNewConversation?.(a, cx);
      } else {
        const m = this.models.get(a);
        if (m) m.at = 0;
        if (this.address === a) this.statusAt = 0;
      }
    } finally {
      this.sending = false;
      cx.notify();
    }
  }

  // The helper's `model` answer for the shown agent, at most every MODEL_MS. An older helper has none: the
  // status line's tags name the model.
  async pollModel(cx) {
    const a = this.address;
    if (!a || this.modelBusy) return;
    const had = this.models.get(a);
    if (had && Date.now() - had.at < MODEL_MS) return;
    if (!this.cmds.has(a)) await this.loadCommands(cx);   // once: does the model button open a menu
    this.modelBusy = true;
    try {
      this.models.set(a, { at: Date.now(), line: had?.line ?? null });
      if (!(await sidecar.hasCommand("model"))) return;
      const r = await sidecar.model(a);
      const line = r.error ? had?.line ?? null : r.lines.find((l) => l.type === "model") ?? null;
      this.models.set(a, { at: Date.now(), line });
      if (JSON.stringify(line) !== JSON.stringify(had?.line ?? null)) cx.notify();
    } finally { this.modelBusy = false; }
  }

  // Sets the box's text from here (no change report counts on it).
  setText(v) { this.input.set_value(v); this.typed = [v, v]; }

  // Shows `agent`'s draft in the box. Safe while drawing: it only sets values.
  box(agent) {
    this.agent = agent ?? null;
    const a = agent?.address ?? null;
    if (a !== this.address) {
      if (this.address) this.drafts.set(this.address, { text: this.input.value(), files: this.files });
      const d = (a && this.drafts.get(a)) || { text: "", files: [] };
      this.drafts.delete(a);
      this.setText(d.text); this.files = d.files; this.empty = !d.text.trim();
      this.address = a; this.note = null; this.statusAt = 0;
    }
    return a ? this : null;
  }

  // Asks the helper for the shown agent's commands (at most every CMDS_MS), if it has `commands`. An agent it lists
  // none for gets an empty list: no list shows.
  async loadCommands(cx) {
    const a = this.address;
    if (!a || !this.agent || this.cmdsBusy) return;
    const had = this.cmds.get(a);
    if (had && Date.now() - had.at < CMDS_MS) return;
    this.cmdsBusy = true;
    try {
      if (!(await sidecar.hasCommand("commands"))) return;
      const r = await sidecar.commands(a);
      const line = r.error ? null : r.lines.find((l) => l.type === "commands");
      const list = r.error ? had?.list ?? [] : (line?.commands ?? []);
      const screens = r.error ? had?.screens ?? [] : (line?.screens ?? []);
      this.cmds.set(a, { at: Date.now(), list, screens });
      cx.notify();
    } finally { this.cmdsBusy = false; }
  }

  // The list to show now: { kind, query, items, pick, top, cmd } or null. `kind` "cmd": commands for "/<part>";
  // "arg": the values of `cmd` for "/<cmd> <part>" (items are { name: value, description: label }).
  slash() {
    const v = this.input.value();
    return v === this.slashClosed ? null : this.slashFor(v);
  }

  slashFor(v) {
    if (!this.agent || !this.address) return null;
    const list = this.cmds.get(this.address)?.list ?? [];
    let kind = "cmd", query = slashQuery(v), items, cmd = null;
    if (query !== null) items = matchCommands(list, query);
    else {
      const a = argQuery(v);
      cmd = a && list.find((c) => c.name === a.name && Array.isArray(c.args) && c.args.length);
      if (!cmd) return null;
      kind = "arg"; query = a.query;
      items = matchArgs(argItems(cmd.args), query);
    }
    if (!items.length) return null;
    const pick = Math.min(this.pick, items.length - 1);
    return { kind, query, items, pick, top: windowTop(this.top, pick, items.length), cmd };
  }

  // A command: "/<name> " (whole, and the space closes the list or opens its values). A value: "/<cmd> <value>".
  complete(s, item, cx) {
    this.setText(s.kind === "arg" ? `/${s.cmd.name} ${item.name}` : `/${item.name} `);
    this.empty = false; this.pick = 0; this.top = 0; this.wheel = 0; this.slashClosed = null;
    cx?.notify?.();
  }

  slashKey(what, cx) {
    const s = this.slash();
    if (!s) return;
    if (what === "up") this.pick = Math.max(0, s.pick - 1);
    else if (what === "down") this.pick = Math.min(s.items.length - 1, s.pick + 1);
    else if (what === "complete") return this.complete(s, s.items[s.pick], cx);
    else if (what === "close") this.slashClosed = this.input.value();
    this.top = windowTop(s.top, this.pick, s.items.length);
    cx.notify();
  }

  // Wheel or trackpad over the list: the window moves a row per ROW_PX (small steps add up), and the pick stays on
  // a shown row.
  slashWheel(e, cx) {
    const s = this.slash();
    if (!s || s.items.length <= SLASH_ROWS) return;
    this.wheel -= Number(e?.delta?.y) || 0;   // y > 0: up
    const rows = Math.trunc(this.wheel / ROW_PX);
    if (!rows) return;
    this.wheel -= rows * ROW_PX;
    this.top = Math.max(0, Math.min(s.items.length - SLASH_ROWS, s.top + rows));
    this.pick = Math.max(this.top, Math.min(this.top + SLASH_ROWS - 1, s.pick));
    cx.notify();
  }

  async pollStatus(cx) {
    const a = this.address;
    if (!a || this.statusBusy || Date.now() - this.statusAt < STATUS_MS) return;
    this.statusBusy = true; this.statusAt = Date.now();
    try {
      const r = await sidecar.status(a);
      const line = r.error ? null : r.lines.find((l) => l.type === "status") ?? null;
      const before = JSON.stringify(this.status.get(a) ?? null);
      this.status.set(a, line);
      if (JSON.stringify(line) !== before) cx.notify();
    } finally { this.statusBusy = false; }
    await this.pollModel(cx);
  }

  addFiles(paths, cx) {
    const add = [...new Set(paths || [])].filter((p) => p && !this.files.includes(p));
    if (!add.length) return;
    this.files = this.files.concat(add); this.note = null; cx.notify();
  }

  // ⌘V: copied files (Finder) are attached like picked files; plain text is typed at the cursor, also with a
  // picture beside it (spreadsheet cells); an image alone (a screenshot, a browser's Copy Image) is attached.
  // The sidecar decides (paste-image). `text` is the clipboard's text when the key was pressed. A broken
  // sidecar still lets text paste.
  async paste(cx, text) {
    const a = this.address;
    const r = await sidecar.pasteImage();
    if (this.address !== a) return;
    const files = r.error ? [] : pastedFiles(r.lines);
    if (files.length) return this.addFiles(files, cx);
    if (text) { this.input.replace(text); const v = this.input.value(); this.typed = [v, v]; this.empty = !v.trim(); }
    else if (r.error) this.note = { text: r.error.message, bad: true };
    cx.notify();
  }

  async pick(cx) {
    const r = await chooseFiles();
    if (r.error) this.note = { text: r.error, bad: true };
    this.addFiles(r.files, cx);
    cx.notify();
  }

  async submit(cx) {
    const a = this.address, b = this;
    if (this.sending || !a) return;
    const now = b.input.value();
    const text = withoutEnter(now, now === this.typed[1] ? this.typed[0] : this.typed[1]).replace(/\s+$/, "");
    if (!text.trim() && !b.files.length) { b.setText(""); return; }
    // A screen command alone ("/model", "/status"): not sent; the text stays for a value to be added.
    const had = this.cmds.get(a);
    const blocked = b.files.length ? null : screenNote(had?.list, had?.screens, text);
    if (blocked) { b.setText(text); this.note = { text: blocked, bad: true }; cx.notify(); return; }
    // The list shows and the name is not whole yet: Enter completes it (a part of a name would run another command).
    const s = text !== this.slashClosed ? this.slashFor(text) : null;
    if (s && !s.items.some((c) => c.name === s.query)) return this.complete(s, s.items[s.pick], cx);
    const files = b.files, sent = withFiles(text, files);
    this.sending = true; this.note = null;
    b.setText(""); b.files = [];
    const echo = this.onSent?.(a, sent, cx) ?? null;
    cx.notify();
    let ok = false;
    try {
      const r = await sidecar.send(a, sent, files, this.seen?.(a) ?? null);
      const done = r.lines.find((l) => l.type === "done");
      const moved = sidecar.agentChanged(done), fresh = sidecar.newConversation(done);
      // Only "sent" and "unconfirmed" (it may still arrive) mean the text went out; any other result did not send.
      if (r.error || !done || (done.result !== "sent" && done.result !== "unconfirmed")) {
        // Put the text back, unless the box moved to another agent or something new was typed.
        if (this.address === a && !b.input.value()) { b.setText(text); b.files = files; }
        else if (this.address !== a) this.drafts.set(a, { text, files });
        this.note = { text: fresh ? sidecar.NEW_CONVERSATION : moved ? sidecar.AGENT_CHANGED : (r.error?.message ?? done?.message) || "The message was not sent.", bad: true };
        if (fresh) this.onNewConversation?.(a, cx);
      } else {
        ok = true;
        if (done.result === "unconfirmed") this.note = { text: "Sent. The agent did not confirm it yet.", bad: false };
      }
    } finally {
      this.sending = false;
      if (!ok) this.onSendFailed?.(a, echo, cx);
      cx.notify();
    }
    if (ok) await this.onSendDone?.(a, cx);
  }

  // `width`: the chat's column, so the box lines up with the messages.
  render(agent, t, width = 800, work = null, background = null) {
    const tags = tagsFor(agent, agent ? this.status.get(agent.address) : null);
    const branch = tags.find((x) => x.kind === "branch");
    const b = this.box(agent);
    if (!b) return div();
    const s = this.slash();
    const shown = s ? s.items.slice(s.top, s.top + SLASH_ROWS) : [];
    const row = (c, j) => h_flex().id(`slash-${s.top + j}`).w_full().h(ROW_PX).px(10).gap(10).items_center().rounded(8)
      .when(s.top + j === s.pick, (el) => el.bg(t.hover))
      .hover((st) => st.bg(t.hover))
      .on_click((_e, cx) => this.complete(s, c, cx))
      .child(div().flex_none().text_size(13).font_semibold().text_color(t.foreground).child(s.kind === "arg" ? c.name : `/${c.name}`))
      .when(Boolean(c.hint), (el) => el.child(div().flex_none().text_size(12).text_color(t.dim).child(c.hint)))
      .child(div().flex_1().min_w(0).truncate().text_size(12).text_color(t.muted).child(c.description || ""))
      // Sent bare, it opens a screen in the terminal (a picker, settings): it goes out only with a value.
      .when(Boolean(c.screen), (el) => el.child(div().flex_none().text_size(11).text_color(t.dim).child("needs a value")));
    const more = s && s.items.length > SLASH_ROWS
      ? div().id("slash-more").w_full().px(10).pt(2).text_size(11).text_color(t.dim)
        .child(`${s.top + 1}–${s.top + shown.length} of ${s.items.length}`)
      : null;
    const empty = !b.input.value().trim() && !b.files.length;
    const unattach = (p, i) => div().id(`file-x-${i}`).size(16).flex().items_center().justify_center().rounded(4).text_color(t.muted).hover((s) => s.bg(t.hover))
      .on_click((_e, cx) => { this.files = this.files.filter((f) => f !== p); cx.notify(); }).child("×");
    // An image under the app home: a THUMB_PX high picture (its width follows the image), the × on its corner.
    // The frame has a fixed height, so the box does not grow when the image loads.
    const thumb = (p, i, rel) => div().id(`file-${i}`).relative().flex_none().h(THUMB_PX + 2).rounded(8).border(1).border_color(t.border).bg(t.chrome).overflow_hidden()
      .child(image(rel).h(THUMB_PX).max_w(THUMB_PX * 4).rounded(7))
      .child(div().absolute().top(2).right(2).rounded(4).bg(t.raised).child(unattach(p, i)));
    const fileChip = (p, i) => {
      const rel = thumbPath(p);
      if (rel) return thumb(p, i, rel);
      return h_flex().id(`file-${i}`).h(24).pl(8).pr(4).gap(4).items_center().rounded(8).bg(t.chrome).text_size(12).text_color(t.foreground)
        .child(div().max_w(180).truncate().child(base(p)))
        .child(unattach(p, i));
    };
    // Right of the status line: contr's orchestration as plain text (Orchestrating with the worker count), or for a
    // Claude Code session a quiet Orchestrate that starts /contr:orchestrate in the box.
    const orch = orchestration(agent);
    // The setting that hid the status tags now hides the model button and the context (#99).
    const showTags = setting("status_row") !== "hide";
    const ctxTag = tags.find((x) => x.kind === "context");
    const ctxPct = ctxTag ? contextPercent(ctxTag) : null;
    const model = modelInfo(agent, this.status.get(agent.address), this.models.get(agent.address)?.line);
    const mono = (el) => el.font_family("Menlo").text_size(11).text_color(t.muted);
    // A 28 px button of the bottom row: radius 8, side padding 8, a fill on hover.
    const btn = (id) => h_flex().id(id).h(28).px(8).gap(6).flex_none().items_center().rounded(8).hover((st) => st.bg(t.hover));
    const mark = agentMark(agent, 13, t);
    const modelBtn = btn("model").min_w(0).flex_shrink(1).when(!this.menuOpen(agent.address), (el) => el.tooltip(modelTip(agent, model)))
      .when(this.menuOpen(agent.address), (el) => el.bg(t.hover))
      .when(Boolean(mark), (el) => el.child(mark))
      .child(div().min_w(0).truncate().text_size(12).text_color(t.foreground).child(model.name))
      .when(Boolean(model.effort), (el) => el.child(div().flex_none().text_size(12).text_color(t.muted).child(model.effort)))
      .child(svg(icon("chevron-down")).size(11).flex_none().text_color(t.muted));
    // The model menu opens above the button (#112); the Popover owns the press, a press outside and Escape.
    const menuOn = { page: (p, cx) => this.menuPage(p, cx), choose: (text, cx) => this.choose(text, cx) };
    const modelEl = !model.known || !this.hasMenu(agent) ? modelBtn
      : Popover.new("model-popover").anchor("bottom_left").open(this.menuOpen(agent.address))
        .on_open_change((open, cx) => this.openMenu(open, cx))
        .trigger(modelBtn)
        .content(div().pb(6)
          .child(menuPanel(this.menu ?? { page: "main" }, model, this.cmds.get(agent.address)?.list ?? [], t, menuOn)));
    const ctxColor = ctxTag?.warn ? t.blocked : t.muted;
    const ring = ctxPct === null ? null : div().id("ctx-ring").relative().size(14).flex_none()
      .child(svg(RING_TRACK).size(14).text_color(t.border))
      .when(Boolean(ringIcon(ctxPct)), (el) => el.child(svg(ringIcon(ctxPct)).absolute().top(0).left(0).size(14).text_color(ctxColor)));
    const context = ring && h_flex().id("context").h(28).px(8).gap(6).flex_none().items_center().rounded(8).tooltip(contextTip(ctxTag))
      .child(ring).child(mono(div()).text_color(ctxColor).child(`${ctxPct}%`));
    const right = orch
      ? mono(h_flex().id("orchestrating").flex_shrink(1).min_w(0).gap(4).items_center())
        .child(div().flex_none().child("Orchestrating"))
        .when(orch.workers > 0, (el) => el.child(div().min_w(0).truncate().child(`· ${orch.workers} ${orch.workers === 1 ? "worker" : "workers"}`)))
      : agent?.app === "claude-code"
        ? mono(div().id("orchestrate").flex_none()).hover((st) => st.text_color(t.foreground))
          .on_click((_e, cx) => { b.setText("/contr:orchestrate "); b.empty = false; cx.notify(); })
          .child("Orchestrate")
        : null;
    return v_flex().id("composer").w_full().px(16).pb(14).pt(6).gap(8).items_center()
      .key_context(PASTE_CONTEXT)
      .on_action(PASTE_ACTION, (_e, cx) => { const text = cx.read_from_clipboard?.(); cx.spawn((cx) => this.paste(cx, text)); })
      // Above the box, one quiet line (#99): the working verb and its detail on the left (#47), Claude Code's
      // background note after it ("1 shell still running", #61; it truncates first), contr's orchestration on the right.
      .when(Boolean(work) || Boolean(background) || Boolean(right), (el) => el.child(h_flex().id("status-line").w_full().max_w(width).min_w(0).overflow_hidden().px(4).gap(8).items_center()
        .child(h_flex().flex_1().min_w(0).gap(8).items_center().overflow_hidden()
          .when(Boolean(work), (r) => r.child(h_flex().id("working").flex_none().gap(8).items_center()
            .child(loader(14).text_color(t.foreground))
            .child(div().flex_none().text_size(13).text_color(t.foreground).child(work.text))
            .when(Boolean(work.detail), (w) => w.child(mono(div()).flex_none().child(work.detail)))))
          .when(Boolean(background), (r) => r.child(h_flex().id("background").flex_1().min_w(0).gap(6).items_center().text_color(t.muted)
            .when(!work, (x) => x.child(loader(14)))
            .child(mono(div()).min_w(0).truncate().child(background.text)))))
        .when(Boolean(right), (r) => r.child(right))))
      .when(Boolean(s), (el) => el.child(v_flex().id("slash").w_full().max_w(width).p(4).gap(0).rounded(12).bg(t.raised)
        .border(1).border_color(t.border).shadow_sm().on_scroll_wheel((e, cx) => this.slashWheel(e, cx))
        .children(shown.map(row)).when(Boolean(more), (l) => l.child(more))))
      .child(v_flex().id("box").w_full().max_w(width).rounded(12).bg(t.raised).border(1).border_color(t.border).shadow_sm()
        .pt(12).pr(10).pb(8).pl(14).gap(10)
        .when(this.files.length > 0, (el) => el.child(h_flex().gap(6).flex_wrap().items_center().children(this.files.map(fileChip))))
        .child(div().id("slash-keys").w_full().text_size(14).line_height(20 / 14)
          .when(Boolean(s), (el) => el.key_context(SLASH_CONTEXT))
          .on_action(SLASH.up, (_e, cx) => this.slashKey("up", cx))
          .on_action(SLASH.down, (_e, cx) => this.slashKey("down", cx))
          .on_action(SLASH.complete, (_e, cx) => this.slashKey("complete", cx))
          .on_action(SLASH.close, (_e, cx) => this.slashKey("close", cx))
          .child(Textarea.new(b.input).w_full()))
        .child(h_flex().id("box-row").w_full().min_w(0).gap(4).items_center()
          .when(canPick(), (el) => el.child(div().id("attach").size(28).flex_none().flex().items_center().justify_center().rounded(8).text_color(t.muted)
            .hover((s) => s.bg(t.hover).text_color(t.foreground))
            .on_click((_e, cx) => cx.spawn((cx) => this.pick(cx))).child(svg(icon("plus")).size(16))))
          .when(showTags, (el) => el.child(modelEl))
          .child(div().flex_1())
          .when(showTags && Boolean(context), (el) => el.child(context))
          .when(Boolean(branch), (el) => el.child(mono(h_flex().id("branch").h(28).px(8).gap(6).min_w(0).flex_shrink(1).items_center())
            .child(svg(icon("branch")).size(13).flex_none()).child(div().min_w(0).max_w(200).truncate().child(branch.text))))
          .child(div().id("send").size(28).ml(4).flex_none().flex().items_center().justify_center().rounded(8)
            .bg(empty || this.sending ? t.chrome : t.accent).text_color(empty || this.sending ? t.dim : t.background)
            .on_click((_e, cx) => cx.spawn((cx) => this.submit(cx)))
            .child(this.sending ? loader(13) : svg(icon("send")).size(14)))))
      .when(Boolean(this.note), (el) => el.child(div().id("send-note").w_full().max_w(width).text_size(12)
        .text_color(this.note.bad ? t.blocked : t.muted).child(this.note.text)));
  }
}
