import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { fake, log, reset } from "./stub/herdr.mjs";
import { textOf, byId } from "./stub/ui.mjs";
import { fakeCx } from "./fake-cx.mjs";
import { agent, space } from "./fixtures.mjs";
import { Composer, tagsFor, shortPath, quoted, withFiles, slashQuery, matchCommands, screenNote, argItems, matchArgs, thumbPath, THUMB_PX, modelName, modelInfo, modelTip, ringIcon, contextTip, contextPercent, MODEL_MS } from "../mods/herdr-contr/composer.js";
import { ChatView } from "../mods/herdr-contr/chat.js";
import { otherModels, effortValues, hasModelMenu, titleCase } from "../mods/herdr-contr/modelmenu.js";
import { forgetCommands } from "../mods/herdr-contr/sidecar.js";
import { KEY_CONFIG } from "../mods/herdr-contr/config.js";

const jl = (...lines) => ({ code: 0, stdout: lines.map((l) => JSON.stringify(l)).join("\n") + "\n", stderr: "" });
const header = (command) => ({ type: "header", protocol: 1, sidecar: "0.1.0", command });
const end = { type: "end", ok: true };
const statusLine = { type: "status", agent: "claude", source: "statusline", tags: [
  { kind: "model", text: "Opus 5.5", warn: false }, { kind: "effort", text: "high", warn: false },
  { kind: "context", text: "ctx 84%", percent: 84, warn: true }, { kind: "cost", text: "$0.42 est.", warn: false }] };

let sent;
beforeEach(() => {
  reset(); sent = [];
  fake.spaces = [space()];
  fake.agents = [agent("t1", "idle", { tab_name: "api" }), agent("t2", "idle")];
  fake.panes = [{ terminal: "t1", cwd: "/home/test/w/app", branch: "feat/login", status: "idle" }];
  fake.exec = (cmd, args) => {
    if (args[0] === "status") return jl(header("status"), statusLine, end);
    if (args[0] === "send") { sent.push(args); return jl(header("send"), { type: "done", result: fake.sendResult ?? "sent", message: fake.sendMessage ?? null }, end); }
    return jl(header(args[0]), end);
  };
});

test("shortPath keeps the folder's last part; home is ~", () => {
  assert.equal(shortPath("/home/test/w/app"), "app");
  assert.equal(shortPath("/a/b/c/d/e/"), "e");
  assert.equal(shortPath("/home/test"), "~");
});

test("files are named in the text, quoted when needed", () => {
  assert.equal(quoted("/tmp/a.png"), "/tmp/a.png");
  assert.equal(quoted("/tmp/my shot.png"), "'/tmp/my shot.png'");
  assert.equal(quoted("/tmp/it's.png"), "'/tmp/it'\\''s.png'");
  assert.equal(withFiles("", ["/tmp/a b"]), "'/tmp/a b'");
  assert.equal(withFiles("look", []), "look");
});

test("tags: the sidecar's, then folder, branch and agent name", () => {
  const tags = tagsFor(fake.agents[0], statusLine);
  assert.deepEqual(tags.map((t) => t.text), ["Opus 5.5", "high", "ctx 84%", "$0.42 est.", "app", "feat/login", "api"]);
  const dup = { tags: [...statusLine.tags, { kind: "own", text: "Opus 5.5 · high" }, { kind: "own", text: "…/app" }, { kind: "model", text: "api" }] };
  assert.deepEqual(tagsFor(fake.agents[0], dup).map((t) => t.text), ["Opus 5.5", "high", "ctx 84%", "$0.42 est.", "api", "app", "feat/login"],
    "the sidecar's own tags are left out, and a text shows once");
  assert.equal(tags[2].warn, true);
  assert.deepEqual(tagsFor(fake.agents[1], null).map((t) => t.kind), ["agent"]);
});

test("status line: the working verb and detail (no parentheses), the background note, contr's orchestration on the right (#99)", async () => {
  const cx = fakeCx(); const c = new Composer(cx);
  c.box(fake.agents[0]);
  await c.pollStatus(cx);
  const th = new Proxy({}, { get: (_t, k) => `#${k}` });
  const el = c.render(fake.agents[0], th);
  assert.equal(byId(el, "tags"), null, "no tags row below the box");
  assert.equal(byId(el, "ctx-tag"), null, "no ctx pill on the status line");
  assert.equal(byId(el, "working"), null, "no working line while it does not work");
  assert.equal(byId(el, "background"), null, "no background note without one");
  const ids = el.kids.map((k) => k._a?.id);
  assert.ok(ids.indexOf("status-line") < ids.indexOf("box"), "the status line is above the box");
  assert.equal(el._a.gap, 8, "8 px above the box");
  const both = c.render(fake.agents[0], th, 800, { text: "Brewing…", detail: "26s · ↓1.5k tokens" }, { text: "1 shell still running" });
  const line = byId(both, "status-line");
  assert.equal(line._a.px, 4);
  const w = byId(both, "working");
  assert.equal(textOf(w), "⠋ Brewing… 26s · ↓1.5k tokens");
  assert.equal(w.kids[0]._a.text_color, "#foreground"); assert.equal(w.kids[0]._a.w, 14);
  assert.equal(w.kids[1]._a.text_size, 13);
  assert.equal(w.kids[1]._a.text_color, "#foreground");
  assert.equal(w.kids[2]._a.font_family, "Menlo");
  assert.equal(w.kids[2]._a.text_size, 11);
  assert.equal(w.kids[2]._a.text_color, "#muted");
  const text = textOf(line);
  assert.ok(text.indexOf("Brewing…") < text.indexOf("1 shell still running") && text.indexOf("1 shell still running") < text.indexOf("Orchestrate"),
    "working, then background, then orchestration (#61)");
  assert.equal(byId(both, "background")._a.text_color, "#muted");
  assert.ok("truncate" in byId(both, "background").kids.at(-1)._a, "the background note truncates first");
  const alone = c.render(fake.agents[0], th, 800, null, { text: "2 background tasks" });
  assert.equal(byId(alone, "working"), null);
  assert.match(textOf(byId(alone, "background")), /2 background tasks/);
  const orch = byId(el, "orchestrate");
  assert.equal(orch._a.font_family, "Menlo");
  assert.equal(orch._a.text_color, "#muted");
  assert.equal(orch._a.border, undefined, "plain text, no pill");
  assert.deepEqual(log.exec[0][1].slice(0, 3), ["status", "--terminal", "t1"]);
});

test("box: radius 12, the design's padding, 14 px text; the bottom row's buttons are 28 px with radius 8 (#99)", () => {
  const c = new Composer(fakeCx()); c.box(fake.agents[0]);
  const el = c.render(fake.agents[0], TH);
  const box = byId(el, "box");
  assert.deepEqual([box._a.rounded, box._a.pt, box._a.pr, box._a.pb, box._a.pl, box._a.gap], [12, 12, 10, 8, 14, 10]);
  assert.deepEqual([byId(el, "slash-keys")._a.text_size, byId(el, "slash-keys")._a.line_height], [14, 20 / 14]); // a multiple of the text size, not px (#106)
  assert.equal(byId(el, "box-row")._a.gap, 4);
  const attach = byId(el, "attach");
  assert.deepEqual([attach._a.size, attach._a.rounded, attach.kids[0]._a.size], [28, 8, 16]);
  const send = byId(el, "send");
  assert.deepEqual([send._a.size, send._a.rounded, send._a.ml], [28, 8, 4]);
  assert.equal(send._a.bg, "#chrome", "empty: faint fill");
  assert.equal(send._a.text_color, "#dim");
  c.setText("hi");
  assert.equal(byId(c.render(fake.agents[0], TH), "send")._a.bg, "#accent");
  const branch = byId(el, "branch");
  assert.equal(textOf(branch), "feat/login");
  assert.equal(branch._a.font_family, "Menlo");
  assert.equal(branch.kids[0]._a.size, 13);
  const kids = byId(el, "box-row").kids.map((k) => k._a?.id);
  assert.deepEqual(kids.filter(Boolean), ["attach", "model", "branch", "send"], "no context while it is not known");
});

