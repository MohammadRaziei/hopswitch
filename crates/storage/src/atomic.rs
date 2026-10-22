//! Crash-safe file replacement.

use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

/// Replaces the file at `path` with `bytes` so that a crash or power loss leaves
/// either the complete old content or the complete new content, never a mix.
///
/// Writes a sibling temp file (`.<name>.tmp`), flushes it to disk, renames it over
/// the target, then flushes the directory entry (on Unix). Missing parent
/// directories are created. Assumes a single writer per path.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let file_name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?;
    let parent = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    fs::create_dir_all(parent)?;

    let mut tmp_name = OsString::from(".");
    tmp_name.push(file_name);
    tmp_name.push(".tmp");
    let tmp = parent.join(tmp_name);

    let written = (|| {
        let mut file = File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if let Err(error) = written {
        let _ = fs::remove_file(&tmp); // best effort; the original error matters more
        return Err(error);
    }

    sync_directory(parent)
}

#[cfg(unix)]
fn sync_directory(dir: &Path) -> io::Result<()> {
    File::open(dir)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_dir: &Path) -> io::Result<()> {
    // Windows cannot open a directory like a file; NTFS journals the rename.
    Ok(())
}
