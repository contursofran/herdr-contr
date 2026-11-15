//! `agent-images`: the local images that an agent links in its message (`![alt](path)`, #192), as copies under the
//! app home that the chat can draw. `image()` reads only files under the app home (as `favicon`). A path is
//! absolute, `~/…`, `file://…`, or relative to the session's cwd, which comes from the log. A URL is never read.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Copies that were not written or asked for in this time are removed when one is asked for.
pub const KEEP: Duration = Duration::from_secs(7 * 24 * 3600);
/// At most this many images in one ask.
pub const MAX: usize = 32;
/// A larger file is not copied.
pub const MAX_BYTES: u64 = 32 * 1024 * 1024;
/// The part of the log that is read at each end for the cwd.
const SPAN: u64 = 1 << 20;

/// The extensions `image()` draws.
const EXTS: [&str; 4] = ["png", "jpg", "jpeg", "svg"];

/// The session's cwd: the last `cwd` (Claude Code, pi) or `payload.cwd` (Codex) in the log's last MB, else in its
/// first MB. None when the log has none.
pub fn log_cwd(log: &Path) -> Option<PathBuf> {
    let mut f = std::fs::File::open(log).ok()?;
    let len = f.metadata().ok()?.len();
    let mut read = |from: u64| -> Option<Vec<u8>> {
        f.seek(SeekFrom::Start(from)).ok()?;
        let mut b = Vec::new();
        (&mut f).take(SPAN).read_to_end(&mut b).ok()?;
        Some(b)
    };
    let tail = read(len.saturating_sub(SPAN))?;
    if let Some(c) = last_cwd(&tail) {
        return Some(c);
    }
    if len > SPAN {
        return last_cwd(&read(0)?);
    }
    None
}

fn last_cwd(data: &[u8]) -> Option<PathBuf> {
    data.split(|b| *b == b'\n').rev().find_map(|line| {
        let v: Value = serde_json::from_slice(line).ok()?;
        let c = v.get("cwd").or_else(|| v.get("payload").and_then(|p| p.get("cwd")))?.as_str()?;
        Path::new(c).is_absolute().then(|| PathBuf::from(c))
    })
}

/// The file that `src` names: absolute, `~/…` (in `home`), `file://…`, or relative to `cwd`. None for a URL, an
/// empty src, or a relative path with no cwd.
pub fn resolve(src: &str, cwd: Option<&Path>, home: Option<&Path>) -> Option<PathBuf> {
    let s = src.trim();
    let s = s.strip_prefix('<').and_then(|x| x.strip_suffix('>')).unwrap_or(s);
    if s.is_empty() {
        return None;
    }
    if let Some(p) = s.strip_prefix("file://") {
        return Path::new(p).is_absolute().then(|| PathBuf::from(p));
    }
    // A scheme ("https:", "data:", "mailto:"): never read.
    if let Some(i) = s.find(':') {
        if i > 1 && s[..i].chars().all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c)) {
            return None;
        }
    }
    if let Some(rest) = s.strip_prefix("~/") {
        return home.map(|h| h.join(rest));
    }
    let p = Path::new(s);
    if p.is_absolute() {
        return Some(p.to_path_buf());
    }
    cwd.map(|c| c.join(p))
}

/// The copy's file name: a digest of the source's path and its extension (lower case). None for another format.
pub fn file_name(src: &Path) -> Option<String> {
    let ext = src.extension()?.to_str()?.to_ascii_lowercase();
    if !EXTS.contains(&ext.as_str()) {
        return None;
    }
    let d = Sha256::digest(src.to_string_lossy().as_bytes());
    let hex: String = d.iter().take(8).map(|b| format!("{b:02x}")).collect();
    Some(format!("img-{hex}.{ext}"))
}

fn is_copy(name: &str) -> bool {
    name.starts_with("img-") && EXTS.iter().any(|e| name.ends_with(&format!(".{e}")))
}

