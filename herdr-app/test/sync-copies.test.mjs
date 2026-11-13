// scripts/sync-copies.mjs writes each copied function from its source.
import { test } from "node:test";
import assert from "node:assert/strict";
import { sync } from "../../scripts/sync-copies.mjs";

const copy = { source: "a.js", target: "b.js", lines: ["const clean ="], functions: ["function f("] };
const src = "const clean = 1;\nfunction f() {\n  return 1;\n}\n";

test("sync writes a function and a line that differ, and names them", () => {
  const dst = "// b\nconst clean = 2;\nfunction f() {\n  return 2;\n}\nfunction g() {\n}\n";
  const { text, differ } = sync(src, dst, copy);
  assert.equal(text, "// b\nconst clean = 1;\nfunction f() {\n  return 1;\n}\nfunction g() {\n}\n");
  assert.deepEqual(differ, ["b.js: const clean =", "b.js: function f("]);
});

test("sync names nothing when the copy is equal", () => {
  assert.deepEqual(sync(src, src, copy).differ, []);
});

test("sync copies a whole file", () => {
  assert.deepEqual(sync("x", "y", { target: "b.js", file: true }), { text: "x", differ: ["b.js"] });
  assert.deepEqual(sync("x", "x", { target: "b.js", file: true }).differ, []);
});

test("sync throws when a part is missing", () => {
  assert.throws(() => sync("", src, copy), /a\.js: no const clean =/);
  assert.throws(() => sync(src, "const clean = 1;\n", copy), /b\.js: no function f\(/);
});
