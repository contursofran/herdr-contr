# contr kit

A Claude Code plugin for agent work with `herdr-contr-orch agents` (the
orchestration tool of this repository, `xplat/herdr-contr-orch`):

- `agents/`: the builder, planner and reviewer agent definitions.
- `commands/orchestrate.md`: the coordinator playbook, as `/contr:orchestrate <goal>`.
- `skills/chat-writing/SKILL.md`: what contr's chat shows (remarks, tables, chips, status board,
  merge queue) and when to use each part, for any session in contr.
- `hooks/hooks.json`: the Stop hook (a worker's turn that sent no report; a coordinator's state
  before a compaction) and the SessionStart hook: in an hc terminal one line that names `contr:chat-writing`; at startup of an orchestrator and after a compaction, the playbook.
- `.mcp.json`: starts `herdr-contr-orch mcp`, which gives the session the agent tools (spawn, message,
  interrupt, stop, read, list, clear_ended). `stop` removes the
  worker's terminal unless `keep` is set. `herdr-contr-orch` must be on the PATH.

## Install

Build the tool (`cargo build --release --manifest-path xplat/Cargo.toml -p
herdr-contr-orch`) and put it on the PATH. Then start `claude` with the kit:
`claude --plugin-dir <kit>`. The plugin reads this folder in place, so a
change to the kit needs no update step.

## Use

1. Point `herdr-contr-orch agents` at this folder: set `CONTR_KIT` to its path, or put
   `"kit": "<path>"` in the profiles file (`herdr-contr-orch agents help` names it).
   Every claude profile then starts with the kit:
   `claude --plugin-dir <kit> …`, unless its argv has `--plugin-dir`
   already. A profile with an agent file here also starts as that
   agent: `claude --plugin-dir <kit> --agent contr:<profile> …`. A variant names
   its agent: `"planner-high": {"argv": ["claude", "--model", "opus",
   "--effort", "high"], "agent": "planner"}`. The `spawn` tool reads the
   profiles file on each spawn, so a new profile works at once. To spawn a
   worker with it, name it for its role in the flow:
   `"flow": {"profiles": {"planner": "planner-high"}}`. A role with no name
   uses the profile of the same name. Spawn refuses a profile that the flow
   names for no role; `--any-profile` (`any_profile: true` in the tool) lets
   it through.
   To prepare new worktrees of a project, add its main checkout and a
   shell command: `"setup": {"~/code/app": "cp -R \"$CONTR_MAIN/.husky/_\"
   .husky/ && pnpm install --frozen-lockfile --offline"}`. Spawn runs it
   in the worktree before the agent starts.
2. Start the coordinator in an hc terminal with the plugin:
   `claude --plugin-dir <kit> --model opus --effort medium`, then
   `/contr:orchestrate <goal>`.
3. Workers that the coordinator starts get its address in
   `CONTR_COORDINATOR` and report with `hc send`. When a worker's turn ends
   without one, the kit's Stop hook (`hooks/hooks.json`, `herdr-contr-orch hook stop`)
   sends the coordinator "ended its turn without a report. Last words: …".
   When Claude Code saved no transcript, the hook cannot tell whether the
   turn sent a report, so it sends "ended its turn; it may have sent a report
   already …" with the last words. Workers start without the
   `CLAUDECODE` and `CLAUDE_CODE_*` variables of the hc core, so Claude Code
   saves their transcripts.
   The hook does nothing in sessions without `CONTR_COORDINATOR`.
4. Compaction: in a coordinator (a session that ran `/contr:orchestrate`, or an
   hc terminal with role orchestrator), the Stop hook keeps the turn going
   once when the context passes `CONTR_SAVE_AT` tokens (default 220000, under
   a 250000 `autoCompactWindow`) and asks for `COORDINATOR-STATE.md`. After
   Claude Code compacts, the SessionStart hook (matcher `startup|resume|clear|compact`) adds the
   playbook, the state file to read, the coordinator's live workers and its
   open owner items. Claude Code's own auto-compaction stays on.

## The owner's requests

In an hc terminal (`HC_TERMINAL_ID` set), the UserPromptSubmit hook
(`herdr-contr-orch hook prompt`) asks Claude to add each request of the
owner to its todos, starting with `[you] `. Prompts that start with
`<hc from=` are skipped, and workers (`CONTR_COORDINATOR` set) get nothing.
In a coordinator it also adds the flow when the flow changed. The hook logs
errors to stderr and always exits 0, so it never blocks Claude Code.
