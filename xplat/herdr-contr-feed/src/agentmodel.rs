//! `model`: the agent's model and effort, for the sidebar's agent rows. Read only; nothing goes to the agent.
//!
//! The model: the newest one the session log names, a reply or a `/model` switch (`tail::model`).
//! The effort, the first source that has it:
//! - Claude Code: herdr-contr's status line record (when on; it follows `/effort`), then the effort of a `/model`
//!   switch newer than the last reply, then `--effort <level>` on the
//!   agent's command line (contr profiles pass it), then Claude Code's settings: `.claude/settings.local.json`, then
//!   `.claude/settings.json` in the agent's cwd, then the user's `settings.json`. In each file,
//!   `modelSettings.<model>.effortLevel` for the log's model, then a top-level `effortLevel`.
//! - Codex: the newest `turn_context` in the rollout (it follows `/model`), then `-c model_reasoning_effort=<level>`
//!   on the command line, then `model_reasoning_effort` in Codex's `config.toml`.

use crate::status::shell_words;
use crate::tail;
use herdr_core::resolver::Proc;
use herdr_core::statusline::StatusLineFolder;
use serde_json::{json, Value};
use std::path::Path;
use std::time::SystemTime;

/// What the agent runs on. Either part can be unknown.
#[derive(Debug, Default, PartialEq)]
pub struct ModelInfo {
    pub model: Option<String>,
    pub effort: Option<String>,
}

pub fn line(kind: &str, m: &ModelInfo) -> Value {
    json!({"type": "model", "agent": kind, "model": m.model, "effort": m.effort})
}

/// The files and processes to read. `procs`: the pane's processes and their children.
pub struct Sources<'a> {
    pub log: Option<&'a Path>,
    /// Claude Code's settings.json.
    pub claude_settings: &'a Path,
    /// The agent's cwd, for the project's `.claude/settings*.json`.
    pub cwd: Option<&'a Path>,
    /// Codex's config.toml.
    pub codex_config: &'a Path,
    pub procs: &'a [Proc],
}

pub fn read(kind: &str, s: &Sources, now: SystemTime) -> ModelInfo {
    let lines = s.log.and_then(|l| tail::read(l, None, 256 * 1024).ok()).map(|r| r.lines).unwrap_or_default();
    let model = tail::model(&lines);
    let args = || s.procs.iter().find_map(|p| effort_in_args(kind, &p.line));
    let effort = match kind {
        "claude" => statusline_effort(s.log, s.claude_settings, now).or_else(|| tail::switch_effort(&lines)).or_else(args).or_else(|| claude_settings_effort(s, model.as_deref())),
        "codex" => codex_log_effort(&lines).or_else(args).or_else(|| codex_config_effort(s.codex_config)),
        _ => None,
    };
    ModelInfo { model, effort }
}

/// The pane's processes (`pids`) and their children: a wrapper script can start the agent.
pub fn pane_procs(all: &[Proc], pids: &[i32]) -> Vec<Proc> {
    all.iter().filter(|p| pids.contains(&p.pid) || pids.contains(&p.ppid)).cloned().collect()
}

fn statusline_effort(log: Option<&Path>, settings: &Path, now: SystemTime) -> Option<String> {
    let dir = crate::status::folder(settings)?;
    let sid = log?.file_stem()?.to_string_lossy().to_string();
    StatusLineFolder::new(&dir).record(&sid, now)?.info?.effort
}

/// `--effort low` / `--effort=low` (Claude Code); `-c model_reasoning_effort=high` / `--config ...` (Codex). The
/// command line only: `ps -E` puts the environment after it, and the words there are `NAME=value`.
pub fn effort_in_args(kind: &str, line: &str) -> Option<String> {
    let words = shell_words(line);
    for (i, w) in words.iter().enumerate() {
        let next = words.get(i + 1).map(String::as_str);
        let v = match kind {
            "claude" if w == "--effort" => next,
            "claude" => w.strip_prefix("--effort="),
            "codex" if w == "-c" || w == "--config" => next.and_then(|n| n.strip_prefix("model_reasoning_effort=")),
            "codex" => w.strip_prefix("--config=").and_then(|n| n.strip_prefix("model_reasoning_effort=")),
            _ => None,
        };
        if let Some(v) = v.map(|v| v.trim_matches(|c| c == '"' || c == '\'')).filter(|v| !v.is_empty() && !v.starts_with('-')) {
            return Some(v.to_string());
        }
    }
    None
}