test("model button: the agent's mark in its color, the short name, the effort, a chevron; its tooltip names the app and the folder (#99)", async () => {
  const cx = fakeCx(); const c = new Composer(cx);
  const a = { ...fake.agents[0], program: { ...fake.agents[0].program, tile: "#d97757" } };
  c.box(a);
  // Nothing known yet: the app's name alone.
  let m = byId(c.render(a, TH), "model");
  assert.equal(textOf(m), "Claude Code");
  // The status line's tags.
  await c.pollStatus(cx);
  m = byId(c.render(a, TH), "model");
  assert.equal(textOf(m), "Opus 5.5 high");
  // The helper's `model` line wins: an id becomes the short name.
  c.models.set(a.address, { at: Date.now(), line: { type: "model", model: "claude-fable-5-2", effort: "medium" } });
  m = byId(c.render(a, TH), "model");
  assert.equal(textOf(m), "Fable 5.2 medium");
  assert.equal(m._a.h, 28); assert.equal(m._a.rounded, 8); assert.equal(m._a.px, 8); assert.equal(m._a.gap, 6);
  const [mark, name, effort, chev] = m.kids;
  assert.equal(mark._a.path, "icons/claude.svg"); assert.equal(mark._a.size, 13); assert.equal(mark._a.text_color, "#d97757");
  assert.equal(name._a.text_size, 12); assert.equal(name._a.text_color, "#foreground");
  assert.equal(effort._a.text_color, "#muted");
  assert.match(chev._a.path, /chevron-down\.svg$/); assert.equal(chev._a.size, 11); assert.equal(chev._a.text_color, "#muted");
  assert.equal(m._a.tooltip, "Fable 5.2 · medium effort · Claude Code\n~/w/app");
  assert.ok(m._h.hover, "a fill on hover");
});

test("modelName: ids become the short name; short names stay", () => {
  assert.equal(modelName("claude-opus-5-5"), "Opus 5.5");
  assert.equal(modelName("claude-fable-5-2"), "Fable 5.2");
  assert.equal(modelName("claude-sonnet-4-5-20250929"), "Sonnet 4.5");
  assert.equal(modelName("claude-3-5-haiku-20241022"), "Haiku 3.5");
  assert.equal(modelName("gpt-6-codex"), "GPT-6");
  assert.equal(modelName("opus"), "Opus");
  assert.equal(modelName("Opus 5.5"), "Opus 5.5");
  assert.equal(modelName(null), "");
  assert.deepEqual(modelInfo({ app: "pi", program: { name: "Pi" } }, null, null), { name: "Pi", effort: "", known: false });
  assert.equal(modelTip({ address: "x", app: "pi", program: { name: "Pi" } }, modelInfo({ program: { name: "Pi" } }, null, null)), "Pi");
});

test("the helper's model is asked with the status, at most every MODEL_MS, only when it has the command", async () => {
  forgetCommands();
  let asked = 0;
  fake.exec = (cmd, args) => {
    if (args[0] === "version") return jl(header("version"), { type: "version", commands: ["status", "model"] }, end);
    if (args[0] === "status") return jl(header("status"), statusLine, end);
    if (args[0] === "model") { asked++; return jl(header("model"), { type: "model", agent: "claude", model: "claude-opus-5-5", effort: "low" }, end); }
    return jl(header(args[0]), end);
  };
  const cx = fakeCx(); const c = new Composer(cx);
  c.box(fake.agents[0]);
  await c.pollStatus(cx);
  assert.equal(asked, 1);
  assert.deepEqual(log.exec.find(([, x]) => x[0] === "model")[1].slice(0, 3), ["model", "--terminal", "t1"]);
  assert.equal(textOf(byId(c.render(fake.agents[0], TH), "model")), "Opus 5.5 low");
  c.statusAt = 0; await c.pollStatus(cx);
  assert.equal(asked, 1, "not again before MODEL_MS");
  c.models.get("t1").at -= MODEL_MS; c.statusAt = 0; await c.pollStatus(cx);
  assert.equal(asked, 2);
  forgetCommands();
});

test("context: a ring by 5% steps and the percent; its tooltip says the tokens; a warning keeps the blocked color (#99)", async () => {
  assert.equal(ringIcon(0), null);
  assert.match(ringIcon(1), /ring\/arc-005\.svg$/);
  assert.match(ringIcon(59.6), /ring\/arc-060\.svg$/);
  assert.match(ringIcon(100), /ring\/arc-100\.svg$/);
  assert.match(ringIcon(140), /ring\/arc-100\.svg$/);
  const { existsSync } = await import("node:fs");
  for (let p = 5; p <= 100; p += 5) assert.ok(existsSync(new URL(`../mods/herdr-contr/icons/ring/arc-${String(p).padStart(3, "0")}.svg`, import.meta.url)), `arc ${p}`);
  assert.ok(existsSync(new URL("../mods/herdr-contr/icons/ring/track.svg", import.meta.url)));
  assert.ok(existsSync(new URL("../mods/herdr-contr/icons/chevron-down.svg", import.meta.url)));
  assert.equal(contextTip({ text: "ctx 149k / 250k", percent: 60 }), "149k of 250k tokens · auto-compact at 250k");
  assert.equal(contextTip({ text: "ctx 37%", percent: 37 }), "37% of the model's context window");
  assert.equal(contextPercent({ text: "ctx 37%" }), 37);
  assert.equal(contextPercent({ text: "ctx" }), null);

  const c = new Composer(fakeCx()); c.box(fake.agents[0]);
  c.status.set("t1", { type: "status", tags: [{ kind: "context", text: "ctx 149k / 250k", percent: 60, warn: false }] });
  let ctx = byId(c.render(fake.agents[0], TH), "context");
  assert.equal(textOf(ctx), "60%");
  assert.equal(ctx._a.tooltip, "149k of 250k tokens · auto-compact at 250k");
  const ring = byId(ctx, "ctx-ring");
  assert.equal(ring._a.size, 14);
  assert.equal(ring.kids[0]._a.text_color, "#border", "the track is faint");
  assert.match(ring.kids[1]._a.path, /arc-060/);
  assert.equal(ring.kids[1]._a.text_color, "#muted");
  assert.equal(ctx.kids[1]._a.font_family, "Menlo");
  assert.equal(ctx.kids[1]._a.text_size, 11);
  const row = byId(c.render(fake.agents[0], TH), "box-row").kids.map((k) => k._a?.id).filter(Boolean);
  assert.deepEqual(row, ["attach", "model", "context", "branch", "send"]);
  c.status.set("t1", { type: "status", tags: [{ kind: "context", text: "ctx 84%", percent: 84, warn: true }] });
  ctx = byId(c.render(fake.agents[0], TH), "context");
  assert.equal(byId(ctx, "ctx-ring").kids[1]._a.text_color, "#blocked");
  assert.equal(ctx.kids[1]._a.text_color, "#blocked");
  c.status.set("t1", { type: "status", tags: [{ kind: "context", text: "ctx 0%", percent: 0, warn: false }] });
  assert.equal(byId(c.render(fake.agents[0], TH), "ctx-ring").kids.length, 1, "0%: the track alone");
});

test("the placeholder follows the agent's name; a new state is made outside the draw and keeps the text and the keyboard", async () => {
  const { made } = await import("./stub/ui.mjs");
  const cx = fakeCx(); const c = new Composer(cx);
  assert.equal(c.input.placeholder, "Reply to the agent");
  const before = made.textarea;
  c.box(fake.agents[0]); c.render(fake.agents[0], TH);
  assert.equal(made.textarea, before, "no state made while drawing");
  const old = c.input;
  old._h.focus?.({}, cx);
  old.set_value("half typed");
  assert.equal(c.syncPlaceholder(fake.agents[0], cx), true);
  assert.equal(c.input.placeholder, "Reply to api");
  assert.equal(c.input.value(), "half typed");
  assert.equal(c.input.focusCalls, 1, "the keyboard stays in the box");
  assert.equal(old.released, true);
  assert.equal(c.syncPlaceholder(fake.agents[0], cx), false, "the same name: the same state");
  assert.equal(made.textarea, before + 1);
  c.syncPlaceholder(fake.agents[1], cx);
  assert.equal(c.input.placeholder, "Reply to Claude Code");
  assert.equal(c.input.focusCalls, 1, "a box that had the keyboard keeps it");
  // The new state's handlers work: Enter sends.
  c.box(fake.agents[1]); c.input.set_value("go"); c.typed = ["go", "go"];
  c.input._h.submit({}, cx); await cx.settle();
  assert.equal(sent.length, 1);
});

test("Enter sends the text and files through the sidecar, then empties the box", async () => {
  const cx = fakeCx(); const c = new Composer(cx);
  const b = c.box(fake.agents[0]);
  b.input.set_value("Fix it\n"); c.addFiles(["/tmp/a.png", "/tmp/a.png", "/tmp/b.txt"], cx);
  b.input._h.submit({}, cx); await cx.settle();
  assert.deepEqual(sent[0], ["send", "--terminal", "t1", "--text", "Fix it /tmp/a.png /tmp/b.txt", "--file", "/tmp/a.png", "--file", "/tmp/b.txt", "--cache", "/home/test/.config/herdr-app/herdr-contr/cache.json", "--home", "/home/test/.config/herdr-app"]);
  assert.equal(b.input.value(), ""); assert.deepEqual(b.files, []);
});

