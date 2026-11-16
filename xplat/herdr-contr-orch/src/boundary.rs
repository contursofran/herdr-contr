//! The boundary of a brief (#215): the files and folders that a worker may change. Spawn keeps it by the
//! worktree's branch, and `pr check-boundary <pr>` lists the changed files of the PR outside it.
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

/// The docs that the repository rules say to keep current. A boundary allows them unless it has `no-docs`.
pub const DOCS: &[&str] = &[
    "AGENTS.md",
    "herdr-app/RECIPE.md",
    "herdr-app/docs/protocol.md",
    "kit/README.md",
];

/// The globs of the brief's "Boundary:" part, or None when it has none. The part is the rest of the
/// `Boundary:` line (comma or space separated) and the list items (`- ` or `* `) right after it. Backticks go.
pub fn parse(brief: &str) -> Option<Vec<String>> {
    let mut lines = brief.lines();
    let rest = loop {
        let l = lines
            .next()?
            .trim()
            .trim_start_matches('#')
            .trim()
            .trim_start_matches("**");
        if let Some(r) = l
            .strip_prefix("Boundary:")
            .or_else(|| l.strip_prefix("Boundary**:"))
        {
            break r.trim_start_matches("**").to_string();
        }
    };
    let split = |s: &str| {
        s.split([',', ' '])
            .map(|w| w.trim().trim_matches('`').to_string())
            .filter(|w| !w.is_empty())
            .collect::<Vec<_>>()
    };
    let mut items = split(&rest);
    for l in lines {
        let t = l.trim();
        match t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            Some(item) => items.extend(split(item)),
            None if t.is_empty() && items.is_empty() => continue,
            None => break,
        }
    }
    Some(items)
}

/// Tells if `glob` matches `path`. `*` is any text without `/`, `**` any text. A glob with no `*` also matches
/// all files under it as a folder.
pub fn matches(glob: &str, path: &str) -> bool {
    let g = glob.trim_start_matches("./");
    if !g.contains('*') {
        let dir = g.trim_end_matches('/');
        return path == dir || path.starts_with(&format!("{dir}/"));
    }
    wild(g.as_bytes(), path.as_bytes())
}

fn wild(g: &[u8], p: &[u8]) -> bool {
    if g.starts_with(b"**") {
        let rest = g[2..].strip_prefix(b"/").unwrap_or(&g[2..]);
        return (0..=p.len())
            .any(|i| (i == 0 || p[i - 1] == b'/' || rest.is_empty()) && wild(rest, &p[i..]))
            || wild(&g[2..], p);
    }
    match g.first() {
        None => p.is_empty(),
        Some(b'*') => (0..=p.len())
            .take_while(|&i| i == 0 || p[i - 1] != b'/')
            .any(|i| wild(&g[1..], &p[i..])),
        Some(c) => p.first() == Some(c) && wild(&g[1..], &p[1..]),
    }
}

/// The changed files outside the boundary, in their order.
pub fn outside(globs: &[String], files: &[String]) -> Vec<String> {
    let docs = !globs.iter().any(|g| g == "no-docs");
    files
        .iter()
        .filter(|f| !(docs && DOCS.contains(&f.as_str())) && !globs.iter().any(|g| matches(g, f)))
        .cloned()
        .collect()
}

/// The folder of the kept boundaries: CONTR_BOUNDARY_DIR, else `boundary` in the state folder.
fn folder(env: &dyn Fn(&str) -> String) -> Option<PathBuf> {
    let d = env("CONTR_BOUNDARY_DIR");
    if !d.is_empty() {
        return Some(PathBuf::from(d));
    }
    crate::home::places(env)
        .ok()
        .map(|p| p.state().join("boundary"))
}

fn file(env: &dyn Fn(&str) -> String, branch: &str) -> Option<PathBuf> {
    Some(folder(env)?.join(format!("{}.txt", branch.replace('/', "__"))))
}

/// Keeps the brief's boundary for the branch of `worktree`, one glob a line. A brief with no boundary removes a
/// kept one, so a new worker on the branch does not get an old boundary.
pub fn keep(env: &dyn Fn(&str) -> String, worktree: &str, brief: &str) -> Result<(), String> {
    if worktree.is_empty() {
        return Ok(());
    }
    let branch = crate::spawn::git(worktree, &["branch", "--show-current"]).unwrap_or_default();
    let Some(f) = (!branch.is_empty()).then(|| file(env, &branch)).flatten() else {
        return Ok(());
    };
    match parse(brief) {
        Some(globs) => {
            fs::create_dir_all(f.parent().unwrap_or(&f)).map_err(|e| format!("boundary: {e}"))?;
            fs::write(&f, globs.join("\n") + "\n")
                .map_err(|e| format!("boundary: {}: {e}", f.display()))
        }
        None => {
            let _ = fs::remove_file(&f);
            Ok(())
        }
    }
}

