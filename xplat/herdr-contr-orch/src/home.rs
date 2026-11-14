//! The files of this tool in the Herdr home, and the variables that name other files. The names of the
//! places are in `herdr_core::places`.
use std::path::PathBuf;

use herdr_core::places::{self, Places};

use crate::owner::Store;

/// The places of this tool in the Herdr home (HERDR_APP_HOME, else ~/.config/herdr-app).
pub fn places(env: &dyn Fn(&str) -> String) -> Result<Places, String> {
    places::home(None, None, env).map(Places::new).ok_or_else(|| "neither $HERDR_APP_HOME nor $HOME are defined".to_string())
}

/// The profiles file: CONTR_AGENTS_CONFIG, or agents.json in this tool's folder of the Herdr home.
pub fn config_path(env: &dyn Fn(&str) -> String) -> PathBuf {
    let p = env("CONTR_AGENTS_CONFIG");
    if !p.is_empty() {
        return PathBuf::from(p);
    }
    places(env).map(|p| p.agents()).unwrap_or_default()
}

/// The owner file: CONTR_OWNER_FILE, or owner.json in this tool's folder of the Herdr home.
pub fn owner_store(env: &dyn Fn(&str) -> String) -> Result<Store, String> {
    let p = env("CONTR_OWNER_FILE");
    if !p.is_empty() {
        return Ok(Store::new(p));
    }
    Ok(Store::new(places(env)?.owner()))
}
