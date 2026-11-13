# 100: one Projects list in the sidebar

- The owner decided (2026-11-13) that the sidebar is reworked after the owner's design, in slices: #96 (this),
  #97 the head and the foot, #98 the hover controls and the model tooltip.
- The spaces list and the agents list become one list, drawn by the `herdr-contr-spaces` copy, so that the
  space features (git, favicons, machines, drag, menus) stay as they are. The agent rows and the worker tree
  come from the agents copy into `herdr-contr-spaces/projects.js`.
- An orchestrator is an agent with live sub-agents: the `parent` that hc gives each terminal. No new helper command.
- `herdr-contr-agents` stays installed until #98 moves its model line into a tooltip.
