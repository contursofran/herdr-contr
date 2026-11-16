//! The profiles file (`agents.json`): how each kind of worker starts, and the flow, how the coordinator takes an
//! issue to a merge. Ported from contr's `internal/agents` (config and flow).
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::jsonfile;
use crate::workers::flag_value;

/// How to start one kind of agent: the program and its arguments. The task, when there is one, goes after them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    #[serde(default)]
    pub argv: Vec<String>,
    /// The kit agent definition that the profile starts as. Empty means the profile name, so a variant such as
    /// planner-high can start as the planner agent.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub agent: String,
}

/// The profile that each worker role spawns with. An empty name is the role's own name.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RoleProfiles {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub builder: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub planner: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub reviewer: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub debugger: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub explorer: String,
}

impl RoleProfiles {
    /// The worker roles and their profiles, in a fixed order.
    pub fn roles(&self) -> Vec<(&'static str, String)> {
        let or = |v: &str, role: &'static str| (role, if v.is_empty() { role.to_string() } else { v.to_string() });
        vec![
            or(&self.builder, "builder"),
            or(&self.planner, "planner"),
            or(&self.reviewer, "reviewer"),
            or(&self.debugger, "debugger"),
            or(&self.explorer, "explorer"),
        ]
    }
    fn is_zero(&self) -> bool {
        *self == RoleProfiles::default()
    }
}

/// How the coordinator takes an issue to a merge. A key missing from the file keeps its default.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Flow {
    /// When a planner runs first: never, when-needed or always.
    pub plan: String,
    /// Which PRs get a reviewer: none, code or every.
    pub review: String,
    /// The number of failed fix rounds before a debugger.
    pub debugger_after: i64,
    /// The most workers at once, reviewers included.
    pub workers: i64,
    /// ask (the owner's OK each time) or auto (when CI and review pass).
    pub merge: String,
    #[serde(skip_serializing_if = "RoleProfiles::is_zero")]
    pub profiles: RoleProfiles,
}

impl Default for Flow {
    /// The rules of the kit's orchestrate playbook.
    fn default() -> Flow {
        Flow { plan: "when-needed".into(), review: "code".into(), debugger_after: 2, workers: 4, merge: "ask".into(), profiles: RoleProfiles::default() }
    }
}

/// The valid values of each text key of the flow.
pub const FLOW_CHOICES: [(&str, &[&str]); 3] =
    [("plan", &["never", "when-needed", "always"]), ("review", &["none", "code", "every"]), ("merge", &["ask", "auto"])];
/// The number keys of the flow and their smallest value; they have no largest value.
pub const FLOW_NUMBERS: [&str; 2] = ["debugger_after", "workers"];
pub const FLOW_MIN: i64 = 1;

impl Flow {
    /// An error for a value out of range.
    pub fn validate(&self) -> Result<(), String> {
        let check = |key: &str, v: &str| {
            let ok = FLOW_CHOICES.iter().find(|(k, _)| *k == key).map(|(_, c)| *c).unwrap_or_default();
            if ok.contains(&v) { Ok(()) } else { Err(format!("flow {key}: {v:?} is not one of {}", ok.join(", "))) }
        };
        check("plan", &self.plan)?;
        check("review", &self.review)?;
        check("merge", &self.merge)?;
        if self.debugger_after < FLOW_MIN {
            return Err(format!("flow debugger_after: {} is less than {FLOW_MIN}", self.debugger_after));
        }
        if self.workers < FLOW_MIN {
            return Err(format!("flow workers: {} is less than {FLOW_MIN}", self.workers));
        }
        Ok(())
    }

