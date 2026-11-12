# 42: the project's favicon as the space icon

The owner (2026-11-12): in the sidebar's Spaces list, show the project's favicon in place of the letter
icon when the project has one; else a folder icon.

- As in #38, Herdr's `spaces` mod is copied into `herdr-app/mods/herdr-contr-spaces` (Apache-2.0; credit
  in `NOTICE`). The `herdr-contr` layout uses the copy. The helper's `install` ships it and runs
  `herdr-app check` on it.
- The only change is the icon. Spaces on another machine get the folder icon: no look over the network.
- The look is one `exec` of `sh` with a `[ -f ]` loop in the folder. `readTextFile` reads UTF-8 text only,
  so it cannot test for a binary file. The result is kept per folder; a new folder gets a new look.
- No `.ico`: Herdr's `image()` names SVG, PNG, JPEG and "other GPUI image formats", not ICO.
- The space colors (`shared("space.colors")`) are still written, because other mods read them.
