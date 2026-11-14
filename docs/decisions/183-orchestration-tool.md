# 183: the orchestration tool is in this repository

- The owner (2026-11-13, #160): the orchestration tool moved from the contr repository to this one, as the
  Rust crate `xplat/herdr-contr-orch` and the Claude Code plugin `kit/`. The contr repository is not used.
- The helper's `install` puts the tool and the kit in the Herdr home (`herdr-contr/bin`, `herdr-contr/kit`),
  with a marketplace file. Claude Code registers the kit from there. The kit's hooks and MCP server look for
  the tool next to the kit first, then on the PATH.
- The tool's files (`agents.json`, `owner.json`, `state/`) are in the Herdr home's `herdr-contr` folder. The
  mod's Orchestration settings and its owner question card read them there.
- The names stay (owner, 2026-11-13): the plugin is `contr` (`/contr:orchestrate`, `contr:chat-writing`), the
  variables are `CONTR_…`. The Go kit is retired, so nothing collides, and the owner's commands do not change.
- Not ported, because only the contr app used them: `hook post-tool` (todos and artifacts for the app's
  panels), the session file with the transcript path, `agents watch`, the status line settings of `spawn` and
  `statusline`, the explorer agent.
- The MCP server runs one call at a time.
