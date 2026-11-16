# t3code: chat view and desktop shell (research for #229)

Read-only research. Source: `pingdotgg/t3code`, shallow clone at commit `b5554ac8`
(2026-11-16), MIT, Copyright (c) 2026 T3 Tools Inc. Paths below are relative to that repository.
Line counts are `wc -l` of the file at that commit. "Not run": I read the code and did not
build or run t3code.

## Summary

- The chat is a large React 19 + Tailwind 4 app, not a library. `apps/web/src` is about 59 000
  lines in its chat parts alone (`ChatView.tsx` 4 086, `ChatComposer.tsx` 3 849).
- Almost every chat file imports `@t3tools/contracts` (Effect Schema types), `@t3tools/client-runtime`
  (Effect atoms), a state module (`~/state/*`), or the router. A whole-file copy brings the
  server protocol with it.
- The parts that lift out well are the **leaf renderers** and the **pure logic** behind them:
  the markdown pipeline, code block highlighting, the link chips, the work-log row layout, the
  UI primitives and the theme tokens. We copy those and write the data layer ourselves.
- The terminal is not xterm.js. t3code ships its own Ghostty WASM canvas terminal and uses
  `node-pty` on its server.
- Recommendation: copy about 3 000 to 4 500 lines, write about 1 500 lines (see section 4).

## 1. The chat view's files

Difficulty: **easy** = no t3code state, takes props, copy and restyle. **medium** = needs a few
imports replaced or a small data shim. **hard** = tied to their server protocol, stores or
router; write our own instead.

All paths are under `apps/web/src/` unless the path starts with `packages/`.

### Message list

| Part | Path (lines) | Depends on | Lift |
|---|---|---|---|
| Timeline (the list) | `components/chat/MessagesTimeline.tsx` (1 891) | `@legendapp/list` (virtual list), `@t3tools/contracts`, `client-runtime` (environment, subagent runtime, work-log, worktree-setup), `session-logic`, `~/state/threads`, find, minimap, citations | hard: 35 sibling imports; built for their thread model and virtualization |
| Row building | `components/chat/useTimelineRows.ts`, `MessagesTimeline.logic.ts`, `session-logic.ts` (1 155) | `contracts` v2 turn items | hard: turns their events into rows; our `row` lines replace it |
| Scroll follow, anchoring | `components/chat/useTimelineFollow.ts`, `timelineScrollAnchoring.ts`, `timelineViewport.ts`, `pageScrollController.ts` | DOM only (to confirm per file) | medium: good ideas for stick-to-bottom, tied to Legend List |
| Assistant row | `components/chat/TimelineAssistantRows.tsx` (506) | `ChatMarkdown`, effect atoms, `state/entities`, `state/vcs`, checkpoints, plan card, MCP app frame | medium: the markup is simple, the imports are not. Rewrite on top of `ChatMarkdown` |
| User row | `components/chat/TimelineUserMessageRow.tsx` (1 237) | `state/*`, router, attachments, media, context chips | hard: most of it is attachments and context references we do not have |
| Row parts | `components/chat/TimelineRowParts.tsx` (55) | small | easy |
| Dividers, status | `TimelineSystemDivider.tsx`, `ThreadStatusLine.tsx` | few | easy |

### Markdown rendering

