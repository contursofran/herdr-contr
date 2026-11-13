//! `agents spawn` on a fake hc core. Git runs in temporary folders only; no test starts an agent.
mod common;

use std::fs;
use std::path::Path;
use std::process::Command;

use common::{call, path, Fake};
use orch::spawn::{review_worktree, CLEAN_SCRIPT, SETUP_WRAPPER};
use orch::workers::program;
use serde_json::{json, Value};

fn run_reply(name: &str) -> Fake {
    Fake::default().reply("terminal.run", json!({"address": "t4", "name": name, "state": "live", "labels": {"contr": "agent", "role": "builder"}}))
}

fn argv(params: &Value) -> Vec<String> {
    params["argv"].as_array().unwrap().iter().map(|a| a.as_str().unwrap().to_string()).collect()
}

/// A profiles file in `dir` with this content, as CONTR_AGENTS_CONFIG.
fn config(dir: &Path, content: Value) -> String {
    let file = dir.join("agents.json");
    fs::write(&file, content.to_string()).unwrap();
    path(&file).to_string()
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git").arg("-C").arg(dir).args(["-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false"]).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// A repository with one commit, in a new folder of `tmp`. The path has no symbolic link.
fn repo(tmp: &Path, name: &str) -> std::path::PathBuf {
    let dir = fs::canonicalize(tmp).unwrap().join(name);
    fs::create_dir(&dir).unwrap();
    git(&dir, &["init", "--quiet"]);
    git(&dir, &["commit", "--quiet", "--allow-empty", "-m", "first"]);
    dir
}

#[test]
fn spawn_starts_the_profile_with_its_labels_and_parent() {
    let tmp = tempfile::tempdir().unwrap();
    let wt = path(tmp.path());
    let cfg = config(tmp.path(), json!({}));
    let core = run_reply("b-158");
    let vars = [("HC_TERMINAL_ID", "t1"), ("CONTR_AGENTS_CONFIG", cfg.as_str()), ("PATH", "/usr/bin:/bin:/usr/bin"), ("HOME", "/home/o")];
    let r = call(&["agents", "spawn", "--name", "b-158", "--profile", "builder", "--issue", "158", "--worktree", wt, "read", "TASK.md"], &vars, &core);
    assert_eq!((r.code, r.out.as_str(), r.err.as_str()), (0, "b-158 t4\n", ""));
    let (method, p) = core.calls()[0].clone();
    assert_eq!(method, "terminal.run");
    let full = argv(&p);
    // A Claude worker starts through the clean wrapper, with the task as the last word.
    assert_eq!(full[..4], ["/bin/sh", "-c", CLEAN_SCRIPT, "contr-clean-env"]);
    assert_eq!(program(&full)[0], "claude");
    assert_eq!(full.last().unwrap(), "read TASK.md");
    assert_eq!(p["labels"], json!({"contr": "agent", "role": "builder", "profile": "builder", "worktree": wt, "issue": "158"}));
    assert_eq!(p["env"], json!({"CONTR_COORDINATOR": "t1", "CONTR_PATH": "/usr/bin:/bin:/home/o/.local/bin:/opt/homebrew/bin:/usr/local/bin"}));
    assert_eq!((&p["name"], &p["parent"], &p["cwd"], &p["wait"], &p["outlive_parent"]), (&json!("b-158"), &json!("t1"), &json!(wt), &json!(false), &json!(true)));
    assert!(p.get("no_parent").is_none() && p.get("display").is_none());

    // --role and --project are labels; without hc there is no parent and no variable for it.
    let core = run_reply("o");
    let r = call(&["agents", "spawn", "--name", "o", "--profile", "builder", "--role", "orchestrator", "--project", "contr"], &[("CONTR_AGENTS_CONFIG", &cfg)], &core);
    assert_eq!(r.code, 0, "{}", r.err);
    let p = &core.calls()[0].1;
    assert_eq!(p["labels"], json!({"contr": "agent", "role": "orchestrator", "profile": "builder", "worktree": "", "project": "contr"}));
    assert_eq!((&p["no_parent"], p.get("env"), p.get("cwd"), p.get("parent")), (&json!(true), None, None, None));

    let core = run_reply("x");
    let r = call(&["agents", "spawn", "--no-parent", "--name", "x", "--profile", "builder"], &vars, &core);
    assert_eq!((r.code, &core.calls()[0].1["no_parent"]), (0, &json!(true)));
    assert!(core.calls()[0].1["env"].get("CONTR_COORDINATOR").is_none());
}

#[test]
fn spawn_takes_flags_after_the_task_and_words_after_a_double_dash() {
    let tmp = tempfile::tempdir().unwrap();
    let vars = [("CONTR_AGENTS_CONFIG", &*config(tmp.path(), json!({})))];
    let core = run_reply("b");
    let r = call(&["agents", "spawn", "--name", "b", "fix", "it", "--profile", "builder", "--json"], &vars, &core);
    assert_eq!(r.code, 0, "{}", r.err);
    assert!(r.out.contains("\"address\": \"t4\""), "{}", r.out);
    assert_eq!(argv(&core.calls()[0].1).last().unwrap(), "fix it");

    let core = run_reply("b");
    assert_eq!(call(&["agents", "spawn", "--name", "b", "--profile", "builder", "--", "run", "--all", "tests"], &vars, &core).code, 0);
    assert_eq!(argv(&core.calls()[0].1).last().unwrap(), "run --all tests");
}

#[test]
fn spawn_refuses_a_bad_profile_name_project_or_worktree() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = config(tmp.path(), json!({}));
    let vars = [("CONTR_AGENTS_CONFIG", cfg.as_str())];
    let core = run_reply("x");
    let fails = |args: &[&str], want: &str| {
        let r = call(&[&["agents", "spawn"], args].concat(), &vars, &core);
        assert_eq!(r.code, 1, "{args:?}");
        assert!(r.err.contains(want), "{args:?}: {}", r.err);
    };
    fails(&["--name", "x", "--profile", "nope"], "no profile \"nope\" in");
    // orchestrator is a profile, but the flow names it for no worker role.
    fails(&["--name", "x", "--profile", "orchestrator"], "allowed: builder, planner, reviewer, debugger, explorer");
    fails(&["--profile", "builder"], "a worker needs a name");
    fails(&["--name", "x", "--profile", "builder", "--project", "a\u{7}b"], "control character");
    fails(&["--name", "x", "--profile", "builder", "--worktree", &format!("{}/missing", path(tmp.path()))], "agents: worktree ");
    fails(&["--name", "x", "--profile", "builder", "--worktree", &cfg], "is not a folder");
    assert!(core.calls().is_empty());

    assert_eq!(call(&["agents", "spawn", "--name", "x", "--profile", "orchestrator", "--any-profile"], &vars, &core).code, 0);
    let r = call(&["agents", "spawn", "--name", "x", "--profile", "builder"], &vars, &Fake::default().err("terminal.run", "hc: name in use (conflict)"));
    assert_eq!((r.code, r.err.as_str()), (1, "herdr-contr-orch agents spawn: hc: name in use (conflict)\n"));

    let r = call(&["agents", "spawn", "--name", "x", "--keep"], &vars, &core);
    assert_eq!(r.code, 2);
    assert!(r.err.starts_with("herdr-contr-orch agents spawn: flag --keep does not apply\n"), "{}", r.err);
    let r = call(&["agents", "spawn", "--name"], &vars, &core);
    assert_eq!((r.code, r.err.as_str()), (2, "herdr-contr-orch agents spawn: flag needs an argument: -name\n"));
    let r = call(&["agents", "spawn", "--review-pr", "x"], &vars, &core);
    assert_eq!((r.code, r.err.as_str()), (2, "herdr-contr-orch agents spawn: --review-pr \"x\" is not a number\n"));
}

