//! `herdr-contr-feed install` over older files: the sorting of a home's files against a known-files table.
#![cfg(all(unix, feature = "herdr-mod"))]

// The sorting on its own, with a made-up table: files a newer version dropped, and paths that are never ours.
mod sorting {
    use modfeedkit::install::{known_files, sort_files};
    use sha2::{Digest, Sha256};
    use std::collections::BTreeMap;
    use std::fs;

    fn sha(b: &[u8]) -> String {
        Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
    }

    #[test]
    fn dropped_files_of_an_older_version_are_removed_only_when_unchanged() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let put = |rel: &str, b: &[u8]| {
            fs::create_dir_all(home.join(rel).parent().unwrap()).unwrap();
            fs::write(home.join(rel), b).unwrap();
        };
        put("mods/herdr-contr/old.js", b"old\n");
        put("mods/herdr-contr/edited.js", b"edited by the person\n");
        put("themes/herdr-contr-old.json", b"{}\n");
        put("themes/theirs.json", b"{\"mine\":1}\n");
        put("layouts/herdr-contr-own.json", b"{\"v\":7}\n");
        // A path outside what herdr-contr owns, even with a matching hash, is never touched.
        put("keys-backup/herdr-contr.json", b"x\n");
        let table = format!(
            "# comment\n{}  v0.7.0  mods/herdr-contr/old.js\n{}  v0.7.0  mods/herdr-contr/edited.js\n{}  v0.7.0  themes/herdr-contr-old.json\n{}  v0.7.0  layouts/herdr-contr-own.json\n{}  v0.7.0  keys-backup/herdr-contr.json\n{}  v0.7.0  mods/herdr-contr/../../escape.js\nnot a line\n",
            sha(b"old\n"),
            sha(b"edited by the first version\n"),
            sha(b"{}\n"),
            sha(b"{\"v\":7}\n"),
            sha(b"x\n"),
            sha(b"x\n"),
        );
        let known = known_files(table.as_bytes());
        let files: BTreeMap<&str, &[u8]> = [("layouts/herdr-contr-own.json", &b"{\"v\":8}\n"[..]), ("mods/herdr-contr/main.js", &b"new\n"[..])].into_iter().collect();
        let s = sort_files(home, &files, &BTreeMap::new(), &known);
        assert_eq!(s.older.into_iter().collect::<Vec<_>>(), vec![("layouts/herdr-contr-own.json".into(), "v0.7.0".into())]);
        assert_eq!(
            s.gone.into_iter().collect::<Vec<_>>(),
            vec![("mods/herdr-contr/old.js".into(), "v0.7.0".into()), ("themes/herdr-contr-old.json".into(), "v0.7.0".into())]
        );
        assert_eq!(s.clash.into_iter().collect::<Vec<_>>(), vec!["mods/herdr-contr/edited.js".to_string()]);
    }

    #[test]
    fn an_empty_or_bad_table_knows_nothing() {
        assert!(known_files(b"").is_empty());
        assert!(known_files(b"zz  v0.7.0  layouts/herdr-contr-own.json\n# x\n").is_empty());
    }
}
