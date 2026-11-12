// install.sh is a thin wrapper around `herdr-contr-feed install` (its behaviour is tested in Rust:
// xplat/herdr-contr-feed/tests/install.rs). These run only with a Rust helper in HERDR_CONTR_FEED,
// against temp homes; HOME is a temp folder too.
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, readdirSync, existsSync, copyFileSync, chmodSync, readFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const modDir = join(dirname(fileURLToPath(import.meta.url)), "..");
const helper = process.env.HERDR_CONTR_FEED;
const skip = helper && existsSync(helper) ? false : "needs HERDR_CONTR_FEED (a Rust herdr-contr-feed)";

function sandbox() {
  const root = mkdtempSync(join(tmpdir(), "fb-install-"));
  const home = join(root, "user"), app = join(root, "app");
  mkdirSync(home); mkdirSync(app);
  return { root, home, app };
}
const run = (script, args, env) => spawnSync("/bin/sh", [script, ...args], { encoding: "utf8", env: { PATH: "/usr/bin:/bin", ...env } });

test("without a helper it stops and says how to build one", () => {
  const s = sandbox();
  const r = run(join(modDir, "install.sh"), ["--home", s.app], { HOME: s.home });
  assert.equal(r.status, 1);
  assert.match(r.stderr, /cargo build --release -p herdr-contr-feed/);
  assert.deepEqual(readdirSync(s.app), []);
});

test("--dry-run goes to herdr-contr-feed install and writes nothing", { skip }, () => {
  const s = sandbox();
  const r = run(join(modDir, "install.sh"), ["--home", s.app, "--dry-run"], { HOME: s.home, HERDR_CONTR_FEED: helper });
  assert.equal(r.status, 0, r.stdout + r.stderr);
  assert.match(r.stdout, /Dry run: would install into/);
  assert.match(r.stdout, /herdr-contr\/bin\/herdr-contr-feed/);
  assert.deepEqual(readdirSync(s.app), []);
});

const sha = (b) => createHash("sha256").update(b).digest("hex");
// The helper install.sh picks on this machine: bin/ on macOS, remote/<musl target>/ on Linux.
const mine = process.platform === "darwin" ? "bin/herdr-contr-feed"
  : `remote/${process.arch === "arm64" ? "aarch64" : "x86_64"}-unknown-linux-musl/herdr-contr-feed`;

test("as in the zip: this machine's helper runs and remote/ goes to <home>/herdr-contr/remote", { skip }, () => {
  const s = sandbox();
  const zip = join(s.root, "zip");
  mkdirSync(zip);
  copyFileSync(join(modDir, "install.sh"), join(zip, "install.sh"));
  mkdirSync(dirname(join(zip, mine)), { recursive: true });
  copyFileSync(helper, join(zip, mine)); chmodSync(join(zip, mine), 0o755);
  // A target this machine does not run, so it is only copied.
  const t = process.platform === "linux" && process.arch === "arm64" ? "x86_64-unknown-linux-musl" : "aarch64-unknown-linux-musl";
  const body = "#!/bin/sh\necho remote\n";
  mkdirSync(join(zip, "remote", t), { recursive: true });
  writeFileSync(join(zip, "remote", t, "herdr-contr-feed"), body);
  let sums = `${sha(body)}  ${t}/herdr-contr-feed\n`;
  if (mine.startsWith("remote/")) sums += `${sha(readFileSync(helper))}  ${mine.slice("remote/".length)}\n`;
  writeFileSync(join(zip, "remote/SHA256SUMS"), sums);
  const r = run(join(zip, "install.sh"), ["--home", s.app], { HOME: s.home });
  assert.equal(r.status, 0, r.stdout + r.stderr);
  assert.ok(existsSync(join(s.app, "mods/herdr-contr/mod.json")));
  assert.equal(readFileSync(join(s.app, "herdr-contr/bin/herdr-contr-feed")).length, readFileSync(helper).length);
  assert.equal(readFileSync(join(s.app, "herdr-contr/remote", t, "herdr-contr-feed"), "utf8"), body);
});

// Linux, with a fake uname: install.sh runs remote/<target>/ (never bin/, the macOS helper), and only
// when its sum matches remote/SHA256SUMS. The helpers here are shell scripts that print their name.
function linuxZip(arch) {
  const s = sandbox();
  const zip = join(s.root, "zip"), fake = join(s.root, "fake");
  mkdirSync(join(zip, "bin"), { recursive: true }); mkdirSync(fake);
  copyFileSync(join(modDir, "install.sh"), join(zip, "install.sh"));
  writeFileSync(join(fake, "uname"), `#!/bin/sh\n[ "$1" = -s ] && echo Linux || echo ${arch}\n`); chmodSync(join(fake, "uname"), 0o755);
  writeFileSync(join(zip, "bin/herdr-contr-feed"), "#!/bin/sh\necho mac \"$@\"\n"); chmodSync(join(zip, "bin/herdr-contr-feed"), 0o755);
  let sums = "";
  for (const t of ["x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl"]) {
    const body = `#!/bin/sh\necho ${t} "$@"\n`;
    mkdirSync(join(zip, "remote", t), { recursive: true });
    writeFileSync(join(zip, "remote", t, "herdr-contr-feed"), body); chmodSync(join(zip, "remote", t, "herdr-contr-feed"), 0o755);
    sums += `${sha(body)}  ${t}/herdr-contr-feed\n`;
  }
  writeFileSync(join(zip, "remote/SHA256SUMS"), sums);
  const go = () => spawnSync("/bin/sh", [join(zip, "install.sh"), "--dry-run"], { encoding: "utf8", env: { PATH: `${fake}:/usr/bin:/bin`, HOME: s.home } });
  return { zip, go };
}

test("on Linux x86_64 and arm64 the zip runs its musl helper, not bin/", () => {
  for (const [arch, t] of [["x86_64", "x86_64-unknown-linux-musl"], ["aarch64", "aarch64-unknown-linux-musl"]]) {
    const { zip, go } = linuxZip(arch);
    const r = go();
    assert.equal(r.status, 0, r.stdout + r.stderr);
    assert.equal(r.stdout.trim(), `${t} install --remote ${zip}/remote --helper ${zip}/remote/${t}/herdr-contr-feed --dry-run`);
  }
});

test("on Linux a helper that does not match remote/SHA256SUMS does not run", () => {
  const { zip, go } = linuxZip("x86_64");
  writeFileSync(join(zip, "remote/x86_64-unknown-linux-musl/herdr-contr-feed"), "#!/bin/sh\necho swapped\n");
  const r = go();
  assert.equal(r.status, 1);
  assert.match(r.stderr, /does not match remote\/SHA256SUMS/);
  assert.doesNotMatch(r.stdout, /swapped/);
});