test("a blank box sends nothing; a failed send puts the text back with the reason", async () => {
  const cx = fakeCx(); const c = new Composer(cx);
  const b = c.box(fake.agents[0]);
  b.input.set_value("  \n"); await c.submit(cx);
  assert.equal(sent.length, 0);
  fake.sendResult = "failed"; fake.sendMessage = "The agent's terminal is gone.";
  b.input.set_value("hello"); await c.submit(cx);
  assert.equal(b.input.value(), "hello");
  assert.match(textOf(byId(c.render(fake.agents[0], new Proxy({}, { get: (_t, k) => `#${k}` })), "send-note")), /terminal is gone/);
  // Another agent in the pane: not sent either; the text comes back with the reason.
  fake.sendResult = "changed"; fake.sendMessage = "another agent is in this pane now; nothing was sent";
  b.input.set_value("again"); await c.submit(cx);
  assert.equal(b.input.value(), "again");
  assert.match(textOf(byId(c.render(fake.agents[0], new Proxy({}, { get: (_t, k) => `#${k}` })), "send-note")), /Another agent is in this pane now/);
  // An agent herdr-contr cannot tell apart: not sent, the text comes back, the sidecar's reason as it is.
  fake.sendResult = "no-identity"; fake.sendMessage = "herdr-contr cannot tell this agent from another program in this terminal, so it sends nothing. Use the terminal.";
  b.input.set_value("third"); await c.submit(cx);
  assert.equal(b.input.value(), "third");
  assert.match(textOf(byId(c.render(fake.agents[0], new Proxy({}, { get: (_t, k) => `#${k}` })), "send-note")), /cannot tell this agent/);
  // Text already in the agent's own box: not sent, the text stays, the helper says to clear the box.
  fake.sendResult = "box_not_empty"; fake.sendMessage = "The agent's box has text in it. Clear it in the terminal first. Nothing was sent.";
  b.input.set_value("/status"); await c.submit(cx);
  assert.equal(b.input.value(), "/status");
  assert.match(textOf(byId(c.render(fake.agents[0], new Proxy({}, { get: (_t, k) => `#${k}` })), "send-note")), /Clear it in the terminal first/);
});

test("each agent keeps its own draft", () => {
  const cx = fakeCx(); const c = new Composer(cx);
  c.box(fake.agents[0]).input.set_value("for api");
  c.box(fake.agents[1]).input.set_value("for t2");
  assert.equal(c.box(fake.agents[0]).input.value(), "for api");
});

test("the chat shows the composer, and a drop on it attaches files", async () => {
  fake.exec = (cmd, args) => args[0] === "feed"
    ? jl(header("feed"), { type: "log", agent: "claude", path: "/l.jsonl", start: 0, size: 1, cursor: "c" }, { type: "splice", at: 0, count: 1 }, { type: "row", id: "u1", k: "user", text: "hi" }, end)
    : jl(header(args[0]), end);
  const cx = fakeCx(); const v = new ChatView(cx); await cx.settle();
  let el = v.render(cx);
  assert.ok(byId(el, "composer"));
  el._h.on_drop({ paths: ["/tmp/shot.png"] }, cx);
  el = v.render(cx);
  assert.match(textOf(byId(el, "file-0")), /shot\.png/);
});

test("the + file dialog (osascript) shows on macOS only; drop still attaches on Linux", async () => {
  fake.exec = (cmd, args) => args[0] === "feed"
    ? jl(header("feed"), { type: "log", agent: "claude", path: "/l.jsonl", start: 0, size: 1, cursor: "c" }, { type: "splice", at: 0, count: 1 }, { type: "row", id: "u1", k: "user", text: "hi" }, end)
    : jl(header(args[0]), end);
  const cx = fakeCx(); const v = new ChatView(cx); await cx.settle();
  assert.ok(byId(v.render(cx), "attach"));
  const was = fake.window.platform;
  fake.window.platform = "linux";
  try {
    let el = v.render(cx);
    assert.ok(!byId(el, "attach"));
    el._h.on_drop({ paths: ["/tmp/shot.png"] }, cx);
    assert.match(textOf(byId(v.render(cx), "file-0")), /shot\.png/);
  } finally { fake.window.platform = was; }
});

// ⌘V in the box: the box's own paste runs before any key handler, so the mod binds the chord under its own
// key context and asks the sidecar first: files, else plain text (the box types it), else an image.
const PASTE_DIR = "/home/test/.config/herdr-app/herdr-contr/paste";
function pasteRig(answer) {
  const asked = [];
  fake.exec = (cmd, args) => {
    if (args[0] === "paste-image") { asked.push(args); return answer ? jl(header("paste-image"), ...answer, end) : { code: 1, stdout: "", stderr: "boom" }; }
    return jl(header(args[0]), end);
  };
  const cx = fakeCx(); const v = new ChatView(cx);
  const typed = [];
  v.composer.input.replace = (x) => typed.push(x);
  const press = async (clip) => {
    cx.read_from_clipboard = () => clip;
    const el = byId(v.render(cx), "composer");
    el._h.on_action(null, cx); await cx.settle();
    // An element stub answers every property, `then` too, so it goes back wrapped (or `await` never ends).
    return { el: byId(v.render(cx), "composer") };
  };
  return { asked, cx, v, typed, press };
}
const chips = (el) => [0, 1, 2].map((i) => byId(el, `file-${i}`)).filter(Boolean).map((c) => textOf(c).replace(/\s*×$/, ""));
// The image() paths of the attached files that show as pictures.
const findTag = (el, tag) => (!el || typeof el !== "object" ? null : el.tag === tag ? el : (el.kids || []).reduce((f, k) => f || findTag(k, tag), null));
const pics = (el) => [0, 1, 2].map((i) => findTag(byId(el, `file-${i}`), "image")).filter(Boolean).map((x) => x._a.path);

test("paste: the mod binds ⌘V (Ctrl+V on Linux) for its own box only, once", async () => {
  const cx = fakeCx(); const bound = [];
  cx.bind_keys = (b) => { bound.push(...b); return b.length; };
  const v = new ChatView(cx); await cx.settle(); new ChatView(cx);
  assert.deepEqual(bound.filter((b) => b.action === "herdr_contr_paste"), [{ keystroke: "secondary-v", action: "herdr_contr_paste", context: "HerdrContrComposer > Input" }]);
  // The "/" list's keys, once, under the context its wrapper has only while the list shows.
  assert.deepEqual(bound.filter((x) => x.action.startsWith("herdr_contr_slash")).map((x) => [x.keystroke, x.context]),
    [["up", "HerdrContrSlash > Input"], ["down", "HerdrContrSlash > Input"], ["tab", "HerdrContrSlash > Input"], ["escape", "HerdrContrSlash > Input"]]);
  // The suggestion's Tab (#125), under its own context.
  assert.deepEqual(bound.filter((x) => x.action === "herdr_contr_suggest_accept").map((x) => [x.keystroke, x.context]), [["tab", "HerdrContrSuggest > Input"]]);
  const el = byId(v.render(cx), "composer");
  assert.equal(el._a.key_context, "HerdrContrComposer");
  assert.equal(typeof el._h.on_action, "function");
});

test("paste: a screenshot (image, no text) is attached, nothing typed", async () => {
  const r = pasteRig([{ type: "image", path: `${PASTE_DIR}/paste-1.png` }]);
  const { el } = await r.press(undefined);
  assert.deepEqual(r.asked[0].slice(0, 3), ["paste-image", "--dir", PASTE_DIR]);
  assert.deepEqual(pics(el), ["herdr-contr/paste/paste-1.png"]);
  assert.deepEqual(r.typed, []);
});

test("paste: Finder files are attached, not their names typed", async () => {
  const r = pasteRig([{ type: "files", paths: ["/home/you/Shot 1.png", "/home/you/notes.txt"] }, { type: "image", path: null }]);
  const { el } = await r.press("Shot 1.png\rnotes.txt");
  assert.deepEqual(chips(el), ["Shot 1.png", "notes.txt"]);
  assert.deepEqual(r.typed, []);
});

test("paste: a browser's Copy Image (image and HTML, no plain text) is attached", async () => {
  const r = pasteRig([{ type: "image", path: `${PASTE_DIR}/paste-2.png` }]);
  const { el } = await r.press(undefined);
  assert.equal(chips(el).length, 1);
  assert.deepEqual(r.typed, []);
});

test("paste: plain text (alone, or with a picture of spreadsheet cells) is typed at the cursor; a missing sidecar still pastes text", async () => {
  const r = pasteRig([{ type: "image", path: null }]);
  let { el } = await r.press("hello");
  assert.deepEqual(r.typed, ["hello"]);
  assert.equal(chips(el).length, 0);
  const broken = pasteRig(null);
  ({ el } = await broken.press("still text"));
  assert.deepEqual(broken.typed, ["still text"]);
  assert.ok(!byId(el, "send-note"), "no error for a text paste");
});

