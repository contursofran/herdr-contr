//! `pictures`: a row's pictures written from the log to the cache folder, for the mod's chat.

use chrono::Utc;
use modfeedkit::{cli, pictures, rows, tail};
use serde_json::{json, Value};
use std::time::{Duration, SystemTime};

fn png(w: u32, h: u32) -> Vec<u8> {
    let mut b = vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13];
    b.extend(b"IHDR");
    b.extend(w.to_be_bytes());
    b.extend(h.to_be_bytes());
    b.extend([7u8; 20]);
    b
}

fn block(bytes: &[u8], mime: &str) -> Value {
    use base64::Engine;
    json!({"type": "image", "source": {"type": "base64", "media_type": mime, "data": base64::engine::general_purpose::STANDARD.encode(bytes)}})
}

/// A log with one user message: a PNG, a GIF, a PNG. Its row's `pics`, as `feed --pictures` gives them.
fn log(dir: &std::path::Path) -> (std::path::PathBuf, Vec<Value>, Vec<u8>) {
    let a = png(300, 200);
    let gif: Vec<u8> = [b"GIF89a".as_slice(), &[10, 0, 20, 0], &[0u8; 30]].concat();
    let b = png(40, 80);
    let line = json!({"type": "user", "uuid": "u1", "timestamp": "2026-11-03T01:40:00.000Z", "message": {"role": "user", "content": [
        {"type": "text", "text": "look"}, block(&a, "image/png"), block(&gif, "image/gif"), block(&b, "image/png")]}});
    let f = dir.join("s.jsonl");
    std::fs::write(&f, format!("{{\"type\":\"file-history-snapshot\"}}\n{line}\n")).unwrap();
    let range = tail::read(&f, None, tail::BYTES).unwrap();
    let now = chrono::TimeZone::with_ymd_and_hms(&Utc, 2026, 11, 3, 2, 0, 0).unwrap();
    let rs = rows::rows(&tail::items_at(&range), &now, &Utc);
    let pics = serde_json::to_value(rs[0].pics.as_ref().unwrap()).unwrap().as_array().unwrap().clone();
    (f, pics, a)
}

fn lines(out: &str) -> Vec<Value> {
    out.lines().map(|l| serde_json::from_str(l).unwrap()).collect()
}

#[test]
fn writes_each_picture_once() {
    let dir = tempfile::tempdir().unwrap();
    let (f, pics, a) = log(dir.path());
    let cache = dir.path().join("pics");
    let now = SystemTime::now();
    let got = pictures::lines(&f, &pics, &cache, now).unwrap();
    assert_eq!(got.len(), 3);
    assert_eq!((got[0]["width"].clone(), got[0]["height"].clone()), (json!(300), json!(200)));
    let p0 = got[0]["path"].as_str().unwrap();
    assert!(p0.ends_with(&format!("pic-{}.png", pics[0]["digest"].as_str().unwrap())), "{p0}");
    assert_eq!(std::fs::read(p0).unwrap(), a, "the picture as it is in the log");
    assert_eq!(got[1], json!({"type": "picture", "index": 1, "path": null}), "a GIF is not drawn");
    assert_eq!((got[2]["width"].clone(), got[2]["height"].clone()), (json!(40), json!(80)));

    // Asked again: the same file, not written again; its time is now, so the prune keeps it.
    let old = now - Duration::from_secs(30 * 24 * 3600);
    std::fs::File::options().write(true).open(p0).unwrap().set_modified(old).unwrap();
    let again = pictures::lines(&f, &pics[..1], &cache, now).unwrap();
    assert_eq!(again[0]["path"], got[0]["path"]);
    let m = std::fs::metadata(p0).unwrap().modified().unwrap();
    assert!(now.duration_since(m).unwrap_or_default() < Duration::from_secs(5));
}

#[test]
fn old_pictures_go_when_one_is_written() {
    let dir = tempfile::tempdir().unwrap();
    let (f, pics, _) = log(dir.path());
    let cache = dir.path().join("pics");
    std::fs::create_dir_all(&cache).unwrap();
    let now = SystemTime::now();
    let stale = cache.join("pic-0000000000000001.png");
    let fresh = cache.join("pic-0000000000000002.jpg");
    let other = cache.join("notes.png");
    for p in [&stale, &fresh, &other] {
        std::fs::write(p, b"x").unwrap();
        std::fs::File::options().write(true).open(p).unwrap().set_modified(now - Duration::from_secs(8 * 24 * 3600)).unwrap();
    }
    std::fs::File::options().write(true).open(&fresh).unwrap().set_modified(now - Duration::from_secs(3600)).unwrap();
    pictures::lines(&f, &pics[..1], &cache, now).unwrap();
    assert!(!stale.exists(), "older than 7 days");
    assert!(fresh.exists() && other.exists(), "newer ones, and files that are not pictures, stay");
}

#[test]
fn a_picture_no_longer_in_the_log() {
    let dir = tempfile::tempdir().unwrap();
    let (f, mut pics, _) = log(dir.path());
    pics[0]["digest"] = json!("0123456789abcdef");
    let got = pictures::lines(&f, &pics[..1], &dir.path().join("pics"), SystemTime::now()).unwrap();
    assert_eq!(got[0], json!({"type": "picture", "index": 0, "path": null}), "another fingerprint: not this picture");
}

#[test]
fn the_command() {
    let dir = tempfile::tempdir().unwrap();
    let (f, pics, _) = log(dir.path());
    let cache = dir.path().join("pics");
    let arg = |s: &str| s.to_string();
    let out = lines(&cli::run(&[arg("pictures"), arg("--path"), f.to_string_lossy().into(), arg("--dir"), cache.to_string_lossy().into(),
        arg("--pics"), Value::Array(pics).to_string(), arg("--home"), arg("/nowhere")]));
    assert_eq!(out[0]["type"], "header");
    assert_eq!(out.iter().filter(|l| l["type"] == "picture").count(), 3);
    assert_eq!(out.last().unwrap()["ok"], true);

    let bad = lines(&cli::run(&[arg("pictures"), arg("--path"), arg("x"), arg("--dir"), arg("y"), arg("--pics"), arg("{")]));
    assert_eq!(bad[1]["code"], "usage");
    let missing = lines(&cli::run(&[arg("pictures"), arg("--path"), arg("x")]));
    assert_eq!(missing[1]["code"], "usage");
}
