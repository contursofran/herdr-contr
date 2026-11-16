//! `herdr-contr-feed health`: checks the install in one Herdr home (#217). One `check` line for each check:
//! `{id, state: ok|warn|fail, message, fix}`. It reads files and runs `herdr-contr-orch --version`; it writes nothing.
//! The mod gives what only Herdr knows: the layout in use, Herdr's version and hc's version.

use herdr_core::places::Places;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

use crate::install::MODS;

/// The oldest Herdr that this mod works with.
pub const MIN_HERDR: &str = "0.1.16";

/// The layouts that the install puts in the Herdr home.
pub const LAYOUTS: [&str; 2] = ["herdr-contr", "herdr-contr-own"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub id: &'static str,
    pub state: &'static str,
    pub message: String,
    pub fix: String,
}

impl Check {
    fn new(id: &'static str, state: &'static str, message: impl Into<String>, fix: impl Into<String>) -> Check {
        Check { id, state, message: message.into(), fix: fix.into() }
    }
    pub fn line(&self) -> Value {
        json!({"type": "check", "id": self.id, "state": self.state, "message": self.message, "fix": self.fix})
    }
}

/// What the checks read. Tests give fakes.
pub struct Inputs<'a> {
    pub home: PathBuf,
    /// This helper's build stamp.
    pub build: String,
    /// The layout in use, from the mod.
    pub layout: Option<String>,
    pub herdr: Option<String>,
    pub hc_version: Option<String>,
    pub hc_up: bool,
    /// The Claude Code folder (`~/.claude`, or CLAUDE_CONFIG_DIR).
    pub claude: Option<PathBuf>,
    /// Runs `<orch> --version`: its stdout, or why it did not run.
    pub orch_version: &'a dyn Fn(&Path) -> Result<String, String>,
}

const REINSTALL: &str = "Install again: unzip the latest release and run ./install.sh.";

pub fn checks(i: &Inputs) -> Vec<Check> {
    let p = Places::new(&i.home);
    let record: Option<Value> = fs::read(p.install_record()).ok().and_then(|b| serde_json::from_slice(&b).ok());
    vec![mods(i, &record), layout(i, &record), collisions(i), orch(i, &p), hc(i), kit(i, &p), herdr(i)]
}

fn sha_file(p: &Path) -> Option<String> {
    fs::read(p).ok().map(|b| format!("{:x}", Sha256::digest(&b)))
}

/// The files of the record whose path starts with `prefix`, and those that are missing or changed.
fn differ(home: &Path, record: &Value, prefix: &str) -> (usize, Vec<String>, Vec<String>) {
    let (mut n, mut missing, mut changed) = (0, vec![], vec![]);
    for (rel, want) in record.get("files").and_then(|f| f.as_object()).into_iter().flatten() {
        if !rel.starts_with(prefix) {
            continue;
        }
        n += 1;
        match sha_file(&home.join(rel)) {
            None => missing.push(rel.clone()),
            Some(h) if Some(h.as_str()) != want.as_str() => changed.push(rel.clone()),
            _ => {}
        }
    }
    (n, missing, changed)
}

fn mods(i: &Inputs, record: &Option<Value>) -> Check {
    let Some(r) = record else {
        return Check::new("mods", "fail", "There is no herdr-contr/install.json: the mod was not installed with the helper.", REINSTALL);
    };
    let (n, missing, changed) = differ(&i.home, r, "mods/");
    let built = r.get("build").and_then(|b| b.as_str()).unwrap_or("unknown");
    if n == 0 || !missing.is_empty() {
        let names = if n == 0 { MODS.join(", ") } else { missing.join(", ") };
        return Check::new("mods", "fail", format!("Files of our mods are missing: {names}."), REINSTALL);
    }
    if built != i.build {
        return Check::new("mods", "fail", format!("The mods are from build {built}, the helper is from build {}.", i.build), REINSTALL);
    }
    if !changed.is_empty() {
        return Check::new("mods", "warn", format!("Files of our mods were changed after the install: {}.", changed.join(", ")), REINSTALL);
    }
    Check::new("mods", "ok", format!("{} are installed, build {built}.", MODS.join(", ")), "")
}