/// `pr check-boundary <pr> [--brief <file>]`: 0 when all changed files are inside (or no boundary is known), 1
/// with the list when some are outside, 2 on an error.
pub fn run(
    args: &[String],
    env: &dyn Fn(&str) -> String,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let (pr, brief) = match args {
        [s, pr] if s == "check-boundary" => (pr, None),
        [s, pr, f, b] if s == "check-boundary" && f == "--brief" => (pr, Some(b)),
        _ => {
            let _ = writeln!(
                err,
                "usage: herdr-contr-orch pr check-boundary <pr> [--brief <file>]"
            );
            return 2;
        }
    };
    match check(pr, brief.map(String::as_str), env) {
        Ok(None) => {
            let _ = writeln!(out, "check-boundary: no boundary for PR {pr}");
            0
        }
        Ok(Some(o)) if o.is_empty() => {
            let _ = writeln!(
                out,
                "check-boundary: all changed files of PR {pr} are inside the boundary"
            );
            0
        }
        Ok(Some(o)) => {
            let _ = writeln!(
                out,
                "check-boundary: PR {pr} changes files outside the boundary:"
            );
            for f in o {
                let _ = writeln!(out, "  {f}");
            }
            1
        }
        Err(e) => {
            let _ = writeln!(err, "check-boundary: {e}");
            2
        }
    }
}

fn check(
    pr: &str,
    brief: Option<&str>,
    env: &dyn Fn(&str) -> String,
) -> Result<Option<Vec<String>>, String> {
    let gh = env("CONTR_GH");
    let gh = if gh.is_empty() { "gh".to_string() } else { gh };
    let o = Command::new(&gh)
        .args(["pr", "view", pr, "--json", "files,headRefName"])
        .output()
        .map_err(|e| format!("{gh} pr view: {e}"))?;
    if !o.status.success() {
        return Err(format!(
            "{gh} pr view: {}",
            String::from_utf8_lossy(&o.stderr).trim()
        ));
    }
    let v: Value = serde_json::from_slice(&o.stdout).map_err(|e| format!("{gh} pr view: {e}"))?;
    let files: Vec<String> = v["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|f| f["path"].as_str().map(String::from))
        .collect();
    let globs = match brief {
        Some(b) => {
            let text = fs::read_to_string(b).map_err(|e| format!("brief {b}: {e}"))?;
            Some(parse(&text).ok_or_else(|| format!("brief {b} has no Boundary: part"))?)
        }
        None => {
            let branch = v["headRefName"].as_str().unwrap_or_default();
            file(env, branch)
                .and_then(|f| fs::read_to_string(f).ok())
                .map(|t| {
                    t.lines()
                        .map(String::from)
                        .filter(|l| !l.is_empty())
                        .collect()
                })
        }
    };
    Ok(globs.map(|g| outside(&g, &files)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs() {
        assert!(matches("scripts/land.sh", "scripts/land.sh"));
        assert!(matches("kit", "kit/commands/orchestrate.md"));
        assert!(matches("kit/", "kit/a.md"));
        assert!(!matches("kit", "kitchen/a.md"));
        assert!(matches("xplat/*/src/*.rs", "xplat/orch/src/a.rs"));
        assert!(!matches("xplat/*.rs", "xplat/orch/a.rs"));
        assert!(matches("xplat/**/*.rs", "xplat/a/b/c.rs"));
        assert!(matches("xplat/**/*.rs", "xplat/c.rs"));
        assert!(matches("**/Cargo.toml", "Cargo.toml"));
        assert!(!matches("**/Cargo.toml", "Cargo.lock"));
    }

    #[test]
    fn parses_the_line_and_the_list() {
        let b = "Do this.\n\n## Boundary: `scripts/land.sh`, kit/\n- xplat/herdr-contr-orch/**\n* no-docs\n\nThen more.\n- not this";
        assert_eq!(
            parse(b).unwrap(),
            [
                "scripts/land.sh",
                "kit/",
                "xplat/herdr-contr-orch/**",
                "no-docs"
            ]
        );
        assert_eq!(parse("**Boundary:**\n\n- a\n").unwrap(), ["a"]);
        assert_eq!(parse("no part here"), None);
    }

    #[test]
    fn docs_are_allowed_unless_no_docs() {
        let files: Vec<String> = ["kit/a.md", "AGENTS.md", "herdr-app/mods/x.js"]
            .map(String::from)
            .to_vec();
        assert_eq!(outside(&["kit".into()], &files), ["herdr-app/mods/x.js"]);
        assert_eq!(
            outside(&["kit".into(), "no-docs".into()], &files),
            ["AGENTS.md", "herdr-app/mods/x.js"]
        );
    }
}
