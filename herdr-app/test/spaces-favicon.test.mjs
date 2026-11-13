// Our copy of Herdr's spaces mod: the project's favicon as the space icon, else a folder icon.
import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fake, log, reset, theme } from "./stub/herdr.mjs";
import { fakeCx } from "./fake-cx.mjs";
import Spaces, { FAVICONS, findFavicon } from "../mods/herdr-contr-spaces/main.js";

beforeEach(() => reset());

// A fake folder: exec runs the shell loop against the files it holds.
const folderWith = (files) => (cmd, args, opts) => {
  assert.equal(cmd, "sh");
  assert.equal(opts.on, undefined);
  const hit = args.slice(3).find((f) => files.includes(f));
  return hit ? { code: 0, stdout: hit, stderr: "" } : { code: 1, stdout: "", stderr: "" };
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
  fake.exec = folderWith(["public/favicon.png"]);
  const v = view(), cx = fakeCx();
  const w = { id: "s1", name: "site", folder: "/p/site", tabs: [] };
  v.findIcons([w], cx);
  await cx.settle();
  const el = iconOf(v, w);
  assert.equal(el.tag, "image");
  assert.equal(el._a.path, "/p/site/public/favicon.png");
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
  fake.exec = folderWith(["favicon.svg"]);
  const v = view(), cx = fakeCx();
  const w = { id: "s1", name: "a", folder: "/p/a", tabs: [{ id: "t1" }] };
  for (let i = 0; i < 3; i++) { v.findIcons([w], cx); await cx.settle(); }
  assert.equal(log.exec.length, 1);
  fake.panes = [{ tab: "t1", cwd: "/p/b" }];
  v.findIcons([w], cx);
  await cx.settle();
  assert.equal(log.exec.length, 2);
  assert.equal(log.exec[1][2].cwd, "/p/b");
  assert.equal(iconOf(v, w)._a.path, "/p/b/favicon.svg");
});

test("the copy says what it changed and draws no letter avatar", () => {
  const src = readFileSync(new URL("../mods/herdr-contr-spaces/main.js", import.meta.url), "utf8");
  assert.match(src, /^\/\/ Adapted from Herdr's spaces mod \(Apache-2.0\); changes: the project's favicon, else a folder icon, as the space icon\./);
  assert.ok(!src.includes("toUpperCase()"));
});