fn layout(i: &Inputs, record: &Option<Value>) -> Check {
    let name = i.layout.as_deref().unwrap_or("");
    if !LAYOUTS.contains(&name) {
        let shown = if name.is_empty() { "unknown".to_string() } else { name.to_string() };
        return Check::new("layout", "warn", format!("The layout in use is {shown}, not herdr-contr."), "Switch the layout to herdr-contr (Herdr's layout menu).");
    }
    if let Some(r) = record {
        let (_, missing, changed) = differ(&i.home, r, &format!("layouts/{name}.json"));
        if !missing.is_empty() || !changed.is_empty() {
            return Check::new("layout", "warn", format!("The layout {name} was changed after the install: another mod can dock into it."), REINSTALL);
        }
    }
    Check::new("layout", "ok", format!("The layout in use is {name}."), "")
}

/// Another mod with one of our names or titles, and a mod in our layouts that is not there.
fn collisions(i: &Inputs) -> Check {
    let mods_dir = i.home.join("mods");
    let title = |dir: &Path| -> Option<String> {
        let v: Value = serde_json::from_slice(&fs::read(dir.join("mod.json")).ok()?).ok()?;
        v.get("title").and_then(|t| t.as_str()).map(String::from)
    };
    let ours: Vec<String> = MODS.iter().filter_map(|m| title(&mods_dir.join(m))).collect();
    let mut bad = vec![];
    let mut names: Vec<String> = fs::read_dir(&mods_dir).into_iter().flatten().flatten().filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    for name in &names {
        if MODS.contains(&name.as_str()) || crate::install::RETIRED.contains(&name.as_str()) {
            continue;
        }
        let t = title(&mods_dir.join(name));
        if name.starts_with("herdr-contr") || t.as_ref().is_some_and(|t| ours.contains(t)) {
            bad.push(format!("mods/{name}"));
        }
    }
    for l in LAYOUTS {
        let Ok(b) = fs::read(i.home.join("layouts").join(format!("{l}.json"))) else { continue };
        let Ok(v) = serde_json::from_slice::<Value>(&b) else { continue };
        let mut used = vec![];
        mods_in(&v, &mut used);
        for m in used {
            if !names.contains(&m) && !bad.contains(&format!("layouts/{l}.json: {m}")) {
                bad.push(format!("layouts/{l}.json: {m}"));
            }
        }
    }
    if bad.is_empty() {
        Check::new("collisions", "ok", "No other mod uses our names, and each mod in our layouts is there.", "")
    } else {
        Check::new("collisions", "fail", format!("Other mods collide with ours, or our layouts name a mod that is not there: {}.", bad.join(", ")),
            "Remove or rename the other mod (for example an older contr or Flockbook mod), then install again.")
    }
}

fn mods_in(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Object(o) => {
            if let Some(Value::String(m)) = o.get("mod") {
                out.push(m.clone());
            }
            o.values().for_each(|x| mods_in(x, out));
        }
        Value::Array(a) => a.iter().for_each(|x| mods_in(x, out)),
        _ => {}
    }
}

fn orch(i: &Inputs, p: &Places) -> Check {
    let path = p.orch();
    if !path.is_file() {
        return Check::new("orch", "fail", "herdr-contr-orch is not in herdr-contr/bin.", REINSTALL);
    }
    match (i.orch_version)(&path) {
        Err(why) => Check::new("orch", "fail", format!("herdr-contr-orch does not run: {why}"), REINSTALL),
        Ok(out) => {
            // "herdr-contr-orch <version> (<build>)"
            let build = out.trim().split_once('(').and_then(|(_, b)| b.strip_suffix(')')).unwrap_or("unknown");
            if build != i.build {
                Check::new("orch", "fail", format!("herdr-contr-orch is from build {build}, the helper is from build {}.", i.build), REINSTALL)
            } else {
                Check::new("orch", "ok", format!("The helper and herdr-contr-orch are from build {build}."), "")
            }
        }
    }
}

fn hc(i: &Inputs) -> Check {
    let v = i.hc_version.as_deref().unwrap_or("unknown");
    if i.hc_up {
        Check::new("hc", "ok", format!("hc is reachable, version {v}."), "")
    } else {
        Check::new("hc", "fail", "hc (herdr-core) is not reachable.", "Start Herdr again; it starts hc.")
    }
}

