//! The coordinator's tools, for the MCP server: the workers on the hc core, and the questions and to-dos for
//! the owner. Ported from contr's `contr mcp`.
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::cli::pending_prompt;
use crate::home::config_path;
use crate::config::Config;
use crate::mcp::{Run, Tool};
use crate::owner::{Question, Store, Todo};
use crate::spawn::{review_worktree, spawn, spawn_task, Spec};
use crate::workers::{Core, Manager};

type Env<'a> = &'a dyn Fn(&str) -> String;

/// The schema of a tool's arguments. Each argument is its name, its type ("string[]" for a list of strings)
/// and its description.
fn schema(required: &[&str], props: &[(&str, &str, &str)]) -> Value {
    let mut p = Map::new();
    for (name, kind, desc) in props {
        let v = if *kind == "string[]" { json!({"type": "array", "items": {"type": "string"}, "description": desc}) } else { json!({"type": kind, "description": desc}) };
        p.insert(name.to_string(), v);
    }
    let mut s = json!({"type": "object", "properties": p});
    if !required.is_empty() {
        s["required"] = json!(required);
    }
    s
}

fn pretty<T: serde::Serialize>(v: &T) -> Result<String, String> {
    serde_json::to_string_pretty(v).map_err(|e| e.to_string())
}

/// The profiles file, read now, with CONTR_KIT over its kit.
pub fn load_config(env: Env) -> Result<Config, String> {
    let mut cfg = Config::load(&config_path(env))?;
    let kit = env("CONTR_KIT");
    if !kit.is_empty() {
        cfg.kit = kit;
    }
    Ok(cfg)
}

/// The arguments of the worker tools. An unknown argument is an error, so a misnamed field (message for text)
/// does not become an empty value.
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct AgentArgs {
    name: String,
    profile: String,
    task: String,
    task_file: String,
    issue: String,
    worktree: String,
    project: String,
    role: String,
    text: String,
    lines: i64,
    all: bool,
    keep: bool,
    review_pr: i64,
    any_profile: bool,
}

/// The tools on the workers. The coordinator's hc terminal is HC_TERMINAL_ID; spawn makes it the parent of the
/// worker.
pub fn agent_tools<'a>(core: &'a dyn Core, env: Env<'a>) -> Vec<Tool<'a>> {
    fn run<'a>(core: &'a dyn Core, env: Env<'a>, f: impl Fn(&Manager, AgentArgs) -> Result<String, String> + 'a) -> Run<'a> {
        Box::new(move |raw| {
            let a = AgentArgs::deserialize(raw).map_err(|e| format!("bad arguments: {e}"))?;
            f(&Manager::new(core, env("HC_TERMINAL_ID")), a)
        })
    }
    let name = ("name", "string", "Worker name.");
    vec![
        Tool {
            name: "spawn",
            description: "Start a worker agent in a new hc terminal, in its worktree. It reports back with hc send when its turn ends.",
            schema: schema(
                &["name", "profile"],
                &[
                    ("name", "string", "Unique worker name: 1-32 of a-z 0-9 - _ (for example b-158)."),
                    ("profile", "string", "builder, planner, reviewer or debugger, or a profile from the profiles file."),
                    ("task", "string", "The self-contained brief: the first message. Give task or task_file."),
                    ("task_file", "string", "An absolute path to a file that holds the brief, instead of task."),
                    ("issue", "string", "Issue number."),
                    ("worktree", "string", "The worker's working folder (its git worktree)."),
                    ("role", "string", "Role label; the profile name by default."),
                    ("project", "string", "Project name; groups the worker with its project's questions."),
                    ("any_profile", "boolean", "Allow a profile that the flow does not name for a role. Use only when the owner asks."),
                    ("review_pr", "integer", "A pull request number: start the worker in a detached worktree at the PR's head, <main checkout>-wt/review-<N>, of the repository at worktree (default: the coordinator's folder)."),
                ],
            ),
            // The profiles are read again for each spawn, so a profile added while the session runs works at once.
            run: run(core, env, move |_, a| {
                let cfg = load_config(env)?;
                if !a.any_profile {
                    cfg.check_profile(&a.profile)?;
                }
                let task = spawn_task(&a.task, &a.task_file)?;
                if task.trim().is_empty() {
                    return Err("give the brief as task or task_file".into());
                }
                let mut worktree = a.worktree;
                if a.review_pr != 0 {
                    worktree = review_worktree(if worktree.is_empty() { "." } else { &worktree }, a.review_pr)?;
                }
                let spec = Spec { name: a.name, profile: a.profile, task, issue: a.issue, worktree, role: a.role, project: a.project, parent: env("HC_TERMINAL_ID") };
                pretty(&spawn(core, &cfg, env, spec)?)
            }),
        },
        Tool {
            name: "message",
            description: "Type a message into a worker and press Enter. It keeps the worker's context.",
            schema: schema(&["name", "text"], &[name, ("text", "string", "The message.")]),
            run: run(core, env, |m, a| m.send(&a.name, &a.text).map(|_| "sent".into())),
        },
        Tool {
            name: "interrupt",
            description: "Press a worker's stop key to end its turn; it keeps its context. Returns hc's outcome.",
            schema: schema(&["name"], &[name]),
            run: run(core, env, |m, a| m.interrupt(&a.name)),
        },
        Tool {
            name: "stop",
            description: "End a worker's program and remove its terminal from hc. Its context is lost. Read its RESULT.md first.",
            schema: schema(&["name"], &[name, ("keep", "boolean", "Keep the finished terminal so it can still be read. Default false.")]),
            run: run(core, env, |m, a| m.stop(&a.name, a.keep).map(|_| "stopped".into())),
        },
        Tool {
            name: "read",
            description: "Read a worker's screen and history.",
            schema: schema(&["name"], &[name, ("lines", "integer", "Only the last lines.")]),
            run: run(core, env, |m, a| m.read(&a.name, a.lines)),
        },
        Tool {
            name: "prompt",
            description: "Show the menu a blocked worker waits on: for a permission, the tool and its command, then the question and the numbered choices. \"no prompt\" when it waits on none.",
            schema: schema(&["name"], &[name]),
            run: run(core, env, |m, a| pending_prompt(m, &a.name)),
        },
        Tool {
            name: "list",
            description: "List the workers with their role, issue, state and worktree.",
            schema: schema(&[], &[("all", "boolean", "Include finished workers.")]),
            run: run(core, env, |m, a| pretty(&m.list(a.all)?)),
        },
        Tool {
            name: "clear_ended",
            description: "Remove the finished terminals of the workers this coordinator started. Other coordinators' workers and plain terminals stay. Returns the names removed.",
            schema: schema(&[], &[]),
            run: run(core, env, |m, _| {
                let mut names = Vec::new();
                m.remove_ended(&mut names)?;
                pretty(&names)
            }),
        },
    ]
}

