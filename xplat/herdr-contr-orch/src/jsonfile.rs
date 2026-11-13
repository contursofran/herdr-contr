//! Atomic file writes: a temporary file in the same folder, then a rename, so a reader never sees half a file.
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Writes `data` to `path` with mode `perm` (Unix). The folder is made when it is missing.
pub fn write(path: &Path, data: &[u8], perm: u32) -> io::Result<()> {
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    let base = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let seed = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    for i in 0..100u128 {
        let tmp = dir.join(format!(".{base}.{}-{:x}.tmp", std::process::id(), seed + i));
        let mut opts = OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(perm);
        }
        #[cfg(not(unix))]
        let _ = perm;
        let mut f = match opts.open(&tmp) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        };
        let done = f.write_all(data).and_then(|_| f.sync_all()).and_then(|_| fs::rename(&tmp, path));
        if done.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        return done;
    }
    Err(io::Error::other(format!("temp file in {}: no free name", dir.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_makes_the_folder_and_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a/b/state.json");
        write(&path, b"one", 0o600).unwrap();
        write(&path, b"two", 0o600).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"two");
        let names: Vec<_> = fs::read_dir(path.parent().unwrap()).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names, vec![std::ffi::OsString::from("state.json")]);
    }

    #[cfg(unix)]
    #[test]
    fn keeps_the_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("owner.json");
        write(&path, b"{}", 0o600).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn a_folder_that_cannot_be_made_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("file");
        fs::write(&file, b"x").unwrap();
        assert!(write(&file.join("under/state.json"), b"x", 0o644).is_err());
    }
}
