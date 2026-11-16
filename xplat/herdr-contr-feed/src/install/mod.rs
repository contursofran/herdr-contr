//! `herdr-contr-feed install`: the port of herdr-app/install.sh, with the mod built into the helper.
//! Plain text for a person, and an exit code (0 done, 1 stopped, 2 bad arguments); not JSON Lines.

use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::Path;

use herdr_core::places::{self, Places};

mod backend;
mod files;
mod herdr;

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/embedded.rs"));
}
pub use backend::{KIT, MARKETPLACE};
pub use herdr::{known_files, sort_files, Known, Sorted, FILES, KEYS, KNOWN, MODS, RETIRED};

use backend::{old_orch_files, read_remote, Backend};
use files::{put, sha_file, stop, walk_files};
use herdr::{in_our_mod, merge_keys, remove_retired};

const HELP: &str = "\
Installs the herdr-contr layout, mod, themes, keys and helper into a herdr-app home.
Usage: herdr-contr-feed install [--home <herdr-app home>] [--helper <herdr-contr-feed>] [--orch <herdr-contr-orch>]
                                [--remote <folder>] [--dry-run]
       herdr-contr-feed install --backend [--home <herdr-app home>] [--helper <herdr-contr-feed>]
                                [--orch <herdr-contr-orch>] [--remote <folder>] [--dry-run]
 - The home is --home, else $HERDR_APP_HOME, else ~/.config/herdr-app. With no HOME, give --home.
 - A keys.json, layout or theme that is a symlink stays one; the file it points to is written.
 - It never overwrites what you have: if a layout, mod or theme with the same name is there and
   this did not put it there (or you changed it since), it stops and writes nothing.
 - keys.json is merged: your sections and keys stay; a key you already use keeps your command.
 - --helper copies herdr-contr-feed to <home>/herdr-contr/bin/. Without it, a helper already there
   stays; with none there, it says where to get one.
 - The kit (the Claude Code plugin of a coordinator and its workers) goes to <home>/herdr-contr/kit/.
   That folder is this install's: a file there that this version does not have is removed.
   <home>/herdr-contr/.claude-plugin/marketplace.json names it for `claude plugin marketplace add`.
 - The profiles file and the owner file of the orchestration tool (agents.json, owner.json) are in
   <home>/herdr-contr/. When one is not there and the contr app's is (the user config folder,
   contr/), it is copied once; the old file stays.
 - --orch copies the orchestration tool herdr-contr-orch to <home>/herdr-contr/bin/, where the kit's
   hooks and MCP server look for it first.
 - --remote copies the helpers for other machines (<folder>/<target>/herdr-contr-feed, checked
   against <folder>/SHA256SUMS) to <home>/herdr-contr/remote/.
 - After copying it runs `herdr-app check` on the mod. --dry-run prints the plan and writes nothing.
 - What it installed (with hashes) is kept in <home>/herdr-contr/install.json, so a later install
   can update its own files.
 - --backend installs only the backend, for a machine with no Herdr app. It writes into the backend root
   only: kit/ (a file there that this version does not have is removed), .claude-plugin/marketplace.json,
   bin/herdr-contr-feed (with --helper), bin/herdr-contr-orch (with --orch), remote/ (with --remote), and
   \"build\" in install.json (the record of an install of the mod stays). It never writes a layout, a theme,
   a mod or keys.json, and never runs `herdr-app check`. The root is <home>/herdr-contr with --home, else
   $CONTR_HOME, else $HERDR_APP_HOME/herdr-contr, else ~/.config/herdr-app/herdr-contr.
";

