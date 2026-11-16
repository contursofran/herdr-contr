# 261: the backend and the mod in one repository

- One repository now (owner, 2026-11-16, question D29). This repository is the backend's home: the helper,
  the orchestration tool, the kit and `docs/protocol.md`. The mod becomes a client of the backend, as
  contr-desktop is. The mod leaves this repository, or is deleted, later.
- AGENTS.md "Layout" gives the side (backend, mod or both) of each part. A backend part does not read a file
  of `herdr-app/` at build time or at run time.
- The backend root's variable is `CONTR_HOME`, because the tool's variables are `CONTR_…` (decision 183).
- The names of the programs (`herdr-contr-feed`, `herdr-contr-orch`) and the `HERDR_CONTR_*` variables stay.
  Only new things get neutral names.
- The release stays one zip, with the same name and layout, so the clients that fetch it do not change.
- Not decided yet: the neutral default folder of the backend root, and the move of the owner's files to it
  (S6 of the plan for #253).
