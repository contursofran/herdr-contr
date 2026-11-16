# 214: due dates on owner to-dos

- `pin_for_owner` takes an optional `due` (YYYY-MM-DD). Another form is an error,
  and then nothing is pinned.
- The to-do keeps `due` in the owner file, and `due_told` when the coordinator
  got its note.
- The coordinator's Stop hook gives `contr: due today: <ID> <text>` one time,
  on the due day or the first stop after it. It comes after the held reports
  and before the compaction check. Only the coordinator that pinned it gets it.
- A coordinator's own step on a date is a to-do that it pins with `due`. There
  is no second list for it.
- `owner open --json` gives `due` for each to-do (empty when there is none).
  The coordinator's session-start list shows `(due <date>)`. The mod does not
  show it yet (frontend on hold).
- `CONTR_TODAY` sets today for tests; else it is the local date.