#[test]
fn a_broken_profiles_file_blocks_only_spawn() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("agents.json");
    fs::write(&file, "{").unwrap();
    let vars = [("CONTR_AGENTS_CONFIG", path(&file))];
    let core = run_reply("x");
    let r = call(&["agents", "spawn", "--name", "x", "--profile", "builder"], &vars, &core);
    assert_eq!(r.code, 1);
    assert!(r.err.starts_with("herdr-contr-orch agents: agents: "), "{}", r.err);
    assert!(core.calls().is_empty());
    assert_eq!(call(&["agents", "ls"], &vars, &core).code, 0);
}

#[test]
fn spawn_reads_the_task_from_a_file() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = config(tmp.path(), json!({}));
    let vars = [("CONTR_AGENTS_CONFIG", cfg.as_str())];
    let brief = tmp.path().join("brief.md");
    fs::write(&brief, "Fix \"it\"\nthen $HOME\n").unwrap();
    let core = run_reply("b");
    let r = call(&["agents", "spawn", "--name", "b", "--profile", "builder", "--task-file", path(&brief)], &vars, &core);
    assert_eq!(r.code, 0, "{}", r.err);
    assert_eq!(argv(&core.calls()[0].1).last().unwrap(), "Fix \"it\"\nthen $HOME\n");

    let core = run_reply("b");
    let r = call(&["agents", "spawn", "--name", "b", "--profile", "builder", "--task-file", path(&brief), "and", "this"], &vars, &core);
    assert_eq!((r.code, r.err.as_str()), (1, "herdr-contr-orch agents spawn: give the task as text or with a task file, not both\n"));
    fs::write(&brief, " \n").unwrap();
    let r = call(&["agents", "spawn", "--name", "b", "--profile", "builder", "--task-file", path(&brief)], &vars, &core);
    assert!(r.code == 1 && r.err.ends_with("brief.md is empty\n"), "{}", r.err);
    let r = call(&["agents", "spawn", "--name", "b", "--profile", "builder", "--task-file", "/no/such/brief.md"], &vars, &core);
    assert!(r.code == 1 && r.err.contains("task file: /no/such/brief.md: "), "{}", r.err);
    assert!(core.calls().is_empty());
}

