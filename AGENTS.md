# Working on herdr-contr

herdr-contr is contr for the Herdr desktop app: a layout, a mod (JavaScript)
and a helper program. Each agent tab shows as a conversation, and cards answer
an agent's prompt or question. The orchestrator's questions and workers come
next. The Herdr app gives the window, the sidebar, the tabs and the terminal.

## Where the code comes from

- The first commit is a copy of Flockbook (MIT) at one commit; `NOTICE` names
  it. There is no upstream remote and no plan to merge from it (owner,
  2026-11-11). Change, move or delete any file.
- Keep `LICENSE` and the copyright lines.
- The orchestration (worker profiles, spawn, the owner's questions and
  to-dos, the MCP tools) lives in the contr repository: `kit/`,
  `contr agents`, `contr mcp`. This repository reads that data through the
  `contr` command. It does not copy that code.

## Goal: one thing at a time (owner, 2026-11-11)

Each step is finished and in use before the next one starts.

1. Make the copy ours: only the Herdr app part and its helper stay, and the
   CI passes.
2. A layout with Herdr's own sidebar, and this mod's chat, cards and reply
   box.
3. Our own names, so that an installed Flockbook and this mod do not collide.
4. The inbox: the orchestrator's questions and to-dos in one list.
5. The orchestrator's workers and its missed messages.

Not in scope: a files panel and a pull request reader. Herdr's sidebar stays
as Herdr draws it, for now.

## Layout

| Path | Holds |
|---|---|
| `herdr-app/layouts`, `herdr-app/themes`, `herdr-app/keys.json` | the layouts (`flockbook`, and `flockbook-herdr` with Herdr's sidebar), the themes and the keys that the mod installs |
| `herdr-app/mods/flockbook` | the mod: one file for each screen or part |
| `herdr-app/test` | the mod's tests, with a stub of Herdr's UI in `test/stub` |
| `herdr-app/RECIPE.md` | each part of the mod in words: sizes, colors, behavior |
| `herdr-app/docs/protocol.md` | the contract between the mod and the helper (JSON Lines) |
| `herdr-app/install.sh`, `herdr-app/install-isolated.sh` | the install into a Herdr home, and into a test home |
| `xplat/flockbook-modfeed` | the helper (Rust): reads the agents' session logs and talks to hc |
| `xplat/herdr-core` | the helper's hc client |
| `xplat/fixtures` | the session logs and hub answers that the helper's tests read |
| `xplat/flockbook-modfeed/web` | the phone page that the helper embeds (built output; do not edit) |
| `web`, `scripts/web-build.mjs`, `scripts/sw-test.mjs` | the phone page's source, its build into `xplat/flockbook-modfeed/web`, and its service worker test |
| `docs/design`, `docs/research` | the notes on the phone page's protocol and on the question keys, that the helper's code names |

Herdr's API for mods is in the Herdr app's home: `AGENTS.md`, `herdr.d.ts`
and `gpui-kit.d.ts`. Read them there. Do not copy them into this repository.

## Checks

- Mod: `cd herdr-app && npm test`.
- Helper: `cargo test --locked --manifest-path xplat/Cargo.toml -p flockbook-modfeed -p herdr-core`.
- Both must pass, and the `CI` check on the PR, before you report done.

## Design rules (lessons from contr)

- **Do not rebuild what Herdr or Claude Code draws.** Use Herdr's blocks
  (sidebar, tabs, terminal, settings) as they are. The owner types in the
  real terminal or in the reply box that is there. Add a part only when
  Herdr and the mod have nothing for it, and write why in the PR.
- **Data comes from hooks, session logs and APIs, not from the screen.** A
  new feature takes its data from a Claude Code hook, the session log, hc's
  API or the `contr` command. Terminal text is only for the guard that
  checks the prompt before a key goes out.
- **One inbox.** All that needs the owner is in one list, with one policy
  for each kind of action (`auto` or `ask`). Do not add a second channel for
  the same thing. An answer in any place closes the item.
- **State comes from the source.** Show what GitHub, hc and the `contr`
  command say now. Do not keep a state file that someone writes by hand.
- **No file that each PR appends to.** A decision is one file:
  `docs/decisions/<PR number>-<slug>.md`. Do not keep a table that each PR
  edits. Register each test fixture in a file of its own.
- **The life of a PR is code.** Worktree, CI wait, merge and cleanup are
  commands, not steps that a model does by hand.
- **One thing at a time.** Do not start a new screen while the one before
  it is not in use by the owner.

## Safety

- The owner's Herdr app and hc core are live. Never write to the owner's
  Herdr home (`~/.config/herdr-app`). Never send input to the owner's Herdr
  app. Never stop, restart or send input to the owner's hc core.
- Try the mod only in a test home: `herdr-app/install-isolated.sh <folder>`
  and `HERDR_APP_HOME=<folder>`, with a scratch `HC_SESSION` and scratch XDG
  folders. If you cannot be sure that a run uses the scratch core, do not
  start it. Report that instead.
- Only the owner installs into the real Herdr home.
- Tests use fakes and fixtures. No test reads the owner's session logs
  (`~/.claude`, `~/.codex` and the others), and no test starts the real
  `claude` program.
- Do not turn on remote access (the phone page) on the owner's Mac.
- On the owner's Mac, run one heavy job at a time: a Herdr app run or a
  release build.

## Testing

- Add a test for each new behavior, error paths included.
- Fixtures look like real Claude Code sessions.
- The tests run against a stub of Herdr's UI, so they do not show how a
  screen looks. For a visible change, show the screen in a test home and
  look at the image yourself. Give its path in your report.

## Workflow

- Work is tracked in GitHub issues and pull requests. One issue, one branch
  `agent/<issue>-<slug>`, one PR into `main`, ideally under about 400
  changed lines. A PR that only deletes can be larger.
- Update a branch by merging `main` into it. Never force-push.
- Squash merge. The coordinator merges after CI passes on the exact head. A
  builder does not merge its own PR.
- Docs, comments and commit messages: concise ASD-STE100 Simplified
  Technical English. No `Co-Authored-By`, AI attribution or generated-by
  lines.
- Write no local paths (such as `/Users/...`) in the repository or in commit
  messages. Do not @-mention people. Name other GitHub users or repositories
  only in `NOTICE`, `THIRD_PARTY_NOTICES.md`, `CONTRIBUTORS.md` and
  `LICENSE`.
- Temporary files go in `tmp/`, which git ignores.
- When a change makes a doc wrong (`herdr-app/RECIPE.md`,
  `herdr-app/docs/protocol.md`, this file), update it in the same PR.
- Report the PR number, the head SHA, the check status and any deviation.
