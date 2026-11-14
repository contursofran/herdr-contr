# 189: The mod reads and changes the tool's files only through the tool

## Decision

The orchestration tool (`herdr-contr-orch`) is the one reader and writer of
`agents.json` and `owner.json`. The mod runs the installed tool with
`HERDR_APP_HOME` set to its Herdr home:

- The Orchestration section reads the settings with `agents config --json` and
  changes them with `agents set <key> <value>…`. "Use recommended setup" is one
  `agents set` call with all pairs.
- The owner question cards read `owner open --json` at most every 3 s.

This replaces the "read, change one field, write" part of decision 27. The mod
does not read the files, and it does not read `CONTR_AGENTS_CONFIG` or
`CONTR_OWNER_FILE` through `sh`.

## Why

- One copy of the defaults, the valid values and the flag rules: the tool's.
  The mod had a second copy, which could drift from the tool.
- The tool writes under its lock. The mod's write did not take the lock.
- The tool finds the path from its environment, so the mod needs no `sh` call.

## Cost

One `owner open --json` process, release build, an owner file of 20
questions: 3.66 ms on average over 20 runs (2.88 ms to 5.45 ms). At one run
every 3 s, this is about 0.1% of one core.

## How

- One function (`runOrch` in `orch.js`) runs the tool at all sites.
- A tool that is missing or fails does not break the chat: the settings section
  shows its message and its switches are off; the question cards stay empty.
- The mod keeps only what the UI shows: the labels of the flow values, the
  model and effort lists, the recommended setup and the roles. A model or
  effort that is not in the lists shows as none.
