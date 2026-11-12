# Plan usage in the sidebar (issue 29, 2026-11-12)

## Decision

The mod's sidebar shows the Claude and Codex plan usage above the herdr-contr and Settings rows:
one compact row per plan with data, Claude first.

- Each row: a small meter of the most used window, the plan's name, and each window as "5h 38% 2h"
  (kind, whole percent used, time to its reset). No hue: theme tokens `border`, `muted`, `dim`.
- The data comes from a new helper command, `usage`. It is a port of contr's `internal/usage`:
  - Claude: `usage/claude.json` in contr's state folder, that `contr statusline` writes. Claude
    Code gives its plan limits only to a status line command, so without contr's status line
    there is no Claude row.
  - Codex: `codex app-server` (`account/rateLimits/read`, 10 s limit), else the newest Codex
    rollout file.
- The mod reads it when the sidebar opens and every 5 minutes, with `cx.spawn` (off the UI
  thread). All the views share one read. The chat, Needs you and Overview screens start the
  reads too, so the hidden block can show.
- The herdr-contr layout (Herdr's sidebar) gets a `usage` block of the mod between Herdr's
  sidebar lists and its footer (56 px, view `usage`), with the same rows. It starts hidden and
  shows while a plan has data (`layout.show` and `layout.hide`, as the view switch does). The
  owner uses this layout.
- A plan with no data has no row. An error or an older helper keeps what is shown.

## Why

The owner asked for the plan usage at the bottom left, above Settings, as contr shows it.
Neither Herdr nor the mod showed it. contr shows one percent per plan (UsageMeter, D196); the
issue asks for each window with its reset time, so the row shows both windows as text and the
meter for the most used one.

## Not done

- The helper does not write Claude's status line. It reads what contr writes.
