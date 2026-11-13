import { test } from "node:test";
import assert from "node:assert/strict";
import { linkRefs, slugOf, repoFor, forgetRepos, refsVersion } from "../mods/herdr-contr/refs.js";
import { markdownView } from "../mods/herdr-contr/mdblocks.js";
import { fake, log, reset } from "./stub/herdr.mjs";

const R = "o/r", G = "https://github.com";

test("PR #n, #n, owner/repo#n and GitHub URLs become links", () => {
  assert.equal(linkRefs("See PR #51 now.", R), `See [PR #51](${G}/o/r/pull/51) now.`);
  assert.equal(linkRefs("Fixes #57", R), `Fixes [#57](${G}/o/r/issues/57)`);
  assert.equal(linkRefs("(#9)", R), `([#9](${G}/o/r/issues/9))`);
  assert.equal(linkRefs("in a/b-c#3.", R), `in [a/b-c#3](${G}/a/b-c/issues/3).`);
  const u = `${G}/x/y/pull/12`;
  assert.equal(linkRefs(`at ${u} ok`, R), `at [${u}](${u}) ok`);
  assert.equal(linkRefs(`at ${G}/x/y/issues/4`), `at [${G}/x/y/issues/4](${G}/x/y/issues/4)`);
});

test("code, links, other URLs, words, colors and headings stay as written", () => {
  for (const s of ["`#51` and ``a #5``", "[#51](http://x/1)", "<http://x/#5>", "http://x.io/a#5", "abc#51", "##5", "#fff and #12ab",
    "# Title", "&#39;", "a/#5", `${G}/x/y/pull/12/files`])
    assert.equal(linkRefs(s, R), s, s);
  assert.equal(linkRefs("`#1` #2", R), `\`#1\` [#2](${G}/o/r/issues/2)`);
});

test("a bare #n with no known repository stays text; owner/repo#n does not need one", () => {
  assert.equal(linkRefs("PR #51 and #2"), "PR #51 and #2");
  assert.equal(linkRefs("a/b#2"), `[a/b#2](${G}/a/b/issues/2)`);
});

test("slugOf reads https and ssh remotes on github.com only", () => {
  assert.equal(slugOf("https://github.com/o/r.git\n"), "o/r");
  assert.equal(slugOf("git@github.com:o/r-x"), "o/r-x");
  assert.equal(slugOf("https://gitlab.com/o/r"), null);
  assert.equal(slugOf(""), null);
});

test("repoFor runs git once per folder, off the call, and caches the answer and a failure", async () => {
  reset(); forgetRepos();
  fake.exec = async (_c, args, opts) => ({ code: opts.cwd === "/w" ? 0 : 1, stdout: "git@github.com:o/r.git\n", stderr: "" });
  assert.equal(repoFor("/w", "m1"), null);
  assert.equal(repoFor("/bad"), null);
  assert.equal(repoFor(null), null);
  await new Promise((r) => setTimeout(r, 0));
  assert.equal(repoFor("/w", "m1"), "o/r");
  assert.equal(repoFor("/bad"), null);
  assert.deepEqual(log.exec.map((e) => [e[1].join(" "), e[2].cwd, e[2].on]), [["remote get-url origin", "/w", "m1"], ["remote get-url origin", "/bad", null]]);
  fake.exec = async () => { throw new Error("no git"); };
  assert.equal(repoFor("/gone"), null);
  await new Promise((r) => setTimeout(r, 0));
  assert.equal(repoFor("/gone"), null);
});

test("markdownView links references in prose, not in code boxes", () => {
  const T = new Proxy({}, { get: (_t, k) => `#${k}` });
  const all = (el) => { const out = []; (function walk(n) { if (!n) return; out.push(n); (n.kids ?? []).forEach(walk); })(el); return out; };
  const texts = all(markdownView("r", "PR #5\n\n```\n#6\n```", T, 600, 1, R)).map((n) => n._a?.text).filter(Boolean);
  assert.ok(texts.some((x) => x.startsWith(`[PR #5](`)), texts.join("|"));
  assert.ok(texts.some((x) => x.includes("\\#6")), "the code box keeps #6 as text");
});

test("a known repository's name before a bare reference links to that repository (#101)", async () => {
  reset(); forgetRepos();
  const remotes = { "/a": "o/r", "/b": "o/herdr-contr", "/c": "x/dup", "/d": "o/dup", "/e": "y/twin", "/f": "z/twin" };
  fake.exec = async (_c, _a, opts) => ({ code: 0, stdout: `https://github.com/${remotes[opts.cwd]}.git\n`, stderr: "" });
  assert.equal(linkRefs("herdr-contr #99", R), `herdr-contr [#99](${G}/o/r/issues/99)`, "not known yet: own repository");
  const v = refsVersion();
  for (const d of Object.keys(remotes)) repoFor(d);
  await new Promise((r) => setTimeout(r, 0));
  assert.ok(refsVersion() > v, "the version goes up when a repository becomes known");
  const H = `${G}/o/herdr-contr`;
  assert.equal(linkRefs("herdr-contr #99", R), `herdr-contr [#99](${H}/issues/99)`);
  assert.equal(linkRefs("see herdr-contr PR #99.", R), `see herdr-contr [PR #99](${H}/pull/99).`);
  assert.equal(linkRefs("herdr-contr#99", R), `herdr-contr[#99](${H}/issues/99)`);
  assert.equal(linkRefs("issue #99 and PR #9", R), `issue [#99](${G}/o/r/issues/99) and [PR #9](${G}/o/r/pull/9)`);
  assert.equal(linkRefs("abc#51", R), "abc#51");
  assert.equal(linkRefs("dup #1", R), `dup [#1](${G}/o/dup/issues/1)`, "two owners: the agent's own owner");
  assert.equal(linkRefs("twin #1", R), `twin [#1](${G}/o/r/issues/1)`, "two owners, none ours: own repository");
  assert.equal(linkRefs("herdr-contr #99"), `herdr-contr [#99](${H}/issues/99)`, "no own repository");
  const v2 = refsVersion(); fake.exec = async () => ({ code: 0, stdout: "git@github.com:o/r.git", stderr: "" }); repoFor("/a2");
  await new Promise((r) => setTimeout(r, 0));
  assert.equal(refsVersion(), v2, "a repository known before does not change the version");
});

test("the kept cuts and fits of a text use the new link when a repository becomes known (#101)", async () => {
  reset(); forgetRepos();
  const { KEY_CONFIG } = await import("../mods/herdr-contr/config.js");
  fake.shared[KEY_CONFIG] = { file: { inline_icons: "on" }, bad: false };
  const T = new Proxy({}, { get: (_t, k) => `#${k}` });
  const dump = (el) => JSON.stringify(el, (k, v) => (k === "parent" ? undefined : v));
  try {
    const before = dump(markdownView("r", "In other #7 now.", T, 600, 1, R));
    assert.ok(before.includes(`${G}/o/r/issues/7`) || before.includes("o/r#7"), before);
    fake.exec = async () => ({ code: 0, stdout: "git@github.com:q/other.git", stderr: "" });
    repoFor("/x"); await new Promise((r) => setTimeout(r, 0));
    const after = dump(markdownView("r", "In other #7 now.", T, 600, 1, R));
    assert.ok(after.includes("q/other"), after);
    assert.ok(!after.includes("o/r/issues/7") && !after.includes("o/r#7"), after);
  } finally { delete fake.shared[KEY_CONFIG]; }
});
