//! The reports that wait for a coordinator: a worker's report that hc did not type, because the coordinator's
//! prompt was busy (such as with a question dialog). Each one is a file in `held/<coordinator>` of the state
//! folder. The coordinator's Stop hook and prompt hook take them and give them to the coordinator, so a report
//! is not lost (#199).
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::flowseen::state_dir;
use crate::workers::{Core, Manager, Sent};

/// The first line of the text that gives the held reports to the coordinator.
pub const HELD_LEAD: &str = "contr: these reports waited for you (not urgent, or your prompt was busy):";

/// One held report.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    /// The sender as hc's tag names it, such as "t7 b-195".
    pub from: String,
    pub text: String,
}

/// The folder of the reports that wait for the terminal `to`.
fn folder(env: &dyn Fn(&str) -> String, to: &str) -> Result<PathBuf, String> {
    let name = Path::new(to).file_name().ok_or_else(|| format!("held: bad terminal name {to:?}"))?;
    Ok(state_dir(env).ok_or("held: no state folder ($CONTR_STATE_DIR, $CONTR_HOME, $HERDR_APP_HOME or $HOME)")?.join("held").join(name))
}

/// Keeps a report for the terminal `to`. The file name starts with the time, so the reports keep their order.
pub fn hold(env: &dyn Fn(&str) -> String, to: &str, report: &Report) -> Result<(), String> {
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let dir = folder(env, to)?;
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let name = format!("{nanos:024}-{}-{}", std::process::id(), COUNT.fetch_add(1, Ordering::Relaxed));
    let body = serde_json::to_vec(report).map_err(|e| e.to_string())?;
    // Written under another name first, so a hook never reads half a file.
    let (tmp, file) = (dir.join(format!(".{name}.tmp")), dir.join(format!("{name}.json")));
    fs::write(&tmp, body).and_then(|_| fs::rename(&tmp, &file)).map_err(|e| format!("{}: {e}", file.display()))
}

/// Removes the reports that wait for the terminal `to` and gives them, oldest first. A file that cannot be
/// removed stays and is not given, so no report comes two times. A file that is not a report is removed. With
/// no state folder nothing can be held, so there are none.
pub fn take(env: &dyn Fn(&str) -> String, to: &str) -> Result<Vec<Report>, String> {
    if state_dir(env).is_none() {
        return Ok(Vec::new());
    }
    let dir = folder(env, to)?;
    let mut names: Vec<PathBuf> = match fs::read_dir(&dir) {
        Ok(entries) => entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "json")).collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("{}: {e}", dir.display())),
    };
    names.sort();
    let mut out = Vec::new();
    for p in names {
        let Ok(body) = fs::read(&p) else { continue };
        if fs::remove_file(&p).is_err() {
            continue;
        }
        if let Ok(r) = serde_json::from_slice::<Report>(&body) {
            out.push(r);
        }
    }
    Ok(out)
}

/// The text that gives held reports to the coordinator: the lead, then each report as hc would have typed it.
/// "" when there are none.
pub fn text(reports: &[Report]) -> String {
    if reports.is_empty() {
        return String::new();
    }
    let mut out = HELD_LEAD.to_string();
    for r in reports {
        out += &format!("\n<hc from=\"{}\">{}</hc>", r.from, r.text);
    }
    out
}

/// Sends a report from this terminal (HC_TERMINAL_ID) to the terminal `to`. When hc refuses it, the report is
/// held for `to`. The result says which: "sent", or "held" with hc's reason.
pub fn report(core: &dyn Core, env: &dyn Fn(&str) -> String, to: &str, text: &str) -> Result<String, String> {
    let m = Manager::new(core, env("HC_TERMINAL_ID"));
    match m.try_send(to, text)? {
        Sent::Typed => Ok("sent".into()),
        Sent::Refused(reason) => {
            hold(env, to, &Report { from: m.caller_tag(), text: text.to_string() }).map_err(|e| format!("{to} did not take the report ({reason}), and it was not held: {e}"))?;
            Ok(format!("held: {to} did not take it now ({reason}); contr gives it to {to} when its prompt is free"))
        }
    }
}
