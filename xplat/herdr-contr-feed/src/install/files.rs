//! The file steps of `install`: hashes, the atomic write, relative paths and the walk of a folder.

use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(feature = "herdr-mod")]
pub(super) fn sha_file(p: &Path) -> Option<String> {
    fs::read(p).ok().map(|b| sha(&b))
}

pub(super) fn stop(lines: &[String]) -> i32 {
    for l in lines {
        eprintln!("{l}");
    }
    1
}

/// Writes through a temp file beside `dest` and a rename, so a reader never sees half a file.
/// When `dest` is a symlink (a keys.json kept in a dotfiles folder), the file it points to is written,
/// in that file's own folder, and the link stays.
pub(super) fn put(dest: &Path, bytes: &[u8], exe: bool) -> std::io::Result<()> {
    let real;
    let dest = if fs::symlink_metadata(dest).is_ok_and(|m| m.file_type().is_symlink()) {
        // A link to a file that is not there yet: create it where the link points.
        real = match fs::canonicalize(dest) {
            Ok(p) => p,
            Err(_) => dest.parent().unwrap_or(Path::new(".")).join(fs::read_link(dest)?),
        };
        real.as_path()
    } else {
        dest
    };
    if let Some(d) = dest.parent() {
        fs::create_dir_all(d)?;
    }
    let mut tmp = dest.as_os_str().to_owned();
    tmp.push(".herdr-contr-new");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, bytes)?;
    #[cfg(unix)]
    if exe {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755))?;
    }
    #[cfg(not(unix))]
    let _ = exe;
    fs::rename(&tmp, dest)
}

pub(super) fn rel_of(p: &Path, base: &Path) -> String {
    p.strip_prefix(base).unwrap_or(p).components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/")
}

pub(super) fn walk_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk_files(&p, out);
        } else {
            out.push(p);
        }
    }
}
