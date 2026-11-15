//! `agents spawn`: starts a worker in an hc terminal, with its profile, its worktree and its labels.
//! Ported from contr's `internal/agents` (Spawn, the setup wrapper, the review worktree) and the clean wrapper
//! of its hc client. The status line and the terminal colors are not here: the status line comes with the
//! hooks (#160), and the colors are those of the contr app.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{json, Map, Value};

use crate::config::Config;
use crate::workers::{self, is_claude, Core, Worker, CLEAN_NAME, SETUP_NAME};

/// Holds the main checkout of a worker's worktree while the setup command runs, so the command can copy files
/// from it.
pub const ENV_MAIN: &str = "CONTR_MAIN";
/// Holds the hc terminal that a worker reports to.
pub const ENV_COORDINATOR: &str = "CONTR_COORDINATOR";
/// The folders that the clean wrapper adds after the PATH of the hc core.
pub const ENV_PATH: &str = "CONTR_PATH";

/// Runs the setup command ($1) in the worktree, then starts the agent (the words after it). A failed setup ends
/// the terminal with the setup's exit code, so the parent gets a note and the output stays readable.
pub const SETUP_WRAPPER: &str = r#"/bin/sh -c "$1" || { s=$?; echo "contr: worktree setup failed (exit $s)" >&2; exit $s; }; shift; exec "$@""#;

/// Unsets the Claude Code session variables of the hc core's environment, so Claude Code saves its transcript,
/// adds the folders of CONTR_PATH after PATH, then runs its arguments. terminal.run cannot unset a name.
pub const CLEAN_SCRIPT: &str = r#"for v in $(env | sed -n -E 's/^(CLAUDECODE|CLAUDE_CODE_[A-Za-z0-9_]*)=.*/\1/p'); do unset "$v"; done; if [ -n "$CONTR_PATH" ]; then PATH="$PATH:$CONTR_PATH"; export PATH; unset CONTR_PATH; fi; exec "$@""#;

/// A worker to start.
#[derive(Clone, Debug, Default)]
pub struct Spec {
    /// Unique among live terminals: 1-32 of a-z 0-9 - _
    pub name: String,
    pub profile: String,
    /// "" for the profile name.
    pub role: String,
    pub issue: String,
    /// The working folder, or "".
    pub worktree: String,
    /// The first message, or "".
    pub task: String,
    /// The hc terminal that gets notes about the worker, or "".
    pub parent: String,
    /// The project name; no control characters.
    pub project: String,
    /// A few words of what the worker does, or ""; no control characters.
    pub title: String,
}

