use super::*;
use crate::protocol::JsonLines;

const FABLE: &str = include_str!("../../../fixtures/statusline-fable.json");

fn args(dir: &Path, own: Option<&str>) -> Vec<String> {
    let mut a = vec!["--dir".to_string(), dir.to_string_lossy().into()];
    if let Some(o) = own {
        a.extend(["--own".to_string(), b64().encode(o)]);
    }
    a
}

#[test]
fn wrap_runs_the_own_command_with_the_same_input_and_saves_both() {
    let t = tempfile::tempdir().unwrap();
    let out = wrap(&args(t.path(), Some(r#"{"type":"command","command":"printf 'mine '; wc -c | tr -d ' '"}"#)), FABLE.as_bytes(), SystemTime::now());
    assert_eq!(String::from_utf8(out).unwrap(), format!("mine {}\n", FABLE.len()));
    assert_eq!(std::fs::read_to_string(t.path().join("fable-1.json")).unwrap(), FABLE);
    assert!(std::fs::read_to_string(t.path().join("fable-1.out")).unwrap().starts_with("mine "));
}

#[test]
fn wrap_with_no_own_command_prints_nothing_and_saves_the_input() {
    let t = tempfile::tempdir().unwrap();
    assert!(wrap(&args(t.path(), None), FABLE.as_bytes(), SystemTime::now()).is_empty());
    assert!(t.path().join("fable-1.json").exists());
}

#[test]
fn wrap_prints_what_a_failing_own_command_printed() {
    let t = tempfile::tempdir().unwrap();
    let out = wrap(&args(t.path(), Some(r#"{"command":"echo half; exit 3"}"#)), FABLE.as_bytes(), SystemTime::now());
    assert_eq!(out, b"half\n");
}

#[test]
fn wrap_with_bad_input_still_runs_the_own_command_and_saves_nothing() {
    let t = tempfile::tempdir().unwrap();
    let out = wrap(&args(t.path(), Some(r#"{"command":"cat"}"#)), b"not json", SystemTime::now());
    assert_eq!(out, b"not json");
    assert_eq!(std::fs::read_dir(t.path()).unwrap().count(), 0);
}

#[test]
fn wrap_with_a_folder_it_cannot_write_still_prints() {
    let t = tempfile::tempdir().unwrap();
    let file = t.path().join("file");
    std::fs::write(&file, "x").unwrap();
    let out = wrap(&args(&file.join("sub"), Some(r#"{"command":"echo ok"}"#)), FABLE.as_bytes(), SystemTime::now());
    assert_eq!(out, b"ok\n");
}

#[test]
fn run_own_stops_at_the_timeout() {
    let t0 = Instant::now();
    let out = run_own("echo early; sleep 5", b"", Duration::from_millis(200));
    assert!(t0.elapsed() < Duration::from_secs(2));
    assert_eq!(out, b"early\n");
}

const SETTINGS: &str = "{\n  \"model\": \"opus\",\n  \"statusLine\": {\n    \"type\": \"command\",\n    \"command\": \"~/bin/my line\",\n    \"padding\": 0\n  },\n  \"hooks\": {\"Stop\": []}\n}\n";

fn setup(text: Option<&str>) -> (tempfile::TempDir, PathBuf) {
    let t = tempfile::tempdir().unwrap();
    let s = t.path().join("claude").join("settings.json");
    if let Some(x) = text {
        std::fs::create_dir_all(s.parent().unwrap()).unwrap();
        std::fs::write(&s, x).unwrap();
    }
    (t, s)
}

#[test]
fn on_wraps_the_own_command_and_off_puts_it_back_exactly() {
    let (t, s) = setup(Some(SETTINGS));
    let dir = t.path().join("home/herdr-contr/statusline");
    on(&s, Path::new("/opt/x/herdr-contr-feed"), &dir).unwrap();
    let text = std::fs::read_to_string(&s).unwrap();
    assert!(text.starts_with("{\n  \"model\": \"opus\",\n  \"statusLine\": {"));
    assert!(text.ends_with("},\n  \"hooks\": {\"Stop\": []}\n}\n"));
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["statusLine"]["padding"], 0);
    assert_eq!(crate::status::folder(&s), Some(dir));
    let st = state(&s).unwrap();
    assert!(st.on);
    assert_eq!(st.own.as_deref(), Some("~/bin/my line"));
    off(&s).unwrap();
    assert_eq!(std::fs::read_to_string(&s).unwrap(), SETTINGS);
    assert!(!state(&s).unwrap().on);
}

#[test]
fn on_twice_does_not_wrap_twice() {
    let (t, s) = setup(Some(SETTINGS));
    on(&s, Path::new("/x"), t.path()).unwrap();
    let once = std::fs::read_to_string(&s).unwrap();
    on(&s, Path::new("/x"), t.path()).unwrap();
    assert_eq!(std::fs::read_to_string(&s).unwrap(), once);
    off(&s).unwrap();
    assert_eq!(std::fs::read_to_string(&s).unwrap(), SETTINGS);
}

#[test]
fn on_and_off_with_no_own_status_line() {
    let original = "{\n  \"model\": \"opus\"\n}\n";
    let (t, s) = setup(Some(original));
    on(&s, Path::new("/x"), t.path()).unwrap();
    let st = state(&s).unwrap();
    assert!(st.on && st.own.is_none());
    off(&s).unwrap();
    assert_eq!(std::fs::read_to_string(&s).unwrap(), original);
}

#[test]
fn on_with_a_missing_settings_file_makes_one() {
    let (t, s) = setup(None);
    assert!(!state(&s).unwrap().on);
    on(&s, Path::new("/x"), t.path()).unwrap();
    assert!(state(&s).unwrap().on);
    assert_eq!(crate::status::folder(&s), Some(t.path().to_path_buf()));
}

#[test]
fn a_bad_settings_file_is_not_written() {
    for bad in ["{ not json", "[1, 2]"] {
        let (t, s) = setup(Some(bad));
        assert!(on(&s, Path::new("/x"), t.path()).unwrap_err().contains("not a JSON object"));
        assert!(off(&s).is_err());
        assert!(state(&s).is_err());
        assert_eq!(std::fs::read_to_string(&s).unwrap(), bad);
    }
}

#[test]
fn the_wrapper_record_gives_status_the_context_of_a_model_with_no_limit() {
    let (t, s) = setup(Some("{}"));
    let dir = t.path().join("statusline");
    on(&s, Path::new("/x"), &dir).unwrap();
    let words = crate::status::shell_words(&state_command(&s));
    let i = words.iter().position(|w| w == WORD).unwrap();
    wrap(&words[i + 1..], FABLE.as_bytes(), SystemTime::now());
    let log = t.path().join("fable-1.jsonl");
    std::fs::write(&log, "{\"type\":\"assistant\",\"message\":{\"model\":\"claude-fable-5-2\",\"role\":\"assistant\",\"content\":[]}}\n").unwrap();
    let mut out = JsonLines::new();
    crate::status::run("claude", Some(&log), &s, None, &mut out, SystemTime::now());
    let line: Value = serde_json::from_str(out.text().lines().last().unwrap()).unwrap();
    assert_eq!(line["source"], "statusline");
    let ctx = line["tags"].as_array().unwrap().iter().find(|t| t["kind"] == "context").unwrap();
    assert_eq!(ctx["text"], "ctx 17%");
}

fn state_command(s: &Path) -> String {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(s).unwrap()).unwrap();
    v["statusLine"]["command"].as_str().unwrap().to_string()
}

#[test]
fn wrap_never_runs_a_wrapper_as_the_own_command() {
    let t = tempfile::tempdir().unwrap();
    let out = wrap(&args(t.path(), Some(r#"{"command":"echo herdr-contr-statusline"}"#)), FABLE.as_bytes(), SystemTime::now());
    assert!(out.is_empty());
}