fn kit(i: &Inputs, p: &Places) -> Check {
    if !p.kit().join(".claude-plugin").join("plugin.json").is_file() || !p.marketplace().is_file() {
        return Check::new("kit", "fail", "The kit is not in herdr-contr/kit.", REINSTALL);
    }
    let add = format!("Run: claude plugin marketplace add {} and claude plugin install contr@contr.", p.folder().display());
    let Some(dir) = &i.claude else { return Check::new("kit", "warn", "The Claude Code folder is not known.", add) };
    let known: Option<Value> = fs::read(dir.join("plugins").join("known_marketplaces.json")).ok().and_then(|b| serde_json::from_slice(&b).ok());
    let path = known.as_ref().and_then(|k| k.pointer("/contr/source/path")).and_then(|v| v.as_str());
    match path {
        None => Check::new("kit", "warn", "The Claude Code plugin contr is not added.", add),
        Some(x) if Path::new(x) != p.folder() => Check::new("kit", "fail", format!("The Claude Code plugin contr reads {x}, not the installed kit."), add),
        Some(_) => Check::new("kit", "ok", "The Claude Code plugin contr reads the installed kit.", ""),
    }
}

fn parse(v: &str) -> Vec<u64> {
    v.trim().trim_start_matches('v').split(['.', '-', ' ']).map_while(|x| x.parse().ok()).collect()
}

fn herdr(i: &Inputs) -> Check {
    match i.herdr.as_deref() {
        None => Check::new("herdr", "warn", "Herdr's version is not known.", ""),
        Some(v) if parse(v) < parse(MIN_HERDR) => {
            Check::new("herdr", "fail", format!("Herdr is {v}; herdr-contr needs {MIN_HERDR} or later."), "Update Herdr.")
        }
        Some(v) => Check::new("herdr", "ok", format!("Herdr is {v}."), ""),
    }
}

/// The Claude Code folder: CLAUDE_CONFIG_DIR, else ~/.claude.
pub fn claude_dir(env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    env("CLAUDE_CONFIG_DIR").map(PathBuf::from).or_else(|| env("HOME").map(|h| Path::new(&h).join(".claude")))
}