| Part | Path (lines) | Depends on | Lift |
|---|---|---|---|
| Pipeline (remark and rehype plugin lists, sanitize, GitHub alerts, list indent, math) | `packages/shared/src/markdownPipeline.ts` (889), `markdownLinks.ts` (533), `markdownGithubAlerts.ts`, `markdownListIndentation.ts`, `markdownOrderedListNumbers.ts`, `markdownWindowsPaths.ts` | `unified`, `remark-*`, `rehype-raw`, `rehype-sanitize`; `@t3tools/contracts` for the local-file link schemes only | medium: plain unified code; cut the contracts import and the thread-link and Windows-path plugins |
| Renderer (hast to React, with a cache of parsed elements) | `components/chat/markdownRenderer.ts` (114), `markdownSyntaxGates.ts` | `hast-util-to-jsx-runtime`, `unified`, `vfile` | easy |
| Streaming (parse only the changed block) | `markdown-incremental.ts` (98), `components/chat/markdownStreamBlocks.ts` | none heavy | easy; needed when the log grows while the agent writes |
| The component map (`ChatMarkdown`) | `components/ChatMarkdown.tsx` (1 924) | 60 imports: `contracts`, effect atoms, `~/state/*` (assets, filesystem, preview, projects, session), router-adjacent hooks, editor preferences, `localApi` | hard as a file; **easy as a pattern**. The file maps each element (`a`, `code`, `table`, `img`) to a component. We write a 200-line map and copy the leaf components |
| Table | `components/chat/MarkdownTable.tsx` (70) | `ui/button`, `ui/menu`, `ui/scroll-area`, copy hook | easy |
| Math, Mermaid | `MarkdownMath.tsx`, `MathTypeset.tsx`, `MarkdownMermaidCodeBlock.tsx`, `MermaidDiagram.tsx` (320) | `katex`, mermaid | easy, optional; our helper already renders mermaid to a picture |

### Code blocks

| Part | Path (lines) | Depends on | Lift |
|---|---|---|---|
| Code block shell (header, language, copy, wrap) | `components/chat/MarkdownCodeBlock.tsx` (372), `MarkdownCodeBlock.logic.ts` (125) | `ui/button`, `ui/tooltip`, copy hook, settings hook, `pierre-icons`, LRU cache | easy: replace `useSettings` and icons |
| Highlighting | `lib/syntaxHighlighting.ts` (44), `lib/incrementalHighlighting.ts` (108), `components/chat/HighlightedCodeLines.tsx` (46), `HighlightedTokens.tsx` (121) | `@pierre/diffs` (WASM "highlights" lexers, or Shiki), `hast-util-to-jsx-runtime` | easy to medium: one new dependency (`@pierre/diffs`) |
| Diff view | `components/DiffPanel.tsx`, `components/diffs/*` | `@pierre/diffs`, their git RPC | out of scope |
| Shell command block | `components/chat/ShellCommandBlock.tsx` (64) | highlighting, a label helper from `client-runtime` | easy |

### Inline references (PR, issue, file, thread links)

| Part | Path (lines) | Depends on | Lift |
|---|---|---|---|
| External link with a host icon | `components/chat/MarkdownExternalLink.tsx` (117), `sourceControlPresentation.tsx` (60) | `client-runtime` source-control clients (host to icon), favicon helper | easy: it is a link plus a 14 px icon. Swap the host lookup for ours |
| Change-request (PR) link parsing | `packages/client-runtime/src/changeRequestLinks.ts` (170) | `client-runtime` only | easy: pure parsing of a PR URL into host, repo, number |
| PR state icons and tones | `components/pullRequest/pullRequestIcons.tsx` (54) | `lucide-react`, the `PullRequestState` type | easy: open green, draft grey, closed red, merged violet, as Tailwind classes. This is the answer to the inline PR icon need |
| PR hover preview | `components/pullRequest/PullRequestLinkPreview.tsx` (254) | their pull-request RPC | hard: needs their server; we feed state from our helper |
| File link chip | `components/chat/MarkdownFileLink.tsx` (452), `MarkdownFileLink.logic.ts`, `ContextChip.tsx` (102), `FileTagChip.tsx` | `localApi`, `state/*`, `contracts`, `pierre-icons` | medium: the chip is easy; opening a file is their `localApi` |
| Thread link | `components/chat/MarkdownThreadLink.tsx` (71) | their thread routes | skip |
| Chip punctuation, spacing | `components/chat/markdownChipPunctuation.ts` | none | easy |

Issue links: t3code has PR (change request) chips and generic host chips. It has no issue
chip colored by state; we add that ourselves from the same pattern (about 60 lines).

### Tool-call rows

