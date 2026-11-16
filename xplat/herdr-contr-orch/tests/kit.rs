//! The kit of this repository: its hooks, its MCP server and its playbook name commands that the tool has.
mod common;

use std::fs;
use std::path::PathBuf;

use common::{call, path, Fake};
use orch::cli::AGENTS_USAGE;
use serde_json::Value;

const TOOL: &str = "herdr-contr-orch";

fn kit() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../kit")
}

fn json(file: &str) -> Value {
    serde_json::from_str(&fs::read_to_string(kit().join(file)).unwrap()).unwrap()
}

/// The arguments that a shell command gives the tool: the words after its name, to the next `||`.
fn tool_args(command: &str) -> Vec<&str> {
    let at = command.find(TOOL).unwrap_or_else(|| panic!("{command:?} does not run {TOOL}"));
    command[at + TOOL.len()..].split("||").next().unwrap().split_whitespace().collect()
}

#[test]
fn each_hook_runs_a_hook_of_the_tool() {
    let hooks = json("hooks/hooks.json");
    let mut seen = Vec::new();
    for (event, groups) in hooks["hooks"].as_object().unwrap() {
        for group in groups.as_array().unwrap() {
            for hook in group["hooks"].as_array().unwrap() {
                let args = tool_args(hook["command"].as_str().unwrap());
                // Outside hc a hook does nothing; a name that the tool does not have prints the usage.
                let r = call(&args, &[], &Fake::default());
                assert_eq!((r.code, r.err.as_str()), (0, ""), "{event}: {args:?}");
                seen.push(args.join(" "));
            }
        }
    }
    seen.sort();
    assert_eq!(seen, ["hook prompt", "hook session-start", "hook stop"]);
}

#[test]
fn the_mcp_server_is_the_tool_with_the_kit_folder() {
    // The server starts through sh, which looks for the tool next to an installed kit first.
    let server = &json(".mcp.json")["mcpServers"]["contr"];
    assert_eq!(server["command"], "sh");
    assert_eq!(server["env"]["CONTR_KIT"], "${CLAUDE_PLUGIN_ROOT}");
    let args = tool_args(server["args"][1].as_str().unwrap());
    assert_eq!(args, ["mcp"]);
    // No input: the server ends at once.
    let dir = tempfile::tempdir().unwrap();
    let (cfg, owner) = (dir.path().join("agents.json"), dir.path().join("owner.json"));
    let vars = [("CONTR_AGENTS_CONFIG", path(&cfg)), ("CONTR_OWNER_FILE", path(&owner))];
    let r = call(&args, &vars, &Fake::default());
    assert_eq!((r.code, r.err.as_str()), (0, ""));
}

#[test]
fn the_playbook_names_commands_of_the_tool() {
    let text = fs::read_to_string(kit().join("commands/orchestrate.md")).unwrap();
    assert!(text.contains("$ARGUMENTS"), "the playbook takes the goal");
    let lead = format!("`{TOOL} agents ");
    let names: Vec<&str> = text.match_indices(&lead).map(|(at, _)| text[at + lead.len()..].split(|c: char| c == '`' || c == ' ').next().unwrap()).collect();
    assert!(!names.is_empty());
    for name in names {
        assert!(name == "help" || AGENTS_USAGE.contains(&format!("{TOOL} agents {name} ")), "agents {name} is not a command");
    }
    assert!(!text.contains("`contr agents"), "the playbook names the old tool");
}

#[test]
fn each_agent_file_is_a_built_in_profile() {
    for entry in fs::read_dir(kit().join("agents")).unwrap() {
        let name = entry.unwrap().path().file_stem().unwrap().to_str().unwrap().to_string();
        assert!(AGENTS_USAGE.contains(&format!(" {name}")), "{name} is not a built-in profile");
    }
}

#[test]
fn the_report_tool_has_the_name_that_claude_code_gives_it() {
    // Claude Code names a plugin's MCP tool mcp__plugin_<plugin>_<server>__<tool>. Spawn allows this name.
    let plugin = json(".claude-plugin/plugin.json")["name"].as_str().unwrap().to_string();
    let servers = json(".mcp.json")["mcpServers"].as_object().unwrap().keys().cloned().collect::<Vec<_>>();
    assert_eq!(orch::tools::REPORT_TOOL, format!("mcp__plugin_{plugin}_{}__report", servers[0]));
    // Each agent definition tells the worker to report with the tool.
    for entry in fs::read_dir(kit().join("agents")).unwrap() {
        let text = fs::read_to_string(entry.unwrap().path()).unwrap();
        assert!(text.contains("with the contr `report` tool"), "an agent does not name the report tool");
    }
}

/// A copy of the folder `from` at `to`.
fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
    fs::create_dir_all(to).unwrap();
    for e in fs::read_dir(from).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            copy_dir(&p, &to.join(e.file_name()));
        } else {
            fs::copy(&p, to.join(e.file_name())).unwrap();
        }
    }
}

#[cfg(unix)]
#[test]
fn hooks_find_the_tool_next_to_an_installed_kit() {
    use std::os::unix::fs::PermissionsExt;
    // The layout of `install --backend`: <root>/kit and <root>/bin/herdr-contr-orch.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("r");
    copy_dir(&kit(), &root.join("kit"));
    fs::create_dir_all(root.join("bin")).unwrap();
    let calls = dir.path().join("calls");
    let tool = root.join("bin").join(TOOL);
    fs::write(&tool, format!("#!/bin/sh\necho \"$*\" >> '{}'\n", calls.display())).unwrap();
    fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();
    let hooks: Value = serde_json::from_str(&fs::read_to_string(root.join("kit/hooks/hooks.json")).unwrap()).unwrap();
    let mut seen = Vec::new();
    for groups in hooks["hooks"].as_object().unwrap().values() {
        for group in groups.as_array().unwrap() {
            for hook in group["hooks"].as_array().unwrap() {
                let command = hook["command"].as_str().unwrap();
                let r = std::process::Command::new("/bin/sh")
                    .args(["-c", command])
                    .env_clear()
                    .env("PATH", "/usr/bin:/bin")
                    .env("CLAUDE_PLUGIN_ROOT", root.join("kit"))
                    .env("HC_TERMINAL_ID", "t1")
                    .env("CONTR_COORDINATOR", "lead")
                    .output()
                    .unwrap();
                assert!(r.status.success(), "{command}: {}", String::from_utf8_lossy(&r.stderr));
                seen.push(tool_args(command).join(" "));
            }
        }
    }
    let mut ran: Vec<String> = fs::read_to_string(&calls).unwrap().lines().map(String::from).collect();
    seen.sort();
    ran.sort();
    assert_eq!(ran, seen, "each hook runs the tool in <root>/bin");
}
