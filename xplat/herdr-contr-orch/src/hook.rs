//! The kit's hooks, which Claude Code runs with the hook input on stdin. Ported from contr's `contr hook`.
//!
//! `stop` in a worker (CONTR_COORDINATOR is set): a turn that sent no report gets one, sent to the
//! coordinator, or held for it when hc refuses it. It never stops the turn. In other hc sessions, `stop` and
//! `prompt` give the reports that were held for the terminal (`held`).
//!
//! `stop` in a coordinator: before Claude Code compacts the context, the hook keeps the turn going one time
//! and asks for an update of the state file. `session-start` gives a coordinator its playbook and state.
use std::fs::{self, File};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::cli::flow_text;
use crate::home::owner_store;
use crate::flowseen;
use crate::held;
use crate::spawn::ENV_COORDINATOR;
use crate::turn::{context_and_model, last_turn, ran_command, Turn};
use crate::workers::{Core, Manager, LABEL_CONTEXT, LABEL_CONTEXT_LIMIT, LABEL_EFFORT, LABEL_MODEL, LABEL_ORCHESTRATING, ROLE_ORCHESTRATOR};

pub const USAGE: &str = "Usage:
  herdr-contr-orch hook stop            the kit's Stop hook
  herdr-contr-orch hook session-start   the kit's SessionStart hook (startup, resume, clear, compact)
  herdr-contr-orch hook prompt          the kit's UserPromptSubmit hook

Claude Code runs them and gives them the hook input on stdin.

stop, in a worker (CONTR_COORDINATOR is set): a turn that sent no report
(hc send or the report tool, with a result that is not an error) gets one,
\"ended its turn without a report. Last words: …\", sent to the
coordinator. When Claude Code saved no transcript, the hook cannot tell if
the turn sent one: it sends \"ended its turn; it may have sent a report
already …\" with the last words. When hc refuses it (the coordinator's
prompt is busy), it is held for the coordinator. It never stops the turn.

stop, in an hc session that is not a worker: when reports were held for
its terminal, it keeps the turn going one time and gives them, and does
nothing more in this Stop.

stop, in a coordinator (a session that ran /contr:orchestrate, or an hc
terminal with role orchestrator): when the context passes CONTR_SAVE_AT
tokens (default 220000), it keeps the turn going one time and asks the
coordinator to update COORDINATOR-STATE.md before Claude Code compacts.
In each hc session that is not a worker it also labels the terminal with
the context size (contr.context, contr.context-limit) and with the model
and the effort (contr.model, contr.effort).

session-start, at startup in an hc terminal with role orchestrator, and
after a compaction in a coordinator: prints the playbook, the state file
to read (at startup, only when there is one), the live workers that the
coordinator started, its open owner items and the flow. Claude Code adds
them to the new context. After resume and clear it prints nothing more.
In an hc terminal it first prints one line that names the skill
contr:chat-writing.

prompt runs only in an hc terminal (HC_TERMINAL_ID), and not in a worker.
It asks Claude to add each request of the owner to its todos with
\"[you] \". A prompt with only task notifications, system reminders and hc
notes gets no such line. When the
owner changed the flow or the models of the profiles after the session
saw them last, a coordinator gets them again. Reports that were held for
the terminal come with it. It always exits 0.
";

/// What the prompt hook adds to a prompt of the owner.
pub const YOU_CONTEXT: &str = "contr: before you start, add each new request in this prompt to your todo list (TodoWrite or TaskCreate). Start the text of each such item with \"[you] \".";

/// The context size, in tokens, from which a coordinator is asked to save its state: below the owner's
/// compaction window.
pub const DEFAULT_SAVE_AT: i64 = 220_000;
/// The owner's compaction window (autoCompactWindow in the Claude Code settings).
pub const DEFAULT_COMPACT_AT: i64 = 250_000;
/// The most characters that session-start prints. Claude Code cuts a hook's context at 10000 characters.
pub const RESTORE_MAX: usize = 9500;
/// The line that session-start prints first in an hc terminal: it names the kit's skill for answers that the
/// owner reads in the chat.
pub const CHAT_SKILL_LINE: &str = "contr: the owner reads your answers in contr's chat. Before you write one, use the skill contr:chat-writing: it says what the chat shows.\n";
/// The kit's coordinator command.
const ORCHESTRATE_COMMAND: &str = "contr:orchestrate";
/// The coordinator's state file in its folder.
pub const STATE_FILE: &str = "COORDINATOR-STATE.md";

