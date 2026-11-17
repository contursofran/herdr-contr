//! `update check` and `update install` (#219): the newest release build of this repository, and its install.
//! A release is tagged `build-<date>-<sha7>` and has one asset `herdr-contr-<tag>.zip` (.github/workflows/release.yml).
//! The releases come from a `Source`: `gh` now (the repository is private and gh has the owner's login). A source
//! that reads the public release URLs over HTTPS can take its place when the repository is public.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The repository whose releases count: HERDR_CONTR_REPO at run time, else the one baked in at build time.
pub const REPO: &str = env!("HERDR_CONTR_REPO");
/// A check is kept this long in update.json; `update check --force` asks again.
pub const DAY_S: u64 = 24 * 3600;

pub type Fail = (&'static str, String);

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub tag: String,
    pub url: String,
    /// The zip's "sha256:<hex>" when the source gives it.
    pub digest: Option<String>,
}

pub trait Source {
    fn newest(&self) -> Result<Release, Fail>;
    /// Puts the release's zip in `dir` and gives its path.
    fn download(&self, rel: &Release, dir: &Path) -> Result<PathBuf, Fail>;
}

pub fn zip_name(tag: &str) -> String {
    format!("herdr-contr-{tag}.zip")
}

/// `gh` (HERDR_CONTR_GH, else `gh` on PATH) for the repository `repo`.
pub struct Gh {
    pub program: String,
    pub repo: String,
}

impl Gh {
    pub fn from_process() -> Gh {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        Gh { program: var("HERDR_CONTR_GH").unwrap_or_else(|| "gh".into()), repo: var("HERDR_CONTR_REPO").unwrap_or_else(|| REPO.into()) }
    }
    fn run(&self, args: &[&str]) -> Result<String, Fail> {
        if self.repo.is_empty() {
            return Err(("no-repo", "This helper does not know its GitHub repository (set HERDR_CONTR_REPO).".into()));
        }
        let o = Command::new(&self.program).args(args).args(["--repo", &self.repo]).output().map_err(|e| ("no-gh", format!("gh could not start ({e}). Install the GitHub CLI and log in.")))?;
        if !o.status.success() {
            return Err(("gh", format!("gh failed: {}", String::from_utf8_lossy(&o.stderr).trim())));
        }
        Ok(String::from_utf8_lossy(&o.stdout).into_owned())
    }
}

impl Source for Gh {
    fn newest(&self) -> Result<Release, Fail> {
        let text = self.run(&["release", "view", "--json", "tagName,url,assets"])?;
        let v: Value = serde_json::from_str(&text).map_err(|e| ("gh", format!("gh gave no release JSON: {e}")))?;
        let tag = v["tagName"].as_str().unwrap_or_default().to_string();
        if stamp_of(&tag).is_none() {
            return Err(("no-build", format!("The newest release ({tag}) is not a build- release.")));
        }
        let name = zip_name(&tag);
        let asset = v["assets"].as_array().and_then(|a| a.iter().find(|x| x["name"] == name.as_str()));
        let Some(asset) = asset else { return Err(("no-build", format!("The release {tag} has no {name}."))) };
        Ok(Release { url: v["url"].as_str().unwrap_or_default().into(), digest: asset["digest"].as_str().map(String::from), tag })
    }
    fn download(&self, rel: &Release, dir: &Path) -> Result<PathBuf, Fail> {
        let name = zip_name(&rel.tag);
        self.run(&["release", "download", &rel.tag, "--pattern", &name, "--dir", &dir.to_string_lossy()])?;
        Ok(dir.join(name))
    }
}

/// "<sha7> <date>", the build stamp, from a tag `build-YYYY-MM-DD-<sha7>`.
pub fn stamp_of(tag: &str) -> Option<String> {
    let rest = tag.strip_prefix("build-")?;
    let (date, sha) = rest.rsplit_once('-')?;
    let ok = date.len() == 10 && sha.len() == 7 && sha.chars().all(|c| c.is_ascii_hexdigit());
    ok.then(|| format!("{sha} {date}"))
}

