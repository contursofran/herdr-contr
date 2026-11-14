//! The Herdr home and the files of herdr-contr in it. This is the only code that knows these names.
//!
//! The home is the flag value, else HERDR_APP_HOME, else the installed place (the home that a helper in
//! `<home>/herdr-contr/bin` is in), else `~/.config/herdr-app` (HOME, else USERPROFILE).
use std::path::{Path, PathBuf};

/// The name of the folder of herdr-contr in the Herdr home.
pub const FOLDER: &str = "herdr-contr";

/// The Herdr home. `env` gives the value of a variable, or an empty string. None: no home is known.
pub fn home(flag: Option<&str>, installed: Option<&Path>, env: &dyn Fn(&str) -> String) -> Option<PathBuf> {
    let given = |v: String| Some(v).filter(|v| !v.is_empty());
    flag.map(String::from)
        .and_then(given)
        .or_else(|| given(env("HERDR_APP_HOME")))
        .map(PathBuf::from)
        .or_else(|| installed.map(Path::to_path_buf))
        .or_else(|| given(env("HOME")).or_else(|| given(env("USERPROFILE"))).map(|h| Path::new(&h).join(".config").join("herdr-app")))
}

/// The home that the program `exe` is installed in, when it is `<home>/herdr-contr/bin/<program>`.
pub fn installed_home(exe: &Path) -> Option<PathBuf> {
    let bin = exe.parent()?;
    let folder = bin.parent()?;
    (bin.file_name()? == "bin" && folder.file_name()? == FOLDER).then(|| folder.parent().map(Path::to_path_buf)).flatten()
}

/// The places in the folder of herdr-contr of one Herdr home.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Places {
    pub home: PathBuf,
}

impl Places {
    pub fn new(home: impl Into<PathBuf>) -> Self {
        Places { home: home.into() }
    }
    /// `<home>/herdr-contr`.
    pub fn folder(&self) -> PathBuf {
        self.home.join(FOLDER)
    }
    /// The profiles file of the orchestration tool.
    pub fn agents(&self) -> PathBuf {
        self.folder().join("agents.json")
    }
    /// The owner file of the orchestration tool.
    pub fn owner(&self) -> PathBuf {
        self.folder().join("owner.json")
    }
    /// The state folder of the orchestration tool.
    pub fn state(&self) -> PathBuf {
        self.folder().join("state")
    }
    pub fn bin(&self) -> PathBuf {
        self.folder().join("bin")
    }
    pub fn feed(&self) -> PathBuf {
        self.bin().join(exe_name("herdr-contr-feed"))
    }
    pub fn orch(&self) -> PathBuf {
        self.bin().join(exe_name("herdr-contr-orch"))
    }
    /// The Claude Code plugin of a coordinator and its workers.
    pub fn kit(&self) -> PathBuf {
        self.folder().join("kit")
    }
    pub fn marketplace(&self) -> PathBuf {
        self.folder().join(".claude-plugin").join("marketplace.json")
    }
    /// The record of the last install.
    pub fn install_record(&self) -> PathBuf {
        self.folder().join("install.json")
    }
}

fn exe_name(name: &str) -> String {
    if cfg!(windows) { format!("{name}.exe") } else { name.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_home_and_its_places() {
        let inst = Path::new("/opt/h");
        // (flag, installed, HERDR_APP_HOME, HOME, USERPROFILE, the home)
        let rows: [(Option<&str>, Option<&Path>, &str, &str, &str, Option<&str>); 9] = [
            (Some("/flag"), Some(inst), "/env", "/u", "/w", Some("/flag")),
            (Some(""), Some(inst), "/env", "/u", "/w", Some("/env")),
            (None, Some(inst), "/env", "/u", "/w", Some("/env")),
            (None, Some(inst), "", "/u", "/w", Some("/opt/h")),
            (None, None, "", "/u", "/w", Some("/u/.config/herdr-app")),
            (None, None, "", "", "/w", Some("/w/.config/herdr-app")),
            (None, None, "", "", "", None),
            (Some(""), None, "", "", "", None),
            (Some("/flag"), None, "", "", "", Some("/flag")),
        ];
        for (flag, installed, app, user, profile, want) in rows {
            let env = |k: &str| match k { "HERDR_APP_HOME" => app, "HOME" => user, "USERPROFILE" => profile, _ => "" }.to_string();
            assert_eq!(home(flag, installed, &env), want.map(PathBuf::from), "{flag:?} {installed:?} {app} {user} {profile}");
        }
        assert_eq!(installed_home(Path::new("/opt/h/herdr-contr/bin/herdr-contr-feed")), Some(PathBuf::from("/opt/h")));
        assert_eq!(installed_home(Path::new("/opt/h/other/bin/herdr-contr-feed")), None);
        assert_eq!(installed_home(Path::new("/usr/local/bin/herdr-contr-feed")), None);
        let p = Places::new("/h");
        let x = if cfg!(windows) { ".exe" } else { "" };
        for (got, want) in [
            (p.folder(), "herdr-contr".to_string()),
            (p.agents(), "herdr-contr/agents.json".into()),
            (p.owner(), "herdr-contr/owner.json".into()),
            (p.state(), "herdr-contr/state".into()),
            (p.bin(), "herdr-contr/bin".into()),
            (p.feed(), format!("herdr-contr/bin/herdr-contr-feed{x}")),
            (p.orch(), format!("herdr-contr/bin/herdr-contr-orch{x}")),
            (p.kit(), "herdr-contr/kit".into()),
            (p.marketplace(), "herdr-contr/.claude-plugin/marketplace.json".into()),
            (p.install_record(), "herdr-contr/install.json".into()),
        ] {
            let want: PathBuf = want.split('/').fold(PathBuf::from("/h"), |a, s| a.join(s));
            assert_eq!(got, want);
        }
    }
}