/// Runs `<orch> --version`.
pub fn run_orch_version(path: &Path) -> Result<String, String> {
    let o = std::process::Command::new(path).arg("--version").output().map_err(|e| e.to_string())?;
    if !o.status.success() {
        return Err(format!("exit {}", o.status.code().unwrap_or(-1)));
    }
    Ok(String::from_utf8_lossy(&o.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const B: &str = "abc1234 2026-11-16";

    fn put(home: &Path, rel: &str, body: &str) {
        let p = home.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, body).unwrap();
    }

    /// A good install in a temp home.
    fn good() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        let h = d.path();
        let mut files = serde_json::Map::new();
        for (rel, body) in [
            ("mods/herdr-contr/mod.json", r#"{"title":"herdr-contr"}"#),
            ("mods/herdr-contr-spaces/mod.json", r#"{"title":"Spaces C"}"#),
            ("mods/herdr-contr-tabs/mod.json", r#"{"title":"Tabs C"}"#),
            ("layouts/herdr-contr.json", r#"{"children":[{"content":{"mod":"herdr-contr"}},{"content":{"mod":"pane"}}]}"#),
        ] {
            put(h, rel, body);
            files.insert(rel.into(), json!(sha_file(&h.join(rel)).unwrap()));
        }
        put(h, "mods/pane/mod.json", r#"{"title":"Pane"}"#);
        put(h, "herdr-contr/install.json", &json!({"build": B, "files": files}).to_string());
        put(h, "herdr-contr/bin/herdr-contr-orch", "");
        put(h, "herdr-contr/kit/.claude-plugin/plugin.json", "{}");
        put(h, "herdr-contr/.claude-plugin/marketplace.json", "{}");
        let src = h.join("herdr-contr").display().to_string();
        put(h, "claude/plugins/known_marketplaces.json", &json!({"contr": {"source": {"path": src}}}).to_string());
        d
    }

    fn ok_orch(_: &Path) -> Result<String, String> {
        Ok(format!("herdr-contr-orch 0.1.0 ({B})\n"))
    }

    fn inputs<'a>(h: &Path, orch: &'a dyn Fn(&Path) -> Result<String, String>) -> Inputs<'a> {
        Inputs {
            home: h.into(),
            build: B.into(),
            layout: Some("herdr-contr".into()),
            herdr: Some("0.1.16".into()),
            hc_version: Some("0.5.0".into()),
            hc_up: true,
            claude: Some(h.join("claude")),
            orch_version: orch,
        }
    }

    fn state(c: &[Check], id: &str) -> (&'static str, String) {
        let x = c.iter().find(|x| x.id == id).unwrap();
        (x.state, x.message.clone())
    }

    #[test]
    fn a_good_install_is_all_ok() {
        let d = good();
        let c = checks(&inputs(d.path(), &ok_orch));
        assert_eq!(c.len(), 7);
        for x in &c {
            assert_eq!(x.state, "ok", "{x:?}");
        }
        assert_eq!(c[0].line()["type"], "check");
    }

    #[test]
    fn mods_missing_changed_or_from_another_build() {
        let d = good();
        let h = d.path();
        assert_eq!(state(&checks(&inputs(h, &ok_orch)), "mods").0, "ok");
        put(h, "mods/herdr-contr-tabs/mod.json", "{}");
        assert_eq!(state(&checks(&inputs(h, &ok_orch)), "mods").0, "warn");
        let mut i = inputs(h, &ok_orch);
        i.build = "other".into();
        assert!(state(&checks(&i), "mods").1.contains("build abc1234"));
        fs::remove_file(h.join("mods/herdr-contr-tabs/mod.json")).unwrap();
        assert_eq!(state(&checks(&inputs(h, &ok_orch)), "mods").0, "fail");
        fs::remove_file(h.join("herdr-contr/install.json")).unwrap();
        assert!(state(&checks(&inputs(h, &ok_orch)), "mods").1.contains("install.json"));
    }

    #[test]
    fn another_layout_or_a_changed_one_warns() {
        let d = good();
        let mut i = inputs(d.path(), &ok_orch);
        i.layout = Some("main".into());
        assert_eq!(state(&checks(&i), "layout"), ("warn", "The layout in use is main, not herdr-contr.".into()));
        put(d.path(), "layouts/herdr-contr.json", r#"{"mod":"herdr-contr"}"#);
        assert!(state(&checks(&inputs(d.path(), &ok_orch)), "layout").1.contains("changed"));
    }

    #[test]
    fn a_mod_with_our_name_or_title_collides() {
        let d = good();
        put(d.path(), "mods/flock/mod.json", r#"{"title":"herdr-contr"}"#);
        put(d.path(), "mods/herdr-contr-x/mod.json", "{}");
        let (s, m) = state(&checks(&inputs(d.path(), &ok_orch)), "collisions");
        assert_eq!(s, "fail");
        assert!(m.contains("mods/flock") && m.contains("mods/herdr-contr-x"), "{m}");
    }

    #[test]
    fn our_layout_naming_a_missing_mod_fails() {
        let d = good();
        fs::remove_dir_all(d.path().join("mods/pane")).unwrap();
        assert!(state(&checks(&inputs(d.path(), &ok_orch)), "collisions").1.contains("layouts/herdr-contr.json: pane"));
    }

    #[test]
    fn orch_missing_broken_or_another_build() {
        let d = good();
        let bad = |_: &Path| -> Result<String, String> { Err("exit 1".into()) };
        assert!(state(&checks(&inputs(d.path(), &bad)), "orch").1.contains("does not run"));
        let old = |_: &Path| -> Result<String, String> { Ok("herdr-contr-orch 0.1.0 (old 2026-01-01)".into()) };
        assert_eq!(state(&checks(&inputs(d.path(), &old)), "orch").0, "fail");
        fs::remove_file(d.path().join("herdr-contr/bin/herdr-contr-orch")).unwrap();
        assert_eq!(state(&checks(&inputs(d.path(), &ok_orch)), "orch").0, "fail");
    }

    #[test]
    fn hc_down_fails() {
        let d = good();
        let mut i = inputs(d.path(), &ok_orch);
        i.hc_up = false;
        assert_eq!(state(&checks(&i), "hc").0, "fail");
    }

    #[test]
    fn kit_plugin_elsewhere_absent_or_kit_missing() {
        let d = good();
        let h = d.path();
        put(h, "claude/plugins/known_marketplaces.json", r#"{"contr":{"source":{"path":"/elsewhere"}}}"#);
        assert!(state(&checks(&inputs(h, &ok_orch)), "kit").1.contains("/elsewhere"));
        put(h, "claude/plugins/known_marketplaces.json", "{}");
        assert_eq!(state(&checks(&inputs(h, &ok_orch)), "kit").0, "warn");
        fs::remove_dir_all(h.join("herdr-contr/kit")).unwrap();
        assert_eq!(state(&checks(&inputs(h, &ok_orch)), "kit").0, "fail");
    }

    #[test]
    fn herdr_too_old_fails() {
        let d = good();
        let mut i = inputs(d.path(), &ok_orch);
        i.herdr = Some("0.1.9".into());
        assert_eq!(state(&checks(&i), "herdr").0, "fail");
        i.herdr = Some("0.2.0".into());
        assert_eq!(state(&checks(&i), "herdr").0, "ok");
        i.herdr = None;
        assert_eq!(state(&checks(&i), "herdr").0, "warn");
    }
}