pub fn run(args: &[String]) -> i32 {
    let mut home: Option<String> = None;
    let mut helper: Option<String> = None;
    let mut orch: Option<String> = None;
    let mut remote: Option<String> = None;
    let mut dry = false;
    let mut backend_only = false;
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        let value = |what: &str| -> Result<String, i32> {
            args.get(i + 1).cloned().ok_or_else(|| {
                eprintln!("{a} needs {what}");
                2
            })
        };
        match a {
            "--home" => match value("a folder") {
                Ok(v) => home = Some(v),
                Err(c) => return c,
            },
            "--helper" => match value("a file") {
                Ok(v) => helper = Some(v),
                Err(c) => return c,
            },
            "--orch" => match value("a file") {
                Ok(v) => orch = Some(v),
                Err(c) => return c,
            },
            "--remote" => match value("a folder") {
                Ok(v) => remote = Some(v),
                Err(c) => return c,
            },
            "--dry-run" => {
                dry = true;
                i += 1;
                continue;
            }
            "--backend" => {
                backend_only = true;
                i += 1;
                continue;
            }
            "-h" | "--help" => {
                print!("{HELP}");
                return 0;
            }
            _ => {
                eprintln!("unknown argument: {a} (see --help)");
                return 2;
            }
        }
        i += 2;
    }
    let nonempty = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let env = |k: &str| std::env::var(k).unwrap_or_default();
    if backend_only {
        let Some(root) = places::root(home.as_deref(), None, &env) else {
            return stop(&["STOP: no backend root is known: CONTR_HOME, HERDR_APP_HOME and HOME are not set. Name the folder with CONTR_HOME=<folder> or --home <herdr-app home>. Nothing was written.".into()]);
        };
        return install_backend(&Places::at(root), helper.as_deref(), orch.as_deref(), remote.as_deref(), dry);
    }
    let Some(home) = places::home(home.as_deref(), None, &env) else {
        return stop(&["STOP: HOME is not set, so there is no ~/.config/herdr-app. Name the folder with --home <herdr-app home>. Nothing was written.".into()]);
    };
    let places = Places::new(&home);
    if let Some(h) = &helper {
        if !Path::new(h).is_file() {
            return stop(&[format!("STOP: no helper at {h}")]);
        }
    }
    if let Some(o) = &orch {
        if !Path::new(o).is_file() {
            return stop(&[format!("STOP: no orchestration tool at {o}")]);
        }
    }
    let marker_path = places.install_record();
    let files: BTreeMap<&str, &[u8]> = FILES.iter().copied().collect();

    let mine: BTreeMap<String, String> = fs::read(&marker_path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| v.get("files").and_then(|f| f.as_object()).cloned())
        .map(|m| m.into_iter().filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_string()))).collect())
        .unwrap_or_default();

    let known = known_files(KNOWN);
    let Sorted { clash, older, gone } = sort_files(&home, &files, &mine, &known);
    if !clash.is_empty() {
        let mut lines = vec![format!("STOP: these are already in {} and were not installed by this script (or you changed them):", home.display())];
        lines.extend(clash.iter().map(|r| format!("  {r}")));
        lines.push("Nothing was written. Move or rename them, then run this again.".into());
        return stop(&lines);
    }

    let keys_path = home.join("keys.json");
    let (keys, old_keys, kept) = match merge_keys(&keys_path) {
        Ok(k) => k,
        Err(e) => return stop(&[e]),
    };

    let remote = match remote {
        None => None,
        Some(dir) => match read_remote(Path::new(&dir)) {
            Ok(r) => Some(r),
            Err(e) => return stop(&[e, "Nothing was written.".into()]),
        },
    };

    let bin_dest = places.feed();
    let kit_dest = places.kit();
    // The kit folder is ours: what is there and not in this version goes.
    let mut kit_old = Vec::new();
    walk_files(&kit_dest, &mut kit_old);
    kit_old.retain(|p| !KIT.iter().any(|(rel, _)| kit_dest.join(rel) == *p));
    let backend = Backend { helper: helper.as_deref(), orch: orch.as_deref(), remote, moved: old_orch_files(&home, &nonempty, cfg!(target_os = "macos")), kit_old };
    let show = |v: &Value| v.as_str().map(String::from).unwrap_or_else(|| v.to_string());
    println!("{}{}", if dry { "Dry run: would install into " } else { "Installing into " }, home.display());
    for rel in files.keys().filter(|r| !r.starts_with("mods/")) {
        println!("  {rel}");
    }
    println!("  mods/herdr-contr*/... ({} files)", files.keys().filter(|r| r.starts_with("mods/")).count());
    for (rel, v) in &older {
        println!("    {rel}: older herdr-contr ({v}), replaced");
    }
    for (rel, v) in &gone {
        println!("    {rel}: older herdr-contr ({v}), removed (this version does not have it)");
    }
    println!("  keys.json ({})", if old_keys.is_empty() { "new" } else { "merged" });
    for (section, key, theirs, cmd) in &kept {
        println!("    kept your {section} {key} = {} (herdr-contr uses it for {})", show(theirs), show(cmd));
    }
    backend.plan(&home, "herdr-contr/");
    if dry {
        println!("Dry run: nothing was written; herdr-app check did not run.");
        return 0;
    }

    let written = (|| -> std::io::Result<()> {
        for (rel, bytes) in &files {
            put(&home.join(rel), bytes, false)?;
        }
        // Files of an older herdr-contr that this version no longer has, if still byte for byte that version's.
        for rel in gone.keys() {
            let dest = home.join(rel);
            if sha_file(&dest).is_some_and(|h| known.get(rel).is_some_and(|m| m.contains_key(&h))) {
                fs::remove_file(&dest)?;
            }
        }
        // Files an earlier install put there that this version no longer has, if still unchanged.
        for (rel, h) in &mine {
            let dest = home.join(rel);
            let own = (in_our_mod(rel) || rel.starts_with("themes/herdr-contr")) && !rel.split('/').any(|c| c == "..");
            if own && !files.contains_key(rel.as_str()) && dest.is_file() && sha_file(&dest).as_ref() == Some(h) {
                fs::remove_file(&dest)?;
            }
        }
        remove_retired(&home)?;
        if keys != old_keys {
            put(&keys_path, (serde_json::to_string_pretty(&keys).unwrap() + "\n").as_bytes(), false)?;
        }
        backend.write(&places)?;
        let marker: BTreeMap<&str, String> = files.keys().map(|rel| (*rel, sha_file(&home.join(rel)).unwrap_or_default())).collect();
        put(&marker_path, (serde_json::to_string_pretty(&serde_json::json!({ "build": crate::protocol::BUILD, "files": marker })).unwrap() + "\n").as_bytes(), false)
    })();
    if let Err(e) = written {
        return stop(&[format!("STOP: writing into {} failed: {e}", home.display())]);
    }
    // The phone page runs this helper as a service (herdr-contr settings): restart it, or it keeps the old one.
    if helper.is_some() {
        let path = bin_dest.canonicalize().unwrap_or_else(|_| bin_dest.clone());
        if let Some(m) = crate::phone::restart_after_install(&crate::phone::Env::from_process(), &path) {
            println!("{m}");
        }
    }

    if !bin_dest.is_file() {
        let me = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "herdr-contr-feed".into());
        println!("The helper herdr-contr-feed is not installed; the chat stays empty without it.");
        println!("This program is one: run this again with --helper {me}");
        println!("or build one in this repository: cargo build --release --manifest-path xplat/Cargo.toml -p herdr-contr-feed,");
        println!("then run: xplat/target/release/herdr-contr-feed install --helper xplat/target/release/herdr-contr-feed");
    }

    let _ = std::io::stdout().flush();
    herdr::check(&home)
}

