//! The status line wrapper and its switch (#252). Claude Code gives the context window and the used percent only to
//! its `statusLine` command. The wrapper is that command: it saves Claude Code's input for the session, runs the
//! owner's own command with the same input, saves its output and prints it unchanged. The switch puts the wrapper
//! in the user's Claude Code settings (`on`) and puts the owner's `statusLine` back (`off`).
//!
//!   herdr-contr-feed herdr-contr-statusline --dir <folder> [--own <base64>]   (Claude Code runs this)
//!   herdr-contr-feed statusline on|off|status [--settings <file>] [--home <herdr-app home>]
//!
//! `--own` is the owner's `statusLine` value as it was in the settings file (the JSON text, base64). `off` writes
//! that text back as it was, so the settings file keeps no second copy.

use base64::Engine;
use herdr_core::statusline::StatusLineFolder;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

/// The word that `status::folder()` looks for in the command.
pub const WORD: &str = "herdr-contr-statusline";
/// The owner's command is stopped after this time; what it printed so far is kept.
pub const TIMEOUT: Duration = Duration::from_secs(5);

/// The record folder in the Herdr home.
pub fn dir(home: &Path) -> PathBuf {
    in_root(&herdr_core::places::Places::new(home).folder())
}

/// The record folder in the backend root.
pub fn in_root(root: &Path) -> PathBuf {
    root.join("statusline")
}

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

/// The owner's command in a `statusLine` value: `command` when `type` is `command` or absent; never a wrapper.
fn own_command(v: &Value) -> Option<String> {
    if v.get("type").and_then(Value::as_str).unwrap_or("command") != "command" {
        return None;
    }
    let c = v.get("command")?.as_str()?;
    (!c.trim().is_empty() && !c.contains(WORD)).then(|| c.to_string())
}

/// The wrapper. Returns the bytes to print. Errors of its own (bad input, a folder it cannot write) do not stop
/// the owner's command.
pub fn wrap(args: &[String], input: &[u8], now: SystemTime) -> Vec<u8> {
    let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let own = opt("--own")
        .and_then(|s| b64().decode(s).ok())
        .and_then(|raw| serde_json::from_slice::<Value>(&raw).ok())
        .and_then(|v| own_command(&v));
    let output = own.map(|c| run_own(&c, input, TIMEOUT)).unwrap_or_default();
    if let Some(dir) = opt("--dir") {
        let sid = serde_json::from_slice::<Value>(input).ok().and_then(|o| o.get("session_id")?.as_str().map(String::from));
        if let Some(sid) = sid {
            let f = StatusLineFolder::new(Path::new(&dir));
            f.save(input, &output, &sid);
            f.clean(now);
        }
    }
    output
}

/// Runs `command` through `/bin/sh -c` with `input` on stdin, as Claude Code does. Its stdout, also on failure;
/// on a timeout, what it printed so far. Its stderr is dropped.
pub fn run_own(command: &str, input: &[u8], timeout: Duration) -> Vec<u8> {
    let mut cmd = Command::new("/bin/sh");
    cmd.arg("-c").arg(command).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    let Ok(mut child) = cmd.spawn() else { return vec![] };
    let mut stdin = child.stdin.take();
    let data = input.to_vec();
    std::thread::spawn(move || {
        if let Some(s) = stdin.as_mut() {
            let _ = s.write_all(&data);
        }
    });
    // Read in pieces into a shared buffer: on a timeout, what came so far is kept.
    let got = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
    let (mut stdout, sink) = (child.stdout.take(), got.clone());
    let reader = std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        while let Some(Ok(n @ 1..)) = stdout.as_mut().map(|s| s.read(&mut buf)) {
            sink.lock().unwrap().extend_from_slice(&buf[..n]);
        }
    });
    let t0 = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if t0.elapsed() < timeout => std::thread::sleep(Duration::from_millis(5)),
            _ => {
                // The whole group: a child of the shell can keep stdout open.
                let _ = Command::new("kill").arg("-KILL").arg(format!("-{}", child.id())).stderr(Stdio::null()).status();
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
        }
    }
    let t1 = Instant::now();
    while !reader.is_finished() && t1.elapsed() < Duration::from_millis(200) {
        std::thread::sleep(Duration::from_millis(2));
    }
    let out = got.lock().unwrap().clone();
    out
}

/// One JSON value at `at` (after white space): its end.
fn value_end(text: &str, at: usize) -> Option<usize> {
    let mut it = serde_json::Deserializer::from_str(&text[at..]).into_iter::<Value>();
    it.next()?.ok()?;
    Some(at + it.byte_offset())
}