/// The most characters of the worker's last words in a report.
pub const SAID_MAX: usize = 300;

/// The part of Claude Code's hook input that the hooks read.
#[derive(Default, Deserialize)]
#[serde(default)]
struct Input {
    session_id: String,
    transcript_path: String,
    cwd: String,
    /// Newer Claude Code versions give it.
    last_assistant_message: String,
    stop_hook_active: bool,
    /// SessionStart: startup, resume, clear or compact.
    source: String,
    /// The session's effort level. Claude Code leaves it out for a model with no effort setting.
    effort: Option<Effort>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Effort {
    level: String,
}

/// `hook`. The exit code is 0 when the hook is done or has nothing to do, and 1 on an error, usage errors
/// included. It is never 2: for a Stop hook, 2 blocks the turn and sends stderr to the model.
pub fn run(args: &[String], env: &dyn Fn(&str) -> String, input: &mut dyn BufRead, out: &mut dyn Write, err: &mut dyn Write, core: &dyn Core) -> i32 {
    if args.len() == 1 && args[0] == "prompt" {
        if let Err(e) = prompt(env, input, out, err, core) {
            let _ = writeln!(err, "herdr-contr-orch hook prompt: {e}");
        }
        return 0;
    }
    if args.len() != 1 || (args[0] != "stop" && args[0] != "session-start") {
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
    let h = Hook { input: hook_input, env, core };
    let done = if args[0] == "session-start" {
        if !env("HC_TERMINAL_ID").is_empty() {
            let _ = out.write_all(CHAT_SKILL_LINE.as_bytes());
        }
        // The session starts with the flow that it has now.
        if let Err(e) = flowseen::record(env, &env("HC_TERMINAL_ID")) {
            let _ = writeln!(err, "herdr-contr-orch hook: record flow: {e}");
        }
        h.session_start(out)
    } else if !env(ENV_COORDINATOR).is_empty() {
        worker_stop(&h.input, env, core)
    } else {
        // Held reports come first: they keep the turn going, and the next Stop does the rest. An error with the
        // held reports does not stop the rest.
        match h.give_held(out) {
            Ok(true) => Ok(()),
            Ok(false) => h.coordinator_stop(out),
            Err(e) => {
                let _ = writeln!(err, "herdr-contr-orch hook: {e}");
                h.coordinator_stop(out)
            }
        }
    };
    match done {
        Ok(()) => 0,
        Err(e) => {
            let _ = writeln!(err, "herdr-contr-orch hook: {e}");
            1
        }
    }
}

/// The blocks of a prompt that hold no request of the owner: a background-task notification, a system
/// reminder and an hc note (with or without `from`).
const NOTES: [&str; 3] = ["task-notification", "system-reminder", "hc"];

/// True when the prompt holds text other than notes (#206).
pub fn owner_words(prompt: &str) -> bool {
    let mut s = prompt.to_string();
    for name in NOTES {
        s = without_blocks(&s, name);
    }
    !s.trim().is_empty()
}

/// The text with each `<name>…</name>` and `<name …>…</name>` block removed. A block with no end stays.
fn without_blocks(text: &str, name: &str) -> String {
    let (open, close) = (format!("<{name}"), format!("</{name}>"));
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find(&open) {
        let after = &rest[at + open.len()..];
        let is_tag = after.starts_with('>') || after.starts_with(' ');
        match after.find(&close) {
            Some(e) if is_tag => {
                out.push_str(&rest[..at]);
                rest = &after[e + close.len()..];
            }
            _ => {
                out.push_str(&rest[..at + open.len()]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The UserPromptSubmit hook: the todo line for a prompt of the owner, and the flow when it changed. A
/// worker's prompts come from the coordinator, and a worker is never a coordinator: it gets no context.
fn prompt(env: &dyn Fn(&str) -> String, input: &mut dyn BufRead, out: &mut dyn Write, err: &mut dyn Write, core: &dyn Core) -> Result<(), String> {
    let id = env("HC_TERMINAL_ID");
    if id.is_empty() || !env(ENV_COORDINATOR).is_empty() {
        return Ok(());
    }
    let flow = flowseen::change(env, core, &id).unwrap_or_else(|e| {
        let _ = writeln!(err, "herdr-contr-orch hook prompt: flow: {e}");
        String::new()
    });
    let mut raw = String::new();
    input.read_to_string(&mut raw).map_err(|e| format!("bad hook input: {e}"))?;
    let hook_input: Value = serde_json::from_str(&raw).map_err(|e| format!("bad hook input: {e}"))?;
    let asked = hook_input["prompt"].as_str().unwrap_or_default().trim();
    let waiting = held::take(env, &id).map(|r| held::text(&r)).unwrap_or_else(|e| {
        let _ = writeln!(err, "herdr-contr-orch hook prompt: {e}");
        String::new()
    });
    let mut parts = Vec::new();
    if owner_words(asked) {
        parts.push(YOU_CONTEXT);
    }
    if !flow.is_empty() {
        parts.push(&flow);
    }
    if !waiting.is_empty() {
        parts.push(&waiting);
    }
    if parts.is_empty() {
        return Ok(());
    }
    let context = json!({"hookSpecificOutput": {"hookEventName": "UserPromptSubmit", "additionalContext": parts.join("\n\n")}});
    writeln!(out, "{context}").map_err(|e| e.to_string())
}

/// Sends the coordinator a report for a turn that sent none. With no transcript file (Claude Code saved
/// none), the hook cannot tell if the turn sent one, so the report says that it can repeat one. When hc
/// refuses the send, the report is held for the coordinator.
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
    held::report(core, env, &coordinator, &text).map(|_| ()).map_err(|e| format!("report to {coordinator}: {e}"))
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

/// One run of a coordinator's hook.
struct Hook<'a> {
    input: Input,
    env: &'a dyn Fn(&str) -> String,
    core: &'a dyn Core,
}

/// The family word of a model: "opus" for "claude-opus-5-5". A model with no known family gives its id.
pub fn short_model(id: &str) -> String {
    let low = id.to_lowercase();
    ["opus", "sonnet", "haiku", "fable"].iter().find(|f| low.contains(**f)).map_or(id.to_string(), |f| f.to_string())
}

/// At most `n` characters of `s`, with "…" at the end when it cuts.
fn cut(s: String, n: usize) -> String {
    if s.chars().count() > n {
        return s.chars().take(n - 1).collect::<String>() + "…";
    }
    s
}

fn one_line(s: &str) -> String {
    cut(s.split_whitespace().collect::<Vec<_>>().join(" "), 160)
}

impl Hook<'_> {
    fn terminal(&self) -> String {
        (self.env)("HC_TERMINAL_ID")
    }

    /// A positive number in the variable `name`, else `default`.
    fn number(&self, name: &str, default: i64) -> i64 {
        (self.env)(name).parse::<i64>().ok().filter(|n| *n > 0).unwrap_or(default)
    }

    /// Gives this terminal the reports that were held for it, as the reason that keeps the turn going. True
    /// when there were some.
    fn give_held(&self, out: &mut dyn Write) -> Result<bool, String> {
        let terminal = self.terminal();
        if terminal.is_empty() {
            return Ok(false);
        }
        let text = held::text(&held::take(self.env, &terminal)?);
        if text.is_empty() {
            return Ok(false);
        }
        writeln!(out, "{}", json!({"decision": "block", "reason": text})).map_err(|e| e.to_string())?;
        Ok(true)
    }

    /// Asks a coordinator, one time for each compaction, to save its state when the context comes near the
    /// compaction window. With no transcript file it does nothing.
    fn coordinator_stop(&self, out: &mut dyn Write) -> Result<(), String> {
        if self.input.stop_hook_active || self.input.transcript_path.is_empty() {
            return Ok(());
        }
        let (tokens, model) = match context_and_model(Path::new(&self.input.transcript_path)) {
            Ok(v) => v,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(format!("{}: {e}", self.input.transcript_path)),
        };
        let mut labels = self.model_labels(&model);
        if self.ran_orchestrate().unwrap_or(false) {
            labels.insert(LABEL_ORCHESTRATING.into(), json!("1"));
        }
        self.show_context(tokens, labels);
        if tokens < self.number("CONTR_SAVE_AT", DEFAULT_SAVE_AT) || self.asked() {
            return Ok(());
        }
        if !self.coordinator()? {
            return Ok(());
        }
        self.mark_asked()?;
        let reason = format!(
            "contr: the context is at {}k tokens, and Claude Code compacts it soon. Update {STATE_FILE} now (Ready, Running with worker, issue, worktree and PR, Decisions, Next), so you can carry on from it after the compaction. Then end your turn.",
            tokens / 1000
        );
        writeln!(out, "{}", json!({"decision": "block", "reason": reason})).map_err(|e| e.to_string())
    }

    /// Gives a coordinator its playbook and state: at startup, and after a compaction.
    fn session_start(&self, out: &mut dyn Write) -> Result<(), String> {
        let text = match self.input.source.as_str() {
            // A new session has no transcript, so only the role counts. Nothing is cleared or labeled.
            "startup" => {
                if self.terminal().is_empty() || !self.orchestrator_role()? {
                    return Ok(());
                }
                self.startup_text()
            }
            "" | "compact" => {
                self.clear_asked();
                // The size before the compaction is old. The Stop of the next turn sets it again.
                self.show_context(0, Map::new());
                if !self.coordinator()? {
                    return Ok(());
                }
                self.restore_text()
            }
            _ => return Ok(()),
        };
        out.write_all(text.as_bytes()).map_err(|e| e.to_string())
    }

    /// The model and effort labels of the session: the short name of the transcript's model, and the effort
    /// of the hook input, else CLAUDE_EFFORT. A value that is not known is left out, so the label keeps its
    /// last value.
    fn model_labels(&self, model: &str) -> Map<String, Value> {
        let mut labels = Map::new();
        if !model.is_empty() {
            labels.insert(LABEL_MODEL.into(), json!(short_model(model)));
        }
        let effort = self.input.effort.as_ref().map(|e| e.level.clone()).filter(|l| !l.is_empty()).unwrap_or_else(|| (self.env)("CLAUDE_EFFORT"));
        if !effort.is_empty() {
            labels.insert(LABEL_EFFORT.into(), json!(effort));
        }
        labels
    }

    /// Puts the session's context size on its own hc terminal: the tokens and the compaction window. Zero
    /// tokens removes the size. `extra` are more labels for the same call. A failure never fails the hook.
    fn show_context(&self, tokens: i64, extra: Map<String, Value>) {
        let terminal = self.terminal();
        if terminal.is_empty() {
            return;
        }
        let mut labels = extra;
        labels.insert(LABEL_CONTEXT.into(), json!(""));
        if tokens > 0 {
            labels.insert(LABEL_CONTEXT.into(), json!(tokens.to_string()));
            labels.insert(LABEL_CONTEXT_LIMIT.into(), json!(self.number("CLAUDE_CODE_AUTO_COMPACT_WINDOW", DEFAULT_COMPACT_AT).to_string()));
        }
        let _ = self.core.call("terminal.label", json!({"terminal": terminal, "labels": labels}));
    }

    /// The file that says that this session was asked to save its state. None when the input has no session.
    fn marker(&self) -> Option<PathBuf> {
        let name = Path::new(&self.input.session_id).file_name()?.to_string_lossy().into_owned();
        let tmp = (self.env)("TMPDIR");
        let dir = if tmp.is_empty() { std::env::temp_dir() } else { PathBuf::from(tmp) };
        Some(dir.join("herdr-contr-orch-hook").join(name + ".asked"))
    }

    fn asked(&self) -> bool {
        self.marker().is_some_and(|m| m.exists())
    }

    fn mark_asked(&self) -> Result<(), String> {
        let Some(m) = self.marker() else { return Ok(()) };
        m.parent().map_or(Ok(()), fs::create_dir_all).and_then(|_| fs::write(&m, "")).map_err(|e| format!("{}: {e}", m.display()))
    }

    fn clear_asked(&self) {
        if let Some(m) = self.marker() {
            let _ = fs::remove_file(m);
        }
    }

    /// The session's transcript holds a run of the orchestrate command. None when there is no transcript.
    fn ran_orchestrate(&self) -> Result<bool, String> {
        if self.input.transcript_path.is_empty() {
            return Ok(false);
        }
        match ran_command(Path::new(&self.input.transcript_path), ORCHESTRATE_COMMAND) {
            Ok(ran) => Ok(ran),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(format!("{}: {e}", self.input.transcript_path)),
        }
    }

    /// The session is a coordinator: it ran the orchestrate command, or its hc terminal has role orchestrator.
    fn coordinator(&self) -> Result<bool, String> {
        Ok(self.ran_orchestrate()? || self.orchestrator_role()?)
    }

    fn orchestrator_role(&self) -> Result<bool, String> {
        let terminal = self.terminal();
        if terminal.is_empty() {
            return Ok(false);
        }
        Ok(Manager::new(self.core, String::new()).get(&terminal)?.role == ROLE_ORCHESTRATOR)
    }

    /// What a coordinator gets after a compaction: the state file to read, its live workers, its open owner
    /// items, the flow and the playbook, cut to RESTORE_MAX characters.
    fn restore_text(&self) -> String {
        let state = Path::new(&self.input.cwd).join(STATE_FILE);
        let head = format!(
            "contr: the conversation was compacted. You are the coordinator, and the playbook follows again.\nFirst read {}: you wrote it before the compaction.\n",
            state.display()
        );
        self.with_lists(head)
    }

    /// What a new coordinator gets: as `restore_text`, with the state file only when there is one.
    fn startup_text(&self) -> String {
        let mut head = String::from("contr: you are the coordinator of this project; the owner gives the goal in the next message.\n");
        let state = Path::new(&self.input.cwd).join(STATE_FILE);
        if state.exists() {
            head += &format!("First read {}: an earlier coordinator of this project wrote it.\n", state.display());
        }
        self.with_lists(head)
    }

    /// The lists come before the playbook, so a cut takes only playbook text.
    fn with_lists(&self, head: String) -> String {
        cut(format!("{head}\n{}\n{}\n{}\nThe playbook:\n\n{}", self.workers_text(), self.owner_text(), flow_text(self.env), self.playbook()), RESTORE_MAX)
    }

    /// The body of the kit's orchestrate command, without its front matter. The goal is in the state file.
    fn playbook(&self) -> String {
        let mut kit = (self.env)("CLAUDE_PLUGIN_ROOT");
        if kit.is_empty() {
            kit = (self.env)("CONTR_KIT");
        }
        let fail = |why: String| format!("The playbook could not be read ({why}). Run /{ORCHESTRATE_COMMAND} with your goal to read it again.");
        if kit.is_empty() {
            return fail("no kit folder".into());
        }
        let body = match fs::read_to_string(Path::new(&kit).join("commands").join("orchestrate.md")) {
            Ok(b) => b,
            Err(e) => return fail(e.to_string()),
        };
        let body = body.strip_prefix("---\n").and_then(|rest| rest.split_once("\n---\n")).map_or(body.as_str(), |(_, after)| after);
        body.replace("$ARGUMENTS", "(see the state file)").trim().to_string()
    }

    /// The live workers that this coordinator started.
    fn workers_text(&self) -> String {
        let terminal = self.terminal();
        let workers = match Manager::new(self.core, String::new()).list(false) {
            Ok(w) => w,
            Err(e) => return format!("Your live workers: unknown ({e}); use the list tool.\n"),
        };
        let mut text = String::from("Your live workers:\n");
        let mine: Vec<_> = workers.iter().filter(|w| !terminal.is_empty() && w.parent == terminal).collect();
        for w in &mine {
            text += &format!("- {} ({}) {}, {}", w.name, w.address, w.role, if w.status.trim().is_empty() { "no status" } else { &w.status });
            if !w.issue.is_empty() {
                text += &format!(", issue {}", w.issue);
            }
            if !w.worktree.is_empty() {
                text += &format!(", {}", w.worktree);
            }
            text.push('\n');
        }
        if mine.is_empty() {
            text += "- none\n";
        }
        text
    }

    /// The open questions and to-dos that this coordinator asked.
    fn owner_text(&self) -> String {
        let terminal = self.terminal();
        let (questions, todos) = match owner_store(self.env).and_then(|s| s.read()) {
            Ok(state) => state.open(""),
            Err(e) => return format!("Open owner items: unknown ({e}); use the owner_items tool.\n"),
        };
        let mut text = String::from("Your open owner items:\n");
        let before = text.len();
        for q in questions.iter().filter(|q| q.coordinator == terminal) {
            text += &format!("- {} {}\n", q.id, one_line(if q.title.trim().is_empty() { &q.question } else { &q.title }));
        }
        for t in todos.iter().filter(|t| t.coordinator == terminal) {
            text += &format!("- {} [{}] {}\n", t.id, t.kind, one_line(&t.text));
        }
        if text.len() == before {
            text += "- none\n";
        }
        text
    }
}