/// `install --backend`: the kit, the marketplace file, the programs given and `remote/`, into the backend root
/// only, and `"build"` in its install.json (an existing `"files"` stays). No mod file and no `herdr-app check`.
fn install_backend(places: &Places, helper: Option<&str>, orch: Option<&str>, remote: Option<&str>, dry: bool) -> i32 {
    let root = places.folder();
    if let Some(h) = helper {
        if !Path::new(h).is_file() {
            return stop(&[format!("STOP: no helper at {h}")]);
        }
    }
    if let Some(o) = orch {
        if !Path::new(o).is_file() {
            return stop(&[format!("STOP: no orchestration tool at {o}")]);
        }
    }
    let remote = match remote {
        None => None,
        Some(dir) => match read_remote(Path::new(dir)) {
            Ok(r) => Some(r),
            Err(e) => return stop(&[e, "Nothing was written.".into()]),
        },
    };
    let kit_dest = places.kit();
    let mut kit_old = Vec::new();
    walk_files(&kit_dest, &mut kit_old);
    kit_old.retain(|p| !KIT.iter().any(|(rel, _)| kit_dest.join(rel) == *p));
    let backend = Backend { helper, orch, remote, moved: Vec::new(), kit_old };
    println!("{}{}", if dry { "Dry run: would install the backend into " } else { "Installing the backend into " }, root.display());
    backend.plan(&root, "");
    println!("  install.json (\"build\")");
    if dry {
        println!("Dry run: nothing was written.");
        return 0;
    }
    let marker_path = places.install_record();
    let written = (|| -> std::io::Result<()> {
        backend.write(places)?;
        // The record of an install of the mod ("files") stays; only "build" changes.
        let mut record = fs::read(&marker_path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        record.insert("build".into(), Value::from(crate::protocol::BUILD));
        put(&marker_path, (serde_json::to_string_pretty(&record).unwrap() + "\n").as_bytes(), false)
    })();
    if let Err(e) = written {
        return stop(&[format!("STOP: writing into {} failed: {e}", root.display())]);
    }
    // The phone page runs this helper as a service: restart it, or it keeps the old one.
    if helper.is_some() {
        let bin = places.feed();
        let path = bin.canonicalize().unwrap_or(bin);
        if let Some(m) = crate::phone::restart_after_install(&crate::phone::Env::from_process(), &path) {
            println!("{m}");
        }
    }
    println!("Done.");
    0
}