fn words(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

/// Runs git in a folder and gives its trimmed standard output. -C, not the folder of the process, so a missing
/// folder gives git's own message.
fn git(dir: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().map_err(|e| format!("git {}: {e}", args[0]))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(format!("git {}: {}", args[0], if stderr.is_empty() { out.status.to_string() } else { stderr }));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// `dir` as an absolute path to a folder. hc resolves a relative cwd from its own folder, not from the caller's.
fn worktree_dir(dir: &str) -> Result<String, String> {
    let abs = std::path::absolute(dir).map_err(|e| format!("agents: worktree {dir}: {e}"))?;
    let meta = fs::metadata(&abs).map_err(|e| format!("agents: worktree {}: {e}", abs.display()))?;
    if !meta.is_dir() {
        return Err(format!("agents: worktree {} is not a folder", abs.display()));
    }
    Ok(abs.display().to_string())
}

/// The main checkout of the git worktree `dir`.
fn main_checkout(dir: &str) -> Result<String, String> {
    let common = PathBuf::from(git(dir, &["rev-parse", "--path-format=absolute", "--git-common-dir"])?);
    match common.parent() {
        Some(main) if common.file_name().is_some_and(|n| n == ".git") => Ok(main.display().to_string()),
        _ => Err(format!("agents: {dir} has no main checkout (git folder {})", common.display())),
    }
}

/// Tells if a and b name the same folder, after symbolic links.
fn same_path(a: &str, b: &str) -> bool {
    let real = |p: &str| fs::canonicalize(p).unwrap_or_else(|_| PathBuf::from(p));
    real(a) == real(b)
}

/// The setup command of the project that `dir` is a worktree of, and its main checkout. The main checkout
/// itself gets no setup: the owner prepares it. A folder outside git gets none.
fn setup_for(cfg: &Config, env: &dyn Fn(&str) -> String, dir: &str) -> Option<(String, String)> {
    if cfg.setup.is_empty() || dir.is_empty() {
        return None;
    }
    let main = main_checkout(dir).ok().filter(|m| !same_path(m, dir))?;
    let home = env("HOME");
    cfg.setup.iter().find_map(|(k, v)| {
        let key = match k.strip_prefix("~/") {
            Some(rest) if !home.is_empty() => Path::new(&home).join(rest).display().to_string(),
            _ => k.clone(),
        };
        same_path(&key, &main).then(|| (v.clone(), main.clone()))
    })
}

/// Makes a detached worktree at the head of pull request `pr` of the repository at `repo` (any of its
/// checkouts), and gives its path: `<main checkout>-wt/review-<pr>`. A review worktree that is there moves to
/// the new head. The head comes from the origin remote's refs/pull/<pr>/head.
pub fn review_worktree(repo: &str, pr: i64) -> Result<String, String> {
    if pr <= 0 {
        return Err("agents: a pull request number is positive".into());
    }
    let main = main_checkout(repo)?;
    let reference = format!("refs/pull/{pr}/head");
    let line = git(&main, &["ls-remote", "origin", &reference])?;
    let sha = line.split('\t').next().unwrap_or_default();
    if sha.is_empty() {
        return Err(format!("agents: origin has no pull request {pr}"));
    }
    git(&main, &["fetch", "--quiet", "origin", &reference])?;
    let dir = format!("{main}-wt/review-{pr}");
    if Path::new(&dir).exists() {
        git(&dir, &["checkout", "--quiet", "--detach", sha])?;
    } else {
        git(&main, &["worktree", "add", "--quiet", "--detach", &dir, sha])?;
    }
    Ok(dir)
}

/// Tells whether args set `flag`, as `flag v` or `flag=v`.
fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a.strip_prefix(flag).is_some_and(|r| r.is_empty() || r.starts_with('=')))
}

/// The profile's command. With a kit, a claude command gets `--plugin-dir <kit>` after the program name, unless
/// it has `--plugin-dir` already. When its agent has a definition in the kit, it also gets
/// `--agent contr:<agent>`.
pub fn agent_argv(kit: &str, agent: &str, argv: &[String]) -> Vec<String> {
    if !is_claude(argv) || kit.is_empty() {
        return argv.to_vec();
    }
    let mut out = vec![argv[0].clone()];
    if !has_flag(&argv[1..], "--plugin-dir") {
        out.extend(words(&["--plugin-dir", kit]));
    }
    if Path::new(kit).join("agents").join(format!("{agent}.md")).exists() {
        out.extend(words(&["--agent", &format!("contr:{agent}")]));
    }
    out.extend_from_slice(&argv[1..]);
    out
}

/// The PATH of this process, then the folders where the tools of a worker usually are. "" with no PATH. The
/// hc core's PATH can be the short one of launchd.
fn spawn_path(env: &dyn Fn(&str) -> String) -> String {
    let mut dirs: Vec<String> = env("PATH").split(':').map(str::to_string).collect();
    if dirs.iter().all(String::is_empty) {
        return String::new();
    }
    let home = env("HOME");
    if !home.is_empty() {
        dirs.push(format!("{home}/.local/bin"));
    }
    dirs.extend(words(&["/opt/homebrew/bin", "/usr/local/bin"]));
    let mut out: Vec<String> = Vec::new();
    for d in dirs {
        if !d.is_empty() && !out.contains(&d) {
            out.push(d);
        }
    }
    out.join(":")
}