#[test]
fn a_claude_profile_loads_the_kit_and_its_agent() {
    let tmp = tempfile::tempdir().unwrap();
    let kit = tmp.path().join("kit");
    fs::create_dir_all(kit.join("agents")).unwrap();
    fs::write(kit.join("agents/planner.md"), "---\n").unwrap();
    let cfg = config(
        tmp.path(),
        json!({"kit": "/file/kit", "profiles": {
            "planner-high": {"argv": ["claude", "--effort", "high"], "agent": "planner"},
            "own": {"argv": ["/opt/bin/claude", "--plugin-dir=/mine"]},
            "pi": {"argv": ["pi", "--fast"]}}}),
    );
    let head = |vars: &[(&str, &str)], profile: &str| {
        let core = run_reply("w");
        let r = call(&["agents", "spawn", "--name", "w", "--profile", profile, "--any-profile", "go"], vars, &core);
        assert_eq!(r.code, 0, "{}", r.err);
        let (full, env) = (argv(&core.calls()[0].1), core.calls()[0].1.get("env").cloned());
        (program(&full).to_vec(), full.len(), env)
    };
    // CONTR_KIT replaces the kit of the file. The agent of the profile picks the kit's definition.
    let vars = [("CONTR_AGENTS_CONFIG", cfg.as_str()), ("CONTR_KIT", path(&kit))];
    assert_eq!(head(&vars, "planner-high").0, ["claude", "--plugin-dir", path(&kit), "--agent", "contr:planner", "--effort", "high", "go"]);
    // No definition for the builder in this kit, and a --plugin-dir of the profile stays.
    assert_eq!(head(&vars, "builder").0[..3], ["claude", "--plugin-dir", path(&kit)]);
    assert!(!head(&vars, "builder").0.contains(&"--agent".to_string()));
    assert_eq!(head(&vars, "own").0, ["/opt/bin/claude", "--plugin-dir=/mine", "go"]);
    assert_eq!(head(&[("CONTR_AGENTS_CONFIG", &cfg)], "builder").0[..3], ["claude", "--plugin-dir", "/file/kit"]);
    // A program that is not claude gets no kit, no clean wrapper and no path.
    let (prog, len, env) = head(&vars, "pi");
    assert_eq!((prog.as_slice(), len, env), (&["pi".to_string(), "--fast".into(), "go".into()][..], 3, None));
}

#[test]
fn spawn_runs_the_project_setup_in_a_new_worktree_only() {
    let tmp = tempfile::tempdir().unwrap();
    let main = repo(tmp.path(), "main");
    let other = repo(tmp.path(), "other");
    let wt = main.with_file_name("main-wt").join("b-1");
    git(&main, &["worktree", "add", "--quiet", "-b", "b-1", path(&wt)]);
    let setup = "cp -R \"$CONTR_MAIN/.husky\" .";
    let cfg = config(tmp.path(), json!({"setup": {path(&main): setup}}));
    let vars = [("CONTR_AGENTS_CONFIG", cfg.as_str()), ("HC_TERMINAL_ID", "t0")];

    let core = run_reply("w");
    let r = call(&["agents", "spawn", "--name", "w", "--profile", "builder", "--worktree", path(&wt), "go"], &vars, &core);
    assert_eq!(r.code, 0, "{}", r.err);
    let p = &core.calls()[0].1;
    let full = argv(p);
    assert_eq!(full[..5], ["/bin/sh", "-c", SETUP_WRAPPER, "contr-setup", setup]);
    assert_eq!((full[8].as_str(), program(&full)[0].as_str(), full.last().unwrap().as_str()), ("contr-clean-env", "claude", "go"));
    assert_eq!(p["env"], json!({"CONTR_MAIN": path(&main), "CONTR_COORDINATOR": "t0"}));

    // The main checkout and a repository with no setup start the agent directly.
    for dir in [&main, &other] {
        let core = run_reply("w");
        assert_eq!(call(&["agents", "spawn", "--name", "w", "--profile", "builder", "--worktree", path(dir)], &vars, &core).code, 0);
        let p = &core.calls()[0].1;
        assert_eq!((argv(p)[3].as_str(), &p["env"]), ("contr-clean-env", &json!({"CONTR_COORDINATOR": "t0"})));
    }
}