test("paste: nothing to paste and a broken sidecar says why", async () => {
  const r = pasteRig(null);
  const { el } = await r.press(undefined);
  assert.match(textOf(byId(el, "send-note")), /sidecar/);
  assert.deepEqual(r.typed, []);
});

test("paste: when the box's own paste finds no text (no binding), the image is still attached", async () => {
  const r = pasteRig([{ type: "image", path: `${PASTE_DIR}/paste-3.png` }]);
  r.v.render(r.cx);
  r.v.composer.input._h.paste_other({}, r.cx); await r.cx.settle();
  assert.equal(chips(byId(r.v.render(r.cx), "composer")).length, 1);
});

test("Shift+Enter leaves the new line to the box and sends nothing; Enter sends", async () => {
  // The box puts the new line in itself (at the cursor, over a selection) and then reports a submit with
  // shift. The handler must not add a second one.
  const cx = fakeCx(); const c = new Composer(cx);
  const b = c.box(fake.agents[0]);
  b.input.set_value("first line");
  b.input._h.submit({ shift: true }, cx); await cx.settle();
  assert.equal(sent.length, 0, "Shift+Enter does not send");
  assert.equal(b.input.value(), "first line", "the handler adds no text");
  b.input.set_value("first line\nsecond line");
  b.input._h.submit({}, cx); await cx.settle();
  assert.equal(sent.length, 1, "Enter sends");
  assert.equal(sent[0][sent[0].indexOf("--text") + 1], "first line\nsecond line");
});

// Enter in the box puts a new line at the cursor (over a selection) before it reports the submit. In the
// middle of the text that line went out, and came back on a failed send. `type` is what the box does: set the
// value, then report the change (or not: the order is the box's).
function enterRig() {
  const cx = fakeCx(); const c = new Composer(cx);
  const b = c.box(fake.agents[0]);
  const type = (v, report = true) => { b.input.set_value(v); if (report) b.input._h.change({}, cx); };
  const text = () => sent[sent.length - 1][sent[sent.length - 1].indexOf("--text") + 1];
  return { cx, b, type, text };
}

test("Enter in the middle of the text sends it without the new line Enter put there", async () => {
  for (const reported of [true, false]) {
    const { cx, b, type, text } = enterRig();
    type("ab"); type("abcd");
    type("ab\ncd", reported);
    b.input._h.submit({}, cx); await cx.settle();
    assert.equal(text(), "abcd", `change reported: ${reported}`);
  }
});

test("Enter over a selection sends the text as it was", async () => {
  const { cx, b, type, text } = enterRig();
  type("abXcd"); type("ab\ncd");
  b.input._h.submit({}, cx); await cx.settle();
  assert.equal(text(), "abXcd");
});

test("a failed send puts the text back without Enter's new line", async () => {
  fake.sendResult = "failed"; fake.sendMessage = "Not sent.";
  const { cx, b, type } = enterRig();
  type("abcd"); type("ab\ncd");
  b.input._h.submit({}, cx); await cx.settle();
  assert.equal(b.input.value(), "abcd");
});

test("Shift+Enter's new lines stay; Enter at the end adds none", async () => {
  const { cx, b, type, text } = enterRig();
  type("ab"); type("ab\n"); b.input._h.submit({ shift: true }, cx);
  type("ab\ncd"); type("ab\ncd\n");
  b.input._h.submit({}, cx); await cx.settle();
  assert.equal(text(), "ab\ncd");
  // A typed new line in the middle, then Enter at the end.
  type("x"); type("x\n\ny"); type("x\n\ny\n", false);
  b.input._h.submit({}, cx); await cx.settle();
  assert.equal(text(), "x\n\ny");
});

test("send names the agent and the log the chat showed, so the sidecar can refuse another one", async () => {
  const cx = fakeCx(); const c = new Composer(cx);
  c.seen = (address) => address === "t1" ? { agent: "claude", path: "/logs/a.jsonl" } : null;
  const b = c.box(fake.agents[0]);
  b.input.set_value("Go on"); b.input._h.submit({}, cx); await cx.settle();
  assert.deepEqual(sent[0], ["send", "--terminal", "t1", "--text", "Go on", "--expect-kind", "claude", "--expect-path", "/logs/a.jsonl", "--cache", "/home/test/.config/herdr-app/herdr-contr/cache.json", "--home", "/home/test/.config/herdr-app"]);
});

test("after /clear the send says the agent started a new conversation, and the chat reads the new log at once", async () => {
  const cx = fakeCx(); const c = new Composer(cx);
  const moved = [];
  c.onNewConversation = (address) => moved.push(address);
  const b = c.box(fake.agents[0]);
  fake.sendResult = "changed"; fake.sendMessage = "the agent started a new conversation; the chat now shows it; nothing was sent";
  b.input.set_value("again"); await c.submit(cx);
  assert.equal(b.input.value(), "again", "nothing was sent: the text comes back");
  const note = textOf(byId(c.render(fake.agents[0], new Proxy({}, { get: (_t, k) => `#${k}` })), "send-note"));
  assert.match(note, /The agent started a new conversation; the chat now shows it/);
  assert.doesNotMatch(note, /Another agent/);
  assert.deepEqual(moved, ["t1"]);
  // Another agent: the old note, and the chat is not told of a new conversation.
  fake.sendMessage = "another agent is in this pane now; nothing was sent";
  b.input.set_value("third"); await c.submit(cx);
  assert.deepEqual(moved, ["t1"]);
});

test("the chat polls its feed again when the composer reports a new conversation", async () => {
  let feeds = 0;
  fake.exec = (cmd, args) => { if (args[0] === "feed") feeds++; return jl(header(args[0]), end); };
  const cx = fakeCx(); const v = new ChatView(cx); await cx.settle();
  const before = feeds;
  v.composer.onNewConversation?.("t1", cx); await cx.settle();
  assert.ok(feeds > before, `feed asked again (${before} → ${feeds})`);
});

test("the chat gives the composer what its feed showed", async () => {
  const cx = fakeCx(); const v = new ChatView(cx); await cx.settle();
  v.feeds.set("t9", { agent: "codex", path: "/l/x.jsonl" });
  assert.deepEqual(v.composer.seen("t9"), { agent: "codex", path: "/l/x.jsonl" });
});

// "/" commands (Claude Code only): the list above the box while the text is "/" and a name, no space.
const CMDS = [
  { name: "clear", description: "Start a new conversation", source: "builtin" },
  { name: "compact", description: "Summarise the conversation", source: "builtin" },
  { name: "context", description: "Show how full the context is", source: "builtin" },
  { name: "deploy", description: "Deploy the app", source: "project" },
];
const TH = new Proxy({}, { get: (_t, k) => `#${k}` });
const MENU_CMDS = [...CMDS,
  { name: "effort", description: "Set effort level for model usage", source: "builtin", hint: "low|medium|high|xhigh|max", args: [{ value: "low" }, { value: "medium" }, { value: "high" }, { value: "xhigh" }, { value: "max" }], screen: true },
  { name: "model", description: "Set the AI model for Claude Code", source: "builtin", hint: "<model>", screen: true, args: [
    { value: "claude-fable-5-2", label: "Fable 5.2" }, { value: "claude-opus-5-5", label: "Opus 5.5" }, { value: "claude-sonnet-5-5", label: "Sonnet 5.5" },
    { value: "opus", label: "The latest Opus" }, { value: "sonnet", label: "Sonnet 5.5" }, { value: "my-model" }] },
];
function slashRig({ hasCommands = true, list = CMDS, screens = [] } = {}) {
  const asked = [];
  forgetCommands();
  fake.exec = (cmd, args) => {
    if (args[0] === "version") return jl(header("version"), { type: "version", commands: ["version", "send", ...(hasCommands ? ["commands"] : [])] }, end);
    if (args[0] === "commands") { asked.push(args); return jl(header("commands"), { type: "commands", agent: "claude", commands: list, screens }, end); }
    if (args[0] === "send") { sent.push(args); return jl(header("send"), { type: "done", result: "sent" }, end); }
    return jl(header(args[0]), end);
  };
  const cx = fakeCx(); const bound = [];
  cx.bind_keys = (b) => { bound.push(...b); return b.length; };
  const c = new Composer(cx);
  const b = c.box(fake.agents[0]);
  const type = async (v) => { b.input.set_value(v); b.input._h.change({}, cx); await cx.settle(); };
  const view = () => c.render(fake.agents[0], TH);
  const rows = () => { const l = byId(view(), "slash"); return l ? l.kids.map((k) => textOf(k)) : null; };
  const act = async (name) => { byId(view(), "slash-keys")._h[`on_action:${name}`](null, cx); await cx.settle(); };
  return { cx, c, b, type, view, rows, act, asked, bound };
}