/// The size in px of an image `image()` draws: PNG and JPEG from the header, SVG from the root's `width` and
/// `height`, else its `viewBox`. The content must match the extension. None when it does not, or no size is there.
pub fn size(bytes: &[u8], ext: &str) -> Option<(f64, f64)> {
    use herdr_core::log_image::Format;
    match ext {
        "png" | "jpg" | "jpeg" => {
            let (f, w, h) = herdr_core::log_image::header(bytes)?;
            let want = if ext == "png" { Format::Png } else { Format::Jpeg };
            (f == want).then_some((w as f64, h as f64))
        }
        "svg" => svg_size(&String::from_utf8_lossy(bytes)),
        _ => None,
    }
}

fn svg_size(text: &str) -> Option<(f64, f64)> {
    let start = text.find("<svg")?;
    let tag = &text[start..start + text[start..].find('>')?];
    let attr = |name: &str| -> Option<&str> {
        let mut at = 0;
        while let Some(i) = tag[at..].find(name) {
            let i = at + i;
            at = i + name.len();
            let before = tag[..i].chars().last();
            if !before.is_some_and(char::is_whitespace) {
                continue;
            }
            let rest = tag[at..].trim_start().strip_prefix('=')?.trim_start();
            let q = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
            return rest[1..].find(q).map(|e| &rest[1..1 + e]);
        }
        None
    };
    let px = |v: &str| -> Option<f64> {
        let n = v.trim().strip_suffix("px").unwrap_or(v.trim());
        n.parse::<f64>().ok().filter(|x| x.is_finite() && *x > 0.0)
    };
    if let (Some(w), Some(h)) = (attr("width").and_then(px), attr("height").and_then(px)) {
        return Some((w, h));
    }
    let vb: Vec<f64> = attr("viewBox")?.split(|c: char| c.is_whitespace() || c == ',').filter(|s| !s.is_empty()).filter_map(|s| s.parse().ok()).collect();
    (vb.len() == 4 && vb[2] > 0.0 && vb[3] > 0.0).then(|| (vb[2], vb[3]))
}

