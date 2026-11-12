# Our own names (PR 6, 2026-11-11)

## Decision

| What | Name |
|---|---|
| Base name, mod folder and mod id | `herdr-contr` |
| Layout with Herdr's own sidebar | `herdr-contr` |
| Layout with the mod's own sidebar | `herdr-contr-own` |
| Themes | `herdr-contr`, `herdr-contr-light`, `herdr-contr-mac`, `herdr-contr-mac-light` |
| Keys sections | `layout:herdr-contr`, `layout:herdr-contr-own` |
| Mod commands and shared keys | `herdr-contr.*` |
| Helper crate and program | `herdr-contr-feed` |
| Folder in the Herdr home | `herdr-contr/` |
| Environment variables | `HERDR_CONTR_*` |

## Why

- The base name is the repository name that the owner picked.
- The layout that the owner uses (Herdr's own sidebar) gets the short name.
- The helper is not `contr`: that is the name of another program, the contr command.
- The crate has the program's name, because Cargo names the program after the crate.
- No name is the same as a Flockbook name, so Flockbook and this mod can be installed in one Herdr
  home. `xplat/herdr-contr-feed/tests/flockbook.rs` tests both directions.

## The `herdr-contr` layout (owner, 2026-11-11)

Needs you, Overview and New tab stay in the mod's top bar only. There is no block for them under
Herdr's sidebar.

## Also removed

The "Check for updates" setting and the update check: it offered a newer Flockbook, which is not
this mod.