test("slashQuery: the name after a leading /, while there is no space; matchCommands: by prefix, at most 8", () => {
  assert.equal(slashQuery("/"), "");
  assert.equal(slashQuery("/de"), "de");
  assert.equal(slashQuery("/kit:he"), "kit:he");
  for (const t of ["", "de", " /de", "/de ", "/deploy now", "/a\nb"]) assert.equal(slashQuery(t), null, JSON.stringify(t));
  assert.deepEqual(matchCommands(CMDS, "c").map((x) => x.name), ["clear", "compact", "context"]);
  assert.deepEqual(matchCommands(CMDS, "CO").map((x) => x.name), ["compact", "context"]);
  assert.deepEqual(matchCommands(CMDS, "zz"), []);
  const many = Array.from({ length: 20 }, (_v, i) => ({ name: `cmd${i}` }));
  assert.equal(matchCommands(many, "").length, 20, "all matches: the list scrolls");
});

test("typing / in a Claude Code chat shows its commands above the box, filtered as you type", async () => {
  const r = slashRig();
  assert.equal(r.rows(), null, "nothing before a /");
  await r.type("/");
  assert.equal(r.rows().length, 4);
  assert.match(r.rows()[3], /deploy.*Deploy the app/);
  await r.type("/co");
  assert.deepEqual(r.rows().map((x) => x.split(/\s/)[0]), ["/compact", "/context"]);
  await r.type("/co now");
  assert.equal(r.rows(), null, "a space closes it");
  assert.deepEqual(r.asked.map((a) => a.slice(0, 3)), [["commands", "--terminal", "t1"]], "the list is asked once");
  // The keys go to the list only while it shows: its key context is on then, and off without it.
  assert.equal(byId(r.view(), "slash-keys")._a.key_context, undefined);
  await r.type("/d");
  assert.equal(byId(r.view(), "slash-keys")._a.key_context, "HerdrContrSlash");
});

test("up and down choose (stopping at the ends), Tab completes the full name and a space; Esc closes until the text changes", async () => {
  const r = slashRig();
  await r.type("/c");
  await r.act("herdr_contr_slash_up");
  await r.act("herdr_contr_slash_down");
  await r.act("herdr_contr_slash_complete");
  assert.equal(r.b.input.value(), "/compact ", "up on the first row stays there; down: the second");
  assert.equal(r.rows(), null);
  assert.equal(sent.length, 0, "completing sends nothing");
  await r.type("/c");
  for (let i = 0; i < 5; i++) await r.act("herdr_contr_slash_down");
  await r.act("herdr_contr_slash_complete");
  assert.equal(r.b.input.value(), "/context ", "down on the last row stays there (no wrap)");
  await r.type("/c");
  await r.act("herdr_contr_slash_close");
  assert.equal(r.rows(), null, "Esc closes it");
  await r.type("/cl");
  assert.equal(r.rows().length, 1, "typing opens it again");
});

test("Enter completes a part of a name; on a full name it sends that name", async () => {
  const r = slashRig();
  await r.type("/dep");
  r.b.input.set_value("/dep\n"); r.b.input._h.submit({}, r.cx); await r.cx.settle();
  assert.equal(sent.length, 0, "a part of a name is not sent (Claude Code's own menu would run its first match)");
  assert.equal(r.b.input.value(), "/deploy ");
  await r.type("/clear");
  r.b.input._h.submit({}, r.cx); await r.cx.settle();
  assert.equal(sent.length, 1);
  assert.equal(sent[0][sent[0].indexOf("--text") + 1], "/clear");
});

test("a list for any agent the helper lists commands for; none when its list is empty or the helper has no commands", async () => {
  const r = slashRig({ hasCommands: false });
  await r.type("/");
  assert.equal(r.rows(), null);
  assert.equal(r.asked.length, 0, "a helper without commands is not asked");
  const s = slashRig();
  fake.agents[0] = agent("t1", "idle", { tab_name: "api", app: "codex", program: { key: "codex", name: "Codex" } });
  s.c.box(fake.agents[0]);
  await s.type("/");
  assert.equal(byId(s.c.render(fake.agents[0], TH), "slash").kids.length, 4, "a Codex chat shows what the helper lists");
  assert.equal(s.asked.length, 1);
  const e = slashRig({ list: [] });
  await e.type("/");
  assert.equal(e.rows(), null, "an empty list (an agent the helper has no commands for) shows nothing");
  await e.type("/c");
  assert.equal(e.asked.length, 1, "and is not asked again at once");
});

// More matches than rows: a window of 8 follows the pick; the wheel moves it.
const MANY = Array.from({ length: 20 }, (_v, i) => ({ name: `cmd${String(i).padStart(2, "0")}`, description: `Command ${i}`, source: "user" }));
const shown = (r) => r.rows().filter((x) => x.startsWith("/")).map((x) => x.split(/\s/)[0]);

test("20 matches: 8 rows show; down past the last row moves the window; the count says there are more", async () => {
  const r = slashRig({ list: MANY });
  await r.type("/cmd");
  assert.deepEqual(shown(r), MANY.slice(0, 8).map((c) => `/${c.name}`));
  assert.match(textOf(byId(r.view(), "slash-more")), /1–8 of 20/);
  for (let i = 0; i < 10; i++) await r.act("herdr_contr_slash_down");
  assert.deepEqual(shown(r), MANY.slice(3, 11).map((c) => `/${c.name}`), "pick 10 is the window's last row: rows 3 to 10");
  assert.match(textOf(byId(r.view(), "slash-more")), /4–11 of 20/);
  for (let i = 0; i < 20; i++) await r.act("herdr_contr_slash_down");
  assert.deepEqual(shown(r).at(-1), "/cmd19", "down stops at the last match");
  for (let i = 0; i < 19; i++) await r.act("herdr_contr_slash_up");
  assert.deepEqual(shown(r), MANY.slice(0, 8).map((c) => `/${c.name}`), "up to the first: the window follows back");
  for (let i = 0; i < 12; i++) await r.act("herdr_contr_slash_down");
  await r.type("/cmd1");
  assert.deepEqual(shown(r), MANY.slice(10, 18).map((c) => `/${c.name}`), "a new filter starts at its first match");
  await r.act("herdr_contr_slash_complete");
  assert.equal(r.b.input.value(), "/cmd10 ");
});

test("the wheel moves the window a row per 30 px; the pick stays on a shown row", async () => {
  const r = slashRig({ list: MANY });
  await r.type("/cmd");
  const wheel = async (dy) => { byId(r.view(), "slash")._h.on_scroll_wheel({ delta: { x: 0, y: dy }, touch_phase: "moved" }, r.cx); await r.cx.settle(); };
  await wheel(-90);
  assert.deepEqual(shown(r), MANY.slice(3, 11).map((c) => `/${c.name}`), "down 90 px: three rows");
  await r.act("herdr_contr_slash_complete");
  assert.equal(r.b.input.value(), "/cmd03 ", "the pick moved into the window");
  await r.type("/cmd");
  await wheel(-15); await wheel(-15);
  assert.equal(shown(r)[0], "/cmd01", "small trackpad steps add up");
  await wheel(+1000);
  assert.equal(shown(r)[0], "/cmd00", "up stops at the top");
  await wheel(-10000);
  assert.equal(shown(r).at(-1), "/cmd19", "down stops at the bottom");
});

const BUILT = [
  { name: "clear", description: "Start a new conversation", source: "builtin" },
  { name: "effort", description: "Set effort level", source: "builtin", hint: "low|medium|high|xhigh|max", args: ["low", "medium", "high", "xhigh", "max"], screen: true },
  { name: "model", description: "Set the AI model", source: "builtin", hint: "<model>", args: ["opus", "sonnet", "haiku", "fable", "default"], screen: true },
];

test("a built-in shows its argument hint, and \"needs a value\" when sent bare it opens a screen in the terminal", async () => {
  const r = slashRig({ list: BUILT });
  await r.type("/");
  const rows = r.rows();
  assert.match(rows[2], /\/model\s*<model>.*Set the AI model.*needs a value/);
  assert.doesNotMatch(rows[0], /needs a value/);
});

