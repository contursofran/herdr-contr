// Checks scripts/backend-paths.txt against the tracked files, and the guards
// and the message filter of scripts/export-backend.sh, and scripts/frozen-check.sh (#253).
// Run: node --test scripts/backend-paths.test.mjs
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('..', import.meta.url));
const script = join(root, 'scripts/export-backend.sh');

const lines = readFileSync(join(root, 'scripts/backend-paths.txt'), 'utf8')
  .split('\n').map((l) => l.trim()).filter((l) => l && !l.startsWith('#'));
// A line with `==>` renames; only the other lines select a path.
const selected = lines.filter((l) => !l.includes('==>'));

// The files that stay in herdr-contr (section 1 of the plan), and this export's own files.
const stays = new Set([
  'scripts/sync-copies.mjs',
  'LICENSE-APACHE',
  ...['10-herdr-tab-bar', '11-view-switch', '25-orchestrate-pill', '27-orchestration-settings',
    '31-wider-chat', '38-braille-chrome', '42-space-favicon', '100-projects-list',
    '108-hover-controls', '109-head-foot'].map((d) => `docs/decisions/${d}.md`),
  'scripts/backend-paths.txt',
  'scripts/export-backend.sh',
  'scripts/backend-paths.test.mjs',
  'scripts/frozen-check.sh',
]);

const matches = (file, entry) => (entry.endsWith('/') ? file.startsWith(entry) : file === entry);

const tmp = () => mkdtempSync(join(tmpdir(), 'backend-paths-'));

test('each tracked file is backend or mod', () => {
  const files = execFileSync('git', ['ls-files'], { cwd: root, encoding: 'utf8' }).split('\n').filter(Boolean);
  const loose = files.filter((f) => !f.startsWith('herdr-app/') && !stays.has(f) && !selected.some((e) => matches(f, e)));
  assert.deepEqual(loose, [], 'files with no side: add each to scripts/backend-paths.txt or to the stays list');
  const unused = selected.filter((e) => !files.some((f) => matches(f, e)));
  assert.deepEqual(unused, [], 'list lines that match no tracked file');
});

test('no backend path is inside herdr-app but install.sh', () => {
  assert.deepEqual(lines.filter((l) => l.startsWith('herdr-app/')),
    ['herdr-app/install.sh', 'herdr-app/install.sh==>install.sh']);
});

test('the export script stops on an existing folder', () => {
  const dir = tmp();
  try {
    const r = spawnSync('sh', [script, dir], { encoding: 'utf8' });
    assert.equal(r.status, 1, r.stderr);
    assert.match(r.stderr, /exists/);
    assert.deepEqual(readdirSync(dir), [], 'no clone');
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('the export script stops without git-filter-repo', () => {
  const dir = tmp();
  try {
    // A PATH with only git and dirname: no git-filter-repo.
    const bin = join(dir, 'bin');
    execFileSync('mkdir', [bin]);
    for (const tool of ['git', 'dirname']) {
      const path = execFileSync('sh', ['-c', `command -v ${tool}`], { encoding: 'utf8' }).trim();
      symlinkSync(path, join(bin, tool));
    }
    const out = join(dir, 'out');
    const r = spawnSync('/bin/sh', [script, out], { encoding: 'utf8', env: { ...process.env, PATH: bin } });
    assert.equal(r.status, 1, r.stderr);
    assert.match(r.stderr, /brew install git-filter-repo/);
    assert.equal(existsSync(out), false, 'no clone');
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('the export script needs one out folder', () => {
  for (const args of [[], ['a', 'b'], ['--bad', 'a']]) {
    const r = spawnSync('sh', [script, ...args], { encoding: 'utf8' });
    assert.equal(r.status, 2, `${args}: ${r.stderr}`);
    assert.match(r.stderr, /usage/);
  }
});

test('the message filter of --rewrite-issues names the old repository', () => {
  const filter = (input, slug = 'owner/herdr-contr') =>
    spawnSync('sh', [script, '--filter-message', slug], { input, encoding: 'utf8' });
  const r = filter('Fix the tag (#12) (#13)\n\nSee #7 and #8.\nKeep x#4, a/b#5 and "#9".\n#6 starts a line.\n');
  assert.equal(r.status, 0, r.stderr);
  assert.equal(r.stdout,
    'Fix the tag (owner/herdr-contr#12) (owner/herdr-contr#13)\n\n'
    + 'See owner/herdr-contr#7 and owner/herdr-contr#8.\nKeep x#4, a/b#5 and "#9".\n#6 starts a line.\n');
  const bad = filter('x', 'not a slug');
  assert.equal(bad.status, 1);
  assert.match(bad.stderr, /not an <owner>\/<repo>/);
});

// A temp git repository with the check and the list, a base commit on `base`, and one commit on top that
// changes `file`. Returns the result of `scripts/frozen-check.sh base`.
const frozenCheck = (file) => {
  const dir = tmp();
  try {
    const git = (...args) => execFileSync('git', ['-c', 'user.name=t', '-c', 'user.email=t@t', ...args],
      { cwd: dir, encoding: 'utf8' });
    git('init', '-q', '-b', 'base');
    mkdirSync(join(dir, 'scripts'));
    for (const f of ['scripts/frozen-check.sh', 'scripts/backend-paths.txt']) copyFileSync(join(root, f), join(dir, f));
    git('add', '.');
    git('commit', '-q', '-m', 'base');
    git('switch', '-q', '-c', 'pr');
    mkdirSync(join(dir, dirname(file)), { recursive: true });
    writeFileSync(join(dir, file), 'x\n');
    git('add', '.');
    git('commit', '-q', '-m', 'change');
    return spawnSync('sh', [join(dir, 'scripts/frozen-check.sh'), 'base'], { cwd: dir, encoding: 'utf8' });
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
};

test('frozen-check fails on a change in xplat', () => {
  const r = frozenCheck('xplat/x.rs');
  assert.equal(r.status, 1, r.stderr);
  assert.equal(r.stdout, 'xplat/x.rs\n');
  assert.match(r.stderr, /contr-backend/);
});

test('frozen-check fails on a backend file and on the old install.sh', () => {
  for (const file of ['docs/protocol.md', 'herdr-app/install.sh']) {
    const r = frozenCheck(file);
    assert.equal(r.status, 1, `${file}: ${r.stderr}`);
    assert.equal(r.stdout, `${file}\n`);
  }
});

test('frozen-check passes on a change in herdr-app', () => {
  const r = frozenCheck('herdr-app/mods/herdr-contr/x.js');
  assert.equal(r.status, 0, r.stdout + r.stderr);
  assert.equal(r.stdout, '');
});

test('frozen-check passes on README.md', () => {
  for (const file of ['README.md', 'AGENTS.md', '.github/workflows/ci.yml', 'scripts/land.sh', 'xplat/README.md',
    'kit/README.md']) {
    const r = frozenCheck(file);
    assert.equal(r.status, 0, `${file}: ${r.stdout}${r.stderr}`);
  }
});

test('frozen-check needs one base ref', () => {
  for (const args of [[], ['a', 'b']]) {
    const r = spawnSync('sh', [join(root, 'scripts/frozen-check.sh'), ...args], { encoding: 'utf8' });
    assert.equal(r.status, 2, `${args}: ${r.stderr}`);
    assert.match(r.stderr, /usage/);
  }
  const r = spawnSync('sh', [join(root, 'scripts/frozen-check.sh'), 'no-such-ref-253'], { encoding: 'utf8' });
  assert.equal(r.status, 2, r.stderr);
  assert.match(r.stderr, /no diff/);
});
