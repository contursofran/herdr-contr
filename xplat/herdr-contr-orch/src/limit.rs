//! The limits of a coordinator (#212): the workers at once of the flow, and the open pull requests of a
//! project. Spawn and a message to a settled worker check them before a worker starts work.
use std::process::Command;

use serde_json::Value;

use crate::config::Config;
use crate::workers::Manager;

/// Refuses when the caller's live workers that are `working` (reviewers included) are at the flow's limit. The
/// error names them. Outside hc (no caller) it checks nothing.
pub fn check_workers(m: &Manager, cfg: &Config) -> Result<(), String> {
    if m.caller().is_empty() {
        return Ok(());
    }
    let busy: Vec<String> = m.list(false)?.into_iter().filter(|w| w.live && w.status == "working" && w.parent == m.caller()).map(|w| w.name).collect();
    if (busy.len() as i64) < cfg.flow.workers {
        return Ok(());
    }
    Err(format!("agents: {} workers at once is the flow's limit; working now: {}. Wait for a report.", cfg.flow.workers, busy.join(", ")))
}

/// Refuses a message that starts a settled worker (live and not `working`) of the caller past the flow's limit.
pub fn check_message(m: &Manager, cfg: &Config, name: &str) -> Result<(), String> {
    if m.caller().is_empty() {
        return Ok(());
    }
    let w = m.get(name)?;
    if !w.live || w.status == "working" || w.parent != m.caller() {
        return Ok(());
    }
    check_workers(m, cfg)
}

/// The open pull requests of the repository at `main`: one `gh pr list` call. `gh` is the program (CONTR_GH, else
/// gh). Each item is the PR number and its head branch.
fn open_prs(gh: &str, main: &str) -> Result<Vec<(i64, String)>, String> {
    let out = Command::new(gh)
        .current_dir(main)
        .args(["pr", "list", "--state", "open", "--limit", "200", "--json", "number,headRefName"])
        .output()
        .map_err(|e| format!("agents: {gh} pr list: {e}"))?;
    if !out.status.success() {
        return Err(format!("agents: {gh} pr list: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    let v: Value = serde_json::from_slice(&out.stdout).map_err(|e| format!("agents: {gh} pr list: {e}"))?;
    Ok(v.as_array().map(|a| a.iter().map(|p| (p["number"].as_i64().unwrap_or(0), p["headRefName"].as_str().unwrap_or_default().to_string())).collect()).unwrap_or_default())
}

/// Refuses a worker for a new issue when its project (the main checkout of `worktree`) is at or over its open PR
/// limit. A fix round is allowed: `fix_pr` is set, or the worktree's branch has an open PR.
pub fn check_prs(cfg: &Config, env: &dyn Fn(&str) -> String, worktree: &str, fix_pr: bool) -> Result<(), String> {
    if cfg.open_prs.is_empty() || fix_pr || worktree.is_empty() {
        return Ok(());
    }
    let Ok(main) = crate::spawn::main_checkout(worktree) else { return Ok(()) };
    let home = env("HOME");
    let limit = cfg.open_prs.iter().find_map(|(k, n)| {
        let key = match k.strip_prefix("~/") {
            Some(rest) if !home.is_empty() => std::path::Path::new(&home).join(rest).display().to_string(),
            _ => k.clone(),
        };
        crate::spawn::same_path(&key, &main).then_some(*n)
    });
    let Some(limit) = limit else { return Ok(()) };
    let gh = env("CONTR_GH");
    let prs = open_prs(if gh.is_empty() { "gh" } else { &gh }, &main)?;
    let branch = crate::spawn::git(worktree, &["branch", "--show-current"]).unwrap_or_default();
    if !branch.is_empty() && prs.iter().any(|(_, b)| *b == branch) {
        return Ok(());
    }
    if (prs.len() as i64) < limit {
        return Ok(());
    }
    let nums: Vec<String> = prs.iter().map(|(n, _)| format!("#{n}")).collect();
    Err(format!("agents: {} open PRs is the project's limit ({}); land one first, or spawn a fix round on an open PR (--fix-pr)", limit, nums.join(", ")))
}
