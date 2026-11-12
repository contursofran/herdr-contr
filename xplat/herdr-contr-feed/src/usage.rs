//! The plan usage of Claude and Codex (the `usage` command), as contr's `internal/usage` reads it.
//!
//! - Claude: Claude Code gives its plan limits only to its status line command. contr's `contr statusline`
//!   saves them to `<contr state dir>/usage/claude.json` (schema 1); this reads that file.
//! - Codex: `codex app-server` (JSON-RPC, one message per line): `initialize`, `initialized`,
//!   `account/rateLimits/read`. No answer in 10 s: killed. No `codex`, or no answer: the newest rollout file
//!   under `<CODEX_HOME>/sessions` (the two newest day folders), its last `token_count` line.
//!
//! Read only. A plan with no data is left out.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The longest a `codex app-server` read (or the login shell's PATH read) may take.
pub const CODEX_TIMEOUT: Duration = Duration::from_secs(10);
/// The bytes read from the end of a rollout file.
const TAIL_BYTES: u64 = 256 * 1024;
/// The day folders searched for the newest rollout file.
const DAY_FOLDERS: usize = 2;

#[derive(Debug, Clone, PartialEq)]
pub struct Window {
    /// Claude: `five_hour`, `seven_day`. Codex: `primary` (5 h), `secondary` (week).
    pub kind: String,
    pub used_percent: f64,
    /// Unix seconds.
    pub resets_at: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub plan: &'static str,
    pub windows: Vec<Window>,
    /// Unix seconds of the data.
    pub read_at: u64,
}

/// Where the data is.
pub struct Sources {
    /// contr's state folder; None: no Claude data.
    pub state_dir: Option<PathBuf>,
    /// Codex's home; None: no rollout files.
    pub codex_home: Option<PathBuf>,
    /// The `codex` program and the PATH to give it; None: no app-server read.
    pub codex: Option<(PathBuf, Option<String>)>,
    pub timeout: Duration,
}

impl Sources {
    /// From the process: CONTR_STATE_DIR, CODEX_HOME, HOME, PATH, else the login shell's PATH.
    pub fn from_process() -> Sources {
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let home = env("HOME").map(PathBuf::from);
        Sources {
            state_dir: state_dir(&env, home.as_deref(), cfg!(target_os = "macos")),
            codex_home: env("CODEX_HOME").map(PathBuf::from).or_else(|| home.as_ref().map(|h| h.join(".codex"))),
            codex: find_codex(env("PATH").as_deref(), || login_path(&env("SHELL").unwrap_or_else(|| "/bin/zsh".into()), CODEX_TIMEOUT)),
            timeout: CODEX_TIMEOUT,
        }
    }
}

/// contr's state folder: CONTR_STATE_DIR; else macOS `~/Library/Application Support/contr`; else
/// `$XDG_STATE_HOME/contr` or `~/.local/state/contr`.
pub fn state_dir(env: &dyn Fn(&str) -> Option<String>, home: Option<&Path>, macos: bool) -> Option<PathBuf> {
    if let Some(d) = env("CONTR_STATE_DIR") {
        return Some(d.into());
    }
    if macos {
        return home.map(|h| h.join("Library/Application Support/contr"));
    }
    if let Some(s) = env("XDG_STATE_HOME") {
        return Some(Path::new(&s).join("contr"));
    }
    home.map(|h| h.join(".local/state/contr"))
}

/// Both plans, Claude first; a plan with no data is left out.
pub fn read(src: &Sources) -> Vec<Plan> {
    let mut out = Vec::new();
    if let Some(p) = src.state_dir.as_deref().and_then(read_claude) {
        out.push(p);
    }
    let codex = src.codex.as_ref().and_then(|(prog, path)| read_app_server(prog, path.as_deref(), src.timeout).ok());
    let codex = codex.map(|windows| Plan { plan: "codex", windows, read_at: now() }).or_else(|| src.codex_home.as_deref().and_then(read_rollouts));
    if let Some(p) = codex {
        out.push(p);
    }
    out
}