/// Claude Code's settings, in its order: local, project, user.
fn claude_settings_effort(s: &Sources, model: Option<&str>) -> Option<String> {
    let project = s.cwd.map(|c| c.join(".claude"));
    let files = [project.as_ref().map(|p| p.join("settings.local.json")), project.as_ref().map(|p| p.join("settings.json")), Some(s.claude_settings.to_path_buf())];
    files.iter().flatten().find_map(|f| settings_file_effort(f, model))
}

/// `modelSettings.<model>.effortLevel`, then the top-level `effortLevel`. Other nested keys do not count.
fn settings_file_effort(path: &Path, model: Option<&str>) -> Option<String> {
    let o: Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    let level = |v: Option<&Value>| v.and_then(|v| v.get("effortLevel")).and_then(Value::as_str).filter(|s| !s.is_empty()).map(String::from);
    model.and_then(|m| level(o.get("modelSettings").and_then(|ms| ms.get(m)))).or_else(|| level(Some(&o)))
}

/// The newest `turn_context` line's `effort`.
pub fn codex_log_effort(lines: &[String]) -> Option<String> {
    lines.iter().rev().filter(|l| l.contains("\"turn_context\"")).find_map(|l| {
        let o: Value = serde_json::from_str(l).ok()?;
        if o.get("type")?.as_str()? != "turn_context" {
            return None;
        }
        o.get("payload")?.get("effort")?.as_str().filter(|s| !s.is_empty()).map(String::from)
    })
}

