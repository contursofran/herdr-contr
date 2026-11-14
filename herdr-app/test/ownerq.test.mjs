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
import { OwnerCards, cardOf, openByCoordinator, answerOwner, OWNER_MS } from "../mods/herdr-contr/ownerq.js";
import { ChatView } from "../mods/herdr-contr/chat.js";

const T = new Proxy({}, { get: (_t, k) => `#${k}` });
const jl = (...lines) => ({ code: 0, stdout: lines.map((l) => JSON.stringify(l)).join("\n") + "\n", stderr: "" });
const sidecarOk = (args) => jl({ type: "header", protocol: 1, sidecar: "0.1.0", command: args[0] }, { type: "end", ok: true });

// As contr's ask_owner writes them (internal/owner: Question).
const q = (id, coordinator, extra = {}) => ({ id, project: "herdr-contr", coordinator, question: `Question ${id}?`, asked: "2026-11-13T10:00:00Z", ...extra });
const D1 = q("D1", "t1", { title: "Merge order", from: "builder #96", options: ["Merge #96 first", "Merge #97 first", "Wait"],
  recommended: 1, impacts: ["#97 needs a merge of main", "#96 needs a merge of main", "Both wait"], refs: ["PR 96", "#97"] });

let dir, file, contrRuns, contrResult, opens, openResult;
const write = (questions) => writeFileSync(file, JSON.stringify({ questions, todos: [], next_question: 9, next_todo: 1 }, null, 2));
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
        // As settings.rs open_questions: the questions with no answer, oldest first.
        const qs = existsSync(file) ? state().questions.filter((x) => !x.answer) : [];
        return { code: 0, stdout: JSON.stringify(qs), stderr: "" };
      }
      contrRuns.push(args);
      if (contrResult) return contrResult;
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