/// Starts a worker and gives it. `env` reads the environment of this process.
pub fn spawn(core: &dyn Core, cfg: &Config, env: &dyn Fn(&str) -> String, mut s: Spec) -> Result<Worker, String> {
    let Some(p) = cfg.profiles.get(&s.profile) else {
        return Err(cfg.no_profile(&s.profile));
    };
    if s.name.is_empty() {
        return Err("agents: a worker needs a name".into());
    }
    if s.project.chars().any(char::is_control) {
        return Err("agents: the project name has a control character".into());
    }
    s.title = workers::check_title(&s.title)?;
    if !s.worktree.is_empty() {
        s.worktree = worktree_dir(&s.worktree)?;
    }
    let role = if s.role.is_empty() { &s.profile } else { &s.role };
    let mut labels = Map::new();
    for (k, v) in [
        (workers::LABEL_APP, workers::APP_AGENT),
        (workers::LABEL_ROLE, role.as_str()),
        (workers::LABEL_PROFILE, s.profile.as_str()),
        (workers::LABEL_WORKTREE, s.worktree.as_str()),
    ] {
        labels.insert(k.into(), json!(v));
    }
    if !s.project.is_empty() {
        labels.insert(workers::LABEL_PROJECT.into(), json!(s.project));
    }
    if !s.issue.is_empty() {
        labels.insert(workers::LABEL_ISSUE.into(), json!(s.issue));
    }
    if !s.title.is_empty() {
        labels.insert(workers::LABEL_TITLE.into(), json!(s.title));
    }
    // A relative kit path is made absolute, because the agent runs in its worktree.
    let kit = if cfg.kit.is_empty() { String::new() } else { std::path::absolute(&cfg.kit).map(|k| k.display().to_string()).unwrap_or_else(|_| cfg.kit.clone()) };
    let agent = if p.agent.is_empty() { &s.profile } else { &p.agent };
    let mut argv = agent_argv(&kit, agent, &p.argv);
    if !s.task.is_empty() {
        argv.push(s.task.clone());
    }
    let mut vars = Map::new();
    if is_claude(&argv) {
        argv = [words(&["/bin/sh", "-c", CLEAN_SCRIPT, CLEAN_NAME]), argv].concat();
        let path = spawn_path(env);
        if !path.is_empty() {
            vars.insert(ENV_PATH.into(), json!(path));
        }
    }
    if let Some((setup, main)) = setup_for(cfg, env, &s.worktree) {
        argv = [words(&["/bin/sh", "-c", SETUP_WRAPPER, SETUP_NAME, &setup]), argv].concat();
        vars.insert(ENV_MAIN.into(), json!(main));
    }
    let mut params = Map::new();
    params.insert("argv".into(), json!(argv));
    params.insert("name".into(), json!(s.name));
    params.insert("labels".into(), Value::Object(labels));
    params.insert("wait".into(), json!(false));
    params.insert("outlive_parent".into(), json!(true));
    if !s.worktree.is_empty() {
        params.insert("cwd".into(), json!(s.worktree));
    }
    if s.parent.is_empty() {
        params.insert("no_parent".into(), json!(true));
    } else {
        vars.insert(ENV_COORDINATOR.into(), json!(s.parent));
        params.insert("parent".into(), json!(s.parent));
    }
    if !vars.is_empty() {
        params.insert("env".into(), Value::Object(vars));
    }
    Ok(Worker::from_terminal(&core.call("terminal.run", Value::Object(params))?))
}

/// The task of a spawn: the text, or the content of `file`. It refuses both at once, and a file that is missing
/// or holds only blanks, so a quoting slip or a wrong path never starts a worker with a wrong brief.
pub fn spawn_task(text: &str, file: &str) -> Result<String, String> {
    if file.is_empty() {
        return Ok(text.to_string());
    }
    if !text.trim().is_empty() {
        return Err("give the task as text or with a task file, not both".into());
    }
    let task = fs::read_to_string(file).map_err(|e| format!("task file: {file}: {e}"))?;
    if task.trim().is_empty() {
        return Err(format!("task file {file} is empty"));
    }
    Ok(task)
}
