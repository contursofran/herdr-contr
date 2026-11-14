// Embeds the herdr-app mods (layout, themes, mods/herdr-contr*, keys.json) for `herdr-contr-feed install`, so
// the helper alone can install the mod. The files are the repo's herdr-app/ folder at build time.
use std::fs;
use std::path::{Path, PathBuf};

fn walk(dir: &Path, base: &Path, out: &mut Vec<(String, PathBuf)>) {
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut entries: Vec<_> = fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())).map(|e| e.unwrap().path()).collect();
    entries.sort();
    for p in entries {
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        if p.is_dir() {
            walk(&p, base, out);
            continue;
        }
        // herdr-app writes jsconfig.json and *.d.ts at a mod's top level (herdr-app/.gitignore); never ours.
        let generated = p.parent().and_then(|d| d.parent()) == Some(base.join("mods").as_path()) && (name == "jsconfig.json" || name.ends_with(".d.ts"));
        if name == ".DS_Store" || generated {
            continue;
        }
        let rel = p.strip_prefix(base).unwrap().components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/");
        out.push((rel, p));
    }
}

fn main() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../herdr-app");
    assert!(base.join("keys.json").is_file(), "herdr-app/ must sit next to xplat/");
    let mut files = vec![
        ("layouts/herdr-contr-own.json".to_string(), base.join("layouts/herdr-contr-own.json")),
        ("layouts/herdr-contr.json".to_string(), base.join("layouts/herdr-contr.json")),
    ];
    let themes = base.join("themes");
    println!("cargo:rerun-if-changed={}", themes.display());
    let mut names: Vec<_> = fs::read_dir(&themes).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    names.sort();
    for p in names {
        files.push((format!("themes/{}", p.file_name().unwrap().to_string_lossy()), p));
    }
    for m in ["herdr-contr", "herdr-contr-spaces", "herdr-contr-tabs"] {
        walk(&base.join("mods").join(m), &base, &mut files);
    }
    files.sort();
    let mut src = String::from("/// Every file `install` puts in the home, as (path relative to the home, bytes), sorted by path.\npub static FILES: &[(&str, &[u8])] = &[\n");
    for (rel, p) in &files {
        println!("cargo:rerun-if-changed={}", p.display());
        src.push_str(&format!("    ({rel:?}, include_bytes!({:?})),\n", p.display().to_string()));
    }
    src.push_str("];\n");
    // The kit (the Claude Code plugin of a coordinator and its workers), for <home>/herdr-contr/kit.
    let kit_base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../kit");
    assert!(kit_base.join(".claude-plugin/plugin.json").is_file(), "kit/ must sit next to xplat/");
    let mut kit = Vec::new();
    walk(&kit_base, &kit_base, &mut kit);
    kit.sort();
    src.push_str("/// Every file of the kit, as (path relative to the kit folder, bytes), sorted by path.\npub static KIT: &[(&str, &[u8])] = &[\n");
    for (rel, p) in &kit {
        println!("cargo:rerun-if-changed={}", p.display());
        src.push_str(&format!("    ({rel:?}, include_bytes!({:?})),\n", p.display().to_string()));
    }
    src.push_str("];\n");
    let keys = base.join("keys.json");
    println!("cargo:rerun-if-changed={}", keys.display());
    src.push_str(&format!("/// herdr-app/keys.json.\npub static KEYS: &[u8] = include_bytes!({:?});\n", keys.display().to_string()));
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let known = known_files(&out);
    src.push_str(&format!("/// The files of earlier versions (HERDR_CONTR_KNOWN_FILES, else empty): `<sha256>  <version>  <path>` lines.\npub static KNOWN: &[u8] = include_bytes!({:?});\n", known.display().to_string()));
    fs::write(out.join("embedded.rs"), src).unwrap();
}

/// The known-files table: HERDR_CONTR_KNOWN_FILES when set (a file of `<sha256>  <version>  <path>` lines), else
/// an empty table. This repository has no older releases, so by default no file in a home is an older one of ours.
fn known_files(out: &Path) -> PathBuf {
    println!("cargo:rerun-if-env-changed=HERDR_CONTR_KNOWN_FILES");
    if let Ok(p) = std::env::var("HERDR_CONTR_KNOWN_FILES") {
        let p = PathBuf::from(p);
        println!("cargo:rerun-if-changed={}", p.display());
        assert!(p.is_file(), "HERDR_CONTR_KNOWN_FILES={} is not a file", p.display());
        return p;
    }
    let dest = out.join("known-files.txt");
    fs::write(&dest, "").unwrap();
    dest
}
