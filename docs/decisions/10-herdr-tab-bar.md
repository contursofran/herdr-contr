# Herdr's own tab bar (PR 10, 2026-11-12)

## Decision

The `herdr-contr` layout uses Herdr's `tabs` block as its top bar, and Herdr's Files and Changes
panel beside the screens, as in Herdr's `main` layout. In that layout the mod draws no bar of its
own. `herdr-contr-own` keeps the mod's capsule bar.

## Why

- The owner asked for the top bar to be like Herdr's normal tabs.
- Herdr already draws a tab bar: do not rebuild what Herdr draws.
- Herdr's bar always shows Files and Changes buttons, so the layout has Herdr's panel for them.

## Cost

Herdr's bar has no Needs you, Overview or agent terminal button. They stay on the keys:
⌘⇧I, ⌘⇧\ and ⌃`.