/// The `usage` line.
pub fn line(plans: &[Plan]) -> Value {
    let plans: Vec<Value> = plans
        .iter()
        .map(|p| {
            let windows: Vec<Value> = p.windows.iter().map(|w| json!({"kind": w.kind, "used_percent": w.used_percent, "resets_at": w.resets_at})).collect();
            json!({"plan": p.plan, "windows": windows, "read_at": p.read_at})
        })
        .collect();
    json!({"type": "usage", "plans": plans})
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// A number as whole seconds: None when negative, NaN or not a number.
fn whole_seconds(v: Option<&Value>) -> Option<u64> {
    let f = v?.as_f64()?;
    (f.is_finite() && f >= 0.0).then_some(if f >= u64::MAX as f64 { u64::MAX } else { f as u64 })
}

// ---- Claude ----

/// `usage/claude.json`: `{"schema":1,"updated_at":…,"windows":[{"kind","used_percent","resets_at"}]}`.
pub fn read_claude(state_dir: &Path) -> Option<Plan> {
    let text = std::fs::read_to_string(state_dir.join("usage/claude.json")).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    if v.get("schema")?.as_u64()? != 1 {
        return None;
    }
    let windows: Vec<Window> = v
        .get("windows")?
        .as_array()?
        .iter()
        .filter_map(|w| {
            Some(Window { kind: w.get("kind")?.as_str()?.to_string(), used_percent: w.get("used_percent")?.as_f64()?, resets_at: w.get("resets_at").and_then(Value::as_u64) })
        })
        .collect();
    if windows.is_empty() {
        return None;
    }
    Some(Plan { plan: "claude", windows, read_at: v.get("updated_at").and_then(Value::as_u64).unwrap_or(0) })
}

// ---- Codex app-server ----

/// `codex` on PATH, else on the login shell's PATH (an app opened from Finder has a short PATH). The PATH
/// returned goes to the program when it came from the login shell.
pub fn find_codex(path: Option<&str>, login: impl FnOnce() -> Option<String>) -> Option<(PathBuf, Option<String>)> {
    if let Some(p) = path.and_then(|l| look_in("codex", l)) {
        return Some((p, None));
    }
    let list = login()?;
    look_in("codex", &list).map(|p| (p, Some(list)))
}

fn look_in(name: &str, list: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    std::env::split_paths(list).filter(|d| d.is_absolute()).map(|d| d.join(name)).find(|p| {
        std::fs::metadata(p).map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
    })
}

const PATH_MARKER: &str = "__HERDR_CONTR_PATH__=";

/// The PATH of the owner's interactive login shell.
fn login_path(shell: &str, timeout: Duration) -> Option<String> {
    let script = format!("printf '\\n{PATH_MARKER}%s\\n' \"$PATH\"");
    let mut cmd = Command::new(shell);
    cmd.args(["-l", "-i", "-c", &script]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    let out = run_capped(cmd, timeout, |_, mut out| {
        let mut s = String::new();
        out.read_to_string(&mut s).ok()?;
        Some(s)
    })??;
    out.lines().find_map(|l| l.trim_end_matches('\r').strip_prefix(PATH_MARKER).map(str::to_string))
}

/// Starts `cmd` in its own process group, gives its stdin and stdout to `talk` on a thread, and kills the
/// group when `talk` takes longer than `timeout`. None: no start, or the timeout.
fn run_capped<T: Send + 'static>(
    mut cmd: Command,
    timeout: Duration,
    talk: impl FnOnce(Option<std::process::ChildStdin>, std::process::ChildStdout) -> T + Send + 'static,
) -> Option<T> {
    use std::os::unix::process::CommandExt;
    cmd.process_group(0);
    let mut child = cmd.spawn().ok()?;
    let stdin = child.stdin.take();
    let stdout = child.stdout.take()?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(talk(stdin, stdout));
    });
    let got = rx.recv_timeout(timeout).ok();
    // The group: a shell or codex can start children of its own.
    kill_group(child.id());
    let _ = child.kill();
    let _ = child.wait();
    got
}