fn skip_ws(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && b[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

/// The span of the value of top-level key `key` in a JSON object text, and the position of the object's closing
/// brace. None when the text is not one JSON object.
fn top_value(text: &str, key: &str) -> Option<(Option<(usize, usize)>, usize)> {
    serde_json::from_str::<Value>(text).ok()?.as_object()?;
    let b = text.as_bytes();
    let mut i = skip_ws(b, 0) + 1;
    let mut found = None;
    loop {
        i = skip_ws(b, i);
        if b[i] == b'}' {
            return Some((found, i));
        }
        let k_end = value_end(text, i)?;
        let k: String = serde_json::from_str(&text[i..k_end]).ok()?;
        let v = skip_ws(b, skip_ws(b, k_end) + 1);
        let v_end = value_end(text, v)?;
        if k == key {
            found = Some((v, v_end));
        }
        i = skip_ws(b, v_end);
        if b[i] == b',' {
            i += 1;
        }
    }
}

/// What `status` tells: on or off, and the owner's command.
pub struct State {
    pub on: bool,
    pub own: Option<String>,
}

fn wrapper_own(v: &Value) -> Option<Option<String>> {
    let c = v.get("command")?.as_str()?;
    if !c.contains(WORD) {
        return None;
    }
    let w = crate::status::shell_words(c);
    let own = w.iter().position(|a| a == "--own").and_then(|i| w.get(i + 1)).and_then(|s| b64().decode(s).ok());
    Some(own.and_then(|r| serde_json::from_slice::<Value>(&r).ok()).and_then(|v| own_command(&v)))
}

/// Reads the settings file. A missing file is an empty one. Err: not a JSON object.
fn read(settings: &Path) -> Result<String, String> {
    let text = match std::fs::read_to_string(settings) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
        Err(e) => return Err(format!("cannot read {}: {e}", settings.display())),
    };
    if top_value(&text, "statusLine").is_none() {
        return Err(format!("{} is not a JSON object; nothing written", settings.display()));
    }
    Ok(text)
}

pub fn state(settings: &Path) -> Result<State, String> {
    let text = read(settings)?;
    let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    let sl = v.get("statusLine").cloned().unwrap_or(Value::Null);
    Ok(match wrapper_own(&sl) {
        Some(own) => State { on: true, own },
        None => State { on: false, own: own_command(&sl) },
    })
}

fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// The wrapper's command text.
pub fn command(exe: &Path, dir: &Path, own_raw: Option<&str>) -> String {
    let mut c = format!("{} {WORD} --dir {}", quote(&exe.to_string_lossy()), quote(&dir.to_string_lossy()));
    if let Some(r) = own_raw {
        c += &format!(" --own {}", b64().encode(r));
    }
    c
}

/// Writes the wrapper as `statusLine.command`. The owner's `statusLine` goes into `--own`; its other fields
/// (`padding`) stay. A second `on` writes nothing.
pub fn on(settings: &Path, exe: &Path, dir: &Path) -> Result<(), String> {
    let text = read(settings)?;
    if state(settings)?.on {
        return Ok(());
    }
    let text = if text.trim().is_empty() { "{}\n".to_string() } else { text };
    let (span, close) = top_value(&text, "statusLine").ok_or("not a JSON object")?;
    let raw = span.map(|(a, b)| &text[a..b]);
    let old: Value = raw.and_then(|r| serde_json::from_str(r).ok()).unwrap_or(Value::Null);
    let mut new = if old.is_object() { old.clone() } else { json!({}) };
    new["type"] = json!("command");
    new["command"] = json!(command(exe, dir, raw));
    let new = serde_json::to_string(&new).map_err(|e| e.to_string())?;
    let out = match span {
        Some((a, b)) => format!("{}{new}{}", &text[..a], &text[b..]),
        None => {
            let head = text[..close].trim_end();
            let comma = if head.ends_with('{') { "" } else { "," };
            format!("{head}{comma}\n  \"statusLine\": {new}\n{}", &text[close..])
        }
    };
    write(settings, &out)
}

/// Puts the owner's `statusLine` back as it was, or removes the key when there was none.
pub fn off(settings: &Path) -> Result<(), String> {
    let text = read(settings)?;
    if !state(settings)?.on {
        return Ok(());
    }
    let (Some((a, b)), _) = top_value(&text, "statusLine").ok_or("not a JSON object")? else { return Ok(()) };
    let v: Value = serde_json::from_str(&text[a..b]).map_err(|e| e.to_string())?;
    let w = crate::status::shell_words(v.get("command").and_then(Value::as_str).unwrap_or_default());
    let own = w.iter().position(|x| x == "--own").and_then(|i| w.get(i + 1)).and_then(|s| b64().decode(s).ok()).and_then(|r| String::from_utf8(r).ok());
    let out = match own {
        Some(raw) => format!("{}{raw}{}", &text[..a], &text[b..]),
        None => {
            // No own line: remove the key, its name and one comma next to it.
            let k = text[..a].rfind("\"statusLine\"").unwrap_or(a);
            let before = text[..k].trim_end();
            let after = &text[b..];
            match (before.strip_suffix(','), after.trim_start().strip_prefix(',')) {
                (_, Some(rest)) => format!("{}{}", &text[..k], rest.trim_start()),
                (Some(head), None) => format!("{head}{after}"),
                _ => format!("{before}{after}"),
            }
        }
    };
    write(settings, &out)
}

/// Writes whole through a temp file in the same folder, then a rename.
fn write(settings: &Path, text: &str) -> Result<(), String> {
    let parent = settings.parent().ok_or("no folder")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let tmp = parent.join(format!(".settings.json.{}.tmp", std::process::id()));
    let r = std::fs::write(&tmp, text).and_then(|_| std::fs::rename(&tmp, settings));
    if r.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    r.map_err(|e| format!("cannot write {}: {e}", settings.display()))
}

#[cfg(test)]
mod tests;
