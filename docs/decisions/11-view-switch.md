# Chat | Terminal switch (issue 15, 2026-11-12)

## Decision

A tab with an agent shows a Chat | Terminal switch on the right of a 36 px strip: over the chat,
and in the `view-switch` block over the tab's terminal. ⌘⇧T does the same. The pick is kept per
tab until the app restarts. Without a pick, the old rule stays: the chat for one pane, the
terminal for a split tab. A tab without an agent hides the block.

The switch shows only in the `herdr-contr` layout. In `herdr-contr-own`, the tab menu's
Show chat and Show panes do the same, and they keep the pick too.

## Why

- The owner asked for a toggle between the terminal and the chat.
- Herdr's tab bar has no place for it, so the switch has its own strip.
- Before, the chat came back each time the tab was shown again: a pick that does not stay is not
  a toggle.

## Cost

The strip takes 36 px over the chat and over the terminal of a tab with an agent.
