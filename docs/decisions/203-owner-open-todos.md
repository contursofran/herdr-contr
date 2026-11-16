# 203: `owner open --json` gives the open to-dos too

## Decision

`herdr-contr-orch owner open --json` gives one list: the open questions
(`kind: "question"`), then the open to-dos (`kind: "todo"`), each group oldest
first. A to-do has `id`, `coordinator`, `from`, `todo_kind` (`do` or `know`)
and `text`. A done to-do leaves the list. There is no `owner todos --json`.

## Why

- One process and one read of the owner file for each poll of the mod (every
  3 s), and one snapshot for the question cards and the to-do cards.
- One inbox: one list of all that needs the owner.
- An older mod skips an item with no `question` text, so the to-dos do not
  break it.
