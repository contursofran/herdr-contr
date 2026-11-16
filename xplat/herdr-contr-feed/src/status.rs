//! Swift: `Status.swift`. `status`: the tags over the composer (model, effort, ctx, cost, and the owner's own status
//! line parts). Claude Code's facts come from herdr-contr's status line wrapper, when the owner turned it on; otherwise
//! the model named in the log is the only tag.

use crate::protocol::JsonLines;
use crate::tail;
use herdr_core::statusline::{StatusLineFolder, StatusTag};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Tag {
    /// "model", "effort", "context", "cost", "own".
    pub kind: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent: Option<i64>,
    pub warn: bool,
}

/// The folder herdr-contr's wrapper writes to: the `--dir` of a `herdr-contr-statusline` status line command in Claude
/// Code's settings. None when the wrapper is not on.
pub fn folder(settings: &Path) -> Option<PathBuf> {
    let d = std::fs::read(settings).ok()?;
    let o: Value = serde_json::from_slice(&d).ok()?;
    let cmd = o.get("statusLine")?.get("command")?.as_str()?;
    if !cmd.contains("herdr-contr-statusline") {
        return None;
    }
    let words = shell_words(cmd);
    let i = words.iter().position(|w| w == "--dir")?;
    words.get(i + 1).map(PathBuf::from)
}

/// Claude Code's settings files, in its order (the first that has a value wins): the project's
/// `.claude/settings.local.json` and `.claude/settings.json` in `cwd` (when known), then the user's `settings`.
pub fn settings_files(settings: &Path, cwd: Option<&Path>) -> Vec<PathBuf> {
    let project = cwd.map(|c| c.join(".claude"));
    let mut out: Vec<PathBuf> = project.iter().flat_map(|p| [p.join("settings.local.json"), p.join("settings.json")]).collect();
    out.push(settings.to_path_buf());
    out
}

/// Claude Code's auto-compact limit in tokens for `model` (the id as the log writes it): the
/// `modelSettings.<model>.autoCompactWindow` of the first file that has one, else the first top-level
/// `autoCompactWindow`. A key matches the model also with or without a date (`-20261101`) or a `[1m]` suffix; the
/// exact key first. A value that is not a positive number does not count. None when no file sets one.
pub fn compact_limit(files: &[PathBuf], model: Option<&str>) -> Option<i64> {
    let docs: Vec<Value> = files.iter().filter_map(|f| serde_json::from_slice(&std::fs::read(f).ok()?).ok()).collect();
    model
        .and_then(|m| docs.iter().find_map(|o| model_limit(o, m)))
        .or_else(|| docs.iter().find_map(|o| tokens(o.get("autoCompactWindow"))))
}

fn model_limit(o: &Value, model: &str) -> Option<i64> {
    let ms = o.get("modelSettings")?.as_object()?;
    let base = base_model(model);
    let near = ms.iter().filter(|(k, _)| k.as_str() != model && base_model(k) == base).map(|(_, v)| v);
    ms.get(model).into_iter().chain(near).find_map(|e| tokens(e.get("autoCompactWindow")))
}

/// A positive token count; a fraction is cut.
fn tokens(v: Option<&Value>) -> Option<i64> {
    let n = v?;
    n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)).filter(|n| *n > 0)
}

/// The model id without a `[1m]` suffix and without a date suffix (`-YYYYMMDD`), in lower case.
pub fn base_model(id: &str) -> String {
    let id = id.trim().to_lowercase();
    let id = id.strip_suffix("[1m]").unwrap_or(&id);
    match id.rsplit_once('-') {
        Some((head, date)) if date.len() == 8 && date.bytes().all(|b| b.is_ascii_digit()) => head.to_string(),
        _ => id.to_string(),
    }
}

/// The model's context window in tokens, when the id tells it: `[1m]` is 1,000,000. Other windows are not in the
/// log or the settings, so None.
pub fn context_window(model: &str) -> Option<i64> {
    model.trim().to_lowercase().ends_with("[1m]").then_some(1_000_000)
}

/// Splits a command line into words: single quotes, double quotes and backslashes as a POSIX shell reads them.
pub fn shell_words(s: &str) -> Vec<String> {
    let (mut out, mut cur, mut has, mut quote, mut esc): (Vec<String>, String, bool, Option<char>, bool) = (vec![], String::new(), false, None, false);
    for ch in s.chars() {
        if esc {
            cur.push(ch);
            esc = false;
            continue;
        }
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            } else if ch == '\\' && q == '"' {
                esc = true;
            } else {
                cur.push(ch);
            }
            continue;
        }
        match ch {
            '\'' | '"' => {
                quote = Some(ch);
                has = true;
            }
            '\\' => {
                esc = true;
                has = true;
            }
            ' ' | '\t' | '\n' => {
                if has || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                has = false;
            }
            _ => cur.push(ch),
        }
    }
    if has || !cur.is_empty() {
        out.push(cur);
    }
    out
}

pub fn default_settings(env: &HashMap<String, String>) -> PathBuf {
    let dir = env
        .get("CLAUDE_CONFIG_DIR")
        .filter(|d| !d.is_empty())
        .cloned()
        .unwrap_or_else(|| herdr_core::hc::session::home_dir(env) + "/.claude");
    PathBuf::from(dir + "/settings.json")
}

#[derive(Serialize)]
struct Line<'a> {
    #[serde(rename = "type")]
    t: &'a str,
    agent: &'a str,
    source: &'a str,
    tags: Vec<Tag>,
}

/// Prints one `status` line. `log`: the session log (its file name is Claude Code's session id). `settings`: the
/// user's Claude Code settings; `cwd`: the agent's folder, for the project's settings.
pub fn run(agent: &str, log: Option<&Path>, settings: &Path, cwd: Option<&Path>, out: &mut JsonLines, now: SystemTime) {
    let (mut tags, mut source): (Vec<Tag>, &str) = (vec![], "none");
    let lines = log.and_then(|l| tail::read(l, None, 256 * 1024).ok()).map(|r| r.lines);
    let model = lines.as_deref().and_then(tail::model);
    let limit = || compact_limit(&settings_files(settings, cwd), model.as_deref());
    if agent == "claude" {
        if let (Some(log), Some(dir)) = (log, folder(settings)) {
            let sid = log.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            if let Some(rec) = StatusLineFolder::new(&dir).record(&sid, now) {
                if !rec.is_empty() {
                    tags = rec.tags_with_limit(limit()).into_iter().map(|t| Tag { kind: t.kind.raw().into(), text: t.text, percent: t.percent, warn: t.warn }).collect();
                    source = "statusline";
                }
            }
        }
    }
    if tags.is_empty() {
        if let Some(lines) = &lines {
            if let Some(m) = &model {
                tags = vec![Tag { kind: "model".into(), text: m.clone(), percent: None, warn: false }];
                source = "log";
            }
            // No status line record: the context from the log's usage, against the auto-compact limit; with no
            // limit, as a percent of the model's context window when the id tells it; else no context tag.
            if agent == "claude" {
                let t = tail::context_used(lines).and_then(|u| match limit() {
                    Some(l) => StatusTag::context_against(u, l),
                    None => model.as_deref().and_then(context_window).and_then(|w| StatusTag::context_of_window(u, w)),
                });
                if let Some(t) = t {
                    tags.push(Tag { kind: t.kind.raw().into(), text: t.text, percent: t.percent, warn: t.warn });
                    source = "log";
                }
            }
        }
    }
    out.emit(&Line { t: "status", agent, source, tags });
}
