// The build stamp, for build.rs of herdr-contr-feed and herdr-contr-orch (include!). It sets HERDR_CONTR_BUILD
// to "<short sha> <commit date>". CI sets HERDR_CONTR_BUILD_SHA and HERDR_CONTR_BUILD_DATE; a local build asks
// git. Without both, the stamp is "unknown". HERDR_CONTR_REPO is the GitHub repository (owner/name) whose
// releases `update` reads: GITHUB_REPOSITORY in CI, else the origin remote; empty when neither is known.
fn build_stamp() {
    use std::process::Command;
    println!("cargo:rerun-if-env-changed=HERDR_CONTR_BUILD_SHA");
    println!("cargo:rerun-if-env-changed=HERDR_CONTR_BUILD_DATE");
    let git = |args: &[&str]| -> Option<String> {
        let o = Command::new("git").args(args).current_dir(env!("CARGO_MANIFEST_DIR")).output().ok()?;
        let s = String::from_utf8(o.stdout).ok()?.trim().to_string();
        (o.status.success() && !s.is_empty()).then_some(s)
    };
    // A new commit or a checkout changes HEAD (or the ref it names): build again.
    for p in ["HEAD", "logs/HEAD"] {
        if let Some(f) = git(&["rev-parse", "--path-format=absolute", "--git-path", p]) {
            println!("cargo:rerun-if-changed={f}");
        }
    }
    let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let sha = var("HERDR_CONTR_BUILD_SHA").map(|s| s.chars().take(7).collect()).or_else(|| git(&["log", "-1", "--format=%h"]));
    let date = var("HERDR_CONTR_BUILD_DATE").or_else(|| git(&["log", "-1", "--format=%cs"]));
    let stamp = match (sha, date) {
        (Some(s), Some(d)) => format!("{s} {d}"),
        _ => "unknown".to_string(),
    };
    println!("cargo:rustc-env=HERDR_CONTR_BUILD={stamp}");
    println!("cargo:rerun-if-env-changed=GITHUB_REPOSITORY");
    let repo = var("GITHUB_REPOSITORY").or_else(|| {
        let url = git(&["remote", "get-url", "origin"])?;
        let tail = url.trim_end_matches(".git").rsplit(|c| c == ':' || c == '/').take(2).collect::<Vec<_>>();
        (tail.len() == 2).then(|| format!("{}/{}", tail[1], tail[0]))
    });
    println!("cargo:rustc-env=HERDR_CONTR_REPO={}", repo.unwrap_or_default());
}