fn kill_group(pid: u32) {
    // `kill -KILL -<pid>`: the whole group; no libc crate here.
    let _ = Command::new("kill").args(["-KILL", &format!("-{pid}")]).stdout(Stdio::null()).stderr(Stdio::null()).status();
}

/// One `codex app-server` read: the windows of the `codex` limit.
pub fn read_app_server(program: &Path, path: Option<&str>, timeout: Duration) -> Result<Vec<Window>, String> {
    let mut cmd = Command::new(program);
    cmd.arg("app-server").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    if let Some(p) = path {
        cmd.env("PATH", p);
    }
    run_capped(cmd, timeout, |stdin, stdout| match stdin {
        Some(mut w) => talk(&mut w, stdout),
        None => Err("no stdin".to_string()),
    })
    .unwrap_or_else(|| Err("codex app-server did not start or did not answer in time".into()))
}

/// The JSON-RPC talk: initialize, initialized, account/rateLimits/read.
pub fn talk(w: &mut dyn Write, r: impl Read) -> Result<Vec<Window>, String> {
    let mut lines = BufReader::new(r).lines();
    let mut send = |v: Value| writeln!(w, "{v}").and_then(|_| w.flush()).map_err(|e| e.to_string());
    let version = env!("CARGO_PKG_VERSION");
    send(json!({"id": 1, "method": "initialize", "params": {"clientInfo": {"name": "herdr-contr", "title": "herdr-contr", "version": version}}}))?;
    answer_to(&mut lines, 1)?;
    send(json!({"method": "initialized"}))?;
    send(json!({"id": 2, "method": "account/rateLimits/read"}))?;
    let result = answer_to(&mut lines, 2)?;
    parse_rate_limits(&result).ok_or_else(|| "the answer has no codex limit window".into())
}

fn answer_to(lines: &mut impl Iterator<Item = std::io::Result<String>>, id: u64) -> Result<Value, String> {
    for l in lines {
        let l = l.map_err(|e| e.to_string())?;
        let Ok(msg) = serde_json::from_str::<Value>(&l) else { continue };
        if msg.get("method").is_some() || msg.get("id").and_then(Value::as_u64) != Some(id) {
            continue;
        }
        if let Some(e) = msg.get("error") {
            return Err(format!("error answer: {}", e.get("message").and_then(Value::as_str).unwrap_or("?")));
        }
        return Ok(msg.get("result").cloned().unwrap_or(Value::Null));
    }
    Err("app-server ended before it answered".into())
}

/// `rateLimitsByLimitId.codex`, else `rateLimits` when its `limitId` is absent or "codex".
pub fn parse_rate_limits(result: &Value) -> Option<Vec<Window>> {
    let snap = result.pointer("/rateLimitsByLimitId/codex").filter(|v| v.is_object()).or_else(|| {
        result.get("rateLimits").filter(|s| s.is_object() && s.get("limitId").and_then(Value::as_str).is_none_or(|id| id == "codex"))
    })?;
    let windows: Vec<Window> = ["primary", "secondary"]
        .iter()
        .filter_map(|kind| {
            let w = snap.get(*kind)?;
            Some(Window { kind: kind.to_string(), used_percent: w.get("usedPercent")?.as_f64()?, resets_at: whole_seconds(w.get("resetsAt")) })
        })
        .collect();
    (!windows.is_empty()).then_some(windows)
}

// ---- Codex rollout files ----

/// The newest rollout file's last `token_count` line with `codex` limits.
pub fn read_rollouts(codex_home: &Path) -> Option<Plan> {
    let (path, mtime) = newest_rollout(&codex_home.join("sessions"))?;
    let tail = read_tail(&path)?;
    let windows = parse_rollout_tail(&tail, mtime)?;
    Some(Plan { plan: "codex", windows, read_at: mtime })
}