fn sh(script: &str, name: &str, args: &[&str], vars: &[(&str, &str)], dir: &Path) -> (i32, String, String) {
    let out = Command::new("/bin/sh").args(["-c", script, name]).args(args).envs(vars.iter().copied()).current_dir(dir).output().unwrap();
    (out.status.code().unwrap(), String::from_utf8(out.stdout).unwrap(), String::from_utf8(out.stderr).unwrap())
}

#[test]
fn the_setup_wrapper_runs_the_setup_then_the_agent() {
    let tmp = tempfile::tempdir().unwrap();
    let (code, out, _) = sh(SETUP_WRAPPER, "contr-setup", &["echo made > made.txt", "/bin/sh", "-c", "cat made.txt; echo \"$1\"", "agent", "a b"], &[], tmp.path());
    assert_eq!((code, out.as_str()), (0, "made\na b\n"));
    // A failed setup ends with its exit code and does not start the agent.
    let (code, out, err) = sh(SETUP_WRAPPER, "contr-setup", &["exit 7", "/bin/echo", "started"], &[], tmp.path());
    assert_eq!((code, out.as_str(), err.as_str()), (7, "", "contr: worktree setup failed (exit 7)\n"));
}

#[test]
fn the_clean_wrapper_removes_the_session_variables_and_adds_the_path() {
    let tmp = tempfile::tempdir().unwrap();
    let vars = [("CLAUDECODE", "1"), ("CLAUDE_CODE_ENTRYPOINT", "cli"), ("CLAUDE_CONFIG_DIR", "/keep"), ("PATH", "/usr/bin:/bin"), ("CONTR_PATH", "/extra/bin")];
    let show = "echo \"$CLAUDECODE|$CLAUDE_CODE_ENTRYPOINT|$CLAUDE_CONFIG_DIR|$PATH|$CONTR_PATH|$1\"";
    let (code, out, _) = sh(CLEAN_SCRIPT, "contr-clean-env", &["/bin/sh", "-c", show, "agent", "a b"], &vars, tmp.path());
    assert_eq!((code, out.as_str()), (0, "||/keep|/usr/bin:/bin:/extra/bin||a b\n"));
}

#[test]
fn review_worktree_is_at_the_head_of_the_pull_request() {
    let tmp = tempfile::tempdir().unwrap();
    let origin = repo(tmp.path(), "origin");
    let head = |msg: &str| {
        git(&origin, &["commit", "--quiet", "--allow-empty", "-m", msg]);
        git(&origin, &["update-ref", "refs/pull/7/head", "HEAD"]);
        git(&origin, &["rev-parse", "HEAD"])
    };
    let sha = head("pr");
    let main = origin.with_file_name("clone");
    git(tmp.path(), &["clone", "--quiet", path(&origin), path(&main)]);

    let dir = review_worktree(path(&main), 7).unwrap();
    assert_eq!(dir, format!("{}-wt/review-7", path(&main)));
    assert_eq!(git(Path::new(&dir), &["rev-parse", "HEAD"]), sha);
    // A second review of the same pull request moves the worktree to the new head.
    let sha = head("fix");
    assert_eq!(review_worktree(&dir, 7).unwrap(), dir);
    assert_eq!(git(Path::new(&dir), &["rev-parse", "HEAD"]), sha);

    assert_eq!(review_worktree(path(&main), 8).unwrap_err(), "agents: origin has no pull request 8");
    assert_eq!(review_worktree(path(&main), 0).unwrap_err(), "agents: a pull request number is positive");

    // --review-pr starts the worker in that worktree; --worktree names the repository.
    let cfg = config(tmp.path(), json!({}));
    let core = run_reply("r-7");
    let r = call(&["agents", "spawn", "--name", "r-7", "--profile", "reviewer", "--review-pr", "7", "--worktree", path(&main)], &[("CONTR_AGENTS_CONFIG", &cfg)], &core);
    assert_eq!(r.code, 0, "{}", r.err);
    assert_eq!((&core.calls()[0].1["cwd"], &core.calls()[0].1["labels"]["worktree"]), (&json!(dir), &json!(dir)));

    // Outside a repository git's own message comes back, and no worker starts.
    let outside = tempfile::tempdir().unwrap();
    let core = run_reply("r");
    let r = call(&["agents", "spawn", "--name", "r", "--profile", "reviewer", "--review-pr", "3", "--worktree", path(outside.path())], &[("CONTR_AGENTS_CONFIG", &cfg)], &core);
    assert!(r.code == 1 && r.err.contains("git rev-parse: fatal: not a git repository"), "{}", r.err);
    assert!(core.calls().is_empty());
}
