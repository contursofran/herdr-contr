//! The command line: `agents flow`, and `owner ask | answer | done`. Ported from contr's `contr agents` and
//! `contr owner`; the worker commands of `agents` come with the next slice (#160).
use std::io::Write;
use std::path::PathBuf;

use crate::config::{Config, Flow};
use crate::owner::{Question, Store};

pub const USAGE: &str = "Usage:
  herdr-contr-orch agents flow             print the flow: how the coordinator takes an issue to a merge
  herdr-contr-orch owner <command> [args…] answer questions and mark to-dos done (herdr-contr-orch owner help)
  herdr-contr-orch --help, -h              print this text
";

pub const AGENTS_USAGE: &str = "Usage:
  herdr-contr-orch agents flow   print the flow: how the coordinator takes an issue to a merge

Environment:
  CONTR_AGENTS_CONFIG   profiles file (default: the user config folder, contr/agents.json).
                        Built-in profiles: orchestrator (claude opus, medium effort), builder,
                        planner, reviewer, debugger, explorer. A profile in the file replaces the
                        built-in one of the same name. A file with \"presets\" and no
                        \"profiles\" is the old format: it is ignored and help says so.
                        {\"kit\": DIR, \"profiles\": {NAME: {\"argv\": [...], \"agent\": KIT_AGENT}},
                         \"setup\": {MAIN_CHECKOUT: SHELL_COMMAND},
                         \"flow\": {\"plan\": \"never|when-needed|always\", \"review\": \"none|code|every\",
                                  \"debugger_after\": N, \"workers\": N, \"merge\": \"ask|auto\",
                                  \"profiles\": {ROLE: PROFILE}}}
";

pub const OWNER_USAGE: &str = "Usage:
  herdr-contr-orch owner ask [flags] <question…> ask a question; prints its id (D1, …)
  herdr-contr-orch owner answer <id> <answer…>   answer a question (D1, …) and tell its coordinator
  herdr-contr-orch owner done <id>               mark a to-do (T1, …) done and tell its coordinator

Flags of ask (each of option, impact and ref can repeat):
  --title T         a 2-4 word name
  --from F          the worker, issue or PR it is about
  --option O        a short answer to pick from
  --impact I        what choosing the option means; the nth impact is for the nth option
  --recommended N   index (from 0) of the recommended option
  --ref R           what helps to decide: \"PR 12\", \"#12\" or \"issue 12\", an http(s) URL, a file path
  --source S        a short excerpt of the text that raised the question

answer and done do what the question card does: the owner file changes first, then the
coordinator that asked gets \"Owner answered <id>: <answer>\" or
\"Owner did <id>: <text>\". When the coordinator cannot be reached the change
stays saved and the command exits 1.

Environment:
  CONTR_OWNER_FILE   the owner file (default: the user config folder, contr/owner.json)
";

/// Tells a coordinator's terminal what the owner did.
pub trait Coordinator {
    fn send(&mut self, terminal: &str, text: &str) -> Result<(), String>;
}

/// The coordinator of this slice: the send to a terminal comes with the worker commands (#160).
pub struct NoCoordinator;

impl Coordinator for NoCoordinator {
    fn send(&mut self, _terminal: &str, _text: &str) -> Result<(), String> {
        Err("this tool cannot send to a terminal yet".into())
    }
}

/// The user config folder: on macOS ~/Library/Application Support; else $XDG_CONFIG_HOME, or ~/.config.
fn config_dir(env: &dyn Fn(&str) -> String) -> Result<PathBuf, String> {
    let home = env("HOME");
    if cfg!(target_os = "macos") {
        if home.is_empty() {
            return Err("$HOME is not defined".into());
        }
        return Ok(PathBuf::from(home).join("Library/Application Support"));
    }
    let xdg = env("XDG_CONFIG_HOME");
    if !xdg.is_empty() {
        return Ok(PathBuf::from(xdg));
    }
    if home.is_empty() {
        return Err("neither $XDG_CONFIG_HOME nor $HOME are defined".into());
    }
    Ok(PathBuf::from(home).join(".config"))
}

/// The profiles file: CONTR_AGENTS_CONFIG, or contr/agents.json in the user config folder.
pub fn config_path(env: &dyn Fn(&str) -> String) -> PathBuf {
    let p = env("CONTR_AGENTS_CONFIG");
    if !p.is_empty() {
        return PathBuf::from(p);
    }
    config_dir(env).map(|d| d.join("contr/agents.json")).unwrap_or_default()
}

/// The owner file: CONTR_OWNER_FILE, or contr/owner.json in the user config folder.
pub fn owner_store(env: &dyn Fn(&str) -> String) -> Result<Store, String> {
    let p = env("CONTR_OWNER_FILE");
    if !p.is_empty() {
        return Ok(Store::new(p));
    }
    Ok(Store::new(config_dir(env)?.join("contr/owner.json")))
}

/// CONTR_PROJECT, or the name of the folder the session runs in.
fn project_name(env: &dyn Fn(&str) -> String) -> String {
    let p = env("CONTR_PROJECT");
    if !p.is_empty() {
        return p;
    }
    std::env::current_dir().ok().and_then(|d| d.file_name().map(|n| n.to_string_lossy().into_owned())).unwrap_or_default()
}

/// The flow block of the profiles file, or the default flow with the error when the file cannot be read.
pub fn flow_text(env: &dyn Fn(&str) -> String) -> String {
    match Config::load(&config_path(env)) {
        Ok(cfg) => cfg.flow.text(),
        Err(e) => format!("The flow could not be read ({e}); the default flow follows.\n{}", Flow::default().text()),
    }
}

fn is_help(a: &str) -> bool {
    a == "help" || a == "-h" || a == "--help"
}

/// Runs the command line (`args` without the program name). The exit code: 0 on success, 1 on an error, 2 on a
/// usage error.
pub fn run(args: &[String], env: &dyn Fn(&str) -> String, out: &mut dyn Write, err: &mut dyn Write, coordinator: &mut dyn Coordinator) -> i32 {
    match args.first().map(String::as_str) {
        Some("agents") => run_agents(&args[1..], env, out, err),
        Some("owner") => run_owner(&args[1..], env, out, err, coordinator),
        Some(a) if is_help(a) => {
            let _ = out.write_all(USAGE.as_bytes());
            0
        }
        Some(a) => {
            let _ = write!(err, "herdr-contr-orch: unknown argument {a}\n{USAGE}");
            2
        }
        None => {
            let _ = err.write_all(USAGE.as_bytes());
            2
        }
    }
}

fn run_agents(args: &[String], env: &dyn Fn(&str) -> String, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let Some(sub) = args.first() else {
        let _ = err.write_all(AGENTS_USAGE.as_bytes());
        return 2;
    };
    if is_help(sub) {
        let _ = write!(out, "{AGENTS_USAGE}\n{}", flow_text(env));
        if let Ok(cfg) = Config::load(&config_path(env)) {
            if !cfg.notice.is_empty() {
                let _ = write!(out, "\nNote: {}.\n", cfg.notice);
            }
        }
        return 0;
    }
    if sub != "flow" {
        let _ = write!(err, "herdr-contr-orch agents: unknown command {sub}\n{AGENTS_USAGE}");
        return 2;
    }
    if args.len() > 1 {
        let _ = writeln!(err, "herdr-contr-orch agents flow: takes no arguments");
        return 2;
    }
    match Config::load(&config_path(env)) {
        Ok(cfg) => {
            let _ = out.write_all(cfg.flow.text().as_bytes());
            0
        }
        Err(e) => {
            let _ = writeln!(err, "herdr-contr-orch agents: {e}");
            1
        }
    }
}

fn run_owner(args: &[String], env: &dyn Fn(&str) -> String, out: &mut dyn Write, err: &mut dyn Write, coordinator: &mut dyn Coordinator) -> i32 {
    let Some(sub) = args.first().map(String::as_str) else {
        let _ = err.write_all(OWNER_USAGE.as_bytes());
        return 2;
    };
    if is_help(sub) {
        let _ = out.write_all(OWNER_USAGE.as_bytes());
        return 0;
    }
    let rest = &args[1..];
    if sub == "ask" {
        return run_owner_ask(rest, env, out, err);
    }
    let answer = rest.get(1..).unwrap_or_default().join(" ");
    match sub {
        "answer" if rest.len() >= 2 && !answer.trim().is_empty() => {}
        "done" if rest.len() == 1 => {}
        "answer" | "done" => {
            let _ = write!(err, "herdr-contr-orch owner {sub}: wrong number of arguments\n{OWNER_USAGE}");
            return 2;
        }
        _ => {
            let _ = write!(err, "herdr-contr-orch owner: unknown command {sub}\n{OWNER_USAGE}");
            return 2;
        }
    }
    let store = match owner_store(env) {
        Ok(s) => s,
        Err(e) => {
            let _ = writeln!(err, "herdr-contr-orch owner: {e}");
            return 1;
        }
    };
    let (terminal, text) = match owner_change(&store, sub, &rest[0], &answer) {
        Ok(t) => t,
        Err(e) => {
            let _ = writeln!(err, "herdr-contr-orch owner {sub}: {e}");
            return 1;
        }
    };
    if terminal.is_empty() {
        return 0;
    }
    if let Err(e) = coordinator.send(&terminal, &text) {
        let _ = writeln!(err, "herdr-contr-orch owner {sub}: saved; coordinator not reached: {e}");
        return 1;
    }
    0
}

/// Saves the answer or the done mark. Gives the coordinator to tell ("" when no one asked) and the text for it.
fn owner_change(store: &Store, sub: &str, id: &str, answer: &str) -> Result<(String, String), String> {
    let st = store.read()?;
    let ids = [id.to_string()];
    if sub == "answer" {
        let terminal = st.questions.iter().find(|q| q.id == id).map(|q| q.coordinator.clone()).unwrap_or_default();
        store.decide(&ids, answer, "owner")?;
        return Ok((terminal, format!("Owner answered {id}: {answer}\n")));
    }
    let todo = st.todos.iter().find(|t| t.id == id).cloned().unwrap_or_default();
    store.unpin(&ids)?;
    Ok((todo.coordinator, format!("Owner did {id}: {}\n", todo.text)))
}

/// `owner ask`: adds a question to the owner file for the project of the session and prints its id.
fn run_owner_ask(args: &[String], env: &dyn Fn(&str) -> String, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let mut q = Question::default();
    let mut words: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        i += 1;
        if a == "--" {
            words.extend(args[i..].iter().map(String::as_str));
            break;
        }
        // A flag is -name or --name, with its value after "=" or as the next argument. The first word that is
        // no flag starts the question.
        let Some(flag) = a.strip_prefix("--").or_else(|| a.strip_prefix('-')).filter(|f| !f.is_empty()) else {
            words.push(a);
            words.extend(args[i..].iter().map(String::as_str));
            break;
        };
        let (name, mut value) = match flag.split_once('=') {
            Some((n, v)) => (n, Some(v.to_string())),
            None => (flag, None),
        };
        if !["title", "from", "option", "impact", "recommended", "ref", "source"].contains(&name) {
            let _ = write!(err, "herdr-contr-orch owner ask: flag provided but not defined: -{name}\n{OWNER_USAGE}");
            return 2;
        }
        if value.is_none() {
            let Some(v) = args.get(i) else {
                let _ = writeln!(err, "herdr-contr-orch owner ask: flag needs an argument: -{name}");
                return 2;
            };
            value = Some(v.clone());
            i += 1;
        }
        let value = value.unwrap_or_default();
        match name {
            "title" => q.title = value,
            "from" => q.from = value,
            "source" => q.source = value,
            "option" => q.options.push(value),
            "impact" => q.impacts.push(value),
            "ref" => q.refs.push(value),
            _ => match value.parse::<i64>() {
                Ok(n) => q.recommended = Some(n),
                Err(_) => {
                    let _ = writeln!(err, "herdr-contr-orch owner ask: --recommended {value:?} is not a number");
                    return 2;
                }
            },
        }
    }
    q.question = words.join(" ").trim().to_string();
    if q.question.is_empty() {
        let _ = write!(err, "herdr-contr-orch owner ask: give the question\n{OWNER_USAGE}");
        return 2;
    }
    // -1 is "none", as in the Go tool.
    q.recommended = q.recommended.filter(|r| *r >= 0);
    if q.recommended.is_some_and(|r| r >= q.options.len() as i64) || q.impacts.len() > q.options.len() {
        let _ = writeln!(err, "herdr-contr-orch owner ask: --recommended and --impact need a matching --option");
        return 2;
    }
    q.project = project_name(env);
    q.coordinator = env("HC_TERMINAL_ID");
    let store = match owner_store(env) {
        Ok(s) => s,
        Err(e) => {
            let _ = writeln!(err, "herdr-contr-orch owner: {e}");
            return 1;
        }
    };
    match store.ask(q) {
        Ok(q) => {
            let _ = writeln!(out, "{}", q.id);
            0
        }
        Err(e) => {
            let _ = writeln!(err, "herdr-contr-orch owner ask: {e}");
            1
        }
    }
}