pub fn parse_rollout_tail(tail: &str, mtime: u64) -> Option<Vec<Window>> {
    tail.lines().rev().filter(|l| l.contains("token_count")).find_map(|l| windows_of_line(l, mtime))
}

fn windows_of_line(line: &str, mtime: u64) -> Option<Vec<Window>> {
    let v: Value = serde_json::from_str(line).ok()?;
    let p = v.get("payload")?;
    if p.get("type")?.as_str()? != "token_count" {
        return None;
    }
    let limits = p.get("rate_limits")?;
    if limits.get("limit_id").and_then(Value::as_str).is_some_and(|id| id != "codex") {
        return None;
    }
    let window = |kind: &str| -> Option<Window> {
        let w = limits.get(kind)?;
        let used = w.get("used_percent")?.as_f64()?;
        let resets_at = whole_seconds(w.get("resets_at")).or_else(|| whole_seconds(w.get("resets_in_seconds")).map(|s| mtime.saturating_add(s)));
        Some(Window { kind: kind.to_string(), used_percent: used, resets_at })
    };
    let mut out = vec![window("primary")?];
    out.extend(window("secondary"));
    Some(out)
}

fn read_tail(path: &Path) -> Option<String> {
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    f.seek(SeekFrom::Start(len.saturating_sub(TAIL_BYTES))).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// `sessions/YYYY/MM/DD/rollout-*.jsonl`: the newest by mtime in the newest day folders.
fn newest_rollout(sessions: &Path) -> Option<(PathBuf, u64)> {
    let mut days = Vec::new();
    'out: for y in numbered(sessions) {
        for m in numbered(&y) {
            for d in numbered(&m) {
                days.push(d);
                if days.len() == DAY_FOLDERS {
                    break 'out;
                }
            }
        }
    }
    days.iter()
        .flat_map(|d| std::fs::read_dir(d).into_iter().flatten().flatten())
        .filter(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            n.starts_with("rollout-") && n.ends_with(".jsonl") && e.file_type().is_ok_and(|t| t.is_file())
        })
        .filter_map(|e| {
            let m = e.metadata().ok()?.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs();
            Some((e.path(), m))
        })
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)))
}

