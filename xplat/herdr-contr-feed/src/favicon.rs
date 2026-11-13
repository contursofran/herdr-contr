//! `favicon`: a copy of a project's favicon under the app home, so that the sidebar can draw it. `image()` reads
//! only files under the app home (`gpui-kit.d.ts`: "from the application's own directory"); a project's folder is
//! not there. The copy's name comes from the source file's path, and the copy is made again when the source is newer.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// The extensions `image()` draws that a favicon has.
const EXTS: [&str; 4] = ["png", "svg", "jpg", "jpeg"];

/// The copy's file name: a digest of the source's path and the source's extension. None for another format.
pub fn name(src: &Path) -> Option<String> {
    let ext = src.extension()?.to_str()?.to_ascii_lowercase();
    if !EXTS.contains(&ext.as_str()) {
        return None;
    }
    let d = Sha256::digest(src.to_string_lossy().as_bytes());
    let hex: String = d.iter().take(8).map(|b| format!("{b:02x}")).collect();
    Some(format!("fav-{hex}.{ext}"))
}

/// Copies `src` into `dir` when the copy is not there or is older than `src`. The copy's path, or None when `src`
/// is not a file `image()` draws.
pub fn copy(src: &Path, dir: &Path) -> std::io::Result<Option<PathBuf>> {
    let Some(n) = name(src) else { return Ok(None) };
    let meta = match std::fs::metadata(src) {
        Ok(m) if m.is_file() => m,
        _ => return Ok(None),
    };
    let dst = dir.join(n);
    let fresh = match std::fs::metadata(&dst) {
        Ok(d) => match (d.modified(), meta.modified()) {
            (Ok(c), Ok(s)) => c >= s && d.len() == meta.len(),
            _ => false,
        },
        Err(_) => false,
    };
    if !fresh {
        std::fs::create_dir_all(dir)?;
        let tmp = dir.join(format!(".{}.tmp", std::process::id()));
        std::fs::copy(src, &tmp)?;
        std::fs::rename(&tmp, &dst)?;
    }
    Ok(Some(dst))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("hcf-favicon-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn names() {
        let a = name(Path::new("/p/a/public/apple-touch-icon.png")).unwrap();
        assert!(a.starts_with("fav-") && a.ends_with(".png") && a.len() == 4 + 16 + 4);
        assert_eq!(Some(a.clone()), name(Path::new("/p/a/public/apple-touch-icon.png")));
        assert_ne!(Some(a), name(Path::new("/p/b/public/apple-touch-icon.png")));
        assert!(name(Path::new("/p/a/icon.SVG")).unwrap().ends_with(".svg"));
        assert_eq!(name(Path::new("/p/a/favicon.ico")), None);
        assert_eq!(name(Path::new("/p/a/favicon")), None);
    }

    #[test]
    fn copies_once_and_again_when_newer() {
        let d = tmp("copy");
        let src = d.join("icon.png");
        let cache = d.join("cache");
        std::fs::write(&src, b"one").unwrap();
        let p = copy(&src, &cache).unwrap().unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"one");
        // Not newer: kept as it is.
        std::fs::write(&p, b"own").unwrap();
        let later = SystemTime::now() + Duration::from_secs(60);
        std::fs::File::options().write(true).open(&p).unwrap().set_modified(later).unwrap();
        assert_eq!(copy(&src, &cache).unwrap().unwrap(), p);
        assert_eq!(std::fs::read(&p).unwrap(), b"own");
        // The source is newer: copied again.
        std::fs::write(&src, b"two").unwrap();
        std::fs::File::options().write(true).open(&src).unwrap().set_modified(later + Duration::from_secs(60)).unwrap();
        copy(&src, &cache).unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"two");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn no_file_or_other_format() {
        let d = tmp("none");
        assert_eq!(copy(&d.join("gone.png"), &d.join("c")).unwrap(), None);
        std::fs::write(d.join("favicon.ico"), b"x").unwrap();
        assert_eq!(copy(&d.join("favicon.ico"), &d.join("c")).unwrap(), None);
        std::fs::create_dir_all(d.join("dir.png")).unwrap();
        assert_eq!(copy(&d.join("dir.png"), &d.join("c")).unwrap(), None);
        assert!(!d.join("c").exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}