| Part | Path (lines) | Depends on | Lift |
|---|---|---|---|
| Work entry row | `components/chat/TimelineWorkEntryRow.tsx` (679), `TimelineWorkEntryRow.logic.ts` | `session-logic`, `state/entities`, router, `client-runtime` work-log presentation, V2 inspector | hard: tied to their `WorkLogEntry` (30 fields) |
| Work group (collapsed "Ran 3 commands") | `components/chat/TimelineWorkGroupRows.tsx` (710), `WorkLog.tsx` (170) | `@legendapp/list`, `session-logic`, popover, scroll area | medium: the layout is the model for our `tools` chips |
| Tool icons | `components/chat/TimelineToolActivityIcon.tsx` (276) | `lucide-react`, asset URLs, `shared/toolActivity` | medium: an icon map; reuse the idea, map our `symbol` names |
| Tool classification | `packages/shared/src/toolActivity.ts` (474) | `contracts` | medium: names a tool call's surface and icon; ours already gives `symbol` |
| Approval row | `TimelinePendingApprovalRow.tsx`, `ComposerPendingApprovalActions.tsx` | their approval RPC | hard: our `perm` and `question` cards stay ours |

### Composer

| Part | Path (lines) | Depends on | Lift |
|---|---|---|---|
| Composer shell | `components/chat/ChatComposer.tsx` (3 849) plus about 70 `Composer*` and `useComposer*` files | `composerDraftStore`, `contracts`, effect, router, speech, attachments, 100+ imports | hard: do not copy |
| Rich editor | `components/ComposerPromptEditorTiptap.tsx` (2 009), `composer-*.ts` | Tiptap 3 (14 packages), `shared`, `state/server`, keybindings | hard; also more than we need |
| Send button, controls bar | `ComposerPrimaryActions.tsx` (506), `ComposerControlsBar.tsx` (312) | their state | medium for looks, hard for logic |

Our reply box needs a text area, Enter to send, attachments by paste, and the guard from the
mod. About 250 lines of our own React is cheaper than lifting the editor.

### Model picker

| Part | Path (lines) | Depends on | Lift |
|---|---|---|---|
| Picker trigger | `components/chat/ProviderModelPicker.tsx` (495), `.logic.ts` | `state/server`, provider instances, effect atoms, popover | hard |
| Picker list | `components/chat/ModelPickerContent.tsx` (1 090), `ModelListRow.tsx`, `ModelPickerSidebar.tsx` | `@legendapp/list`, `client-runtime/model-search`, `contracts`, settings, router | hard |

Our helper reports `model` only as text (`log.model`, the `model` command). A model picker
changes the agent's model through the agent's own slash command, which is the mod's job. Skip
it in the spike. Show the model name as a label.

### UI primitives and tokens

| Part | Path (lines) | Depends on | Lift |
|---|---|---|---|
| Primitives (button, tooltip, popover, menu, scroll area, dialog, toast, kbd, badge…) | `components/ui/*` (50 files; `tooltip.tsx` 164) | `@base-ui/react`, `class-variance-authority`, `tailwind-merge`, `lucide-react`, `lib/utils.ts` (55) | easy: copy the few we use. Base UI is a normal npm package |
| Tokens and theme | `index.css` (2 642 lines, about 375 custom properties), `themeCss.ts`, `themePalette.ts` | Tailwind 4 `@theme` | medium: copy the token block only, not the 2 600 lines |

## 2. The desktop shell

Path: `apps/desktop`. Electron `44.10.1`, `electron-builder 26.16.1`, `electron-updater 6.8.9`,
`electron-store`. It is written in Effect: every module is a service layer.