/// The installed build: install.json's "build" in the herdr-contr folder, else this helper's own.
pub fn installed(folder: &Path) -> String {
    std::fs::read(folder.join("install.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| v["build"].as_str().map(String::from))
        .unwrap_or_else(|| crate::protocol::BUILD.into())
}

fn same(a: &str, b: &str) -> bool {
    a.split(' ').next() == b.split(' ').next()
}

/// The `update` line: the installed build, the newest, and whether they differ. The newest comes from
/// update.json when it is less than a day old and not `force`.
pub fn check(src: &dyn Source, folder: &Path, now_s: u64, force: bool) -> Result<Value, Fail> {
    let cache = folder.join("update.json");
    let kept: Option<Value> = std::fs::read(&cache).ok().and_then(|b| serde_json::from_slice(&b).ok());
    let fresh = kept.as_ref().filter(|k| !force && now_s.saturating_sub(k["checked_at"].as_u64().unwrap_or(0)) < DAY_S);
    let (tag, url, at) = match fresh {
        Some(k) => (k["tag"].as_str().unwrap_or_default().to_string(), k["url"].as_str().unwrap_or_default().to_string(), k["checked_at"].as_u64().unwrap_or(0)),
        None => {
            let r = src.newest()?;
            let _ = std::fs::write(&cache, json!({"checked_at": now_s, "tag": r.tag, "url": r.url}).to_string());
            (r.tag, r.url, now_s)
        }
    };
    let installed = installed(folder);
    let newest = stamp_of(&tag).unwrap_or_default();
    Ok(json!({"type": "update", "installed": installed, "newest": newest, "tag": tag, "url": url, "differs": !newest.is_empty() && !same(&installed, &newest), "checked_at": at}))
}

fn sha_file(p: &Path) -> Result<String, Fail> {
    let b = std::fs::read(p).map_err(|e| ("check", format!("cannot read {}: {e}", p.display())))?;
    Ok(Sha256::digest(&b).iter().map(|b| format!("{b:02x}")).collect())
}

/// The release target of this machine, as in the zip's remote/ folder.
pub fn own_target() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", _) => "universal-apple-darwin",
        ("linux", "aarch64") => "aarch64-unknown-linux-musl",
        _ => "x86_64-unknown-linux-musl",
    }
}

/// Checks the unzipped folder: every line of remote/SHA256SUMS, and that the helper for this machine is the one
/// the sums name. Gives that helper, and the orchestration tool when the zip has one (macOS).
pub fn checked(top: &Path) -> Result<(PathBuf, Option<PathBuf>), Fail> {
    let sums = std::fs::read_to_string(top.join("remote/SHA256SUMS")).map_err(|e| ("check", format!("the zip has no remote/SHA256SUMS ({e})")))?;
    let mut mine = None;
    for line in sums.lines().filter(|l| !l.trim().is_empty()) {
        let (want, rel) = line.split_once(char::is_whitespace).map(|(a, b)| (a, b.trim().trim_start_matches('*'))).ok_or(("check", format!("bad line in SHA256SUMS: {line}")))?;
        if rel.contains("..") || sha_file(&top.join("remote").join(rel))? != want {
            return Err(("check", format!("remote/{rel} does not match SHA256SUMS. Nothing was installed.")));
        }
        if rel.starts_with(&format!("{}/", own_target())) {
            mine = Some(want.to_string());
        }
    }
    let Some(want) = mine else { return Err(("check", format!("SHA256SUMS names no helper for {}.", own_target()))) };
    let mac = own_target() == "universal-apple-darwin";
    let helper = if mac { top.join("bin/herdr-contr-feed") } else { top.join("remote").join(own_target()).join("herdr-contr-feed") };
    if sha_file(&helper)? != want {
        return Err(("check", "The helper in the zip does not match SHA256SUMS. Nothing was installed.".into()));
    }
    let orch = top.join("bin/herdr-contr-orch");
    Ok((helper, (mac && orch.is_file()).then_some(orch)))
}

