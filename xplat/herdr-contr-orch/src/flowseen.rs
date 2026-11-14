//! The flow pickup: a coordinator gets the flow and the models of the profiles again when the owner changes
//! them in the profiles file while the session runs. Each hc terminal has a file with the flow that it saw
//! last, in the state folder. Ported from contr's `contr hook`.
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::cli::config_path;
use crate::config::Config;
use crate::workers::{Core, Manager, ROLE_ORCHESTRATOR};

/// The first line of the context of a changed flow.
pub const CHANGED_LEAD: &str = "contr: the owner changed the flow and models in agents.json; follow these from now on.\n";

/// The state folder: CONTR_STATE_DIR; else `state` in this tool's folder of the Herdr home. None when no
/// folder can be found.
pub fn state_dir(env: &dyn Fn(&str) -> String) -> Option<PathBuf> {
    let set = env("CONTR_STATE_DIR");
    if !set.is_empty() {
        return Some(PathBuf::from(set));
    }
    crate::cli::data_dir(env).ok().map(|d| d.join("state"))
}

/// The flow text, and one line for each profile with its model and effort, in name order.
pub fn flow_state(env: &dyn Fn(&str) -> String) -> Result<String, String> {
    let cfg = Config::load(&config_path(env))?;
    let or_default = |s: String| if s.is_empty() { "default".to_string() } else { s };
    let mut text = cfg.flow.text() + "\nProfiles:\n";
    for name in cfg.profiles.keys() {
        text += &format!("- {name}: model {}, effort {}\n", or_default(cfg.model_of(name)), or_default(cfg.effort_of(name)));
    }
    Ok(text)
}

/// The file with the flow that the terminal `id` saw last. None with no state folder or no terminal.
fn seen_path(env: &dyn Fn(&str) -> String, id: &str) -> Option<PathBuf> {
    Some(state_dir(env)?.join("flow-seen").join(Path::new(id).file_name()?))
}

fn save(path: &Path, text: &str) -> Result<(), String> {
    path.parent().map_or(Ok(()), fs::create_dir_all).and_then(|_| fs::write(path, text)).map_err(|e| format!("{}: {e}", path.display()))
}

/// Saves the flow of now as seen by the terminal `id`: a session starts with the flow that it has.
pub fn record(env: &dyn Fn(&str) -> String, id: &str) -> Result<(), String> {
    match seen_path(env, id) {
        Some(p) => save(&p, &flow_state(env)?),
        None => Ok(()),
    }
}

/// The context for a coordinator when the flow or the models changed after the terminal `id` saw them last,
/// else "". With no saved flow it only saves one: there is nothing to compare. hc is asked for the role only
/// after a change, so most calls read two small files and no more.
pub fn change(env: &dyn Fn(&str) -> String, core: &dyn Core, id: &str) -> Result<String, String> {
    let Some(path) = seen_path(env, id) else { return Ok(String::new()) };
    let text = flow_state(env)?;
    match fs::read_to_string(&path) {
        Ok(seen) if seen == text => return Ok(String::new()),
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => return save(&path, &text).map(|_| String::new()),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    }
    // Save first: a role check that fails does not ask hc again on each call.
    save(&path, &text)?;
    let w = Manager::new(core, String::new()).get(id)?;
    Ok(if w.orchestrating || w.role == ROLE_ORCHESTRATOR { format!("{CHANGED_LEAD}{text}") } else { String::new() })
}
