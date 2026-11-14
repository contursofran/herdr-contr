//! The kit's hooks, which Claude Code runs with the hook input on stdin. Ported from contr's `contr hook`.
//!
//! `stop` in a worker (CONTR_COORDINATOR is set): a turn that sent no report gets one, sent to the
//! coordinator. It never stops the turn.
use std::fs::File;
use std::io::{self, BufRead, Write};
use std::path::Path;

use serde::Deserialize;

use crate::spawn::ENV_COORDINATOR;
use crate::turn::{last_turn, Turn};
use crate::workers::{Core, Manager};

pub const USAGE: &str = "Usage:
  herdr-contr-orch hook stop   the kit's Stop hook

Claude Code runs it and gives it the hook input on stdin.

stop, in a worker (CONTR_COORDINATOR is set): a turn that sent no report
with hc send gets one, \"ended its turn without a report. Last words: …\",
sent to the coordinator. When Claude Code saved no transcript, the hook
cannot tell if the turn sent one: it sends \"ended its turn; it may have
sent a report already …\" with the last words. It never stops the turn.
";

/// The most characters of the worker's last words in a report.
pub const SAID_MAX: usize = 300;

/// The part of Claude Code's hook input that the hooks read.
#[derive(Default, Deserialize)]
#[serde(default)]
struct Input {
    transcript_path: String,
    cwd: String,
    /// Newer Claude Code versions give it.
    last_assistant_message: String,
}

/// `hook`. The exit code is 0 when the hook is done or has nothing to do, and 1 on an error, usage errors
/// included. It is never 2: for a Stop hook, 2 blocks the turn and sends stderr to the model.
pub fn run(args: &[String], env: &dyn Fn(&str) -> String, input: &mut dyn BufRead, _out: &mut dyn Write, err: &mut dyn Write, core: &dyn Core) -> i32 {
    if args.len() != 1 || args[0] != "stop" {
        let _ = err.write_all(USAGE.as_bytes());
        return 1;
    }
    // Outside hc there are no workers and no coordinators.
    if env(ENV_COORDINATOR).is_empty() && env("HC_TERMINAL_ID").is_empty() {
        return 0;
    }
    let mut text = String::new();
    let hook_input: Input = match input.read_to_string(&mut text).map_err(|e| e.to_string()).and_then(|_| serde_json::from_str(&text).map_err(|e| e.to_string())) {
        Ok(i) => i,
        Err(e) => {
            let _ = writeln!(err, "herdr-contr-orch hook: bad hook input: {e}");
            return 1;
        }
    };
    // A coordinator's Stop has no work yet.
    if env(ENV_COORDINATOR).is_empty() {
        return 0;
    }
    match worker_stop(&hook_input, env, core) {
        Ok(()) => 0,
        Err(e) => {
            let _ = writeln!(err, "herdr-contr-orch hook: {e}");
            1
        }
    }
}

/// Sends the coordinator a report for a turn that sent none. With no transcript file (Claude Code saved
/// none), the hook cannot tell if the turn sent one, so the report says that it can repeat one.
fn worker_stop(input: &Input, env: &dyn Fn(&str) -> String, core: &dyn Core) -> Result<(), String> {
    let coordinator = env(ENV_COORDINATOR);
    if input.transcript_path.is_empty() {
        return Err("the hook input has no transcript_path".into());
    }
    let (turn, no_transcript) = match File::open(&input.transcript_path).and_then(last_turn) {
        Ok(t) => (t, false),
        Err(e) if e.kind() == io::ErrorKind::NotFound => (Turn::default(), true),
        Err(e) => return Err(format!("{}: {e}", input.transcript_path)),
    };
    if turn.reported {
        return Ok(());
    }
    let said = if input.last_assistant_message.trim().is_empty() { &turn.said } else { &input.last_assistant_message };
    let text = if no_transcript { unsure_report(said, &input.cwd) } else { missed_report(said, &input.cwd) };
    Manager::new(core, env("HC_TERMINAL_ID")).send(&coordinator, &text).map_err(|e| format!("report to {coordinator}: {e}"))
}

/// The report for a turn that sent none: the worker's last words on one line, cut to SAID_MAX characters, and
/// RESULT.md when the worker's folder has one.
pub fn missed_report(said: &str, cwd: &str) -> String {
    report("ended its turn without a report", said, cwd)
}

/// The report for a turn with no transcript: as `missed_report`, but it says that it can repeat a report that
/// the worker sent.
pub fn unsure_report(said: &str, cwd: &str) -> String {
    report("ended its turn; it may have sent a report already (no transcript, so contr cannot tell)", said, cwd)
}

fn report(lead: &str, said: &str, cwd: &str) -> String {
    let mut said = said.split_whitespace().collect::<Vec<_>>().join(" ");
    if said.chars().count() > SAID_MAX {
        said = said.chars().take(SAID_MAX).collect::<String>() + "…";
    }
    if said.is_empty() {
        said = "(none)".into();
    }
    let mut text = format!("{lead}. Last words: {said}");
    if !cwd.is_empty() && Path::new(cwd).join("RESULT.md").exists() {
        text += &format!("; RESULT.md in {cwd}");
    }
    text
}
