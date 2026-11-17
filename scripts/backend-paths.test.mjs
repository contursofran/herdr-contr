// Checks scripts/backend-paths.txt against the tracked files, and the guards
// and the message filter of scripts/export-backend.sh (#253).
// Run: node --test scripts/backend-paths.test.mjs
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, readdirSync, readFileSync, rmSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
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
