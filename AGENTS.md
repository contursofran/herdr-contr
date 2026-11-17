# Working on herdr-contr

herdr-contr is contr for the Herdr desktop app: a layout, a mod (JavaScript)
and a helper program. Each agent tab shows as a conversation, and cards answer
an agent's prompt or question. The orchestrator's questions and workers come
next. The Herdr app gives the window, the sidebar, the tabs and the terminal.

## The backend moved

The helper, the orchestration tool and the kit are in the repository contr-core. Do not change `xplat/`, `kit/`, `web/` and the other paths of `scripts/backend-paths.txt` here. This repository will be archived.
`scripts/frozen-check.sh` checks it on each PR (CI job `frozen`).

## Where the code comes from

- The first commit is a copy of Flockbook (MIT) at one commit; `NOTICE` names
  it. There is no upstream remote and no plan to merge from it (owner,
  2026-11-11). Change, move or delete any file.
- Keep `LICENSE` and the copyright lines.
- The orchestration (worker profiles, spawn, the owner's questions and
  to-dos, the MCP tools, the kit) moves here from the contr repository
  (#160): `xplat/herdr-contr-orch` and `kit/`. The helper's `install` puts both in
  the Herdr home (`herdr-contr/kit`, `herdr-contr/bin`). The tool's files
  (`agents.json`, `owner.json`, `state/`) are in the backend root:
  `CONTR_HOME`, else the Herdr home's `herdr-contr` folder (#255). The
  default root is the neutral folder `~/.config/contr-backend` when it
  exists, else `~/.config/herdr-app/herdr-contr` when it exists (#264). The
  mod reads and changes them only through the tool (#186).

## Goal: one thing at a time (owner, 2026-11-11)

Each step is finished and in use before the next one starts.

1. Make the copy ours: only the Herdr app part and its helper stay, and the
   CI passes.
2. A layout with Herdr's own sidebar, and this mod's chat, cards and reply
   box.
3. Our own names, so that an installed Flockbook and this mod do not collide.
4. The inbox: the orchestrator's questions and to-dos in one list.
5. The orchestrator's workers and its missed messages.

**Frontend on hold (owner, 2026-11-16).** The owner tries contr as an
Electron app (repository `contr-desktop`), which uses this repository's
helper, orchestration tool and kit. Until the owner decides, do no new work
in `herdr-app/mods`, `herdr-app/layouts` and `herdr-app/themes`: work only on
the helper, `herdr-contr-orch`, `kit/` and the release. Issues for the mod
have the label `frontend-hold`. A bug that stops the owner's daily work in
the mod is still fixed.

Not in scope: a files panel and a pull request reader. The sidebar is
reworked after the owner's design (owner, 2026-11-13), in slices: #96 one
Projects list (each project with its agents and their workers), #97 the head
and the foot, #98 the hover controls and the model tooltip. The agent icons
stay ours. The top bar is our copy of Herdr's `tabs` mod.

## Layout

| Path | Side | Holds |
|---|---|---|
| `herdr-app/layouts`, `herdr-app/themes`, `herdr-app/keys.json` | mod | the layouts (`herdr-contr` with Herdr's sidebar, and `herdr-contr-own` with the mod's own sidebar), the themes and the keys that the mod installs |
| `herdr-app/mods/herdr-contr` | mod | the mod: one file for each screen or part |
| `herdr-app/mods/herdr-contr-spaces` | mod | the sidebar's Projects list (#96): a copy of Herdr's `spaces` mod that also draws each project's agents and their workers, with the hover controls and the model tooltip (#98); also the sidebar's head with the machine switch (#97) |
| `herdr-app/mods/herdr-contr-tabs` | mod | a copy of Herdr's `tabs` mod, with the braille loader (the agents copy went in #98) |
| `herdr-app/test` | mod | the mod's tests, with a stub of Herdr's UI in `test/stub` |
| `herdr-app/RECIPE.md`, `herdr-app/README.md`, `herdr-app/preview*.png` | mod | each part of the mod in words: sizes, colors, behavior; the mod's readme and its pictures |
| `herdr-app/install.sh`, `herdr-app/install-isolated.sh` | mod | the install into a Herdr home, and into a test home; `install.sh` is also the release zip's installer |
| `scripts/sync-copies.mjs` | mod | writes the functions that one mod copies from another |
| `docs/protocol.md` | backend | the contract of the helper, which the mod and contr-desktop use (JSON Lines) |
| `xplat/herdr-contr-feed` | backend | the helper (Rust): reads the agents' session logs and talks to hc; its `install`, `build.rs` and `health` also hold mod parts, behind the Cargo feature `herdr-mod` (on by default) |
| `xplat/herdr-core` | backend | the helper's hc client, its parsers and `places` |
| `xplat/herdr-contr-orch` | backend | the orchestration tool (Rust), ported from contr slice by slice (#160); its parts are in the list below the table |
| `kit`, `.claude-plugin/marketplace.json` | backend | the Claude Code plugin of a coordinator and its workers: the agent definitions, the playbook (`commands/orchestrate.md`), the chat-writing skill, the hooks and the MCP server, which run `herdr-contr-orch`; `.claude-plugin/marketplace.json` at the root names it for Claude Code |
| `xplat/fixtures` | backend | the session logs and hub answers that the helper's tests read |
| `xplat/build-stamp.rs` | backend | the build stamp of the helper and the tool |
| `xplat/herdr-contr-feed/web` | backend | the phone page that the helper embeds (built output; do not edit) |
| `web`, `scripts/web-build.mjs`, `scripts/sw-test.mjs` | backend | the phone page's source, its build into `xplat/herdr-contr-feed/web`, and its service worker test |
| `docs/design`, `docs/research` | backend | the notes on the phone page's protocol and on the question keys, that the helper's code names |
| `scripts/backend-paths.txt`, `scripts/export-backend.sh`, `scripts/backend-paths.test.mjs` | both | the paths that move to the repository `contr-backend`, and the script that makes its history with `git filter-repo` in a new folder, with the checks before the first push (#253); the test gives each tracked file a side |
| `scripts/frozen-check.sh` | mod | the check that a PR changes no backend path (CI job `frozen`, #253) |
| `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `scripts/land.sh` | both | the CI, the release and the landing of a PR: one job and one zip for both sides |

A backend part does not read a file of `herdr-app/` at build time or at run time.
The CI job `backend` checks it.

The parts of `xplat/herdr-contr-orch`:

- the profiles and the flow
- the owner file and its open questions and to-dos as JSON (`owner open --json`)
- the profiles file as JSON and its changes (`agents config --json`, `agents set`)
- the worker commands on hc (`spawn`, `resume`, `ls`, `title`, `send`, `interrupt`, `stop`, `rm`, `read`, `prompt`)
- the MCP server with the coordinator's tools and a worker's `report` tool (`mcp`)
- the reports held for a busy coordinator (`held`)
- the limits of spawn and send: workers at once and open PRs (`limit`)
- the hooks of a worker and of a coordinator (`hook stop`, `hook session-start`, `hook prompt`)
- the check of a PR against the brief's boundary (`pr check-boundary`)

Herdr's API for mods is in the Herdr app's home: `AGENTS.md`, `herdr.d.ts`
and `gpui-kit.d.ts`. Read them there. Do not copy them into this repository.

## Checks

- Mod: `cd herdr-app && npm test`.
- Helper: `cargo test --locked --manifest-path xplat/Cargo.toml -p herdr-contr-feed -p herdr-core -p herdr-contr-orch`.
- Backend without the mod (the CI job `backend`, which first removes `herdr-app/`):
  `cargo test --locked --no-default-features --manifest-path xplat/Cargo.toml -p herdr-contr-feed -p herdr-core -p herdr-contr-orch`.
  The Cargo feature `herdr-mod` of `herdr-contr-feed` (on by default) holds the mod's parts of the helper.
- All must pass, and the `CI` check on the PR, before you report done.

## Design rules (lessons from contr)

- **Do not rebuild what Herdr or Claude Code draws.** Use Herdr's blocks
  (sidebar, tabs, terminal, settings) as they are. The owner types in the
  real terminal or in the reply box that is there. Add a part only when
  Herdr and the mod have nothing for it, and write why in the PR.
- **Data comes from hooks, session logs and APIs, not from the screen.** A
  new feature takes its data from a Claude Code hook, the session log, hc's
  API or the orchestration tool. Terminal text is only for the guard that
  checks the prompt before a key goes out.
- **One inbox.** All that needs the owner is in one list, with one policy
  for each kind of action (`auto` or `ask`). Do not add a second channel for
  the same thing. An answer in any place closes the item.
- **State comes from the source.** Show what GitHub, hc and the
  orchestration tool say now. Do not keep a state file that someone writes by hand.
- **No file that each PR appends to.** A decision is one file:
  `docs/decisions/<PR number>-<slug>.md`. Do not keep a table that each PR
  edits. Register each test fixture in a file of its own.
- **The life of a PR is code.** Worktree, CI wait, merge and cleanup are
  commands, not steps that a model does by hand.
  `scripts/land.sh <pr> [--after <pr>]` lands a PR: update, CI wait,
  squash merge of the exact head. It installs nothing: the backend moved.
  `node scripts/sync-copies.mjs` writes the functions that one mod copies
  from another; `npm test` fails when a copy differs.
- **One thing at a time.** Do not start a new screen while the one before
  it is not in use by the owner.

## Releases

- `.github/workflows/release.yml` runs after the `CI` check passes on a push
  to `main`. It builds the helper for macOS (universal) and Linux (musl,
  x86_64 and aarch64) and the orchestration tool for macOS. It publishes
  one zip as the release `build-<commit date>-<short sha>`, and deletes all
  but the last 10 `build-` releases.
- The zip holds `install.sh`, `bin/`, `remote/<target>/` and
  `remote/SHA256SUMS`. On the other machine: unzip, then `./install.sh`.
- The build stamp (`<short sha> <commit date>`) comes from
  `HERDR_CONTR_BUILD_SHA` and `HERDR_CONTR_BUILD_DATE`, else from git
  (`xplat/build-stamp.rs`).
- To sign the macOS binaries, set the secrets `MACOS_CERT_P12` (a
  Developer ID Application certificate as base64 `.p12`),
  `MACOS_CERT_PASSWORD` and `MACOS_SIGN_IDENTITY`. Without them the
  release is unsigned and its notes say so.
- Run the workflow by hand (`workflow_dispatch`) to test it on a branch.

## Safety

- The owner's Herdr app and hc core are live. Do not edit files in the
  owner's Herdr home (`~/.config/herdr-app`) or in the neutral backend root
  (`~/.config/contr-backend`) by hand. Never send input to the owner's Herdr
  app. Never stop, restart or send input to the owner's hc core.
- No agent runs `herdr-contr-feed home move`, with or without `--dry-run`,
  on the owner's files. The owner runs it (#264). Tests run it in a
  temporary HOME.
- Never start `herdr-app` (owner, 2026-11-12). It is one app per Mac: a
  second start, even with a test home and a scratch `HC_SESSION`, replaces
  the owner's window. Check the mod with `npm test`. The coordinator looks at
  merged `main` in the owner's app (see below).
- An agent can install merged `main` into the real Herdr home with the
  helper's `install` (owner, 2026-11-12): build the helper and the
  orchestration tool, run `install --dry-run` first, then `install --helper
  <file> --orch <file>`. After it, `herdr-app do
  layout.switch` and `herdr-app shot` are allowed, to look at the result.
  Do not install a branch that is not merged.
  On a machine with no Herdr app, `install --backend` is the form: it
  writes only the kit and the programs into the backend root.
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
- Squash merge. The coordinator merges after CI passes on the exact head,
  without the owner's approval (owner, 2026-11-12). A builder does not merge
  its own PR.
- Docs, comments and commit messages: concise ASD-STE100 Simplified
  Technical English. No `Co-Authored-By`, AI attribution or generated-by
  lines.
- Write no local paths (such as `/Users/...`) in the repository or in commit
  messages. Do not @-mention people. Name other GitHub users or repositories
  only in `NOTICE`, `THIRD_PARTY_NOTICES.md`, `CONTRIBUTORS.md` and
  `LICENSE`.
- Temporary files go in `tmp/`, which git ignores.
- When a change makes a doc wrong (`herdr-app/RECIPE.md`,
  `docs/protocol.md`, this file), update it in the same PR.
- Report the PR number, the head SHA, the check status and any deviation.