/// The subfolders whose names are numbers, the highest first.
fn numbered(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<(u32, PathBuf)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| Some((e.file_name().to_str()?.parse().ok()?, e.path())))
        .collect();
    v.sort_by_key(|x| std::cmp::Reverse(x.0));
    v.into_iter().map(|x| x.1).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn write(p: &Path, text: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    #[test]
    fn claude_file_gives_windows() {
        let d = tempfile::tempdir().unwrap();
        write(&d.path().join("usage/claude.json"), r#"{"schema":1,"updated_at":1700000000,"claude_version":"2.1.0","windows":[{"kind":"five_hour","used_percent":38.4,"resets_at":1700003600},{"kind":"seven_day","used_percent":12,"resets_at":null}]}"#);
        let p = read_claude(d.path()).unwrap();
        assert_eq!(p.read_at, 1700000000);
        assert_eq!(p.windows, vec![
            Window { kind: "five_hour".into(), used_percent: 38.4, resets_at: Some(1700003600) },
            Window { kind: "seven_day".into(), used_percent: 12.0, resets_at: None },
        ]);
    }

    #[test]
    fn claude_missing_corrupt_other_schema_or_empty_is_none() {
        let d = tempfile::tempdir().unwrap();
        assert!(read_claude(d.path()).is_none());
        for text in ["{", r#"{"schema":2,"windows":[{"kind":"five_hour","used_percent":1}]}"#, r#"{"schema":1,"windows":[]}"#] {
            write(&d.path().join("usage/claude.json"), text);
            assert!(read_claude(d.path()).is_none(), "{text}");
        }
    }

    #[test]
    fn state_dir_order() {
        let home = Path::new("/h");
        let none = |_: &str| None;
        assert_eq!(state_dir(&|k| (k == "CONTR_STATE_DIR").then(|| "/s".into()), Some(home), true), Some("/s".into()));
        assert_eq!(state_dir(&none, Some(home), true), Some("/h/Library/Application Support/contr".into()));
        assert_eq!(state_dir(&|k| (k == "XDG_STATE_HOME").then(|| "/x".into()), Some(home), false), Some("/x/contr".into()));
        assert_eq!(state_dir(&none, Some(home), false), Some("/h/.local/state/contr".into()));
        assert_eq!(state_dir(&none, None, true), None);
    }

    #[test]
    fn rate_limits_by_id_then_plain() {
        let by = json!({"rateLimits": {"limitId": "other", "primary": {"usedPercent": 99}}, "rateLimitsByLimitId": {"codex": {"primary": {"usedPercent": 20, "resetsAt": 1700000500}, "secondary": {"usedPercent": 5.5, "resetsAt": -1}}}});
        assert_eq!(parse_rate_limits(&by).unwrap(), vec![
            Window { kind: "primary".into(), used_percent: 20.0, resets_at: Some(1700000500) },
            Window { kind: "secondary".into(), used_percent: 5.5, resets_at: None },
        ]);
        let plain = json!({"rateLimits": {"primary": {"usedPercent": 7}}});
        assert_eq!(parse_rate_limits(&plain).unwrap().len(), 1);
        assert!(parse_rate_limits(&json!({"rateLimits": {"limitId": "other", "primary": {"usedPercent": 7}}})).is_none());
        assert!(parse_rate_limits(&json!({"rateLimits": {"primary": {}}})).is_none());
    }

    #[test]
    fn talk_skips_notifications_and_reads_the_answer() {
        let answers = concat!(
            r#"{"id":1,"result":{"userAgent":"codex"}}"#, "\n",
            r#"{"method":"account/updated","params":{}}"#, "\n",
            r#"{"id":2,"result":{"rateLimits":{"limitId":"codex","primary":{"usedPercent":42,"resetsAt":1700000000}}}}"#, "\n",
        );
        let mut sent = Vec::new();
        let w = talk(&mut sent, answers.as_bytes()).unwrap();
        assert_eq!(w[0].used_percent, 42.0);
        let sent = String::from_utf8(sent).unwrap();
        let methods: Vec<String> = sent.lines().map(|l| serde_json::from_str::<Value>(l).unwrap()["method"].as_str().unwrap().to_string()).collect();
        assert_eq!(methods, ["initialize", "initialized", "account/rateLimits/read"]);
    }

    #[test]
    fn talk_error_answer_and_early_end() {
        let err = concat!(r#"{"id":1,"result":{}}"#, "\n", r#"{"id":2,"error":{"code":-1,"message":"not logged in"}}"#, "\n");
        assert!(talk(&mut Vec::new(), err.as_bytes()).unwrap_err().contains("not logged in"));
        assert!(talk(&mut Vec::new(), &b""[..]).unwrap_err().contains("ended"));
    }

    fn fake_codex(dir: &Path, body: &str) -> PathBuf {
        let p = dir.join("codex");
        std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    #[test]
    fn app_server_program_answers_or_times_out() {
        let d = tempfile::tempdir().unwrap();
        let ok = fake_codex(d.path(), r#"read a; echo '{"id":1,"result":{}}'; read b; read c; echo '{"id":2,"result":{"rateLimits":{"primary":{"usedPercent":3}}}}'"#);
        assert_eq!(read_app_server(&ok, None, Duration::from_secs(5)).unwrap()[0].used_percent, 3.0);
        let slow = fake_codex(d.path(), "sleep 30");
        let t0 = std::time::Instant::now();
        assert!(read_app_server(&slow, None, Duration::from_millis(300)).is_err());
        assert!(t0.elapsed() < Duration::from_secs(5), "killed at the timeout");
    }

    #[test]
    fn find_codex_on_path_else_login_shell() {
        let d = tempfile::tempdir().unwrap();
        fake_codex(d.path(), "true");
        let dir = d.path().to_str().unwrap().to_string();
        assert_eq!(find_codex(Some(&dir), || panic!("PATH has it")), Some((d.path().join("codex"), None)));
        let dir2 = dir.clone();
        assert_eq!(find_codex(Some("/nonexistent"), move || Some(dir2)), Some((d.path().join("codex"), Some(dir))));
        assert_eq!(find_codex(None, || None), None);
    }

    fn rollout_line(used: f64, limit: &str) -> String {
        json!({"type": "event_msg", "payload": {"type": "token_count", "rate_limits": {"limit_id": limit, "primary": {"used_percent": used, "resets_in_seconds": 100}, "secondary": {"used_percent": 1.0, "resets_at": 1700009999}}}}).to_string()
    }

    #[test]
    fn rollout_tail_last_codex_line() {
        let tail = format!("{}\n{}\n{}\n", rollout_line(10.0, "codex"), rollout_line(50.0, "other"), r#"{"payload":{"type":"agent_message"}}"#);
        let w = parse_rollout_tail(&tail, 1000).unwrap();
        assert_eq!(w[0], Window { kind: "primary".into(), used_percent: 10.0, resets_at: Some(1100) });
        assert_eq!(w[1].resets_at, Some(1700009999));
        assert!(parse_rollout_tail("{}\n", 0).is_none());
    }

    #[test]
    fn rollouts_newest_file_in_newest_days() {
        let d = tempfile::tempdir().unwrap();
        let s = d.path().join("sessions");
        write(&s.join("2026/11/01/rollout-a.jsonl"), &rollout_line(90.0, "codex"));
        write(&s.join("2026/11/12/rollout-b.jsonl"), &rollout_line(20.0, "codex"));
        write(&s.join("2026/11/12/notes.jsonl"), &rollout_line(70.0, "codex"));
        let p = read_rollouts(d.path()).unwrap();
        assert_eq!(p.windows[0].used_percent, 20.0);
        assert!(read_rollouts(&d.path().join("none")).is_none());
    }

    #[test]
    fn read_app_server_wins_else_rollouts_and_no_data_hides() {
        let d = tempfile::tempdir().unwrap();
        write(&d.path().join("sessions/2026/11/12/rollout-b.jsonl"), &rollout_line(20.0, "codex"));
        let bad = fake_codex(d.path(), "exit 1");
        let src = Sources { state_dir: Some(d.path().join("state")), codex_home: Some(d.path().into()), codex: Some((bad, None)), timeout: Duration::from_secs(5) };
        let plans = read(&src);
        assert_eq!(plans.len(), 1, "no Claude file: no Claude plan");
        assert_eq!((plans[0].plan, plans[0].windows[0].used_percent), ("codex", 20.0));
        let ok = fake_codex(d.path(), r#"read a; echo '{"id":1,"result":{}}'; read b; read c; echo '{"id":2,"result":{"rateLimits":{"primary":{"usedPercent":3}}}}'"#);
        let src = Sources { codex: Some((ok, None)), ..src };
        assert_eq!(read(&src)[0].windows[0].used_percent, 3.0);
        let none = Sources { state_dir: None, codex_home: None, codex: None, timeout: Duration::from_secs(1) };
        assert_eq!(line(&read(&none)), json!({"type": "usage", "plans": []}));
    }

    #[test]
    fn line_shape() {
        let p = Plan { plan: "claude", windows: vec![Window { kind: "five_hour".into(), used_percent: 38.0, resets_at: None }], read_at: 5 };
        assert_eq!(line(&[p]), json!({"type": "usage", "plans": [{"plan": "claude", "windows": [{"kind": "five_hour", "used_percent": 38.0, "resets_at": null}], "read_at": 5}]}));
    }
}
