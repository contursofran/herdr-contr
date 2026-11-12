# 27: Orchestration settings edit contr's agents.json

## Decision

The herdr-contr settings dialog has an Orchestration section. It edits contr's
`agents.json`, the file that contr's coordinator reads. It does not keep a copy
in herdr-contr's `config.json`.

## Why

- State comes from the source: contr reads this file, so a copy would drift.
- The choices and defaults are contr's (`internal/agents`, and the Agents tab of
  contr's Settings), so both apps show the same values.

## How

- Path: `CONTR_AGENTS_CONFIG`, else the user config folder's `contr/agents.json`.
  The mod reads the variable through `sh`, because a mod has no environment.
- Read, change one field, write against the text just read. All other keys stay.
- A file that is not JSON shows an error and is never written. A missing file
  shows contr's defaults, and the first change creates it.
- Controls: the dialog's existing choice switch, and two small buttons (− and +)
  for the two numbers. No new kind of control.
- Only the five roles of contr's Settings show. Only Claude Code models show; a
  role with another command shows "Custom" until a value is picked.
