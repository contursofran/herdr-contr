# contr kit

A Claude Code plugin for agent work with `herdr-contr-orch agents` (the
orchestration tool of this repository, `xplat/herdr-contr-orch`):

- `agents/`: the builder, planner and reviewer agent definitions.
- `commands/orchestrate.md`: the coordinator playbook, as `/contr:orchestrate <goal>`.
- `skills/chat-writing/SKILL.md`: what contr's chat shows (remarks, tables, chips, status board,
  merge queue) and when to use each part, for any session in contr.
- `hooks/hooks.json`: the Stop hook (a worker's turn that sent no report; the reports held for a busy
  coordinator; a coordinator's state before a compaction) and the SessionStart hook: in an hc terminal one line that names `contr:chat-writing`; at startup of an orchestrator and after a compaction, the playbook.
- `.mcp.json`: starts `herdr-contr-orch mcp`, which gives the session the agent tools (spawn, set_title, message,
  report, interrupt, stop, resume, read, list, clear_ended). A worker reports with `report`. `stop` removes the
  worker's terminal unless `keep` is set. Spawn keeps each Claude worker's session id in
  `state/workers/<name>.json`; `list` shows a worker that ended without a `stop` as `gone`, and `resume`
  starts it again from that session. See Install for where `herdr-contr-orch` is found.

## Install

The helper's `install` puts the kit in `<Herdr home>/herdr-contr/kit` and,
with `--orch <file>`, the tool in `<Herdr home>/herdr-contr/bin`
(`scripts/land.sh` does both for merged `main`). The hooks and the MCP
server look for `herdr-contr-orch` in `../bin` of the kit folder first, then
on the PATH. Start `claude` with the kit: `claude --plugin-dir <kit>`.

To have the kit in every `claude` session, register it once:
`claude plugin marketplace add <Herdr home>/herdr-contr`, then
`claude plugin install contr@contr`. The plugin reads the kit folder in
place, so a later install needs no update step. The repository root has
the same marketplace file, for a checkout.

From a checkout, without an install: build the tool (`cargo build --release
--manifest-path xplat/Cargo.toml -p herdr-contr-orch`), put it on the PATH,
and give the repository's `kit` folder to `--plugin-dir`.

For shell use (`herdr-contr-orch agents …`), put the tool on the PATH.

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
   Spawn refuses past the flow's `workers`: it counts the coordinator's live
   workers that are `working`, and a `message` to a settled worker counts as
   a start. To limit a project's open PRs, add its main checkout and a number:
   `"open_prs": {"~/code/app": 4}`. At or over it, spawn refuses a new issue
   (one `gh pr list` call). A fix round goes through: the worktree's branch
   has an open PR, or `--fix-pr` (`fix_pr: true` in the tool).
2. Start the coordinator in an hc terminal with the plugin:
   `claude --plugin-dir <kit> --model opus --effort medium`, then
   `/contr:orchestrate <goal>`.
3. Workers that the coordinator starts get its address in
   `CONTR_COORDINATOR` and report with the MCP server's `report` tool. Spawn
   allows the tool (`--allowedTools mcp__plugin_contr_contr__report`), so
   auto mode does not block it. When hc refuses the report because the
   coordinator's prompt is busy (a dialog), the tool keeps it in
   `held/<coordinator>` of the state folder. The coordinator's Stop hook
   gives the held reports when its turn ends (it keeps the turn going one
   time), and its prompt hook gives them with the next prompt. Only an
   urgent report (blocked, failed, error, a question, failed CI, a PR merged
   or closed) wakes the coordinator at once. Another report within
   `report_batch_minutes` (agents.json, default 10; 0 turns it off) of the
   last one is held too, and goes with the next urgent report, or with the
   timer (`herdr-contr-orch batch flush`) at the end of the interval. When a
   worker's turn ends without a report, or its report call failed (hc
   refused it, or auto mode blocked it), the kit's Stop hook (`hooks/hooks.json`, `herdr-contr-orch hook stop`)
   sends the coordinator "ended its turn without a report. Last words: …".
   When Claude Code saved no transcript, the hook cannot tell whether the
   turn sent a report, so it sends "ended its turn; it may have sent a report
   already …" with the last words. When hc refuses that note, it is held
   in the same way. Workers start without the
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
owner to its todos, starting with `[you] `. A prompt with only task
notifications, system reminders and hc notes (`<hc …>…</hc>`) gets no such
line, and workers (`CONTR_COORDINATOR` set) get nothing.
In a coordinator it also adds the flow when the flow changed. The hook logs
errors to stderr and always exits 0, so it never blocks Claude Code.
