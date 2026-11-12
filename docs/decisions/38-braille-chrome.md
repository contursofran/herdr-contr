# 38: the braille loader in the sidebar's agent list and the top bar

The owner (2026-11-12): the sidebar and the top bar use the same braille loader as the chat, in place of
the arc spinners.

- Herdr's rule is to clone a shipped mod before you change its code. So `agents` and `tabs` are copied into
  `herdr-app/mods/herdr-contr-agents` and `herdr-app/mods/herdr-contr-tabs` (Apache-2.0; credit in `NOTICE`).
- The only change is the working mark: `loader(12)`, the same frames, 80 ms, Menlo and `line_height(1)` as
  `loader()` in `mods/herdr-contr/state.js`. A mod cannot import another mod, so each copy has its own copy
  of the function. The tabs copy points its window icons at its own `win-*.svg`.
- The `herdr-contr` layout uses the copies. `herdrTabBar()` accepts both `tabs` and `herdr-contr-tabs`.
- The helper's `install` puts the three mods in the home, owns their files, and runs `herdr-app check` on each.
- The rest of Herdr's sidebar (`spaces`, `workspace-chrome`) stays Herdr's.