/// The arguments of the owner tools.
#[derive(Default, Deserialize)]
#[serde(default)]
struct OwnerArgs {
    question: String,
    options: Vec<String>,
    recommended: Option<i64>,
    impacts: Vec<String>,
    refs: Vec<String>,
    source: String,
    title: String,
    from: String,
    text: String,
    kind: String,
    ids: Vec<String>,
    answer: String,
    by: String,
    all: bool,
}

/// The tools that keep questions and to-dos for the owner in the owner file, for `project`. `coordinator` is
/// the terminal that asks: the answer goes there.
pub fn owner_tools<'a>(store: &'a Store, project: &'a str, coordinator: &'a str) -> Vec<Tool<'a>> {
    fn run<'a>(f: impl Fn(OwnerArgs) -> Result<String, String> + 'a) -> Run<'a> {
        Box::new(move |raw| f(OwnerArgs::deserialize(raw).map_err(|e| format!("bad arguments: {e}"))?))
    }
    vec![
        Tool {
            name: "ask_owner",
            description: "Ask the owner a question only they can answer (a product rule, a design choice, an approval). It is kept until it is decided. Returns its id (D1, D2, …). contr shows it in the questions card, and sends the answer to you as `Owner answered <ID>: <answer>`. When the owner answers in chat, close it with decided.",
            schema: schema(
                &["question"],
                &[
                    ("question", "string", "The question, with enough context to answer it from there alone."),
                    ("options", "string[]", "Short answers to pick from (2-4)."),
                    ("recommended", "integer", "Index in options of your recommendation."),
                    ("title", "string", "A 2-4 word name."),
                    ("from", "string", "The worker, issue or PR it is about."),
                    ("impacts", "string[]", "One line per option, in the order of options: what choosing it means. The sheet shows each under its option."),
                    ("refs", "string[]", "What helps the owner decide: `PR 12` (opens the PR pane), `#12` or `issue 12` (opens in the browser), an http or https URL, or a file path. At most a few."),
                    ("source", "string", "A short excerpt (a few lines) of the text that raised the question, such as a worker's report."),
                ],
            ),
            run: run(move |a| {
                let q = Question {
                    project: project.into(),
                    coordinator: coordinator.into(),
                    from: a.from,
                    title: a.title,
                    question: a.question,
                    options: a.options,
                    recommended: a.recommended,
                    impacts: a.impacts,
                    refs: a.refs,
                    source: a.source,
                    ..Question::default()
                };
                Ok(store.ask(q)?.id)
            }),
        },
        Tool {
            name: "decided",
            description: "Close questions: ones the owner answered in chat (by owner), or ones you settled yourself because they are not the owner's (by coordinator).",
            schema: schema(&["ids", "answer", "by"], &[("ids", "string[]", "Question ids, such as D3."), ("answer", "string", "The answer, short."), ("by", "string", "owner or coordinator.")]),
            run: run(move |a| store.decide(&a.ids, &a.answer, &a.by).map(|_| "decided".into())),
        },
        Tool {
            name: "pin_for_owner",
            description: "Pin one line for the owner: something to do (kind do: test a build, merge a PR that waits for them) or to know (kind know). Returns its id (T1, T2, …).",
            schema: schema(&["text"], &[("text", "string", "One short line with all it takes to act."), ("kind", "string", "do (the default) or know."), ("from", "string", "The worker, issue or PR it is about.")]),
            run: run(move |a| Ok(store.pin(Todo { project: project.into(), coordinator: coordinator.into(), from: a.from, kind: a.kind, text: a.text, ..Todo::default() })?.id)),
        },
        Tool {
            name: "unpin_for_owner",
            description: "Remove pinned items once they are done or no longer needed.",
            schema: schema(&["ids"], &[("ids", "string[]", "Ids such as T3.")]),
            run: run(move |a| store.unpin(&a.ids).map(|_| "unpinned".into())),
        },
        Tool {
            name: "owner_items",
            description: "List the open questions and pinned items of this project (all projects with all).",
            schema: schema(&[], &[("all", "boolean", "All projects.")]),
            run: run(move |a| {
                let (questions, todos) = store.read()?.open(if a.all { "" } else { project });
                pretty(&json!({"questions": questions, "todos": todos}))
            }),
        },
    ]
}