| Part | Path | Notes | Lift |
|---|---|---|---|
| Main entry | `src/main.ts` (267), `src/boot.ts`, `src/app/DesktopApp.ts` | Boots an Effect layer graph; 40 files in `src/app` (lifecycle, identity, user data, startup recovery, CLI shim, Linux URL handler, Clerk auth) | hard: copy the ideas, not the files |
| Window | `src/window/DesktopWindow.ts`, `DesktopApplicationMenu.ts`, `QuitHold.ts` | `titleBarStyle: "hiddenInset"` on macOS, `"hidden"` elsewhere, traffic light sync | medium: about 100 lines of it are worth reading and rewriting |
| Preload | `src/preload.ts` (528) | `contextBridge` API for the web app: IPC channels in `src/ipc/channels.ts` and `src/ipc/methods` | medium: our own preload is about 40 lines |
| Backend child | `src/backend/DesktopBackendManager.ts`, `DesktopBackendPool.ts` | Starts their Node server as a child process and gives the web app its URL and a token | hard: this is the part we replace with our helper |
| Packaging | `scripts/build-desktop-artifact.ts` (root `scripts/`), `apps/desktop/package.json`, `linux-after-pack.ts`, Windows signing (`jsign`) | `electron-builder` config in `package.json`; macOS, Windows, Linux AppImage; AUR files in `packaging/` | medium: copy the builder config shape, write a small script |
| Auto-update | `src/updates/*` (state machine, channels, GitHub feed, release notes), `src/electron/ElectronUpdater.ts` | `electron-updater` over a GitHub release feed; a state machine drives the toast in the web app | medium: we have a release workflow already; defer updates past the spike |
| Dev runner | `scripts/dev-electron.mjs`, `wait-for-resources.mjs`, `electron-launcher.mjs` | Vite dev server plus Electron | easy |
| Electron wrappers | `src/electron/*` (dialog, menu, safe storage, theme, shell, protocol, power) | thin Effect wrappers around Electron modules | skip: call Electron directly |

### Terminal

t3code has **no xterm.js**. Its terminal has two halves:

- **Renderer (browser):** `apps/web/src/terminal/ghostty/` (about 8 files). It runs the official
  `libghostty-vt` C library compiled to WebAssembly (`vendor/ghostty-write-pty.wasm` and a build
  script, `build:ghostty-wasm`), draws with Canvas 2D (`renderer.ts`), and owns input, IME,
  selection and links (`surface.ts`). Its README says it is "intentionally not an xterm
  compatibility layer". It needs a build of the WASM from `native/libghostty-vt/`.
- **Server:** `apps/server/src/terminal/` (`Manager.ts`, `NodePtyAdapter.ts`,
  `OutputProtocol.ts`) uses `node-pty` `^1.2.0-beta.15` for local PTYs.

For us the terminal is an hc terminal, not a local PTY. Neither half fits as it is: we need a
renderer that takes hc's output stream and sends keys through hc. xterm.js (which the issue
names) is the cheaper choice for the spike. Reading the Ghostty adapter is still useful if
xterm.js feels worse than Herdr's.

## 3. Data flow: agent to chat

```
agent (Claude Code, Codex, ...)
  -> provider adapter in the server       apps/server/src/orchestration-v2/Adapters
  -> turn items, stored and projected     apps/server/src/orchestration-v2
  -> WebSocket RPC "orchestration"        apps/server/src/ws.ts (3 722 lines)
  -> client-runtime atoms (Effect)        packages/client-runtime, apps/web/src/state/*
  -> session-logic builds the timeline    apps/web/src/session-logic.ts
  -> MessagesTimeline rows                apps/web/src/components/chat/*
```

- **Server.** t3code runs a Node/Effect server (`apps/server`). It starts the agent itself
  through provider adapters, so it owns the conversation. Its events are `turn-item.updated`,
  `provider-turn.updated` and others in `packages/contracts/src/orchestrationV2.ts` (4 616
  lines, Effect Schema, forward-compatible unions on `type`).
- **Message shape.** `ChatMessage` (`apps/web/src/types.ts:75`): `id`, `role`
  (`user | assistant | system`), `text` (markdown), `attachments`, `runId`, `streaming`,
  `createdAt`, `updatedAt`, plus actor and source fields. Tool calls are separate
  `WorkLogEntry` values (`session-logic.ts:72`) with `label`, `detail`, `command`,
  `changedFiles`, `tone`, `toolTitle`, `toolCallId`, `toolIcon`, `toolLifecycleStatus`, and more.
  A `TimelineEntry` (`types.ts:109`) is a union of message, proposed plan, HTML render, sent
  message and others, in time order.
