// The owner's questions in the coordinator's chat (#131): an owner file in a temporary folder, and a fake
// herdr-contr-orch: `owner open --json` gives its open questions, `owner answer` changes it (#186).
import { test, beforeEach, afterEach } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync, readFileSync, existsSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fake, log, reset } from "./stub/herdr.mjs";
import { textOf, byId } from "./stub/ui.mjs";
import { fakeCx } from "./fake-cx.mjs";
import { agent, space } from "./fixtures.mjs";
import { OwnerCards, cardOf, openByCoordinator, answerOwner, OWNER_MS, NOTE_MS, COPY_MS, todosByCoordinator, codeSpans, doneOwner } from "../mods/herdr-contr/ownerq.js";
import { ChatView } from "../mods/herdr-contr/chat.js";

const T = new Proxy({}, { get: (_t, k) => `#${k}` });
const jl = (...lines) => ({ code: 0, stdout: lines.map((l) => JSON.stringify(l)).join("\n") + "\n", stderr: "" });
const sidecarOk = (args) => jl({ type: "header", protocol: 1, sidecar: "0.1.0", command: args[0] }, { type: "end", ok: true });

// As contr's ask_owner writes them (internal/owner: Question).
const q = (id, coordinator, extra = {}) => ({ id, project: "herdr-contr", coordinator, question: `Question ${id}?`, asked: "2026-11-13T10:00:00Z", ...extra });
const D1 = q("D1", "t1", { title: "Merge order", from: "builder #96", options: ["Merge #96 first", "Merge #97 first", "Wait"],
  recommended: 1, impacts: ["#97 needs a merge of main", "#96 needs a merge of main", "Both wait"], refs: ["PR 96", "#97"] });

let dir, file, contrRuns, contrResult, opens, openResult;
const write = (questions, todos = []) => writeFileSync(file, JSON.stringify({ questions, todos, next_question: 9, next_todo: 9 }, null, 2));
// As contr's pin_for_owner writes them (internal/owner: Todo).
const td = (id, coordinator, text, extra = {}) => ({ id, project: "herdr-contr", coordinator, from: "builder #96", kind: "do", text, pinned: "2026-11-15T10:00:00Z", ...extra });
const state = () => JSON.parse(readFileSync(file, "utf8"));

beforeEach(() => {
  reset(); contrRuns = []; contrResult = null; opens = 0; openResult = null;
  dir = mkdtempSync(join(tmpdir(), "herdr-contr-owner-"));
  file = join(dir, "owner.json");
  fake.window = { platform: "macos" };
  fake.spaces = [space()];
  fake.agents = [agent("t1", "idle")];
  fake.exec = (cmd, args) => {
    if (cmd === "env") {
      // env HERDR_APP_HOME=<home> <the installed tool> owner open --json | owner answer <id> <answer>
      assert.deepEqual(args.slice(0, 2), ["HERDR_APP_HOME=/home/test/.config/herdr-app", "/home/test/.config/herdr-app/herdr-contr/bin/herdr-contr-orch"]);
      args = args.slice(2);
      if (args.join(" ") === "owner open --json") {
        opens++;
        if (openResult) return openResult;
        // As settings.rs open_items: the questions with no answer, then the to-dos not done, oldest first.
        const st = existsSync(file) ? state() : { questions: [], todos: [] };
        const qs = st.questions.filter((x) => !x.answer).map((x) => ({ kind: "question", ...x }));
        const ts = st.todos.filter((x) => !x.done).map((x) => ({ kind: "todo", id: x.id, coordinator: x.coordinator, from: x.from, todo_kind: x.kind, text: x.text }));
        return { code: 0, stdout: JSON.stringify([...qs, ...ts]), stderr: "" };
      }
      contrRuns.push(args);
      if (contrResult) return contrResult;
      if (args[1] === "done") {
        const st = state(); const it = st.todos.find((x) => x.id === args[2] && !x.done);
        if (!it) return { code: 1, stdout: "", stderr: `herdr-contr-orch owner done: owner: no open to-do ${args[2]}` };
        it.done = "2026-11-15T10:05:00Z";
        writeFileSync(file, JSON.stringify(st));
        return { code: 0, stdout: "", stderr: "" };
      }
      // contr owner answer <id> <answer>: the file changes first, then the coordinator gets the text.
      const st = state(); const it = st.questions.find((x) => x.id === args[2] && !x.answer);
      if (!it) return { code: 1, stdout: "", stderr: `contr owner answer: owner: no open question ${args[2]}` };
      Object.assign(it, { answer: args[3], answered_by: "owner", answered: "2026-11-13T10:05:00Z" });
      writeFileSync(file, JSON.stringify(st));
      return { code: 0, stdout: "", stderr: "" };
    }
    return sidecarOk(args);
  };
});
afterEach(() => rmSync(dir, { recursive: true, force: true }));

