// Our copy of Herdr's spaces mod: the project's favicon as the space icon, else a folder icon.
import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fake, log, reset, theme } from "./stub/herdr.mjs";
import { fakeCx } from "./fake-cx.mjs";
import Spaces, { FAVICONS, findFavicon, cacheFavicon } from "../mods/herdr-contr-spaces/main.js";

beforeEach(() => reset());

// A fake folder: exec runs the shell loop against the files it holds.
const folderWith = (files) => (cmd, args, opts) => {
  assert.equal(cmd, "sh");
  assert.equal(opts.on, undefined);
  const hit = args.slice(3).find((f) => files.includes(f));
  return hit ? { code: 0, stdout: hit, stderr: "" } : { code: 1, stdout: "", stderr: "" };
};

// The helper's `favicon` command: copies to the cache folder it is given, by default.
const HOME = "/home/test/.config/herdr-app";
const CACHE = `${HOME}/herdr-contr/favicons`;
const copied = (src) => `${CACHE}/fav-${src.replace(/\W/g, "")}`;
const withHelper = (folder, answer = (src, dir) => JSON.stringify({ type: "favicon", path: `${dir}/fav-${src.replace(/\W/g, "")}` })) => (cmd, args, opts) => {
  if (cmd === "sh") return folder(cmd, args, opts);
  assert.equal(cmd, `${HOME}/herdr-contr/bin/herdr-contr-feed`);
  assert.equal(args[0], "favicon");
  const at = (n) => args[args.indexOf(n) + 1];
  assert.equal(at("--dir"), CACHE);
  return { code: 0, stdout: answer(at("--path"), at("--dir")) + "\n", stderr: "" };
};

const view = () => { const v = Object.create(Spaces.prototype); v.icons = {}; return v; };
const iconOf = (v, w) => v.spaceIcon(w, theme());
const isFolder = (el) => {
  assert.equal(el.kids[0].tag, "svg");
  assert.equal(el.kids[0]._a.path, "mods/workspace-chrome/folder.svg");
};

test("the search order: root, then the folders, favicons first; no .ico", () => {
  assert.deepEqual(FAVICONS.slice(0, 4), ["favicon.svg", "favicon.png", "public/favicon.svg", "public/favicon.png"]);
  assert.equal(FAVICONS.indexOf("web/favicon.png"), 13);
  assert.deepEqual(FAVICONS.slice(14, 17), ["apple-touch-icon.png", "icon.png", "icon.svg"]);
  assert.equal(FAVICONS.at(-1), "web/icon.svg");
  assert.ok(!FAVICONS.some((f) => f.endsWith(".ico")));
});

for (const [files, want] of [
  [["favicon.svg", "public/favicon.png"], "favicon.svg"],
  [["public/favicon.png"], "public/favicon.png"],
  [["src/app/favicon.svg", "icon.png"], "src/app/favicon.svg"],
  [["static/apple-touch-icon.png"], "static/apple-touch-icon.png"],
  [["app/icon.svg"], "app/icon.svg"],
]) {
  test(`favicon found: ${want}`, async () => {
    fake.exec = folderWith(files);
    assert.equal(await findFavicon("/p/site/"), `/p/site/${want}`);
  });
}

test("no favicon: null, and the row shows the folder icon", async () => {
  fake.exec = folderWith(["favicon.ico", "README.md"]);
  assert.equal(await findFavicon("/p/lib"), null);
  const v = view(), cx = fakeCx();
  const w = { id: "s1", name: "lib", folder: "/p/lib", tabs: [] };
  v.findIcons([w], cx);
  await cx.settle();
  assert.equal(v.icons["/p/lib"], null);
  isFolder(iconOf(v, w));
});

test("a found favicon draws as a full-color image", async () => {
  fake.exec = withHelper(folderWith(["public/apple-touch-icon.png"]));
  const v = view(), cx = fakeCx();
  const w = { id: "s1", name: "site", folder: "/p/site", tabs: [] };
  v.findIcons([w], cx);
  await cx.settle();
  const el = iconOf(v, w);
  assert.equal(el.tag, "image");
  // A path relative to the app home: image() draws no file outside it (#116).
  assert.equal(el._a.path, copied("/p/site/public/apple-touch-icon.png").slice(HOME.length + 1));
  assert.equal(log.exec[1][1].at(-1), HOME);
  assert.equal(cx.notified, 1);
});

test("a space on another machine: the folder icon, and no look", async () => {
  fake.exec = folderWith(["favicon.svg"]);
  const v = view(), cx = fakeCx();
  const w = { id: "s1", name: "vps", folder: "/srv/site", machine: "vps", tabs: [] };
  v.findIcons([w], cx);
  await cx.settle();
  assert.equal(log.exec.length, 0);
  isFolder(iconOf(v, w));
});

test("a failed look: the folder icon", async () => {
  fake.exec = () => { throw new Error("no such folder"); };
  const v = view(), cx = fakeCx();
  const w = { id: "s1", name: "gone", folder: "/p/gone", tabs: [] };
  v.findIcons([w], cx);
  await cx.settle();
  isFolder(iconOf(v, w));
});

test("the cache: one look per folder, again only when the folder changes", async () => {
  fake.exec = withHelper(folderWith(["favicon.svg"]));
  const v = view(), cx = fakeCx();
  const w = { id: "s1", name: "a", folder: "/p/a", tabs: [{ id: "t1" }] };
  for (let i = 0; i < 3; i++) { v.findIcons([w], cx); await cx.settle(); }
  assert.equal(log.exec.length, 2); // one look, one copy
  fake.panes = [{ tab: "t1", cwd: "/p/b" }];
  v.findIcons([w], cx);
  await cx.settle();
  assert.equal(log.exec.length, 4);
  assert.equal(log.exec[2][2].cwd, "/p/b");
  assert.equal(iconOf(v, w)._a.path, copied("/p/b/favicon.svg").slice(HOME.length + 1));
});

for (const [why, answer] of [
  ["no copy (path null)", () => JSON.stringify({ type: "favicon", path: null })],
  ["an old helper with no favicon command", () => JSON.stringify({ type: "error", code: "usage" })],
  ["a copy outside the app home", () => JSON.stringify({ type: "favicon", path: "/p/site/favicon.png" })],
  ["no answer", () => ""],
]) {
  test(`a favicon the helper cannot copy: the folder icon (${why})`, async () => {
    fake.exec = withHelper(folderWith(["favicon.png"]), answer);
    assert.equal(await cacheFavicon("/p/site/favicon.png"), null);
    const v = view(), cx = fakeCx();
    const w = { id: "s1", name: "site", folder: "/p/site", tabs: [] };
    v.findIcons([w], cx);
    await cx.settle();
    assert.equal(v.icons["/p/site"], null);
    isFolder(iconOf(v, w));
  });
}

test("the helper fails to run: the folder icon", async () => {
  fake.exec = withHelper(folderWith(["favicon.png"]), () => { throw new Error("no helper"); });
  const v = view(), cx = fakeCx();
  const w = { id: "s1", name: "site", folder: "/p/site", tabs: [] };
  v.findIcons([w], cx);
  await cx.settle();
  isFolder(iconOf(v, w));
});

test("the copy says what it changed and draws no letter avatar", () => {
  const src = readFileSync(new URL("../mods/herdr-contr-spaces/main.js", import.meta.url), "utf8");
  assert.match(src, /^\/\/ Adapted from Herdr's spaces mod \(Apache-2.0\); changes: the project's favicon, else a folder icon, as the space icon\./);
  assert.ok(!src.includes("toUpperCase()"));
});
