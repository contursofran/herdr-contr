// Builds the phone PWA (web/) into xplat/flockbook-modfeed/web/, which the helper embeds, so the Rust build never
// needs Node.
//   node scripts/web-build.mjs          type-check (deno), strip types (Node's built-in), write the files
//   node scripts/web-build.mjs --check  fail when the files are not what the sources give
// No packages: Node 24 strips TypeScript types itself; `deno check` does the type check.
import { stripTypeScriptTypes } from "node:module";
import { readFileSync, readdirSync, writeFileSync, existsSync, rmSync, mkdirSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const web = join(root, "web");
const check = process.argv.includes("--check");

const sources = readdirSync(join(web, "src")).filter((f) => f.endsWith(".ts")).sort();
execFileSync("deno", ["check", "--quiet", "--config", join(web, "deno.json"), ...sources.map((f) => join(web, "src", f))], { stdio: "inherit" });

const files = {};
// hub.json: the answer to the page's first question (pair a device). The Rust helper answers it itself.
for (const f of ["index.html", "style.css", "manifest.webmanifest", "hub.json"]) files[f] = readFileSync(join(web, f));
// The icons.
for (const f of readdirSync(web).filter((f) => f.endsWith(".png")).sort()) files[f] = readFileSync(join(web, f));
for (const f of sources) {
  // Module imports name the .ts sources (for deno check); the browser loads the .js files.
  const js = stripTypeScriptTypes(readFileSync(join(web, "src", f), "utf8"), { mode: "strip" })
    .replace(/(from\s+["']\.\/[\w-]+)\.ts(["'])/g, "$1.js$2");
  files[f.replace(/\.ts$/, ".js")] = Buffer.from(js);
}

const names = Object.keys(files).sort();
// The same files for the Rust helper (`flockbook-modfeed serve` embeds this folder with rust-embed).
const rustDir = join(root, "xplat/flockbook-modfeed/web");
const rustSame = () => existsSync(rustDir)
  && readdirSync(rustDir).sort().join("\n") === names.join("\n")
  && names.every((n) => readFileSync(join(rustDir, n)).equals(files[n]));

if (check) {
  const rust = rustSame();
  console.log(rust ? `web-build: up to date (${names.length} files)` : "web-build: xplat/flockbook-modfeed/web/ stale; run node scripts/web-build.mjs");
  process.exit(rust ? 0 : 1);
}
rmSync(rustDir, { recursive: true, force: true });
mkdirSync(rustDir, { recursive: true });
for (const n of names) writeFileSync(join(rustDir, n), files[n]);
console.log(`web-build: wrote ${names.length} files (${names.join(", ")}) to xplat/flockbook-modfeed/web/`);