const click = async (el, cx) => { el._h.on_click({}, cx); await cx.settle(); };

test("cardOf: the recommended option first, each with its impact; title, from and refs kept", () => {
  const c = cardOf(D1);
  assert.equal(c.request, "D1");
  const qq = c.question.questions[0];
  assert.equal(qq.text, "Question D1?");
  assert.equal(qq.header, "Merge order · from builder #96");
  assert.deepEqual(qq.options.map((o) => [o.label, o.detail, o.recommended]), [
    ["Merge #97 first", "#96 needs a merge of main", true],
    ["Merge #96 first", "#97 needs a merge of main", false],
    ["Wait", "Both wait", false]]);
  assert.deepEqual(c.refs, ["PR 96", "#97"]);
  assert.deepEqual(cardOf(q("D2", "t1")).question.questions[0].options, [], "no options: only the own answer");
});

test("openByCoordinator: by coordinator, oldest first; no coordinator or no list gives nothing", () => {
  const m = openByCoordinator([D1, q("D2", "t2"), q("D3", "hc:t1"), q("D5", ""), q("D6", null), { id: "D7", coordinator: "t1" }]);
  assert.deepEqual([...m.keys()], ["t1", "t2"]);
  assert.deepEqual(m.get("t1").map((c) => c.request), ["D1", "D3"]);
  assert.equal(openByCoordinator(null).size, 0);
  assert.equal(openByCoordinator({ questions: [D1] }).size, 0);
});

