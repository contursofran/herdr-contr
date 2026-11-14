//! The command line: `agents flow | spawn | ls | send | interrupt | stop | rm | read | prompt`, and
//! `owner ask | answer | done`. Ported from contr's `contr agents` and `contr owner`. `agents watch` is not
//! ported: no part of the kit or of this repository calls it.
use std::io::{BufRead, Write};
use std::path::PathBuf;

use crate::config::{Config, Flow};
use crate::owner::{Question, Store};
use crate::spawn::{review_worktree, spawn, spawn_task, Spec};
use crate::workers::{self, Core, Manager, Worker};

pub const USAGE: &str = "Usage:
  herdr-contr-orch agents <command> [args…] list and control the workers (herdr-contr-orch agents help)
  herdr-contr-orch hook <name>              the kit's hooks: stop, session-start and prompt (see hook help)
  herdr-contr-orch mcp                      serve the coordinator's tools over MCP on stdin and stdout
  herdr-contr-orch owner <command> [args…]  answer questions and mark to-dos done (herdr-contr-orch owner help)
  herdr-contr-orch --help, -h               print this text
";

pub const AGENTS_USAGE: &str = "Usage:
  herdr-contr-orch agents ls [--all] [--json]      list the workers
  herdr-contr-orch agents spawn --name N --profile P [--issue I] [--worktree DIR] [--project P]
                     [--role R] [--review-pr PR] [--no-parent] [--json] [--any-profile]
                     [--task-file F | [--] task…]
                                                   start a worker; --task-file reads the
                                                   task from a file; --review-pr makes a
                                                   detached worktree at the pull request's
                                                   head (<main>-wt/review-<PR>) of the
                                                   repository at --worktree (default: here)
                                                   and starts the worker there; a profile
                                                   that the flow names for no role needs
                                                   --any-profile
  herdr-contr-orch agents send <name> <text…>      type text and press Enter
  herdr-contr-orch agents interrupt <name>         press the stop key
  herdr-contr-orch agents stop <name> [--keep]     end the worker's program and remove its
                                                   terminal; --keep leaves the finished one
  herdr-contr-orch agents rm <name>                remove a finished worker's terminal
  herdr-contr-orch agents rm --ended               remove all finished workers that this
                                                   terminal started
  herdr-contr-orch agents read <name> [--lines N]  print the screen and history
  herdr-contr-orch agents prompt <name>            print the menu that a blocked worker waits
                                                   on: the tool and its command, the question
                                                   and the numbered choices; \"no prompt\"
                                                   when it waits on none
  herdr-contr-orch agents flow                     print the flow: how the coordinator takes an issue to a merge

Flags can come before or after the other arguments. After --, every word is
an argument.

