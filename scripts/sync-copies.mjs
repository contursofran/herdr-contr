#!/usr/bin/env node
// A mod cannot import another mod, so some mods hold copies of another mod's code.
// This script writes each copy from its source. `--check` writes nothing; it exits 1
// and names each copy that differs. The tests that compare the copies stay.
import { readFileSync, writeFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const root = new URL("../herdr-app/mods/", import.meta.url);

// parts: a function starts at its text and ends at the first "\n}\n" (the cut the tests use);
// a line starts with its text. file: true copies the whole file.
export const COPIES = [
  {
    source: "herdr-contr-spaces/projects.js",
    target: "herdr-contr-tabs/label.js",
    lines: ["const clean ="],
    functions: ["function telling(", "export function leadPanes(", "function title("],
  },
  { source: "herdr-contr-spaces/mainagents.js", target: "herdr-contr/mainagents.js", file: true },
];

const fnPart = (s, start) => {
  const i = s.indexOf(start);
  if (i < 0) return null;
  const end = s.indexOf("\n}\n", i);
  return end < 0 ? null : [i, end + 3];
};
const linePart = (s, start) => {
  const i = s.startsWith(start) ? 0 : s.indexOf("\n" + start) + 1;
  if (i <= 0 && !s.startsWith(start)) return null;
  const end = s.indexOf("\n", i);
  return [i, end < 0 ? s.length : end];
};

// Returns the new target text and the names that differ. Throws when a part is missing.
export function sync(src, dst, copy) {
  if (copy.file) return { text: src, differ: src === dst ? [] : [copy.target] };
  const differ = [];
  let out = dst;
  const parts = [...(copy.lines ?? []).map((n) => [n, linePart]), ...(copy.functions ?? []).map((n) => [n, fnPart])];
  for (const [name, cut] of parts) {
    const a = cut(src, name), b = cut(out, name);
    if (!a) throw new Error(`${copy.source}: no ${name}`);
    if (!b) throw new Error(`${copy.target}: no ${name}`);
    const want = src.slice(...a);
    if (out.slice(...b) === want) continue;
    differ.push(`${copy.target}: ${name}`);
    out = out.slice(0, b[0]) + want + out.slice(b[1]);
  }
  return { text: out, differ };
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  const check = process.argv.includes("--check");
  const all = [];
  for (const copy of COPIES) {
    const s = new URL(copy.source, root), t = new URL(copy.target, root);
    const { text, differ } = sync(readFileSync(s, "utf8"), readFileSync(t, "utf8"), copy);
    all.push(...differ);
    if (!check && differ.length) writeFileSync(t, text);
  }
  if (check && all.length) {
    console.error("Copies differ from their source (run node scripts/sync-copies.mjs):\n" + all.join("\n"));
    process.exit(1);
  }
  if (!check) for (const d of all) console.log("wrote " + d);
}