test("a question shows only in the chat of the coordinator that asked it, with a count of the others", async () => {
  write([D1, q("D2", "t1"), q("D3", "t2")]);
  const cx = fakeCx(); const c = new OwnerCards(cx);
  await c.poll(cx, "t1"); await c.poll(cx, "t9"); await c.poll(cx, "box/t1");
  const el = c.view("t1", T);
  assert.match(textOf(el), /D1 · The coordinator asks · 1 more after this/);
  assert.match(textOf(el), /Merge order · from builder #96.*Question D1\?/);
  assert.match(textOf(el), /Merge #97 first.*Recommended.*#96 needs a merge of main/);
  assert.match(textOf(byId(el, "owner-card-refs")), /Refs: PR 96 · #97/);
  assert.equal(c.view("t9", T), null, "another agent: no card");
  assert.equal(c.view("box/t1", T), null, "an agent on another machine: the file is this machine's");
});

test("a press on an option and Send answer runs contr owner answer; the answered item leaves the card", async () => {
  write([D1, q("D2", "t1")]);
  const cx = fakeCx(); const c = new OwnerCards(cx);
  await c.poll(cx, "t1");
  await click(byId(c.view("t1", T), "owner-card-q0-o0"), cx);
  await click(byId(c.view("t1", T), "owner-card-answer"), cx);
  assert.deepEqual(contrRuns, [["owner", "answer", "D1", "Merge #97 first"]]);
  assert.equal(state().questions[0].answer, "Merge #97 first");
  const el = c.view("t1", T);
  assert.match(textOf(el), /D2 · The coordinator asks/, "the next one shows");
  assert.doesNotMatch(textOf(el), /more after this/);
});

test("the own answer goes as typed; keys 1-9 and Enter answer too", async () => {
  write([q("D7", "t1", { options: ["Yes", "No"] })]);
  const cx = fakeCx(); const c = new OwnerCards(cx);
  await c.poll(cx, "t1");
  c.entry("t1").others.set(0, " Only on Friday ");
  await click(byId(c.view("t1", T), "owner-card-answer"), cx);
  assert.deepEqual(contrRuns.at(-1), ["owner", "answer", "D7", "Only on Friday"]);
  write([q("D8", "t1", { options: ["Yes", "No"] })]);
  await c.poll(cx, "t1", true, true);
  assert.equal(c.key_down({ key: "2", modifiers: {} }, cx, "t1"), true);
  assert.equal(c.key_down({ key: "enter", modifiers: {} }, cx, "t1"), true);
  await cx.settle();
  assert.deepEqual(contrRuns.at(-1), ["owner", "answer", "D8", "No"]);
});

test("an item answered or decided elsewhere leaves the card at the next read", async () => {
  write([D1]);
  const cx = fakeCx(); const c = new OwnerCards(cx);
  await c.poll(cx, "t1");
  assert.ok(c.view("t1", T));
  write([{ ...D1, answer: "Wait", answered_by: "coordinator" }]);
  await c.poll(cx, "t1");
  assert.ok(c.view("t1", T), "not read again before OWNER_MS");
  c.readAt = Date.now() - OWNER_MS - 1;
  await c.poll(cx, "t1");
  assert.equal(c.view("t1", T), null);
  assert.equal(contrRuns.length, 0, "nothing is sent without a press");
});

test("owner open --json runs at most every OWNER_MS, and its output is not parsed again when it is the same", async () => {
  write([D1]);
  const cx = fakeCx(); const c = new OwnerCards(cx);
  await c.poll(cx, "t1"); await c.poll(cx, "t1"); await c.poll(cx, "t2");
  assert.equal(opens, 1);
  const open = c.open; c.readAt = 0;
  await c.poll(cx, "t1");
  assert.equal(opens, 2); assert.equal(c.open, open, "same output: the same map");
});

test("a tool that is missing, fails or gives text that is not a list: no cards, no throw", async () => {
  write([D1]);
  const cx = fakeCx(); const c = new OwnerCards(cx);
  await c.poll(cx, "t1");
  assert.ok(c.view("t1", T));
  for (const r of [{ code: 1, stdout: "", stderr: "owner: lock: busy" }, { code: 0, stdout: "nope", stderr: "" }, { code: 0, stdout: "{}", stderr: "" }]) {
    openResult = r; c.readAt = 0;
    await c.poll(cx, "t1");
    assert.equal(c.view("t1", T), null, JSON.stringify(r));
    openResult = null; c.readAt = 0; await c.poll(cx, "t1"); assert.ok(c.view("t1", T));
  }
  const run = fake.exec; fake.exec = () => { throw new Error("ENOENT"); };
  c.readAt = 0; await c.poll(cx, "t1");
  assert.equal(c.view("t1", T), null);
  fake.exec = run;
});

test("a failed answer keeps the card with contr's message; a saved answer that did not reach the coordinator says so", async () => {
  write([D1]);
  const cx = fakeCx(); const c = new OwnerCards(cx);
  await c.poll(cx, "t1");
  contrResult = { code: 1, stdout: "", stderr: "contr owner answer: owner: lock: busy" };
  c.pick(c.entry("t1"), c.card("t1").question.questions, 0, 0);
  await c.answer(cx, "t1");
  let el = c.view("t1", T);
  assert.match(textOf(byId(el, "owner-card-note")), /lock: busy/);
  assert.match(textOf(el), /Question D1\?/, "the card stays");
  contrResult = null;
  const run = fake.exec; fake.exec = (cmd, args) => {
    const r = run(cmd, args);
    return cmd === "env" ? { code: 1, stdout: "", stderr: "contr owner answer: saved; coordinator not reached: no terminal t1" } : r;
  };
  await c.answer(cx, "t1");
  el = c.view("t1", T);
  assert.match(textOf(el), /The answer is saved, but the coordinator did not get it.*no terminal t1/);
  assert.doesNotMatch(textOf(el), /Question D1\?/, "the item is answered: no card");
});

test("answerOwner: contr that cannot start is an error, not a throw", async () => {
  fake.exec = () => { throw new Error("ENOENT"); };
  const r = await answerOwner("D1", "x");
  assert.match(r.error.message, /herdr-contr-orch could not start.*ENOENT/);
});

test("the chat draws the coordinator's question above Claude Code's own card and answers it on a press", async () => {
  write([D1]);
  const cx = fakeCx(); const v = new ChatView(cx); await cx.settle();
  await v.tick(cx); await cx.settle();
  let el = v.render(cx);
  assert.match(textOf(byId(el, "chat-owner")), /D1 · The coordinator asks/);
  assert.equal(byId(el, "chat-card"), null, "no AskUserQuestion card: the agent is idle");
  assert.equal(byId(el, "chat-owner")._a.max_w, v.colWidth());
  assert.equal(contrRuns.length, 0);
  await click(byId(el, "chat-owner-q0-o2"), cx);
  await click(byId(v.render(cx), "chat-owner-answer"), cx);
  assert.deepEqual(contrRuns, [["owner", "answer", "D1", "Wait"]]);
  assert.equal(byId(v.render(cx), "chat-owner"), null);
  assert.equal(contrRuns.length, 1, "one answer, on the press");
});

// The owner's to-dos (#195).
const TODO_TEXT = "Test the build with `make app` and `` a`b ``, then:\n\n```sh\n  npm test\nnpm run lint\n```\n\nSee `make app` again.";
const copyCx = () => { const cx = fakeCx(); cx.clip = []; cx.write_to_clipboard = (s) => cx.clip.push(s); return cx; };

test("todosByCoordinator: only to-dos, by coordinator, oldest first; the question list keeps no to-do", () => {
  const list = [{ kind: "question", ...D1 }, { kind: "todo", id: "T1", coordinator: "hc:t1", from: "#9", todo_kind: "do", text: "One" },
    { kind: "todo", id: "T2", coordinator: "t2", text: "Two", todo_kind: "know" }, { kind: "todo", id: "T3", coordinator: "t1", text: "Three" },
    { kind: "todo", id: "T4", coordinator: "", text: "No one" }, { kind: "todo", id: "T5", coordinator: "t1" }, { kind: "todo", coordinator: "t1", text: "No id" },
    { kind: "question", id: "D9", coordinator: "t1", question: "Asked?", text: "Asked?" }];
  const m = todosByCoordinator(list);
  assert.deepEqual([...m.keys()], ["t1", "t2"]);
  assert.deepEqual(m.get("t1"), [{ id: "T1", from: "#9", text: "One", know: false }, { id: "T3", from: "", text: "Three", know: false }]);
  assert.equal(m.get("t2")[0].know, true);
  assert.equal(todosByCoordinator(null).size, 0);
  assert.deepEqual([...openByCoordinator([...list, { kind: "todo", id: "T6", coordinator: "t1", question: "Not a question" }]).values()].flat().map((c) => c.request), ["D1", "D9"]);
});

test("codeSpans: each span once, as written, outside code blocks; none in plain text", () => {
  assert.deepEqual(codeSpans(TODO_TEXT), ["make app", "a`b"]);
  assert.deepEqual(codeSpans("Run `gh pr\nview 7` and `  ` and ` x `"), ["gh pr view 7", "x"]);
  assert.deepEqual(codeSpans("Merge PR 12 when CI passes."), []);
  assert.deepEqual(codeSpans("an open ` tick"), []);
});

test("a to-do shows only in the chat of the coordinator that pinned it: text, from, a count of the others", async () => {
  write([], [td("T1", "t1", "Test the **build**"), td("T2", "t1", "CI is slow", { kind: "know" }), td("T3", "t2", "Other")]);
  const cx = fakeCx(); const c = new OwnerCards(cx);
  await c.poll(cx, "t1");
  const el = c.todoView("t1", T);
  assert.match(textOf(el), /T1 · To do · 1 more after this.*from builder #96.*Test the \*\*build\*\*/);
  assert.equal(c.todoKey("t1"), "T1");
  assert.equal(byId(el, "owner-todo-copy-0"), null, "no code: no copy button");
  assert.match(textOf(c.todoView("t2", T)), /T3 · To do/);
  assert.doesNotMatch(textOf(c.todoView("t2", T)), /more after this/);
  assert.equal(c.todoView("t9", T), null, "another agent: no card");
  assert.equal(c.todoView("box/t1", T), null, "an agent on another machine: the file is this machine's");
  assert.equal(c.view("t1", T), null, "a to-do is not a question card");
});

test("Done runs owner done; the next to-do shows, and the last one leaves", async () => {
  write([], [td("T1", "t1", "One"), td("T2", "t1", "CI is slow", { kind: "know", from: "" })]);
  const cx = fakeCx(); const c = new OwnerCards(cx);
  await c.poll(cx, "t1");
  await click(byId(c.todoView("t1", T), "owner-todo-done"), cx);
  assert.deepEqual(contrRuns, [["owner", "done", "T1"]]);
  assert.ok(state().todos[0].done);
  const el = c.todoView("t1", T);
  assert.match(textOf(el), /T2 · To know/);
  assert.doesNotMatch(textOf(el), /from /, "no from: no from line");
  await click(byId(el, "owner-todo-done"), cx);
  assert.equal(c.todoView("t1", T), null);
  assert.equal(contrRuns.length, 2);
});

test("a to-do marked done leaves at once, also while another read of the list runs", async () => {
  write([], [td("T1", "t1", "One")]);
  const cx = fakeCx(); const c = new OwnerCards(cx);
  await c.poll(cx, "t1");
  c.reading = true;
  await c.done(cx, "t1");
  assert.deepEqual(contrRuns, [["owner", "done", "T1"]]);
  assert.equal(c.todoView("t1", T), null);
});

test("a failed Done keeps the to-do with the tool's message; marked done but not told: the note alone, for NOTE_MS", async () => {
  write([], [td("T1", "t1", "One")]);
  const cx = fakeCx(); const c = new OwnerCards(cx);
  await c.poll(cx, "t1");
  contrResult = { code: 1, stdout: "", stderr: "herdr-contr-orch owner done: owner: lock: busy" };
  await c.done(cx, "t1");
  let el = c.todoView("t1", T);
  assert.match(textOf(byId(el, "owner-todo-note")), /lock: busy/);
  assert.match(textOf(el), /T1 · To do/, "the card stays");
  contrResult = null;
  const run = fake.exec; fake.exec = (cmd, args) => {
    const r = run(cmd, args);
    return cmd === "env" && args.includes("done") ? { code: 1, stdout: "", stderr: "herdr-contr-orch owner done: saved; coordinator not reached: no terminal t1" } : r;
  };
  await c.done(cx, "t1");
  fake.exec = run;
  el = c.todoView("t1", T);
  assert.match(textOf(el), /The to-do is marked done, but the coordinator did not get it.*no terminal t1/);
  assert.doesNotMatch(textOf(el), /T1 · To do/);
  c.todoBy.get("t1").noteAt = Date.now() - NOTE_MS - 1;
  await c.poll(cx, "t1");
  assert.equal(c.todoView("t1", T), null, "the note goes");
});

test("doneOwner: a tool that cannot start is a note, not a throw", async () => {
  fake.exec = () => { throw new Error("ENOENT"); };
  const r = await doneOwner("T1");
  assert.equal(r.ok, false);
  assert.match(r.note, /herdr-contr-orch could not start.*ENOENT/);
});

test("each code span and code block has a copy button: it copies the code as written and shows a check for COPY_MS", async () => {
  write([], [td("T1", "t1", TODO_TEXT)]);
  const cx = copyCx(); const c = new OwnerCards(cx);
  await c.poll(cx, "t1");
  let el = c.todoView("t1", T);
  assert.equal(byId(el, "owner-todo-copy-3"), null, "three buttons: two spans (one twice in the text) and a block");
  const iconOf = (n) => byId(c.todoView("t1", T), `owner-todo-copy-${n}`).kids[0]._a.path;
  assert.match(iconOf(0), /copy\.svg$/);
  await click(byId(el, "owner-todo-copy-0"), cx);
  await click(byId(el, "owner-todo-copy-1"), cx);
  await click(byId(el, "owner-todo-copy-2"), cx);
  assert.deepEqual(cx.clip, ["make app", "a`b", "  npm test\nnpm run lint"]);
  assert.match(iconOf(2), /check\.svg$/);
  assert.match(iconOf(0), /copy\.svg$/, "only the last pressed shows the check");
  // The timer of an earlier press does not end the check of a later one.
  const timers = cx.timers.filter((x) => x.once && x.ms === COPY_MS);
  assert.equal(timers.length, 3);
  timers[0].f(cx);
  assert.match(iconOf(2), /check\.svg$/);
  const n = cx.notified; timers[2].f(cx);
  assert.match(iconOf(2), /copy\.svg$/, "the check ends");
  assert.ok(cx.notified > n, "the end draws again");
  // With no timer, the check ends at the next draw after COPY_MS.
  await click(byId(c.todoView("t1", T), "owner-todo-copy-1"), cx);
  c.copied.at -= COPY_MS + 1;
  assert.match(iconOf(1), /copy\.svg$/);
});

test("the chat draws the coordinator's to-do under its question, and Done marks it", async () => {
  write([D1], [td("T1", "t1", "Run `make app`")]);
  const cx = fakeCx(); const v = new ChatView(cx); await cx.settle();
  await v.tick(cx); await cx.settle();
  let el = v.render(cx);
  assert.match(textOf(byId(el, "chat-owner")), /D1 · The coordinator asks/);
  assert.match(textOf(byId(el, "chat-todo")), /T1 · To do.*Run `make app`.*make app/);
  assert.equal(byId(el, "chat-todo")._a.max_w, v.colWidth());
  assert.ok(byId(el, "chat-todo-copy-0"));
  await click(byId(el, "chat-todo-done"), cx);
  assert.deepEqual(contrRuns, [["owner", "done", "T1"]]);
  assert.equal(byId(v.render(cx), "chat-todo"), null);
  assert.ok(byId(v.render(cx), "chat-owner"), "the question stays");
});