    /// The flow as plain rules, one per line, for the coordinator.
    pub fn lines(&self) -> Vec<String> {
        let plan = match self.plan.as_str() {
            "never" => "Plan first: never. Spawn builder directly; the builder plans in its PR body.",
            "when-needed" => "Plan first: when needed. Spawn planner first only when the work needs several PRs or the issue leaves the approach open.",
            "always" => "Plan first: always. Spawn planner first for each issue, then builder with the plan.",
            _ => "",
        };
        let review = match self.review.as_str() {
            "none" => "Review: none. Spawn no reviewer; read the diff yourself.",
            "code" => "Review: code PRs. Every PR that changes code gets a reviewer; for a docs-only or config-only PR, read the diff yourself.",
            "every" => "Review: every PR gets a reviewer.",
            _ => "",
        };
        let merge = match self.merge.as_str() {
            "ask" => "Merge: ask the owner each time.",
            "auto" => "Merge: merge yourself when CI and review pass.",
            _ => "",
        };
        let roles: Vec<String> = self.profiles.roles().into_iter().map(|(role, p)| format!("{role}: {p}")).collect();
        vec![
            plan.to_string(),
            review.to_string(),
            format!("Debugger: spawn debugger after {} failed fix rounds.", self.debugger_after),
            format!("Workers at once: at most {}, reviewers included. Spawn and a message to a settled worker refuse past it.", self.workers),
            merge.to_string(),
            format!("Profiles: {}. Spawn each role with its profile only.", roles.join(", ")),
        ]
    }

    /// The flow block: "The flow:" and one line per rule.
    pub fn text(&self) -> String {
        format!("The flow:\n- {}\n", self.lines().join("\n- "))
    }
}

/// The note for a file with a presets key and no profiles key: the file of the old Swift app.
pub const OLD_FORMAT_NOTICE: &str = "agents.json is in the old format (presets); its presets are ignored and the default profiles apply";

/// The profiles file as read. `kit` is the folder of the Claude Code plugin; `setup` maps a project's main
/// checkout to a shell command that prepares a new worktree of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub profiles: BTreeMap<String, Profile>,
    pub kit: String,
    pub setup: BTreeMap<String, String>,
    /// The most open pull requests of a project, by its main checkout (as in `setup`). At or over it, spawn
    /// refuses a worker for a new issue. A project that is not here has no limit.
    pub open_prs: BTreeMap<String, i64>,
    pub flow: Flow,
    /// Where the profiles come from: the file that `load` read, with "(not found)" after it when it is missing.
    pub source: String,
    /// A note for the owner that `load` made, or "".
    pub notice: String,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct FileConfig {
    profiles: Option<BTreeMap<String, Profile>>,
    kit: Option<String>,
    setup: Option<BTreeMap<String, String>>,
    open_prs: Option<BTreeMap<String, i64>>,
    flow: Option<Flow>,
}

impl Default for Config {
    /// The owner's standard profiles and the default flow.
    fn default() -> Config {
        let claude = |model: &str, effort: &str| Profile {
            argv: ["claude", "--model", model, "--effort", effort, "--permission-mode", "auto"].iter().map(|s| s.to_string()).collect(),
            agent: String::new(),
        };
        let profiles = [
            ("orchestrator", claude("opus", "medium")),
            ("builder", claude("sonnet", "medium")),
            ("planner", claude("opus", "low")),
            ("reviewer", claude("opus", "low")),
            ("debugger", claude("opus", "xhigh")),
            ("explorer", claude("sonnet", "medium")),
        ];
        Config {
            profiles: profiles.into_iter().map(|(n, p)| (n.to_string(), p)).collect(),
            kit: String::new(),
            setup: BTreeMap::new(),
            open_prs: BTreeMap::new(),
            flow: Flow::default(),
            source: String::new(),
            notice: String::new(),
        }
    }
}

