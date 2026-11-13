//! `pictures`: the pictures of a chat row as files that the mod can draw (`image()` reads PNG, JPEG and SVG under
//! the app home). The mod gives the log and the row's `pics` (`feed --pictures`); each picture is read back from the
//! log (`log_image::read` checks its fingerprint) and written once to the folder, named by its fingerprint.

use serde_json::{json, Value};
use std::path::Path;
use std::time::{Duration, SystemTime};

/// Files in the folder that were not written or asked for in this time are removed when a new one is written.
pub const KEEP: Duration = Duration::from_secs(7 * 24 * 3600);
/// At most this many pictures in one ask.
pub const MAX: usize = 32;

/// A cached picture's file name: its fingerprint and the format's extension. None for a format `image()` does not
/// draw (WebP, GIF).
pub fn file_name(p: &Value) -> Option<String> {
    let ext = match p["format"].as_str()? {
        "png" => "png",
        "jpeg" => "jpg",
        _ => return None,
    };
    let d = p["digest"].as_str().filter(|d| d.len() == 16 && d.bytes().all(|b| b.is_ascii_hexdigit()))?;
    Some(format!("pic-{d}.{ext}"))
}

fn is_pic(name: &str) -> bool {
    name.starts_with("pic-") && (name.ends_with(".png") || name.ends_with(".jpg"))
}

/// Removes the pictures that are older than `KEEP`.
fn prune(dir: &Path, now: SystemTime) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        if !is_pic(&e.file_name().to_string_lossy()) {
            continue;
        }
        if let Ok(m) = e.metadata().and_then(|m| m.modified()) {
            if now.duration_since(m).is_ok_and(|d| d > KEEP) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

/// One `picture` line for each entry of `pics`, in order: `path` (the file), `width` and `height` (px); `path` null
/// when the picture is not in the log any more, or its format cannot be drawn.
pub fn lines(log: &Path, pics: &[Value], dir: &Path, now: SystemTime) -> std::io::Result<Vec<Value>> {
    std::fs::create_dir_all(dir)?;
    let mut pruned = false;
    let mut out = Vec::new();
    for (i, p) in pics.iter().enumerate() {
        let none = json!({"type": "picture", "index": i, "path": null});
        let (Some(name), Some(r)) = (file_name(p), crate::serve::hub::pic_ref(p)) else {
            out.push(none);
            continue;
        };
        let path = dir.join(&name);
        if path.is_file() {
            // Asked for again: it counts as new, so the prune keeps it.
            let _ = std::fs::File::options().write(true).open(&path).and_then(|f| f.set_modified(now));
        } else {
            let Some(bytes) = herdr_core::log_image::read(&r, log) else {
                out.push(none);
                continue;
            };
            if !pruned {
                prune(dir, now);
                pruned = true;
            }
            let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
            std::fs::write(&tmp, &bytes)?;
            std::fs::rename(&tmp, &path)?;
        }
        out.push(json!({"type": "picture", "index": i, "path": path.to_string_lossy(), "width": r.width, "height": r.height}));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        let p = |f: &str, d: &str| json!({"format": f, "digest": d});
        assert_eq!(file_name(&p("png", "00ff00ff00ff00ff")).as_deref(), Some("pic-00ff00ff00ff00ff.png"));
        assert_eq!(file_name(&p("jpeg", "00ff00ff00ff00ff")).as_deref(), Some("pic-00ff00ff00ff00ff.jpg"));
        assert_eq!(file_name(&p("webp", "00ff00ff00ff00ff")), None, "image() does not draw WebP");
        assert_eq!(file_name(&p("png", "../../etc/passwd")), None, "only a fingerprint names a file");
        assert!(is_pic("pic-00ff00ff00ff00ff.png") && !is_pic("notes.png") && !is_pic("paste-1.png"));
    }
}