- **Client.** The web app reads the stream through Effect atoms (`@effect/atom-react`). The chat
  components do not take plain props; they read atoms.

Our side (`herdr-app/docs/protocol.md`): the helper does not start the agent. It reads the
agent's session log and answers with JSON Lines: `log`, then `unchanged`, or `splice` and `row`
lines. Row kinds: `user`, `assistant`, `tools`, `perm`, `question`, `status`, `sep`, `footer`.
Each row has a stable `id` and `ts`.

### Mapping our rows to what their leaf components need

| Our row | Feeds | What the Electron app does |
|---|---|---|
| `user` `{text, ts, images, from}` | a user bubble (written by us, or the markdown component only) | render `text` as markdown; `from` gives the one-line note |
| `assistant` `{text}` | the markdown renderer + code block + link chips | pass `text` straight in; this is the best fit |
| `tools` `{chips[{symbol,text,calls[{name,summary,done,error,output}]}]}` | a group row like `TimelineWorkGroupRows` | map `symbol` to a Lucide icon; show `calls` when open |
| `perm`, `question` | our own cards | write ourselves (the mod's card logic) |
| `status`, `sep`, `footer` | divider rows | write ourselves, small |

What our helper would have to emit that it does not emit now:

1. **PR and issue references with state.** The text has `#667` or a GitHub URL; no state.
   Two ways: (a) the helper gets a new command (for example `refs --repo o/n --numbers 1,2`)
   that asks `gh` and answers `ref` lines `{kind: "pr"|"issue", number, state:
   "open"|"draft"|"closed"|"merged", title, url}`; (b) the app asks GitHub itself.
   Choose (a): it keeps one source and it already sits where `gh` and the cache are.
   The app then replaces `#667` and PR URLs in the markdown with a chip (a rehype
   plugin, about 80 lines) and colors it from `PULL_REQUEST_STATE_PRESENTATION`.
2. **A stream, not a poll.** `feed` is one command per call and exits. The Electron app can
   call it on a timer (about 1 s) to start. A long-lived `feed --watch` that prints
   `splice` lines as the log grows would be better; this is a helper change (not for the
   spike).
3. **Terminal bytes.** For xterm.js the app needs hc's terminal output and input over a
   socket. The helper has `herdr-core` (the hc client). The app can spawn a small command or
   connect from Node. This is a separate step.
4. **Nothing else.** Rows already carry `text` as markdown, which is what t3code's pipeline
   expects.

The Electron main process runs the helper with `child_process` (same `exec` contract as the
mod: one JSON Lines answer per call), and sends rows to the renderer over IPC.

## 4. Smallest spike: copy set and what we write

Goal: one agent's chat in an Electron window, fed by `herdr-contr-feed`, with inline PR and
issue icons.

### Copy (with the MIT notice)

| From t3code | Lines | Change |
|---|---|---|
| `markdownPipeline.ts`, `markdownLinks.ts` (parts), GitHub alerts, list plugins | about 900 of 1 500 | cut contracts, thread-link and Windows-path plugins |
| `markdownRenderer.ts`, `markdownSyntaxGates.ts`, `markdown-incremental.ts`, `markdownStreamBlocks.ts` | about 350 | none |
| `MarkdownCodeBlock.tsx` + `.logic.ts`, `HighlightedCodeLines.tsx`, `HighlightedTokens.tsx`, `syntaxHighlighting.ts`, `incrementalHighlighting.ts` | about 700 | replace settings, icons and theme hooks |
| `MarkdownTable.tsx`, `MarkdownExternalLink.tsx`, `markdownChipPunctuation.ts`, `ContextChip.tsx` | about 300 | replace the host lookup |
| `pullRequestIcons.tsx`, `changeRequestLinks.ts` | about 220 | none; add an issue map |
| `ui/` button, tooltip, scroll-area, popover, `lib/utils.ts` | about 500 | drop unused variants |
| Token block from `index.css`, `themeCss.ts` | about 400 | only the variables we use, dark and light |
| Dev runner idea, `electron-builder` config shape | about 100 | rewritten |

Total: about 3 000 to 3 500 lines copied, 12 to 15 npm dependencies (React, `unified` and
`remark`/`rehype` set, `hast-util-to-jsx-runtime`, `@pierre/diffs`, `@base-ui/react`,
`class-variance-authority`, `tailwind-merge`, `lucide-react`, Tailwind 4, Vite, Electron).

### Write ourselves

| Part | Lines (rough) |
|---|---|
| Electron main: window, helper runner, IPC, preload | 250 |
| Feed client in the renderer: apply `splice` and `row` to a list | 150 |
| `ChatMarkdown` component map (`a`, `code`, `table`, `img`) | 200 |
| Rows: user bubble, assistant, tools group, footer, divider, status | 450 |
| PR and issue chip (rehype plugin + component + state colors) | 150 |
| `refs` command in the helper (Rust) + protocol doc | 200 |
| Reply box (text area, Enter, paste image) and send through `send` | 250 |
| Perm and question cards | 300 |
| Playwright harness: start the app against a fixture helper, take a screenshot | 150 |
| xterm.js pane for an hc terminal (a second step, after the chat) | 300 |
| Build script and package config | 100 |

Total: about 2 000 to 2 500 lines for the chat spike, 300 more for the terminal.

### Order for the spike

1. Window + fixture feed + assistant markdown with code blocks. Screenshot with Playwright.
2. PR and issue chips in text, colored by state (the pass/fail test from #229).
3. Tools group, user row, footer.
4. Reply box and `send`.
5. xterm.js pane.

### Risks

- `@pierre/diffs` pulls WASM lexers; check it loads in a packaged Electron app.
- Tailwind 4 plus the t3code token names: copy the variables we use and rename them to ours.
- t3code changes fast (a commit a day). Copy once at a pinned commit and keep that SHA in the
  notice. Do not plan to merge from upstream.
- Memory and start time of a React + Electron app is what #229 measures; t3code's own
  numbers are not known (not run).

## 5. Licence notes

t3code is MIT (`LICENSE`: "Copyright (c) 2026 T3 Tools Inc."). MIT needs the copyright line and
the permission text in all copies or substantial portions.

Check before copying (not checked in this research):

- `third-party-licenses/` and `third-party-licenses.config.json` in t3code list the licences of
  its own dependencies. Copy no vendored file without reading its header.
- `apps/web/src/terminal/ghostty/fonts` vendors a symbols-only Nerd Font (MIT) and
  `native/libghostty-vt` is Ghostty (MIT). We do not plan to copy them.
- `apps/web/src/components/T3Wordmark` and `assets/` are T3 branding. Do not copy them. Our
  app must not use the T3 name or logo.
- `@pierre/diffs` and `@pierre/trees` are npm packages with their own licences. Read each
  before use; add them to the dependency list.

Lines to add:

1. `NOTICE`, a paragraph:

   > The Electron app in `desktop/` contains code adapted from t3code
   > (https://github.com/pingdotgg/t3code) at commit `<sha>`, licensed under the MIT
   > License, Copyright (c) 2026 T3 Tools Inc. See `THIRD_PARTY_NOTICES.md`.

2. `THIRD_PARTY_NOTICES.md`, a section "t3code" that names the copied files (by folder), the
   commit, says that each changed file says so at its top, and holds the full MIT text with
   the line `Copyright (c) 2026 T3 Tools Inc.`.
3. A header comment in each copied file: `Adapted from t3code (MIT, Copyright (c) 2026
   T3 Tools Inc.), <original path> at <sha>. Changed: <what>.`
4. The repository rules say to name other GitHub users or repositories only in `NOTICE`,
   `THIRD_PARTY_NOTICES.md`, `CONTRIBUTORS.md` and `LICENSE`. The file headers above name
   t3code, so keep the header to "Adapted from t3code, see THIRD_PARTY_NOTICES.md" and put the
   URL and the original path only in `THIRD_PARTY_NOTICES.md`.
5. The packaged app's own third-party list (the npm dependencies) needs its own generated
   file. `third-party-licenses` in t3code shows one way to produce it.
