# The ctx tag against the auto-compact limit

Issue #30 (owner, 2026-11-12).

- The helper's `status` reads `autoCompactWindow` from Claude Code's
  `settings.json` (in `CLAUDE_CONFIG_DIR`, else `~/.claude`).
- Tokens used: the sum of `context_window.current_usage` input, cache write
  and cache read tokens from the status line JSON. If not sent, the
  `used_percentage` of `context_window_size`.
- With both, the tag is `ctx 120k / 250k`; its percent and its warning (80%
  and up) are against the limit. Otherwise the tag is `ctx 37%` as before.
- The mod does not change: it shows the helper's text.