impl Config {
    /// Reads the file over the defaults: a profile in the file replaces the default of the same name. A missing
    /// file gives the defaults. A file in the old format loads with the defaults and a notice.
    pub fn load(path: &Path) -> Result<Config, String> {
        let mut cfg = Config { source: path.display().to_string(), ..Config::default() };
        let data = match fs::read(path) {
            Ok(d) => d,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                cfg.source = format!("{} (not found)", path.display());
                return Ok(cfg);
            }
            Err(e) => return Err(format!("agents: {}: {e}", path.display())),
        };
        let file: FileConfig = serde_json::from_slice(&data).map_err(|e| format!("agents: {}: {e}", path.display()))?;
        if let Ok(Value::Object(keys)) = serde_json::from_slice::<Value>(&data) {
            if keys.contains_key("presets") && !keys.contains_key("profiles") {
                cfg.notice = OLD_FORMAT_NOTICE.to_string();
            }
        }
        if let Some(kit) = file.kit.filter(|k| !k.is_empty()) {
            cfg.kit = kit;
        }
        cfg.setup = file.setup.unwrap_or_default();
        cfg.open_prs = file.open_prs.unwrap_or_default();
        if let Some((k, n)) = cfg.open_prs.iter().find(|(_, n)| **n < FLOW_MIN) {
            return Err(format!("agents: {}: open_prs {k:?}: {n} is less than {FLOW_MIN}", path.display()));
        }
        let flow = file.flow.unwrap_or_default();
        flow.validate().map_err(|e| format!("agents: {}: {e}", path.display()))?;
        cfg.flow = flow;
        for (name, p) in file.profiles.unwrap_or_default() {
            if p.argv.is_empty() {
                return Err(format!("agents: {}: profile {name:?} has no argv", path.display()));
            }
            cfg.profiles.insert(name, p);
        }
        Ok(cfg)
    }

    /// The model that the profile starts: the value after --model in its argv, or "".
    pub fn model_of(&self, profile: &str) -> String {
        self.profiles.get(profile).map(|p| flag_value(&p.argv, "--model")).unwrap_or_default()
    }

    /// The effort that the profile starts: the value after --effort in its argv, or "".
    pub fn effort_of(&self, profile: &str) -> String {
        self.profiles.get(profile).map(|p| flag_value(&p.argv, "--effort")).unwrap_or_default()
    }

    /// The error for a profile that the config does not have: it names the file that was read and the profiles.
    pub fn no_profile(&self, name: &str) -> String {
        let names: Vec<&str> = self.profiles.keys().map(String::as_str).collect();
        if self.source.is_empty() {
            return format!("agents: no profile {name:?}; profiles: {}", names.join(", "));
        }
        format!("agents: no profile {name:?} in {}; profiles: {}", self.source, names.join(", "))
    }

    /// An error when the flow names `profile` for no worker role; it names the profiles that the flow allows. A
    /// profile that the config does not have gives no error here: spawn reports it.
    pub fn check_profile(&self, profile: &str) -> Result<(), String> {
        if !self.profiles.contains_key(profile) {
            return Ok(());
        }
        let mut allowed: Vec<String> = Vec::new();
        for (_, p) in self.flow.profiles.roles() {
            if p == profile {
                return Ok(());
            }
            if !allowed.contains(&p) {
                allowed.push(p);
            }
        }
        Err(format!(
            "agents: the flow names no role with profile {profile:?}; allowed: {} (override: --any-profile, or any_profile in the spawn tool)",
            allowed.join(", ")
        ))
    }

    /// Writes the config to `path`. Keys in the file that the config does not know stay as they are. A profile
    /// goes to the file only when the file had it or when it differs from the default, so the file does not
    /// freeze today's defaults.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let mut raw: Map<String, Value> = match fs::read(path) {
            Ok(d) => serde_json::from_slice(&d).map_err(|e| format!("agents: {}: {e}", path.display()))?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Map::new(),
            Err(e) => return Err(format!("agents: {}: {e}", path.display())),
        };
        self.flow.validate().map_err(|e| format!("agents: {e}"))?;
        let had = match raw.get("profiles") {
            None => None,
            Some(Value::Object(m)) => Some(m.clone()),
            Some(Value::Null) => Some(Map::new()),
            Some(_) => return Err(format!("agents: {}: profiles: not an object", path.display())),
        };
        let defaults = Config::default().profiles;
        let mut profiles = Map::new();
        for (name, p) in &self.profiles {
            let in_file = had.as_ref().is_some_and(|h| h.contains_key(name));
            if in_file || defaults.get(name) != Some(p) {
                profiles.insert(name.clone(), serde_json::to_value(p).map_err(|e| e.to_string())?);
            }
        }
        let mut set = |key: &str, v: Value, empty: bool| {
            if empty { raw.remove(key); } else { raw.insert(key.to_string(), v); }
        };
        let no_profiles = profiles.is_empty() && had.is_none();
        set("profiles", Value::Object(profiles), no_profiles);
        set("kit", Value::String(self.kit.clone()), self.kit.is_empty());
        set("setup", serde_json::to_value(&self.setup).map_err(|e| e.to_string())?, self.setup.is_empty());
        set("open_prs", serde_json::to_value(&self.open_prs).map_err(|e| e.to_string())?, self.open_prs.is_empty());
        set("flow", serde_json::to_value(&self.flow).map_err(|e| e.to_string())?, false);
        let mut out = serde_json::to_vec_pretty(&raw).map_err(|e| e.to_string())?;
        out.push(b'\n');
        jsonfile::write(path, &out, 0o644).map_err(|e| format!("agents: {}: {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(text: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agents.json");
        fs::write(&path, text).unwrap();
        (dir, path)
    }

    #[test]
    fn a_missing_file_gives_the_defaults_and_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = Config::load(&dir.path().join("agents.json")).unwrap();
        assert_eq!(cfg.flow, Flow::default());
        assert_eq!(cfg.model_of("builder"), "sonnet");
        assert_eq!(cfg.effort_of("debugger"), "xhigh");
        assert!(cfg.source.ends_with("agents.json (not found)"));
    }

    #[test]
    fn a_profile_in_the_file_replaces_the_default_and_a_missing_flow_key_keeps_its_default() {
        let (_d, path) = file(r#"{"kit":"/kit","profiles":{"builder":{"argv":["claude","--model=opus","--effort","low"]},
            "planner-high":{"argv":["claude"],"agent":"planner"}},"setup":{"~/code/app":"npm ci"},
            "flow":{"plan":"never","merge":"auto","profiles":{"planner":"planner-high"}}}"#);
        let cfg = Config::load(&path).unwrap();
        assert_eq!(cfg.model_of("builder"), "opus");
        assert_eq!(cfg.effort_of("builder"), "low");
        assert_eq!(cfg.profiles["planner-high"].agent, "planner");
        assert_eq!(cfg.model_of("reviewer"), "opus");
        assert_eq!(cfg.kit, "/kit");
        assert_eq!(cfg.setup["~/code/app"], "npm ci");
        assert_eq!((cfg.flow.plan.as_str(), cfg.flow.review.as_str(), cfg.flow.workers, cfg.flow.merge.as_str()), ("never", "code", 4, "auto"));
        assert_eq!(cfg.flow.profiles.roles()[1], ("planner", "planner-high".to_string()));
        assert_eq!(cfg.source, path.display().to_string());
    }

    #[test]
    fn bad_files_are_errors_that_name_the_file() {
        for (text, want) in [
            ("{", "agents.json"),
            (r#"{"profiles":{"x":{"argv":[]}}}"#, "profile \"x\" has no argv"),
            (r#"{"flow":{"plan":"sometimes"}}"#, "flow plan: \"sometimes\" is not one of never, when-needed, always"),
            (r#"{"flow":{"review":"all"}}"#, "flow review"),
            (r#"{"flow":{"merge":"yes"}}"#, "flow merge"),
            (r#"{"flow":{"workers":0}}"#, "flow workers: 0 is less than 1"),
            (r#"{"flow":{"debugger_after":0}}"#, "flow debugger_after: 0 is less than 1"),
        ] {
            let (_d, path) = file(text);
            let err = Config::load(&path).unwrap_err();
            assert!(err.contains(want) && err.contains("agents.json"), "{text}: {err}");
        }
    }

    #[test]
    fn the_old_format_loads_with_the_defaults_and_a_notice() {
        let (_d, path) = file(r#"{"presets":[{"name":"x"}]}"#);
        let cfg = Config::load(&path).unwrap();
        assert_eq!(cfg.notice, OLD_FORMAT_NOTICE);
        assert_eq!(cfg.profiles, Config::default().profiles);
        let (_d, path) = file(r#"{"presets":[],"profiles":{}}"#);
        assert_eq!(Config::load(&path).unwrap().notice, "");
    }

    #[test]
    fn the_flow_text_has_one_line_for_each_rule() {
        let mut f = Flow { plan: "never".into(), review: "none".into(), merge: "auto".into(), ..Flow::default() };
        f.profiles.builder = "builder-medium".into();
        assert_eq!(
            f.text(),
            "The flow:\n- Plan first: never. Spawn builder directly; the builder plans in its PR body.\n\
             - Review: none. Spawn no reviewer; read the diff yourself.\n\
             - Debugger: spawn debugger after 2 failed fix rounds.\n\
             - Workers at once: at most 4, reviewers included. Spawn and a message to a settled worker refuse past it.\n\
             - Merge: merge yourself when CI and review pass.\n\
             - Profiles: builder: builder-medium, planner: planner, reviewer: reviewer, debugger: debugger, explorer: explorer. Spawn each role with its profile only.\n"
        );
        assert!(Flow::default().text().contains("Plan first: when needed.") && Flow::default().text().contains("Merge: ask the owner each time."));
        let all = Flow { plan: "always".into(), review: "every".into(), ..Flow::default() };
        assert!(all.text().contains("Plan first: always.") && all.text().contains("Review: every PR gets a reviewer."));
    }

    #[test]
    fn the_guard_refuses_a_profile_that_the_flow_names_for_no_role() {
        let mut cfg = Config::default();
        cfg.profiles.insert("builder-medium".into(), Profile { argv: vec!["claude".into()], agent: "builder".into() });
        cfg.flow.profiles.builder = "builder-medium".into();
        assert_eq!(cfg.check_profile("builder-medium"), Ok(()));
        assert_eq!(cfg.check_profile("planner"), Ok(()));
        assert_eq!(cfg.check_profile("not-a-profile"), Ok(()));
        let err = cfg.check_profile("builder").unwrap_err();
        assert!(err.contains("no role with profile \"builder\""), "{err}");
        assert!(err.contains("allowed: builder-medium, planner, reviewer, debugger, explorer"), "{err}");
        assert!(err.contains("--any-profile"), "{err}");
        let err = cfg.check_profile("orchestrator").unwrap_err();
        assert!(err.contains("orchestrator"), "{err}");
    }

    #[test]
    fn no_profile_names_the_file_and_the_profiles() {
        let mut cfg = Config::default();
        assert_eq!(cfg.no_profile("x"), "agents: no profile \"x\"; profiles: builder, debugger, explorer, orchestrator, planner, reviewer");
        cfg.source = "/c/agents.json (not found)".into();
        assert!(cfg.no_profile("x").starts_with("agents: no profile \"x\" in /c/agents.json (not found); profiles: builder,"));
    }

    #[test]
    fn save_keeps_unknown_keys_and_does_not_freeze_the_defaults() {
        let (_d, path) = file(r#"{"other":{"a":1},"profiles":{"reviewer":{"argv":["claude","--model","opus","--effort","low","--permission-mode","auto"]}}}"#);
        let mut cfg = Config::load(&path).unwrap();
        cfg.flow.merge = "auto".into();
        cfg.flow.profiles.builder = "builder-medium".into();
        cfg.profiles.insert("builder-medium".into(), Profile { argv: vec!["claude".into(), "--effort".into(), "medium".into()], agent: "builder".into() });
        cfg.save(&path).unwrap();
        let v: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(v["other"]["a"], 1);
        let names: Vec<&String> = v["profiles"].as_object().unwrap().keys().collect();
        assert_eq!(names, ["builder-medium", "reviewer"]);
        assert_eq!(v["profiles"]["builder-medium"]["agent"], "builder");
        assert_eq!(v["flow"]["merge"], "auto");
        assert_eq!(v["flow"]["profiles"], serde_json::json!({"builder": "builder-medium"}));
        assert!(v.get("kit").is_none() && v.get("setup").is_none());
        assert!(fs::read_to_string(&path).unwrap().ends_with("}\n"));
        assert_eq!(Config::load(&path).unwrap().flow, cfg.flow);
    }

    #[test]
    fn save_to_a_new_file_writes_the_flow_only_and_refuses_a_bad_flow() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/agents.json");
        Config::default().save(&path).unwrap();
        let v: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(v.as_object().unwrap().keys().collect::<Vec<_>>(), ["flow"]);
        assert!(v["flow"].get("profiles").is_none());
        let mut bad = Config::default();
        bad.flow.workers = 0;
        assert!(bad.save(&path).unwrap_err().contains("flow workers"));
        fs::write(&path, "not json").unwrap();
        assert!(Config::default().save(&path).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "not json");
    }
}
