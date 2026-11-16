//! Batched reports (#213): only an urgent report wakes the coordinator at once. Other reports are held (as in
//! `held`) and given together, at most every `report_batch_minutes` (agents.json, default 10; 0 turns the
//! batching off), or at once with the next urgent report. A timer process (`batch flush`) gives the batch when
//! its time comes. The files are in `batch/<coordinator>` of the state folder: `last` (the time of the last
//! delivery) and `due` (the time of the armed timer), in seconds since the epoch.
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::flowseen::state_dir;
use crate::held::{self, Report};
use crate::home::config_path;
use crate::workers::{Core, Manager, Sent};

/// The setting in agents.json, and its default in minutes.
pub const SETTING: &str = "report_batch_minutes";
pub const DEFAULT_MINUTES: u64 = 10;

/// The words that make a report urgent: its status (the word after "<name>:"), or a word in its text.
const URGENT_STATUS: [&str; 4] = ["blocked", "failed", "error", "question"];
const URGENT_WORDS: [&str; 6] = ["question for the owner", "ci failed", "failed ci", "ci fails", "merged", "closed"];

/// True for a report that wakes the coordinator at once: blocked, error, failed, a question for the owner,
/// failed CI, or a PR merged or closed.
pub fn urgent(text: &str) -> bool {
    let low = text.to_lowercase();
    let status = low.split_once(':').map(|(_, rest)| rest.trim_start()).unwrap_or("");
    let word: String = status.chars().take_while(|c| c.is_alphanumeric()).collect();
    URGENT_STATUS.contains(&word.as_str()) || URGENT_WORDS.iter().any(|w| low.contains(w))
}

/// The batch interval in seconds, from agents.json. A missing file or key gives the default.
pub fn interval(env: &dyn Fn(&str) -> String) -> u64 {
    let v: Option<serde_json::Value> = fs::read(config_path(env)).ok().and_then(|d| serde_json::from_slice(&d).ok());
    v.and_then(|v| v[SETTING].as_u64()).unwrap_or(DEFAULT_MINUTES) * 60
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn file(env: &dyn Fn(&str) -> String, to: &str, name: &str) -> Option<PathBuf> {
    let base = std::path::Path::new(to).file_name()?;
    Some(state_dir(env)?.join("batch").join(base).join(name))
}

fn read_time(env: &dyn Fn(&str) -> String, to: &str, name: &str) -> Option<u64> {
    fs::read_to_string(file(env, to, name)?).ok()?.trim().parse().ok()
}

fn write_time(env: &dyn Fn(&str) -> String, to: &str, name: &str, t: Option<u64>) {
    let Some(p) = file(env, to, name) else { return };
    match t {
        Some(t) => {
            let _ = p.parent().map(fs::create_dir_all);
            let _ = fs::write(&p, t.to_string());
        }
        None => {
            let _ = fs::remove_file(&p);
        }
    }
}

/// Sends the held reports and then `text` (when not "") to `to` as one text. When hc refuses it, all are held
/// again. Ok(None) when there was nothing to send.
fn deliver(m: &Manager, env: &dyn Fn(&str) -> String, to: &str, now: u64, text: &str) -> Result<Option<Sent>, String> {
    let mut all = held::take(env, to)?;
    if !text.is_empty() {
        all.push(Report { from: m.caller_tag(), text: text.to_string() });
    }
    if all.is_empty() {
        return Ok(None);
    }
    let body = if all.len() == 1 && !text.is_empty() { text.to_string() } else { held::text(&all) };
    let sent = match m.try_send(to, &body) {
        Ok(s) => s,
        Err(e) => {
            all.iter().try_for_each(|r| held::hold(env, to, r))?;
            return Err(e);
        }
    };
    match &sent {
        Sent::Typed => write_time(env, to, "last", Some(now)),
        Sent::Refused(_) => all.iter().try_for_each(|r| held::hold(env, to, r))?,
    }
    Ok(Some(sent))
}

/// Sends a report from this terminal to `to`, or holds it for the batch. `arm(to, seconds)` starts the timer
/// that gives the batch later. The result: "sent", "held: …" (hc refused it), or "batched: …".
pub fn report(core: &dyn Core, env: &dyn Fn(&str) -> String, to: &str, text: &str, now: u64, arm: &dyn Fn(&str, u64)) -> Result<String, String> {
    let every = interval(env);
    if every == 0 || state_dir(env).is_none() {
        return held::report(core, env, to, text);
    }
    if text.trim().is_empty() {
        return Err("agents: no text to send".into());
    }
    let m = Manager::new(core, env("HC_TERMINAL_ID"));
    let last = read_time(env, to, "last");
    let Some(last) = last.filter(|l| !urgent(text) && now < l + every) else {
        return match deliver(&m, env, to, now, text)? {
            Some(Sent::Refused(reason)) => Ok(format!("held: {to} did not take it now ({reason}); contr gives it to {to} when its prompt is free")),
            _ => Ok("sent".into()),
        };
    };
    held::hold(env, to, &Report { from: m.caller_tag(), text: text.to_string() })?;
    let due = last + every;
    if read_time(env, to, "due").is_none_or(|d| d < now) {
        write_time(env, to, "due", Some(due));
        arm(to, due - now);
    }
    Ok(format!("batched: not urgent, so contr gives it to {to} with the next urgent report or in {} min at most", (due - now).div_ceil(60)))
}

/// `batch flush <to>`: the timer's end. Gives the held reports to `to` when the batch is due.
pub fn flush(core: &dyn Core, env: &dyn Fn(&str) -> String, to: &str, now: u64) -> Result<String, String> {
    match read_time(env, to, "due") {
        Some(due) if due <= now => {}
        Some(_) => return Ok("not due".into()),
        None => return Ok("no batch".into()),
    }
    write_time(env, to, "due", None);
    let m = Manager::new(core, env("HC_TERMINAL_ID"));
    Ok(match deliver(&m, env, to, now, "")? {
        None => "no reports".into(),
        Some(Sent::Typed) => "sent".into(),
        Some(Sent::Refused(reason)) => format!("held ({reason})"),
    })
}

/// Starts a detached process that runs `batch flush <to>` after `secs` seconds.
pub fn arm_timer(to: &str, secs: u64) {
    let Ok(exe) = std::env::current_exe() else { return };
    let mut c = std::process::Command::new("sh");
    c.args(["-c", &format!("sleep {secs}; exec \"$0\" batch flush \"$1\""), &exe.to_string_lossy(), to]);
    c.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut c, 0);
    let _ = c.spawn();
}
