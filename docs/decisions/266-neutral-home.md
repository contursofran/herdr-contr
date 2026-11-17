# 266: the neutral backend root, and the move of the owner's files

- The neutral default folder of the backend root is `~/.config/contr-backend` (owner, 2026-11-16, question
  D30). HOME, else USERPROFILE, as for the Herdr home. It is the same on macOS and Linux.
- The owner's files in `~/.config/herdr-app/herdr-contr` stay where they are now (owner, 2026-11-16, question
  D31). The default root is the neutral folder when it exists, else `~/.config/herdr-app/herdr-contr` when
  that exists, else the neutral folder. `--home`, `CONTR_HOME`, `HERDR_APP_HOME` and the installed place come
  before the default.
- The move is one explicit command for later (D31): `herdr-contr-feed home move [--dry-run]`. It moves the
  folder and leaves a link at the old path, so the mod's paths, `--home` and Claude Code's marketplace entry
  stay valid. It stops and writes nothing when the move is not safe.
- No install and no agent runs the move. The owner runs it.