test("screenNote: a screen command alone, with or without a value it takes; anything else is null", () => {
  const screens = ["status", "config"];
  assert.equal(screenNote(BUILT, screens, "/model"), "/model opens a menu in the terminal. Add a value, like /model opus.");
  assert.equal(screenNote(BUILT, screens, "  /status\n"), "/status opens a screen in the terminal. Use it there.");
  const hintOnly = [{ name: "resume", hint: "[session]", screen: true }];
  assert.equal(screenNote(hintOnly, [], "/resume"), "/resume opens a menu in the terminal. Add a value, like /resume [session].");
  for (const t of ["/model sonnet", "/status now", "/clear", "/mode", "model", "hi /model", "/", ""]) assert.equal(screenNote(BUILT, screens, t), null, JSON.stringify(t));
  assert.equal(screenNote(undefined, undefined, "/model"), null, "no list yet: the helper's send checks");
});

test("Enter on a bare screen command sends nothing, keeps the text and says why; with a value it goes out", async () => {
  const r = slashRig({ list: [...BUILT, { name: "statusline", description: "Set up the status line", source: "user" }], screens: ["status"] });
  const before = sent.length;
  const enter = async (v) => { await r.type(v); r.b.input.set_value(`${v}\n`); r.b.input._h.submit({}, r.cx); await r.cx.settle(); };
  await enter("/model");
  assert.equal(sent.length, before, "nothing sent");
  assert.equal(r.b.input.value(), "/model", "the text stays for a value");
  assert.match(textOf(byId(r.view(), "send-note")), /\/model opens a menu in the terminal\. Add a value, like \/model opus\./);
  await enter("/status");
  assert.equal(sent.length, before, "a left-out screen is not completed to another name, and not sent");
  assert.equal(r.b.input.value(), "/status");
  assert.match(textOf(byId(r.view(), "send-note")), /\/status opens a screen in the terminal\. Use it there\./);
  await enter("/model sonnet");
  assert.equal(sent.at(-1)[sent.at(-1).indexOf("--text") + 1], "/model sonnet");
  await enter("/clear");
  assert.equal(sent.at(-1)[sent.at(-1).indexOf("--text") + 1], "/clear");
});

test("after \"/model \" the list offers the model names; Tab or Enter completes one, Enter on a whole one sends it", async () => {
  const r = slashRig({ list: BUILT });
  await r.type("/mo");
  await r.act("herdr_contr_slash_complete");
  assert.equal(r.b.input.value(), "/model ");
  await r.type("/model ");
  assert.deepEqual(shown(r).length, 0, "values are not commands");
  assert.deepEqual(r.rows().map((x) => x.split(/\s/)[0]), ["opus", "sonnet", "haiku", "fable", "default"]);
  await r.type("/model s");
  assert.deepEqual(r.rows().map((x) => x.split(/\s/)[0]), ["opus", "sonnet"], "any part of a value");
  await r.type("/model so");
  assert.deepEqual(r.rows().map((x) => x.split(/\s/)[0]), ["sonnet"]);
  await r.act("herdr_contr_slash_complete");
  assert.equal(r.b.input.value(), "/model sonnet");
  r.b.input._h.submit({}, r.cx); await r.cx.settle();
  assert.equal(sent.at(-1)[sent.at(-1).indexOf("--text") + 1], "/model sonnet");
  await r.type("/effort x");
  r.b.input.set_value("/effort x\n"); r.b.input._h.submit({}, r.cx); await r.cx.settle();
  assert.equal(r.b.input.value(), "/effort xhigh", "Enter on a part of a value completes it");
  await r.type("/clear x");
  assert.equal(r.rows(), null, "a command without values: no list after its space");
});

// The helper's /model lists: { value, label } values (plain strings still work).
const LABELED = [
  { name: "model", description: "Set the AI model", source: "builtin", hint: "<model>", screen: true,
    args: [{ value: "claude-opus-5-5", label: "Opus 5.5" }, { value: "claude-sonnet-5-5", label: "Sonnet 5.5" }, { value: "sonnet", label: "The latest Sonnet" }] },
];

test("argItems and matchArgs: labeled or plain values, found by a part of the value or the label", () => {
  assert.deepEqual(argItems(["low", { value: "x", label: "X one" }, { value: "" }, null].filter((v) => v !== null)),
    [{ name: "low" }, { name: "x", description: "X one" }]);
  const items = argItems(LABELED[0].args);
  assert.deepEqual(matchArgs(items, "SONNET").map((x) => x.name), ["claude-sonnet-5-5", "sonnet"]);
  assert.deepEqual(matchArgs(items, "5.5").map((x) => x.name), ["claude-opus-5-5", "claude-sonnet-5-5"], "by the label");
  assert.deepEqual(matchArgs(items, "").length, 3);
});

test("/model lists full ids with their labels; a part completes; the note's example is the first value", async () => {
  const r = slashRig({ list: LABELED });
  await r.type("/model ");
  const rows = r.rows();
  assert.match(rows[0], /^claude-opus-5-5\s*Opus 5\.5/);
  await r.type("/model opus 5");
  assert.equal(r.rows(), null, "a space ends the value");
  await r.type("/model op");
  assert.deepEqual(r.rows().map((x) => x.split(/\s/)[0]), ["claude-opus-5-5"]);
  r.b.input.set_value("/model op\n"); r.b.input._h.submit({}, r.cx); await r.cx.settle();
  assert.equal(r.b.input.value(), "/model claude-opus-5-5", "Enter on a part completes it");
  r.b.input._h.submit({}, r.cx); await r.cx.settle();
  assert.equal(sent.at(-1)[sent.at(-1).indexOf("--text") + 1], "/model claude-opus-5-5");
  assert.equal(screenNote(LABELED, [], "/model"), "/model opens a menu in the terminal. Add a value, like /model claude-opus-5-5.");
});


test("a Claude Code session that does not orchestrate shows Orchestrate, and a press puts /contr:orchestrate in the box", () => {
  const cx = fakeCx(); const c = new Composer(cx);
  const a = fake.agents[0];
  const b = c.box(a);
  const el = c.render(a, TH);
  assert.equal(byId(el, "orchestrating"), null);
  const btn = byId(el, "orchestrate");
  assert.equal(textOf(btn), "Orchestrate");
  btn._h.on_click({}, cx);
  assert.equal(b.input.value(), "/contr:orchestrate ");
});

test("a session with the contr.orchestrating label shows Orchestrating with no count", () => {
  const a = fake.agents[0];
  fake.terminals = [{ address: a.address, labels: { "contr.orchestrating": "1" }, state: "running" }];
  const c = new Composer(fakeCx()); c.box(a);
  const el = c.render(a, TH);
  assert.equal(textOf(byId(el, "orchestrating")), "Orchestrating");
  assert.equal(byId(el, "orchestrate"), null);
});

test("live workers make a session orchestrate and are counted; finished ones are not", () => {
  const a = fake.agents[0];
  fake.terminals = [
    { address: a.address, labels: {}, state: "running" },
    { address: "w1", parent: a.address, state: "running" },
    { address: "w2", parent: a.address, state: "running" },
    { address: "w3", parent: a.address, state: "finished" },
    { address: "w4", parent: "other", state: "running" },
  ];
  const c = new Composer(fakeCx()); c.box(a);
  assert.match(textOf(byId(c.render(a, TH), "orchestrating")), /Orchestrating.*2 workers/);
  fake.terminals = fake.terminals.filter((x) => x.address !== "w2");
  assert.match(textOf(byId(c.render(a, TH), "orchestrating")), /1 worker$/);
});

test("other agents get no Orchestrate button, and a hidden status row still shows the pill", () => {
  const cx = fakeCx(); const c = new Composer(cx);
  const a = { ...fake.agents[0], app: "codex" };
  c.box(a);
  assert.equal(byId(c.render(a, TH), "orchestrate"), null);
  fake.terminals = [{ address: "w1", parent: a.address, state: "running" }];
  fake.shared[KEY_CONFIG] = { file: { status_row: "hide" } };
  const el = c.render(a, TH);
  assert.equal(byId(el, "tags"), null);
  assert.ok(byId(el, "orchestrating"));
});

// #85: an attached image under the app home shows as a small picture; any other file keeps its chip.
const HOME = "/home/test/.config/herdr-app";
test("thumbPath: a PNG, JPEG or SVG under the app home, relative to it; else null", () => {
  assert.equal(thumbPath(`${HOME}/herdr-contr/paste/paste-9.png`), "herdr-contr/paste/paste-9.png");
  assert.equal(thumbPath(`${HOME}/a/Shot.JPG`), "a/Shot.JPG");
  assert.equal(thumbPath(`${HOME}/a/b.jpeg`), "a/b.jpeg");
  assert.equal(thumbPath(`${HOME}/a/logo.svg`), "a/logo.svg");
  assert.equal(thumbPath(`${HOME}/herdr-contr/paste/notes.txt`), null, "not an image");
  assert.equal(thumbPath(`${HOME}/a/movie.mov`), null);
  assert.equal(thumbPath("/home/you/Desktop/shot.png"), null, "outside the app home: image() may refuse it");
  assert.equal(thumbPath(`${HOME}-other/shot.png`), null, "a folder that only starts like the home");
  assert.equal(thumbPath(undefined), null);
});

