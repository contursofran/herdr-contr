//! A worker's start again after a crash (#210). Spawn gives a Claude Code worker a session id
//! (`--session-id`) and keeps it with the worker's name, profile, worktree and issue in `workers/<name>.json`
//! of the state folder. Stop removes the record. A record with no live worker is `gone`: resume starts
//! `claude --resume <id>` with the same profile in the same worktree, and sends `RESUME_TEXT`.
use std::fs;
use std::io::Read;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::flowseen::state_dir;
use crate::spawn::{spawn, Spec};
use crate::workers::{Core, Manager, Worker};

/// The first message of a resumed worker.
pub const RESUME_TEXT: &str = "Continue from your last step";
/// The status of a worker that ended without a stop.
pub const GONE: &str = "gone";

/// What spawn keeps of a worker, so it can start again.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Record {
    pub name: String,
    pub session: String,
    pub profile: String,
    pub role: String,
    pub worktree: String,
    pub issue: String,
    pub project: String,
    pub title: String,
    pub parent: String,
}

fn folder(env: &dyn Fn(&str) -> String) -> Option<PathBuf> {
    state_dir(env).map(|d| d.join("workers"))
}

/// The record file of `name`. A name with a path in it has none.
fn file(env: &dyn Fn(&str) -> String, name: &str) -> Option<PathBuf> {
    if name.is_empty() || name.contains(['/', '\\']) || name.starts_with('.') {
        return None;
    }
    folder(env).map(|d| d.join(format!("{name}.json")))
}

/// A new random session id (UUID version 4).
pub fn new_session_id() -> Result<String, String> {
    let mut b = [0u8; 16];
    fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b)).map_err(|e| format!("agents: session id: {e}"))?;
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    Ok(format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32]))
}

/// Keeps a record. With no state folder it keeps nothing.
pub fn save(env: &dyn Fn(&str) -> String, r: &Record) -> Result<(), String> {
    let Some(path) = file(env, &r.name) else { return Ok(()) };
    let dir = path.parent().unwrap_or(&path);
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let body = serde_json::to_vec_pretty(r).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, body).and_then(|_| fs::rename(&tmp, &path)).map_err(|e| format!("{}: {e}", path.display()))
}

/// The record of `name`, or an error that says there is none.
pub fn load(env: &dyn Fn(&str) -> String, name: &str) -> Result<Record, String> {
    let path = file(env, name).ok_or_else(|| format!("agents: no record of a worker {name:?}"))?;
    let body = fs::read(&path).map_err(|_| format!("agents: no record of a worker {name:?}; only a worker that spawn started can resume"))?;
    serde_json::from_slice(&body).map_err(|e| format!("{}: {e}", path.display()))
}

/// Removes the record of `name`, if any.
pub fn forget(env: &dyn Fn(&str) -> String, name: &str) {
    if let Some(p) = file(env, name) {
        let _ = fs::remove_file(p);
    }
}

/// All records, sorted by name.
pub fn all(env: &dyn Fn(&str) -> String) -> Vec<Record> {
    let Some(dir) = folder(env) else { return Vec::new() };
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<Record> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter_map(|p| serde_json::from_slice(&fs::read(p).ok()?).ok())
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Marks the workers that are gone: a record with no live worker of that name. A finished terminal gets the
/// `gone` status; a record with no terminal at all is added. With a caller, only the caller's records count.
pub fn mark_gone(env: &dyn Fn(&str) -> String, caller: &str, ws: &mut Vec<Worker>) {
    for r in all(env) {
        if !caller.is_empty() && r.parent != caller {
            continue;
        }
        if ws.iter().any(|w| w.name == r.name && w.live) {
            continue;
        }
        match ws.iter_mut().find(|w| w.name == r.name) {
            Some(w) => w.status = GONE.into(),
            None => ws.push(Worker { name: r.name, role: r.role, issue: r.issue, profile: r.profile, worktree: r.worktree, project: r.project, parent: r.parent, title: r.title, status: GONE.into(), ..Worker::default() }),
        }
    }
    ws.sort_by(|a, b| (&a.name, &a.address).cmp(&(&b.name, &b.address)));
}

/// Starts the worker `name` again from its recorded session, with the same profile, worktree and labels, and
/// `RESUME_TEXT` as its first message. It refuses a worker that is still live, and counts against the flow's
/// workers at once. The coordinator is the caller, else the recorded parent.
pub fn resume(core: &dyn Core, cfg: &Config, env: &dyn Fn(&str) -> String, name: &str) -> Result<Worker, String> {
    let r = load(env, name)?;
    let m = Manager::new(core, env("HC_TERMINAL_ID"));
    if m.list(false)?.iter().any(|w| w.name == r.name && w.live) {
        return Err(format!("agents: {name} is still live; send it a message instead"));
    }
    crate::limit::check_workers(&m, cfg)?;
    let parent = if m.caller().is_empty() { r.parent.clone() } else { m.caller().to_string() };
    let spec = Spec { name: r.name, profile: r.profile, role: r.role, issue: r.issue, worktree: r.worktree, task: RESUME_TEXT.into(), parent, project: r.project, title: r.title, resume: r.session };
    spawn(core, cfg, env, spec)
}