/// Downloads the newest release to a new folder in `tmp`, checks it, runs its install with --dry-run, then
/// without. Gives the `update-plan` and `update-done` lines. Stops at the first error; the folder goes after.
pub fn install(src: &dyn Source, home: &Path, tmp: &Path) -> Result<Vec<Value>, Fail> {
    let rel = src.newest()?;
    let dir = tmp.join(format!("herdr-contr-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| ("write", format!("cannot make {}: {e}", dir.display())))?;
    let out = install_in(src, &rel, home, &dir);
    let _ = std::fs::remove_dir_all(&dir);
    out
}

fn install_in(src: &dyn Source, rel: &Release, home: &Path, dir: &Path) -> Result<Vec<Value>, Fail> {
    let zip = src.download(rel, dir)?;
    if let Some(d) = rel.digest.as_deref().and_then(|d| d.strip_prefix("sha256:")) {
        if sha_file(&zip)? != d {
            return Err(("check", "The downloaded zip does not match the release's digest. Nothing was installed.".into()));
        }
    }
    let x = dir.join("x");
    let o = Command::new("unzip").arg("-q").arg(&zip).arg("-d").arg(&x).output().map_err(|e| ("unzip", format!("unzip could not start ({e})")))?;
    if !o.status.success() {
        return Err(("unzip", format!("cannot unzip the release: {}", String::from_utf8_lossy(&o.stderr).trim())));
    }
    let tops: Vec<PathBuf> = std::fs::read_dir(&x).map_err(|e| ("unzip", e.to_string()))?.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir()).collect();
    let [top] = tops.as_slice() else { return Err(("check", "The zip does not hold one folder.".into())) };
    let (helper, orch) = checked(top)?;
    let run = |dry: bool| -> Result<String, Fail> {
        let mut c = Command::new(&helper);
        c.arg("install");
        // Without the Herdr app mod in this build, the update installs only the backend.
        if !cfg!(feature = "herdr-mod") {
            c.arg("--backend");
        }
        c.arg("--home").arg(home).arg("--helper").arg(&helper).arg("--remote").arg(top.join("remote"));
        if let Some(o) = &orch {
            c.arg("--orch").arg(o);
        }
        if dry {
            c.arg("--dry-run");
        }
        let o = c.output().map_err(|e| ("install", format!("the new helper could not start ({e})")))?;
        let text = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)).trim().to_string();
        if o.status.success() { Ok(text) } else { Err(("install", text)) }
    };
    let plan = run(true)?;
    let done = run(false)?;
    let newest = stamp_of(&rel.tag).unwrap_or_default();
    Ok(vec![json!({"type": "update-plan", "text": plan}), json!({"type": "update-done", "installed": newest, "tag": rel.tag, "text": done})])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::os::unix::fs::PermissionsExt;

    const TAG: &str = "build-2026-11-16-abc1234";

    /// A fake source: the newest release, and a zip it copies on download. Counts the asks.
    struct Fake {
        rel: Release,
        zip: Option<PathBuf>,
        asks: Cell<u32>,
    }
    impl Source for Fake {
        fn newest(&self) -> Result<Release, Fail> {
            self.asks.set(self.asks.get() + 1);
            Ok(self.rel.clone())
        }
        fn download(&self, rel: &Release, dir: &Path) -> Result<PathBuf, Fail> {
            let to = dir.join(zip_name(&rel.tag));
            std::fs::copy(self.zip.as_ref().unwrap(), &to).unwrap();
            Ok(to)
        }
    }
    fn fake(zip: Option<PathBuf>, digest: Option<String>) -> Fake {
        Fake { rel: Release { tag: TAG.into(), url: "https://example.com/r".into(), digest }, zip, asks: Cell::new(0) }
    }
    fn script(p: &Path, body: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    fn hex(b: &[u8]) -> String {
        Sha256::digest(b).iter().map(|b| format!("{b:02x}")).collect()
    }

    /// A release zip as the workflow makes it; its helper logs its arguments to `log`. `bad`: a wrong sum.
    fn release_zip(t: &Path, log: &Path, bad: bool) -> PathBuf {
        let top = t.join("src").join(format!("herdr-contr-{TAG}"));
        let body = format!("echo \"$@\" >> '{}'; echo planned", log.display());
        script(&top.join("bin/herdr-contr-feed"), &body);
        script(&top.join("bin/herdr-contr-orch"), "true");
        let helper = std::fs::read(top.join("bin/herdr-contr-feed")).unwrap();
        let mut sums = String::new();
        for target in ["universal-apple-darwin", "x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl"] {
            script(&top.join("remote").join(target).join("herdr-contr-feed"), &body);
            let sum = if bad { "0".repeat(64) } else { hex(&helper) };
            sums += &format!("{sum}  {target}/herdr-contr-feed\n");
        }
        std::fs::write(top.join("remote/SHA256SUMS"), sums).unwrap();
        let zip = t.join("r.zip");
        let ok = Command::new("zip").arg("-qry").arg(&zip).arg(top.file_name().unwrap()).current_dir(t.join("src")).status().unwrap();
        assert!(ok.success());
        zip
    }

    #[test]
    fn stamps_from_tags() {
        assert_eq!(stamp_of(TAG).as_deref(), Some("abc1234 2026-11-16"));
        assert_eq!(stamp_of("build-2026-11-16-abc123"), None);
        assert_eq!(stamp_of("v0.8.0"), None);
        assert_eq!(stamp_of("build-2026-11-16-zzzzzzz"), None);
    }

    #[test]
    fn check_compares_and_keeps_a_day() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("install.json"), r#"{"build":"1111111 2026-11-10","files":{}}"#).unwrap();
        let f = fake(None, None);
        let l = check(&f, t.path(), 1000, false).unwrap();
        assert_eq!((l["installed"].as_str(), l["newest"].as_str(), l["differs"].as_bool()), (Some("1111111 2026-11-10"), Some("abc1234 2026-11-16"), Some(true)));
        check(&f, t.path(), 1000 + DAY_S - 1, false).unwrap();
        assert_eq!(f.asks.get(), 1, "a check less than a day old is kept");
        check(&f, t.path(), 1000 + DAY_S, false).unwrap();
        assert_eq!(f.asks.get(), 2, "a day later it asks again");
        check(&f, t.path(), 1000 + DAY_S, true).unwrap();
        assert_eq!(f.asks.get(), 3, "--force asks again");
        std::fs::write(t.path().join("install.json"), r#"{"build":"abc1234 2026-11-16"}"#).unwrap();
        assert_eq!(check(&f, t.path(), 1000 + DAY_S, false).unwrap()["differs"], false);
    }

    #[test]
    fn gh_reads_the_newest_build() {
        let t = tempfile::tempdir().unwrap();
        let gh = |out: &str, code: i32| {
            let p = t.path().join("gh");
            script(&p, &format!("echo \"$@\" > '{}/args'; echo '{out}'; echo oops >&2; exit {code}", t.path().display()));
            Gh { program: p.display().to_string(), repo: "o/r".into() }
        };
        let json = format!(r#"{{"tagName":"{TAG}","url":"https://x/r","assets":[{{"name":"herdr-contr-{TAG}.zip","digest":"sha256:ab"}}]}}"#);
        let r = gh(&json, 0).newest().unwrap();
        assert_eq!((r.tag.as_str(), r.digest.as_deref()), (TAG, Some("sha256:ab")));
        assert_eq!(std::fs::read_to_string(t.path().join("args")).unwrap().trim(), "release view --json tagName,url,assets --repo o/r");
        assert_eq!(gh(r#"{"tagName":"v1.0.0","assets":[]}"#, 0).newest().unwrap_err().0, "no-build");
        assert_eq!(gh(&format!(r#"{{"tagName":"{TAG}","assets":[]}}"#), 0).newest().unwrap_err().0, "no-build");
        assert_eq!(gh("", 1).newest().unwrap_err(), ("gh", "gh failed: oops".to_string()));
        assert_eq!(Gh { program: "gh".into(), repo: String::new() }.newest().unwrap_err().0, "no-repo");
        assert_eq!(Gh { program: t.path().join("none").display().to_string(), repo: "o/r".into() }.newest().unwrap_err().0, "no-gh");
    }

    #[test]
    fn install_runs_a_dry_run_then_the_install() {
        let t = tempfile::tempdir().unwrap();
        let log = t.path().join("log");
        let f = fake(Some(release_zip(t.path(), &log, false)), None);
        let home = t.path().join("home");
        let lines = install(&f, &home, t.path()).unwrap();
        assert_eq!((lines[0]["type"].as_str(), lines[0]["text"].as_str()), (Some("update-plan"), Some("planned")));
        assert_eq!((lines[1]["type"].as_str(), lines[1]["installed"].as_str()), (Some("update-done"), Some("abc1234 2026-11-16")));
        let runs: Vec<String> = std::fs::read_to_string(&log).unwrap().lines().map(String::from).collect();
        assert_eq!(runs.len(), 2);
        let first = if cfg!(feature = "herdr-mod") { "install --home" } else { "install --backend --home" };
        assert!(runs[0].starts_with(&format!("{first} {} --helper ", home.display())) && runs[0].ends_with("--dry-run"));
        assert!(!runs[1].contains("--dry-run") && runs[1].contains("--remote"));
        assert!(!t.path().join(format!("herdr-contr-update-{}", std::process::id())).exists(), "the temp folder goes");
    }

    #[test]
    fn a_bad_sum_or_digest_installs_nothing() {
        let t = tempfile::tempdir().unwrap();
        let log = t.path().join("log");
        let bad = fake(Some(release_zip(t.path(), &log, true)), None);
        let e = install(&bad, &t.path().join("home"), t.path()).unwrap_err();
        assert_eq!(e.0, "check");
        assert!(!log.exists(), "no install ran");
        let good = release_zip(&t.path().join("g"), &log, false);
        let wrong = fake(Some(good), Some(format!("sha256:{}", "0".repeat(64))));
        assert!(install(&wrong, &t.path().join("home"), t.path()).unwrap_err().1.contains("digest"));
        assert!(!log.exists());
    }
}
