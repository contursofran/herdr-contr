//! The workers: hc terminals with the label contr=agent and labels for their role, issue, profile and worktree.
//! This file lists them, sends to them, and stops, removes and reads them. Ported from contr's
//! `internal/agents`; spawn, prompt and watch come with the next parts of the slice (#160).
use herdr_core::hc::HcClient;
use herdr_core::HerdrError;
use serde::Serialize;
use serde_json::{json, Map, Value};

pub const LABEL_APP: &str = "contr";
pub const APP_AGENT: &str = "agent";
pub const LABEL_ROLE: &str = "role";
pub const LABEL_ISSUE: &str = "issue";
pub const LABEL_PROFILE: &str = "profile";
pub const LABEL_WORKTREE: &str = "worktree";
pub const LABEL_PROJECT: &str = "project";
/// Set by the kit's Stop hook on a Claude Code session's own terminal: the short model name and the effort.
pub const LABEL_MODEL: &str = "contr.model";
pub const LABEL_EFFORT: &str = "contr.effort";
/// Set by the kit's Stop hook on a session that ran the orchestrate command.
pub const LABEL_ORCHESTRATING: &str = "contr.orchestrating";
/// Set by the kit's Stop hook: the context size in tokens and the window that Claude Code compacts at.
pub const LABEL_CONTEXT: &str = "contr.context";
pub const LABEL_CONTEXT_LIMIT: &str = "contr.context-limit";

pub const ROLE_ORCHESTRATOR: &str = "orchestrator";
/// The role of a plain terminal: a shell, not an agent.
pub const ROLE_TERMINAL: &str = "terminal";

/// `$0` of the setup wrapper and of the clean wrapper that spawn puts before the agent's command.
pub const SETUP_NAME: &str = "contr-setup";
pub const CLEAN_NAME: &str = "contr-clean-env";

/// The part of the hc client that the workers use: one call, its result or the error as text.
pub trait Core {
    fn call(&self, method: &str, params: Value) -> Result<Value, String>;
}

impl Core for HcClient {
    fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        // hc answers these only after the program reacted, so they get the long timeout.
        let slow = matches!(method, "terminal.send" | "terminal.interrupt" | "terminal.kill");
        HcClient::call(self, method, params, slow.then_some(self.send_timeout)).map_err(|e| match e {
            HerdrError::Server { code, message } => format!("hc: {message} ({code})"),
            e => format!("hc: {e}"),
        })
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

/// One worker as hc reports it. The JSON keys are those of the Go tool.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Worker {
    pub name: String,
    pub address: String,
    pub role: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub issue: String,
    pub profile: String,
    pub worktree: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub project: String,
    /// The hc terminal that started the worker, if any.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub parent: String,
    /// The program that runs in the terminal now: hc's foreground name, else the base name of the spawn command.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub command: String,
    /// False once the program ended.
    pub live: bool,
    /// The program's own word: idle, working, done, blocked or error. Empty when it never said.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub status: String,
    /// permission, question or auth, when blocked.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub kind: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub msg: String,
    pub turn: u64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i64>,
    /// The hook's label, else the --model of the program's command line. The label follows /model.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub model: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub effort: String,
    #[serde(skip_serializing_if = "is_false")]
    pub orchestrating: bool,
    /// Both are zero when a label is missing or is not a positive integer.
    #[serde(skip_serializing_if = "is_zero")]
    pub context_tokens: u64,
    #[serde(skip_serializing_if = "is_zero")]
    pub context_limit: u64,
    /// True for a terminal that this tool did not start: it has no contr labels.
    #[serde(skip_serializing_if = "is_false")]
    pub foreign: bool,
    /// True for a plain terminal that runs Claude Code in the foreground.
    #[serde(skip_serializing_if = "is_false")]
    pub session: bool,
}