test("thumbPath keeps its answer by path: a draw does not work it out again", () => {
  const p = `${HOME}/herdr-contr/paste/cached-1.png`;
  assert.equal(thumbPath(p), "herdr-contr/paste/cached-1.png");
  const was = fake.appHome;
  fake.appHome = "/elsewhere";
  try { assert.equal(thumbPath(p), "herdr-contr/paste/cached-1.png"); } finally { fake.appHome = was; }
});

test("an image under the app home shows as a 56 px picture with ×; a text file and an outside image keep chips", async () => {
  const c = new Composer(fakeCx()); const cx = fakeCx();
  c.box(fake.agents[0]);
  c.addFiles([`${HOME}/herdr-contr/paste/paste-4.png`, "/home/you/notes.txt", "/home/you/Desktop/far.png"], cx);
  const t = { border: "#border", chrome: "#chrome", raised: "#raised", foreground: "#fg", muted: "#m", hover: "#h" };
  let el = c.render(fake.agents[0], t);
  assert.deepEqual(pics(el), ["herdr-contr/paste/paste-4.png"]);
  const frame = byId(el, "file-0"), img = findTag(frame, "image");
  assert.equal(THUMB_PX, 56);
  assert.equal(img._a.h, 56);
  assert.equal(frame._a.h, 58, "the frame has a fixed height (picture and border), so the box does not grow when it loads");
  assert.equal(frame._a.border_color, "#border");
  assert.ok(frame._a.rounded > 0 && img._a.rounded > 0, "rounded");
  assert.ok(!/paste-4/.test(textOf(frame)), "no name over the picture");
  assert.deepEqual(chips(el).slice(1), ["notes.txt", "far.png"]);
  assert.ok(!findTag(byId(el, "file-1"), "image") && !findTag(byId(el, "file-2"), "image"));
  // The × on the picture removes it; the chips stay.
  byId(el, "file-x-0")._h.on_click({}, cx);
  assert.deepEqual(c.files, ["/home/you/notes.txt", "/home/you/Desktop/far.png"]);
  el = c.render(fake.agents[0], t);
  assert.deepEqual(pics(el), []);
  assert.deepEqual(chips(el), ["notes.txt", "far.png"]);
});

test("a press on + opens the file dialog and attaches the picked files, also after the slash list set its pick", async () => {
  fake.exec = (cmd) => cmd === "osascript" ? { code: 0, stdout: "/tmp/a.txt\n/tmp/b.png\n", stderr: "" } : { code: 127, stdout: "", stderr: "" };
  const cx = fakeCx(); const c = new Composer(cx);
  const b = c.box(fake.agents[0]);
  b.pick = 0;
  byId(c.render(fake.agents[0], TH), "attach")._h.on_click({}, cx);
  await cx.settle();
  assert.ok(log.exec.some(([cmd]) => cmd === "osascript"));
  const el = c.render(fake.agents[0], TH);
  assert.match(textOf(byId(el, "file-0")), /a\.txt/);
  assert.match(textOf(byId(el, "file-1")), /b\.png/);
});

test("the placeholder names the agent by its own title when no tab is named (#106)", async () => {
  const { replyName, placeholderFor } = await import("../mods/herdr-contr/composer.js");
  const prog = { name: "Claude Code" };
  assert.equal(replyName({ program: prog, title: "\u2733 Flockbook layout" }), "Flockbook layout");
  assert.equal(replyName({ program: prog, tab_name: "Mine", title: "\u2733 Other" }), "Mine");
  assert.equal(replyName({ program: prog, title: "~/code/contr" }), "Claude Code");
  assert.equal(replyName({ program: prog, title: "Claude Code" }), "Claude Code");
  assert.equal(placeholderFor({ program: prog, title: "\u2733 Flockbook layout" }), "Reply to Flockbook layout");
  assert.equal(placeholderFor(null), "Reply to the agent");
});

// The model menu (#112). At the end: its rigs bind the "/" keys, which the paste test above sees bound once.
test("model menu: the helpers (family lines, other models, efforts) (#112)", () => {
  assert.equal(titleCase("xhigh"), "Xhigh");
  assert.ok(hasModelMenu(MENU_CMDS)); assert.ok(!hasModelMenu(CMDS)); assert.ok(!hasModelMenu(null));
  assert.deepEqual(effortValues(MENU_CMDS), ["low", "medium", "high", "xhigh", "max"]);
  assert.deepEqual(effortValues(CMDS), []);
  const o = otherModels(MENU_CMDS, "claude-opus-5-5");
  assert.deepEqual(o.map((x) => x.value), ["claude-fable-5-2", "claude-sonnet-5-5", "my-model"], "not the current one; each name once; no alias of a family with a version");
  assert.deepEqual(o[0], { value: "claude-fable-5-2", name: "Fable 5.2" });
  assert.equal(otherModels(MENU_CMDS, "claude-opus-5-5"), o, "kept: the same list and model give the same answer");
  assert.deepEqual(otherModels(MENU_CMDS, "Opus 5.5").map((x) => x.value), o.map((x) => x.value), "the current model by its name too");
});

test("model menu: a family alias is left out when a model of that family has a version; rows keep their height (#137)", () => {
  const list = [{ name: "model", args: [
    { value: "claude-opus-5-5", label: "" }, { value: "claude-sonnet-5-5", label: "" }, { value: "default", label: "The recommended model" },
    { value: "fable", label: "The latest Fable" }, { value: "opus", label: "The latest Opus" }, { value: "sonnet", label: "The latest Sonnet" },
    { value: "haiku", label: "The latest Haiku" }] }];
  assert.deepEqual(otherModels(list, "claude-fable-5-2").map((x) => x.value), ["claude-opus-5-5", "claude-sonnet-5-5", "default", "haiku"]);
});

function menuRig(opts = {}) {
  const r = slashRig({ list: MENU_CMDS, ...opts });
  r.c.models.set("t1", { at: Date.now(), line: { type: "model", model: "claude-opus-5-5", effort: "medium" } });
  r.pop = () => byId(r.view(), "model-popover");
  r.open = async () => { r.pop()._h.on_open_change(true, r.cx); await r.cx.settle(); };
  r.press = async (id) => { byId(r.view(), id)._h.on_click({}, r.cx); await r.cx.settle(); };
  return r;
}

test("model menu: a press on the model button opens a menu above it: the model with its line and a check, Effort, More models (#112)", async () => {
  const r = menuRig();
  const pop = r.pop();
  assert.equal(pop._a.anchor, "bottom_left", "above the button");
  assert.equal(pop._a.open, false);
  assert.equal(pop._trigger._a.id, "model", "the model button is the trigger");
  assert.equal(byId(r.view(), "model")._h.on_click, undefined, "the Popover owns the press");
  await r.open();
  assert.equal(r.pop()._a.open, true);
  assert.equal(r.asked.length, 1, "the commands are read when it opens");
  const menu = byId(r.view(), "model-menu");
  assert.equal(menu._a.rounded, 12); assert.equal(menu._a.bg, "#raised"); assert.equal(menu._a.border_color, "#border");
  const cur = byId(menu, "menu-current");
  assert.equal(textOf(cur), "Opus 5.5");
  const [names, check] = cur.kids;
  assert.equal(names._a.text_size, 15);
  assert.match(check._a.path, /check\.svg$/); assert.equal(check._a.text_color, "#accent");
  assert.equal(textOf(byId(menu, "menu-effort")), "Effort Medium");
  assert.match(byId(menu, "menu-effort").kids.at(-1)._a.path, /chevron-right\.svg$/);
  assert.equal(textOf(byId(menu, "menu-more")), "More models");
  assert.ok(byId(menu, "menu-div-effort") && byId(menu, "menu-div-more"), "dividers");
  assert.equal(byId(r.view(), "model")._a.tooltip, undefined, "no tooltip over the open menu");
  // A press outside or Escape: the Popover reports it closed.
  r.pop()._h.on_open_change(false, r.cx);
  assert.equal(r.pop()._a.open, false);
  assert.equal(r.c.menu, null);
  assert.equal(sent.length, 0, "nothing is sent by opening or closing");
});