Environment:
  HC_SOCKET, HC_SESSION the hc core to talk to (default: the session \"default\")
  HC_TERMINAL_ID        set inside hc: the terminal that this tool runs in; spawn makes
                        it the parent of the worker
  CONTR_KIT             the Claude Code plugin folder of the kit; it replaces \"kit\" of
                        the profiles file
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
pub(crate) fn project_name(env: &dyn Fn(&str) -> String) -> String {
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
pub fn run(args: &[String], env: &dyn Fn(&str) -> String, input: &mut dyn BufRead, out: &mut dyn Write, err: &mut dyn Write, core: &dyn Core) -> i32 {
    match args.first().map(String::as_str) {
        Some("mcp") if args.len() == 1 => run_mcp(env, input, out, err, core),
        Some("hook") => crate::hook::run(&args[1..], env, input, out, err, core),
        Some("agents") => run_agents(&args[1..], env, out, err, core),
        Some("owner") => run_owner(&args[1..], env, out, err, core),
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

/// `mcp`: the worker tools and the owner tools over MCP, until stdin ends.
fn run_mcp(env: &dyn Fn(&str) -> String, input: &mut dyn BufRead, out: &mut dyn Write, err: &mut dyn Write, core: &dyn Core) -> i32 {
    // A broken profiles file or no place for the owner file stops the server at its start.
    let store = match crate::tools::load_config(env).and_then(|_| owner_store(env)) {
        Ok(s) => s,
        Err(e) => {
            let _ = writeln!(err, "herdr-contr-orch mcp: {e}");
            return 1;
        }
    };
    let (project, coordinator) = (project_name(env), env("HC_TERMINAL_ID"));
    let mut tools = crate::tools::agent_tools(core, env);
    tools.extend(crate::tools::owner_tools(&store, &project, &coordinator));
    if let Err(e) = crate::mcp::serve("herdr-contr-orch", env!("CARGO_PKG_VERSION"), &tools, input, out) {
        let _ = writeln!(err, "herdr-contr-orch mcp: {e}");
        return 1;
    }
    0
}

fn run_agents(args: &[String], env: &dyn Fn(&str) -> String, out: &mut dyn Write, err: &mut dyn Write, core: &dyn Core) -> i32 {
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
    if ["ls", "spawn", "send", "interrupt", "stop", "rm", "read", "prompt"].contains(&sub.as_str()) {
        return run_worker_command(sub, &args[1..], env, out, err, core);
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

/// The flags that take a value; the others are true or false.
const VALUE_FLAGS: [&str; 9] = ["lines", "name", "profile", "issue", "worktree", "project", "role", "task-file", "review-pr"];
const BOOL_FLAGS: [&str; 6] = ["all", "json", "keep", "ended", "no-parent", "any-profile"];

/// The flags of the worker commands that the command line gave, in order, each with its value.
#[derive(Default)]
struct Flags {
    seen: Vec<(String, String)>,
}

impl Flags {
    fn text(&self, name: &str) -> String {
        self.seen.iter().rev().find(|(n, _)| n == name).map(|(_, v)| v.clone()).unwrap_or_default()
    }
    fn on(&self, name: &str) -> bool {
        self.text(name) == "true"
    }
    fn number(&self, name: &str) -> i64 {
        self.text(name).parse().unwrap_or(0)
    }
}

/// Reads the flags before and after the other arguments, which it gives in order. A flag is -name or --name;
/// a flag with a value takes it after "=" or as the next argument. After "--" every word is an argument.
fn parse_flags(args: &[String]) -> Result<(Flags, Vec<String>), String> {
    let mut f = Flags::default();
    let mut pos = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        i += 1;
        if a == "--" {
            pos.extend(args[i..].iter().cloned());
            break;
        }
        let Some(flag) = a.strip_prefix("--").or_else(|| a.strip_prefix('-')).filter(|x| !x.is_empty()) else {
            pos.push(a.to_string());
            continue;
        };
        let (name, value) = match flag.split_once('=') {
            Some((n, v)) => (n, Some(v.to_string())),
            None => (flag, None),
        };
        let value = if VALUE_FLAGS.contains(&name) {
            let Some(v) = value.or_else(|| args.get(i).cloned().inspect(|_| i += 1)) else {
                return Err(format!("flag needs an argument: -{name}"));
            };
            if ["lines", "review-pr"].contains(&name) && v.parse::<i64>().is_err() {
                return Err(format!("--{name} {v:?} is not a number"));
            }
            v
        } else if BOOL_FLAGS.contains(&name) {
            match value.as_deref() {
                None | Some("true") => "true".to_string(),
                Some("false") => "false".to_string(),
                Some(v) => return Err(format!("--{name} {v:?} is not true or false")),
            }
        } else {
            return Err(format!("flag provided but not defined: -{name}"));
        };
        f.seen.push((name.to_string(), value));
    }
    Ok((f, pos))
}

/// `agents ls | send | interrupt | stop | rm | read`: one command on the workers of the hc core.
fn run_worker_command(sub: &str, rest: &[String], env: &dyn Fn(&str) -> String, out: &mut dyn Write, err: &mut dyn Write, core: &dyn Core) -> i32 {
    let (flags, pos) = match parse_flags(rest) {
        Ok(x) => x,
        Err(e) => {
            let _ = writeln!(err, "herdr-contr-orch agents {sub}: {e}");
            return 2;
        }
    };
    let allowed: &[&str] = match sub {
        "ls" => &["all", "json"],
        "spawn" => &["name", "profile", "issue", "worktree", "project", "role", "no-parent", "json", "task-file", "review-pr", "any-profile"],
        "read" => &["lines"],
        "stop" => &["keep"],
        "rm" => &["ended"],
        _ => &[],
    };
    if let Some(bad) = flags.seen.iter().map(|(n, _)| n).find(|n| !allowed.contains(&n.as_str())) {
        let _ = write!(err, "herdr-contr-orch agents {sub}: flag --{bad} does not apply\n{AGENTS_USAGE}");
        return 2;
    }
    let count_ok = match sub {
        "ls" => pos.is_empty(),
        "send" => pos.len() >= 2,
        "rm" | "spawn" => true,
        _ => pos.len() == 1,
    };
    if !count_ok {
        let _ = write!(err, "herdr-contr-orch agents {sub}: wrong number of arguments\n{AGENTS_USAGE}");
        return 2;
    }
    if sub == "rm" && (flags.on("ended") == (pos.len() == 1) || pos.len() > 1) {
        let _ = write!(err, "herdr-contr-orch agents rm: give one name, or --ended\n{AGENTS_USAGE}");
        return 2;
    }
    // Only spawn needs the profiles, so a broken file does not block the rest.
    let mut cfg = Config::default();
    if sub == "spawn" {
        cfg = match Config::load(&config_path(env)) {
            Ok(c) => c,
            Err(e) => {
                let _ = writeln!(err, "herdr-contr-orch agents: {e}");
                return 1;
            }
        };
        let kit = env("CONTR_KIT");
        if !kit.is_empty() {
            cfg.kit = kit;
        }
    }
    let m = Manager::new(core, env("HC_TERMINAL_ID"));
    let mut text = String::new();
    let res = match sub {
        "spawn" => spawn_worker(&flags, &pos, &cfg, env, core).and_then(|w| {
            text = if flags.on("json") { serde_json::to_string_pretty(&w).map_err(|e| e.to_string())? + "\n" } else { format!("{} {}\n", w.name, w.address) };
            Ok(())
        }),
        "ls" => m.list(flags.on("all")).and_then(|ws| {
            text = if flags.on("json") { serde_json::to_string_pretty(&ws).map_err(|e| e.to_string())? + "\n" } else { workers::table(&ws) };
            Ok(())
        }),
        "send" => m.send(&pos[0], &pos[1..].join(" ")),
        "interrupt" => m.interrupt(&pos[0]).map(|o| text = o + "\n"),
        "stop" => m.stop(&pos[0], flags.on("keep")),
        "rm" if !flags.on("ended") => m.remove(&pos[0]),
        "prompt" => pending_prompt(&m, &pos[0]).map(|t| text = t + "\n"),
        "rm" => {
            let mut names = Vec::new();
            let res = m.remove_ended(&mut names);
            text = names.iter().map(|n| format!("{n}\n")).collect();
            res
        }
        _ => m.read(&pos[0], flags.number("lines")).map(|t| text = t.trim_end_matches('\n').to_string() + "\n"),
    };
    let _ = out.write_all(text.as_bytes());
    if let Err(e) = res {
        let _ = writeln!(err, "herdr-contr-orch agents {sub}: {e}");
        return 1;
    }
    0
}

/// How many terminal lines `agents prompt` reads to find the menu.
const PROMPT_LINES: i64 = 40;

/// The menu that a blocked worker waits on, or "no prompt" when the worker is not blocked or its screen has no
/// menu that this tool can read.
pub(crate) fn pending_prompt(m: &Manager, name: &str) -> Result<String, String> {
    let w = m.get(name)?;
    if !w.live || w.status != "blocked" {
        return Ok("no prompt".into());
    }
    let screen = m.read(name, PROMPT_LINES)?;
    Ok(crate::prompt::parse(&w.command, &w.kind, &screen).map(|p| p.text()).unwrap_or_else(|| "no prompt".into()))
}

/// `agents spawn`: the task, the check of the profile against the flow, the review worktree, then the worker.
fn spawn_worker(flags: &Flags, pos: &[String], cfg: &Config, env: &dyn Fn(&str) -> String, core: &dyn Core) -> Result<Worker, String> {
    let task = spawn_task(&pos.join(" "), &flags.text("task-file"))?;
    let profile = flags.text("profile");
    if !flags.on("any-profile") {
        cfg.check_profile(&profile)?;
    }
    let mut spec = Spec { name: flags.text("name"), profile, role: flags.text("role"), issue: flags.text("issue"), worktree: flags.text("worktree"), project: flags.text("project"), task, ..Spec::default() };
    let pr = flags.number("review-pr");
    if pr != 0 {
        spec.worktree = review_worktree(if spec.worktree.is_empty() { "." } else { &spec.worktree }, pr)?;
    }
    if !flags.on("no-parent") {
        spec.parent = env("HC_TERMINAL_ID");
    }
    spawn(core, cfg, env, spec)
}

fn run_owner(args: &[String], env: &dyn Fn(&str) -> String, out: &mut dyn Write, err: &mut dyn Write, core: &dyn Core) -> i32 {
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
    if let Err(e) = Manager::new(core, env("HC_TERMINAL_ID")).send(&terminal, &text) {
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