fn base(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// The value after `flag` (or `flag=`) in argv, or "".
pub fn flag_value(argv: &[String], flag: &str) -> String {
    for (i, a) in argv.iter().enumerate() {
        if a == flag && i + 1 < argv.len() {
            return argv[i + 1].clone();
        }
        if let Some(v) = a.strip_prefix(flag).and_then(|r| r.strip_prefix('=')) {
            return v.to_string();
        }
    }
    String::new()
}

/// argv less the setup wrapper and the clean wrapper, in any nesting.
pub fn program(mut argv: &[String]) -> &[String] {
    loop {
        if argv.len() > 5 && argv[3] == SETUP_NAME {
            argv = &argv[5..];
        } else if argv.len() > 4 && argv[3] == CLEAN_NAME {
            argv = &argv[4..];
        } else {
            return argv;
        }
    }
}

/// Tells whether argv runs the claude command.
pub fn is_claude(argv: &[String]) -> bool {
    argv.first().is_some_and(|a| base(a) == "claude")
}

/// Claude Code names itself by its version as foreground program, such as 2.1.30.
<<<<<<< HEAD
pub(crate) fn is_version(name: &str) -> bool {
||||||| eb09bcd
=======
fn is_version(name: &str) -> bool {
>>>>>>> origin/main
    let mut rest = name;
    for part in 0..3 {
        let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        if digits == 0 {
            return false;
        }
        rest = &rest[digits..];
        if part < 2 {
            let Some(r) = rest.strip_prefix('.') else { return false };
            rest = r;
        }
    }
    true
}

impl Worker {
    /// The worker of an hc terminal record.
    pub fn from_terminal(t: &Value) -> Worker {
        let text = |v: &Value| v.as_str().unwrap_or_default().to_string();
        let label = |k: &str| text(&t["labels"][k]);
        let argv: Vec<String> = t["argv"].as_array().map(|a| a.iter().map(text).collect()).unwrap_or_default();
        let front = text(&t["foreground"]["name"]);
        let claude_in_front = base(&front) == "claude" || is_version(base(&front));
        let or = |a: String, b: String| if a.is_empty() { b } else { a };
        let mut w = Worker {
            name: text(&t["name"]),
            address: text(&t["address"]),
            role: label(LABEL_ROLE),
            issue: label(LABEL_ISSUE),
            profile: label(LABEL_PROFILE),
            worktree: label(LABEL_WORKTREE),
            project: label(LABEL_PROJECT),
            parent: text(&t["parent"]),
            command: or(front, program(&argv).first().map(|a| base(a).to_string()).unwrap_or_default()),
            live: t["state"] != "finished",
            status: text(&t["status"]["state"]),
            kind: text(&t["status"]["kind"]),
            msg: text(&t["status"]["msg"]),
            turn: t["turn"].as_u64().unwrap_or(0),
            title: text(&t["title"]),
            exit_code: t["exit_code"].as_i64(),
            model: or(label(LABEL_MODEL), flag_value(&argv, "--model")),
            effort: or(label(LABEL_EFFORT), flag_value(&argv, "--effort")),
            orchestrating: !label(LABEL_ORCHESTRATING).is_empty(),
            ..Worker::default()
        };
        let tokens = label(LABEL_CONTEXT).parse::<u64>().unwrap_or(0);
        let limit = label(LABEL_CONTEXT_LIMIT).parse::<u64>().unwrap_or(0);
        if tokens > 0 && limit > 0 {
            (w.context_tokens, w.context_limit) = (tokens, limit);
        }
        if label(LABEL_APP) != APP_AGENT {
            // A terminal with no contr labels: the role is orchestrator for Claude Code (an hc status is one that
            // only an agent sets), else terminal; the folder is the one that hc reports.
            w.foreign = true;
            w.worktree = text(&t["cwd"]);
            if w.name.is_empty() {
                w.name = w.address.clone();
            }
            let claude = t["status"].is_object() || claude_in_front || is_claude(program(&argv));
            w.role = if claude { ROLE_ORCHESTRATOR } else { ROLE_TERMINAL }.to_string();
        } else if w.role == ROLE_TERMINAL && claude_in_front {
            // The owner ran Claude Code in a plain terminal: while it runs, it is a Claude session.
            w.role = ROLE_ORCHESTRATOR.to_string();
            w.session = true;
        }
        w
    }

    pub fn orchestrator(&self) -> bool {
        self.role == ROLE_ORCHESTRATOR
    }

    pub fn plain(&self) -> bool {
        self.role == ROLE_TERMINAL
    }
}

/// Lists and controls the workers through `core`. `caller` is the hc terminal this tool runs in
/// (HC_TERMINAL_ID), or "" outside hc.
pub struct Manager<'a> {
    core: &'a dyn Core,
    caller: String,
}

impl<'a> Manager<'a> {
    pub fn new(core: &'a dyn Core, caller: impl Into<String>) -> Manager<'a> {
        Manager { core, caller: caller.into() }
    }

    /// The params of a call that looks at, acts on or sends to a terminal. Inside hc they name this terminal as
    /// the caller. When it is the worker's parent, hc counts the worker's news as seen and types no note about
    /// it into the parent: a coordinator that stops a worker gets no exit note. On a send between two agents,
    /// hc tags the text with the sender.
    fn looking(&self, name: &str) -> Map<String, Value> {
        let mut p = Map::new();
        p.insert("terminal".into(), json!(name));
        if !self.caller.is_empty() {
            p.insert("caller".into(), json!(self.caller));
        }
        p
    }

    /// The workers, sorted by name. With `all`, finished ones too.
    pub fn list(&self, all: bool) -> Result<Vec<Worker>, String> {
        let res = self.core.call("terminal.list", json!({"all": all, "labels": {LABEL_APP: APP_AGENT}}))?;
        let mut out: Vec<Worker> = res["terminals"].as_array().map(|a| a.iter().map(Worker::from_terminal).collect()).unwrap_or_default();
        out.sort_by(|a, b| (&a.name, &a.address).cmp(&(&b.name, &b.address)));
        Ok(out)
    }

    /// One worker by name or address.
    pub fn get(&self, name: &str) -> Result<Worker, String> {
        Ok(Worker::from_terminal(&self.core.call("terminal.get", Value::Object(self.looking(name)))?))
    }

    /// Types text into a worker and presses Enter. Blank text is an error: hc would still submit its sender
    /// tag, an empty message. A send that hc refuses is an error with hc's reason.
    pub fn send(&self, name: &str, text: &str) -> Result<(), String> {
        if text.trim().is_empty() {
            return Err("agents: no text to send".into());
        }
        let mut p = self.looking(name);
        p.insert("text".into(), json!(text));
        let res = self.core.call("terminal.send", Value::Object(p))?;
        if res["sent"] == false {
            let reason = res["reason"].as_str().filter(|r| !r.is_empty()).unwrap_or("no reason given");
            return Err(format!("agents: {name} did not take the text: {reason}"));
        }
        Ok(())
    }

    /// Presses a worker's stop key. The outcome is hc's: stopped, exited, still_working or unknown.
    pub fn interrupt(&self, name: &str) -> Result<String, String> {
        let res = self.core.call("terminal.interrupt", Value::Object(self.looking(name)))?;
        Ok(res["outcome"].as_str().unwrap_or_default().to_string())
    }

    /// Ends a worker's program and then removes its terminal from hc. With `keep`, the finished terminal stays
    /// for reading. A terminal that already finished is not killed again.
    pub fn stop(&self, name: &str, keep: bool) -> Result<(), String> {
        if self.get(name)?.live {
            self.core.call("terminal.kill", Value::Object(self.looking(name)))?;
        }
        if keep {
            return Ok(());
        }
        self.remove(name)
    }

    /// Deletes a finished terminal from hc. hc refuses a live one.
    pub fn remove(&self, name: &str) -> Result<(), String> {
        self.core.call("terminal.rm", Value::Object(self.looking(name))).map(|_| ())
    }

    /// Removes the finished workers that the caller started, and adds their names to `removed`. Orchestrators
    /// and plain terminals stay, and so do the workers of other coordinators. Outside hc it removes nothing.
    pub fn remove_ended(&self, removed: &mut Vec<String>) -> Result<(), String> {
        if self.caller.is_empty() {
            return Ok(());
        }
        for w in self.list(true)? {
            if w.live || w.orchestrator() || w.plain() || w.parent != self.caller {
                continue;
            }
            self.remove(&w.address).map_err(|e| format!("remove {}: {e}", w.name))?;
            removed.push(w.name);
        }
        Ok(())
    }

    /// A worker's screen and history; `lines` > 0 keeps only the last lines.
    pub fn read(&self, name: &str, lines: i64) -> Result<String, String> {
        let mut p = self.looking(name);
        if lines > 0 {
            p.insert("lines".into(), json!(lines));
        }
        let res = self.core.call("terminal.read", Value::Object(p))?;
        Ok(res["text"].as_str().unwrap_or_default().to_string())
    }
}

/// The state column of the list: the program's status, why it is blocked, or how it ended.
pub fn state_text(w: &Worker) -> String {
    match (w.live, w.exit_code) {
        (false, Some(code)) => format!("ended ({code})"),
        (false, None) => "ended".into(),
        _ if w.status == "blocked" && !w.kind.is_empty() => format!("blocked: {}", w.kind),
        _ if !w.status.is_empty() => w.status.clone(),
        _ => "-".into(),
    }
}

/// The list as a table: each column as wide as its longest cell, plus two spaces.
pub fn table(ws: &[Worker]) -> String {
    let mut rows = vec![["NAME", "ADDRESS", "ROLE", "ISSUE", "STATE", "WORKTREE"].map(String::from)];
    for w in ws {
        let issue = if w.issue.is_empty() { "-".to_string() } else { w.issue.clone() };
        rows.push([w.name.clone(), w.address.clone(), w.role.clone(), issue, state_text(w), w.worktree.clone()]);
    }
    let width = |i: usize| rows.iter().map(|r| r[i].chars().count()).max().unwrap_or(0) + 2;
    let widths: Vec<usize> = (0..5).map(width).collect();
    let mut out = String::new();
    for r in &rows {
        for (cell, w) in r.iter().zip(&widths) {
            out.push_str(cell);
            out.extend(std::iter::repeat_n(' ', w - cell.chars().count()));
        }
        out.push_str(&r[5]);
        out.push('\n');
    }
    out
}
