# #217 health check

- PR: #232 (draft), head 50804b7, branch agent/217-health-check
- Checks: `npm test` 577 pass; `cargo test` (feed, core, orch) passes. CI is started by the push.

## What changed
- `herdr-contr-feed health`: one `check` line for each of mods, layout, collisions, orch, hc, kit and herdr.
- The Settings Health section is in `health.js`, with a Re-check button.
- `protocol.md` and `RECIPE.md` are updated.

## Deviations and open items
- About 580 lines, about 260 of them tests; not split.
- The minimum Herdr version is 0.1.16, the version installed now; the owner can change it.
- No screenshot: herdr-app must not be started, and an unmerged branch must not be installed.
- #219 also edits `settings.js` (imports, SECTIONS, the constructor, content); expect small conflicts.
