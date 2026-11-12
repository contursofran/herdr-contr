# Orchestrate pill (issue 25, 2026-11-12)

## Decision

The row above the reply box has a pill at its right end.

- When the session orchestrates: "Orchestrating" with a `working` dot and the number of live
  workers ("· 2 workers"). A session orchestrates when its hc label `contr.orchestrating` is
  set, or when terminals that are not finished have it as their parent. This is contr's rule.
- Otherwise, for a Claude Code session: an "Orchestrate" button. A press puts
  `/contr:orchestrate ` in the box, so the owner types the goal and sends it.
- Other agents get no button.

The pill shows also when the status row is set to Hide.

## Why

The owner asked for contr's orchestrating in Herdr, with the sign at the top right of the
input. `terminals()` gives the labels and parents live, so no helper command is needed.