/// Removes the copies that are older than `KEEP`.
fn prune(dir: &Path, now: SystemTime) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        if !is_copy(&e.file_name().to_string_lossy()) {
            continue;
        }
        if let Ok(m) = e.metadata().and_then(|m| m.modified()) {
            if now.duration_since(m).is_ok_and(|d| d > KEEP) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

/// One copy: its path and size, or None when the file is not there, is too large, or is not a drawable image.
fn one(src: &str, cwd: Option<&Path>, home: Option<&Path>, dir: &Path, now: SystemTime) -> std::io::Result<Option<(PathBuf, f64, f64)>> {
    let Some(file) = resolve(src, cwd, home) else { return Ok(None) };
    let Some(name) = file_name(&file) else { return Ok(None) };
    let ext = name.rsplit('.').next().unwrap_or_default().to_string();
    match std::fs::metadata(&file) {
        Ok(m) if m.is_file() && m.len() <= MAX_BYTES => {}
        _ => return Ok(None),
    }
    let Ok(bytes) = std::fs::read(&file) else { return Ok(None) };
    let Some((w, h)) = size(&bytes, &ext) else { return Ok(None) };
    let dst = dir.join(name);
    if !crate::favicon::copy_to(&file, &dst)? {
        return Ok(None);
    }
    // Written or asked for now: the prune keeps it. Not older than the source, so it is not copied again.
    let _ = std::fs::File::options().write(true).open(&dst).and_then(|f| f.set_modified(now));
    Ok(Some((dst, w, h)))
}

/// One `agent-image` line for each src, in order: `path` (the copy), `width` and `height` (px); `path` null when
/// the src is a URL, the file is not there, or it is not a PNG, JPEG or SVG with a size.
pub fn lines(log: &Path, srcs: &[String], dir: &Path, home: Option<&Path>, now: SystemTime) -> std::io::Result<Vec<Value>> {
    std::fs::create_dir_all(dir)?;
    prune(dir, now);
    let cwd = log_cwd(log);
    let mut out = Vec::new();
    for (i, src) in srcs.iter().enumerate() {
        out.push(match one(src, cwd.as_deref(), home, dir, now)? {
            Some((p, w, h)) => json!({"type": "agent-image", "index": i, "path": p.to_string_lossy(), "width": w, "height": h}),
            None => json!({"type": "agent-image", "index": i, "path": null}),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("hcf-agentimg-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A PNG header of `w` x `h` px (all that `size` reads).
    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut b = vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13];
        b.extend_from_slice(b"IHDR");
        b.extend_from_slice(&w.to_be_bytes());
        b.extend_from_slice(&h.to_be_bytes());
        b.extend_from_slice(&[8, 6, 0, 0, 0]);
        b
    }

    #[test]
    fn resolves_paths_and_refuses_urls() {
        let cwd = Path::new("/w/proj");
        let home = Path::new("/home/you");
        let r = |s: &str| resolve(s, Some(cwd), Some(home));
        assert_eq!(r("/tmp/a.png"), Some(PathBuf::from("/tmp/a.png")));
        assert_eq!(r("shots/a.png"), Some(PathBuf::from("/w/proj/shots/a.png")));
        assert_eq!(r("./a.svg"), Some(PathBuf::from("/w/proj/./a.svg")));
        assert_eq!(r("~/Desktop/a.jpg"), Some(PathBuf::from("/home/you/Desktop/a.jpg")));
        assert_eq!(r("file:///tmp/a.png"), Some(PathBuf::from("/tmp/a.png")));
        assert_eq!(r("</tmp/my shot.png>"), Some(PathBuf::from("/tmp/my shot.png")));
        assert_eq!(r("https://example.com/a.png"), None);
        assert_eq!(r("http://example.com/a.png"), None);
        assert_eq!(r("data:image/png;base64,AAAA"), None);
        assert_eq!(r("file://relative.png"), None);
        assert_eq!(r("  "), None);
        assert_eq!(resolve("a.png", None, Some(home)), None, "a relative path needs the cwd");
        assert_eq!(resolve("~/a.png", Some(cwd), None), None, "~ needs the home");
    }

    #[test]
    fn names() {
        let a = file_name(Path::new("/w/a.png")).unwrap();
        assert!(a.starts_with("img-") && a.ends_with(".png") && a.len() == 4 + 16 + 4);
        assert_ne!(Some(a), file_name(Path::new("/w/b.png")));
        assert!(file_name(Path::new("/w/a.JPEG")).unwrap().ends_with(".jpeg"));
        assert_eq!(file_name(Path::new("/w/a.gif")), None);
        assert_eq!(file_name(Path::new("/w/a.webp")), None);
        assert_eq!(file_name(Path::new("/w/README")), None);
        assert!(is_copy("img-00ff00ff00ff00ff.svg") && !is_copy("fav-00ff.png") && !is_copy("img-notes.txt"));
    }

    #[test]
    fn sizes() {
        assert_eq!(size(&png(640, 480), "png"), Some((640.0, 480.0)));
        assert_eq!(size(&png(640, 480), "jpg"), None, "the content must match the extension");
        assert_eq!(size(b"not an image", "png"), None);
        let svg = |s: &str| size(s.as_bytes(), "svg");
        assert_eq!(svg(r#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg" width="200" height="100px">"#), Some((200.0, 100.0)));
        assert_eq!(svg(r#"<svg stroke-width="2" width="300" height="150">"#), Some((300.0, 150.0)), "stroke-width is not width");
        assert_eq!(svg(r#"<svg viewBox="0 0 300 150">"#), Some((300.0, 150.0)));
        assert_eq!(svg(r#"<svg width='100%' height='100%' viewBox='0,0,40,20'>"#), Some((40.0, 20.0)));
        assert_eq!(svg(r#"<svg xmlns="http://www.w3.org/2000/svg">"#), None);
        assert_eq!(svg("<html>"), None);
    }

    #[test]
    fn cwd_from_the_log() {
        let d = tmp("cwd");
        let claude = d.join("c.jsonl");
        std::fs::write(&claude, "{\"type\":\"user\",\"cwd\":\"/w/one\"}\n{\"type\":\"assistant\",\"cwd\":\"/w/two\"}\n{\"type\":\"summary\"}\n").unwrap();
        assert_eq!(log_cwd(&claude), Some(PathBuf::from("/w/two")), "the last cwd");
        let codex = d.join("x.jsonl");
        std::fs::write(&codex, "{\"type\":\"session_meta\",\"payload\":{\"cwd\":\"/w/codex\"}}\n{\"type\":\"response_item\",\"payload\":{}}\n").unwrap();
        assert_eq!(log_cwd(&codex), Some(PathBuf::from("/w/codex")));
        let none = d.join("n.jsonl");
        std::fs::write(&none, "{\"type\":\"user\",\"cwd\":\"relative\"}\nnot json\n").unwrap();
        assert_eq!(log_cwd(&none), None);
        assert_eq!(log_cwd(&d.join("gone.jsonl")), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn copies_drawable_files_and_null_for_the_rest() {
        let d = tmp("lines");
        let proj = d.join("proj");
        std::fs::create_dir_all(proj.join("shots")).unwrap();
        std::fs::write(proj.join("shots/a.png"), png(64, 32)).unwrap();
        std::fs::write(d.join("b.svg"), r#"<svg width="10" height="20"></svg>"#).unwrap();
        std::fs::write(d.join("fake.png"), b"text").unwrap();
        std::fs::write(d.join("c.gif"), b"GIF89a\x01\x00\x01\x00").unwrap();
        let log = d.join("s.jsonl");
        std::fs::write(&log, format!("{{\"cwd\":{}}}\n", json!(proj.to_string_lossy()))).unwrap();
        let cache = d.join("cache");
        let srcs: Vec<String> = ["shots/a.png", &d.join("b.svg").to_string_lossy(), "https://x.dev/a.png", "gone.png", &d.join("fake.png").to_string_lossy(), &d.join("c.gif").to_string_lossy()]
            .iter().map(|s| s.to_string()).collect();
        let now = SystemTime::now();
        let out = lines(&log, &srcs, &cache, None, now).unwrap();
        assert_eq!(out.len(), 6);
        assert!(out.iter().enumerate().all(|(i, l)| l["type"] == "agent-image" && l["index"] == i));
        let p = PathBuf::from(out[0]["path"].as_str().unwrap());
        assert!(p.starts_with(&cache) && p.file_name().unwrap().to_string_lossy().starts_with("img-"));
        assert_eq!(std::fs::read(&p).unwrap(), png(64, 32));
        assert_eq!((out[0]["width"].as_f64(), out[0]["height"].as_f64()), (Some(64.0), Some(32.0)));
        assert_eq!((out[1]["width"].as_f64(), out[1]["height"].as_f64()), (Some(10.0), Some(20.0)));
        for l in &out[2..] {
            assert!(l["path"].is_null(), "{l}");
        }
        // An old copy goes; the copy that is asked for again stays.
        let old = cache.join("img-00ff00ff00ff00ff.png");
        std::fs::write(&old, b"x").unwrap();
        let later = now + KEEP + Duration::from_secs(60);
        std::fs::File::options().write(true).open(&old).unwrap().set_modified(now).unwrap();
        let again = lines(&log, &srcs[..1], &cache, None, later).unwrap();
        assert_eq!(again[0]["path"], out[0]["path"]);
        assert!(!old.exists() && p.exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}