test("model menu: Effort opens the efforts with a check on the current one; a press sends /effort and closes (#112)", async () => {
  const r = menuRig();
  r.b.input.set_value("half a reply");
  await r.open();
  await r.press("menu-effort");
  const menu = byId(r.view(), "model-menu");
  assert.equal(textOf(byId(menu, "menu-back")), "Effort");
  assert.deepEqual(["low", "medium", "high", "xhigh", "max"].map((v) => textOf(byId(menu, `menu-effort-${v}`))), ["Low", "Medium", "High", "Xhigh", "Max"]);
  assert.match(byId(menu, "menu-effort-medium").kids.at(-1)._a.path, /check\.svg$/);
  assert.equal(byId(menu, "menu-effort-high").kids.length, 1, "no check on the others");
  await r.press("menu-effort-high");
  assert.equal(r.c.menu, null, "closed");
  assert.equal(sent.length, 1);
  assert.equal(sent[0][sent[0].indexOf("--text") + 1], "/effort high");
  assert.equal(r.b.input.value(), "half a reply", "the box keeps its text");
  assert.equal(r.c.models.get("t1").at, 0, "the model is read again at the next poll");
  assert.equal(r.c.statusAt, 0);
  // The back row goes to the first page.
  await r.open(); await r.press("menu-effort"); await r.press("menu-back");
  assert.ok(byId(r.view(), "menu-current"));
});

test("model menu: More models lists the others; a press sends /model <id> (#112)", async () => {
  const r = menuRig();
  await r.open();
  await r.press("menu-more");
  const menu = byId(r.view(), "model-menu");
  assert.equal(textOf(byId(menu, "menu-back")), "More models");
  assert.equal(textOf(byId(menu, "menu-model-0")), "Fable 5.2", "the name alone, no line");
  assert.equal(textOf(byId(menu, "menu-model-2")), "my-model");
  assert.equal("flex_none" in byId(menu, "menu-model-0")._a, true, "a row keeps its height in the list (#137)");
  assert.equal(byId(menu, "menu-models")._a.max_h, 320);
  await r.press("menu-model-1");
  assert.equal(sent[0][sent[0].indexOf("--text") + 1], "/model claude-sonnet-5-5");
  assert.equal(r.c.menu, null);
});

test("model menu: a refused send shows its note; no /effort: the row shows the value with no sub menu (#112)", async () => {
  const noEffort = MENU_CMDS.filter((c) => c.name !== "effort");
  const r = menuRig({ list: noEffort });
  await r.open();
  const row = byId(r.view(), "menu-effort");
  assert.equal(textOf(row), "Effort Medium");
  assert.equal(row._h.on_click, undefined);
  assert.ok(!row.kids.some((k) => /chevron/.test(k._a?.path ?? "")));
  fake.exec = (cmd, args) => args[0] === "send" ? jl(header("send"), { type: "done", result: "busy", message: "The agent is busy." }, end) : jl(header(args[0]), end);
  await r.press("menu-more"); await r.press("menu-model-0");
  assert.deepEqual(r.c.note, { text: "The agent is busy.", bad: true });
  assert.notEqual(r.c.models.get("t1").at, 0, "not read again early");
});

test("model menu: no menu while the model is not known, or for an agent whose /model has no values (#112)", async () => {
  const r = slashRig({ list: MENU_CMDS });
  assert.equal(byId(r.view(), "model-popover"), null, "model not known yet");
  const pi = { ...fake.agents[1], app: "pi" };
  r.c.box(pi);
  r.c.models.set(pi.address, { at: Date.now(), line: { type: "model", model: "gpt-6", effort: null } });
  r.c.cmds.set(pi.address, { at: Date.now(), list: CMDS, screens: [] });
  const el = r.c.render(pi, TH);
  assert.equal(byId(el, "model-popover"), null);
  assert.equal(byId(el, "model")._h.on_click, undefined);
  // Another agent with values in its /model gets the menu.
  r.c.cmds.set(pi.address, { at: Date.now(), list: MENU_CMDS, screens: [] });
  assert.ok(byId(r.c.render(pi, TH), "model-popover"));
});

test("model menu: the model poll reads the commands once per agent, so the button knows its menu (#112)", async () => {
  const r = slashRig({ list: MENU_CMDS });
  fake.exec = ((inner) => (cmd, args) => args[0] === "version"
    ? jl(header("version"), { type: "version", commands: ["version", "send", "commands", "model", "status"] }, end) : inner(cmd, args))(fake.exec);
  forgetCommands();
  await r.c.pollModel(r.cx);
  r.c.models.get("t1").at -= MODEL_MS;
  await r.c.pollModel(r.cx);
  assert.equal(r.asked.length, 1);
});

// #125: Claude Code's suggested prompt in the reply box.
function suggestRig({ has = true, text = "run the tests" } = {}) {
  forgetCommands();
  const calls = [];
  fake.agents = [agent("t1", "idle", { tab_name: "api" }), agent("t2", "idle")];
  const prev = fake.exec;
  fake.exec = (cmd, args) => {
    if (args[0] === "version") return jl(header("version"), { type: "version", commands: ["version", "send", ...(has ? ["suggestion"] : [])] }, end);
    if (args[0] === "suggestion") { calls.push(args[2]); return jl(header("suggestion"), { type: "suggestion", text: rig.text }, end); }
    return prev(cmd, args);
  };
  const cx = fakeCx(); const c = new Composer(cx);
  const rig = { cx, c, calls, text, a: fake.agents[0] };
  c.box(rig.a); c.syncPlaceholder(rig.a, cx);
  rig.poll = async (calm = true, ag = rig.a) => { await c.pollSuggestion(cx, ag, calm); await cx.settle(); };
  return rig;
}

test("suggestion: an idle Claude Code agent with an empty box reads it; it becomes the placeholder with a Tab hint", async () => {
  const { c, calls, poll, a } = suggestRig();
  await poll();
  assert.deepEqual(calls, ["t1"]);
  assert.equal(c.input.placeholder, "run the tests");
  assert.ok(byId(c.render(a, TH), "suggest-hint"));
  assert.ok(textOf(c.render(a, TH)).includes("Tab to use"));
});

test("suggestion: read once on going idle, then at most every 3 s; never while working, typing or for other agents", async () => {
  const { c, calls, poll, a } = suggestRig();
  await poll(); await poll();
  assert.equal(calls.length, 1, "within 3 s: no new read");
  c.suggestAt -= 3001; await poll();
  assert.equal(calls.length, 2, "after 3 s: read again");
  await poll(false);
  assert.equal(c.suggest, null, "work forgets it");
  assert.equal(c.syncPlaceholder(a, null), true);
  assert.equal(c.input.placeholder, "Reply to api");
  await poll(true);
  assert.equal(calls.length, 3, "back to idle: read at once");
  c.empty = false; c.suggestAt = 0; c.calm = true; await poll();
  assert.equal(calls.length, 3, "text in the box: no read");
  c.empty = true;
  await poll(true, { ...a, app: "codex" });
  assert.equal(calls.length, 3, "not Claude Code: no read");
  await poll(true, fake.agents[1]);
  assert.equal(calls.length, 3, "not the shown agent: no read");
});

test("suggestion: hidden when the helper has no suggestion command; empty text keeps the placeholder", async () => {
  const r = suggestRig({ has: false });
  await r.poll();
  assert.deepEqual(r.calls, []);
  assert.equal(r.c.input.placeholder, "Reply to api");
  const e = suggestRig({ text: "" });
  await e.poll();
  assert.equal(e.calls.length, 1);
  assert.equal(e.c.input.placeholder, "Reply to api");
  assert.equal(byId(e.c.render(e.a, TH), "suggest-hint"), null);
});

test("suggestion: Tab in an empty box puts it in the box and sends nothing; with text Tab is not taken", async () => {
  const { c, cx, poll, a } = suggestRig();
  await poll();
  const keys = () => byId(c.render(a, TH), "slash-keys");
  assert.equal(keys()._a.key_context, "HerdrContrSuggest");
  keys()._h["on_action:herdr_contr_suggest_accept"](null, cx);
  assert.equal(c.input.value(), "run the tests");
  assert.equal(sent.length, 0);
  assert.notEqual(keys()._a.key_context, "HerdrContrSuggest", "text in the box: Tab does what it did");
  assert.equal(byId(c.render(a, TH), "suggest-hint"), null);
  c.setText("mine"); c.acceptSuggestion(cx);
  assert.equal(c.input.value(), "mine");
});

test("suggestion: a send and another agent forget it; a read under way when forgotten counts for nothing", async () => {
  const { c, cx, poll, a } = suggestRig();
  await poll();
  c.input.set_value("go"); c.typed = ["go", "go"]; c.empty = false;
  await c.submit(cx); await cx.settle();
  assert.equal(c.suggest, null, "a send forgets it");
  c.empty = true; c.suggestAt = 0; await poll();
  assert.ok(c.suggest);
  c.box(fake.agents[1]);
  assert.equal(c.suggest, null, "another agent forgets it");
  c.box(a); c.calm = false; c.empty = true;
  const p = c.pollSuggestion(cx, a, true);
  c.forgetSuggestion(cx);
  await p; await cx.settle();
  assert.equal(c.suggest, null);
});