/// `model_reasoning_effort = "high"` at the top of config.toml (before any `[table]`).
fn codex_config_effort(config: &Path) -> Option<String> {
    let text = std::fs::read_to_string(config).ok()?;
    for l in text.lines().map(str::trim) {
        if l.starts_with('[') {
            return None;
        }
        let Some((k, v)) = l.split_once('=') else { continue };
        if k.trim() == "model_reasoning_effort" {
            let v = v.split('#').next().unwrap_or("").trim().trim_matches(|c| c == '"' || c == '\'');
            return (!v.is_empty()).then(|| v.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn proc(pid: i32, ppid: i32, line: &str) -> Proc {
        Proc { pid, ppid, start: Default::default(), line: line.into() }
    }

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("hcf-agentmodel-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn effort_from_command_lines() {
        assert_eq!(effort_in_args("claude", "claude --model opus --effort low HOME=/x").as_deref(), Some("low"));
        assert_eq!(effort_in_args("claude", "node cli.js --effort=xhigh").as_deref(), Some("xhigh"));
        assert_eq!(effort_in_args("claude", "claude --effort --verbose"), None);
        assert_eq!(effort_in_args("claude", "claude --model opus"), None);
        assert_eq!(effort_in_args("codex", "codex -c model_reasoning_effort=\"high\"").as_deref(), Some("high"));
        assert_eq!(effort_in_args("codex", "codex --config=model_reasoning_effort=low").as_deref(), Some("low"));
        assert_eq!(effort_in_args("codex", "codex --effort low"), None, "not Codex's flag");
        assert_eq!(effort_in_args("pi", "pi --effort low"), None);
    }

    #[test]
    fn pane_procs_takes_the_pids_and_their_children() {
        let all = [proc(1, 0, "zsh"), proc(2, 1, "claude"), proc(3, 2, "node"), proc(9, 0, "other")];
        let got: Vec<i32> = pane_procs(&all, &[2]).iter().map(|p| p.pid).collect();
        assert_eq!(got, vec![2, 3]);
    }

    #[test]
    fn claude_model_from_log_effort_from_args_then_settings() {
        let d = dir("claude");
        let log = d.join("abc.jsonl");
        fs::write(&log, r#"{"type":"assistant","message":{"model":"claude-opus-5-5","content":[]}}"#.to_string() + "\n").unwrap();
        let settings = d.join("settings.json");
        fs::write(&settings, r#"{"effortLevel":"medium"}"#).unwrap();
        let procs = [proc(5, 1, "claude --effort low")];
        let s = Sources { log: Some(&log), claude_settings: &settings, cwd: None, codex_config: &d.join("none.toml"), procs: &procs };
        assert_eq!(read("claude", &s, SystemTime::now()), ModelInfo { model: Some("claude-opus-5-5".into()), effort: Some("low".into()) });
        let s = Sources { procs: &[], ..s };
        assert_eq!(read("claude", &s, SystemTime::now()).effort.as_deref(), Some("medium"), "settings as the default");
        let s = Sources { log: None, claude_settings: &d.join("missing.json"), ..s };
        assert_eq!(read("claude", &s, SystemTime::now()), ModelInfo::default(), "unknown parts stay unknown");
    }

    #[test]
    fn claude_settings_effort_per_model_then_top_level_then_project_files() {
        let d = dir("claude-settings");
        let log = d.join("abc.jsonl");
        fs::write(&log, r#"{"type":"assistant","message":{"model":"claude-opus-5-5","content":[]}}"#.to_string() + "\n").unwrap();
        let user = d.join("settings.json");
        fs::write(&user, r#"{"model":"opus","modelSettings":{"claude-sonnet-5-5":{"effortLevel":"xhigh"},"claude-opus-5-5":{"effortLevel":"medium"}},"x":{"effortLevel":"low"}}"#).unwrap();
        let s = Sources { log: Some(&log), claude_settings: &user, cwd: None, codex_config: &d.join("none.toml"), procs: &[] };
        assert_eq!(read("claude", &s, SystemTime::now()).effort.as_deref(), Some("medium"), "the log's model in modelSettings");
        fs::write(&user, r#"{"effortLevel":"high","modelSettings":{"claude-sonnet-5-5":{"effortLevel":"xhigh"}},"x":{"effortLevel":"low"}}"#).unwrap();
        assert_eq!(read("claude", &s, SystemTime::now()).effort.as_deref(), Some("high"), "top level when the model has none");
        fs::write(&user, r#"{"modelSettings":{"claude-sonnet-5-5":{"effortLevel":"xhigh"}},"x":{"effortLevel":"low"}}"#).unwrap();
        assert_eq!(read("claude", &s, SystemTime::now()).effort, None, "another model's or a nested key does not count");
        let cwd = d.join("proj");
        fs::create_dir_all(cwd.join(".claude")).unwrap();
        fs::write(cwd.join(".claude/settings.json"), r#"{"effortLevel":"low"}"#).unwrap();
        let s = Sources { cwd: Some(&cwd), ..s };
        assert_eq!(read("claude", &s, SystemTime::now()).effort.as_deref(), Some("low"), "project settings");
        fs::write(cwd.join(".claude/settings.local.json"), r#"{"modelSettings":{"claude-opus-5-5":{"effortLevel":"max"}}}"#).unwrap();
        assert_eq!(read("claude", &s, SystemTime::now()).effort.as_deref(), Some("max"), "local settings first");
    }

    #[test]
    fn codex_effort_from_log_then_args_then_config() {
        let d = dir("codex");
        let log = d.join("rollout.jsonl");
        fs::write(&log, concat!(
            r#"{"type":"turn_context","payload":{"model":"gpt-6","effort":"low"}}"#, "\n",
            r#"{"type":"turn_context","payload":{"model":"gpt-6","effort":"high"}}"#, "\n",
        )).unwrap();
        let config = d.join("config.toml");
        fs::write(&config, "model = \"gpt-6\"\nmodel_reasoning_effort = \"medium\" # default\n[profiles.x]\nmodel_reasoning_effort = \"low\"\n").unwrap();
        let procs = [proc(5, 1, "codex -c model_reasoning_effort=xhigh")];
        let s = Sources { log: Some(&log), claude_settings: &d.join("none.json"), cwd: None, codex_config: &config, procs: &procs };
        assert_eq!(read("codex", &s, SystemTime::now()), ModelInfo { model: Some("gpt-6".into()), effort: Some("high".into()) });
        let s = Sources { log: None, ..s };
        assert_eq!(read("codex", &s, SystemTime::now()).effort.as_deref(), Some("xhigh"));
        let s = Sources { procs: &[], ..s };
        assert_eq!(read("codex", &s, SystemTime::now()).effort.as_deref(), Some("medium"));
        fs::write(&config, "[profiles.x]\nmodel_reasoning_effort = \"low\"\n").unwrap();
        assert_eq!(read("codex", &s, SystemTime::now()).effort, None, "a profile's value is not the default");
    }

    #[test]
    fn the_line() {
        assert_eq!(line("claude", &ModelInfo::default()), json!({"type": "model", "agent": "claude", "model": null, "effort": null}));
    }
}
