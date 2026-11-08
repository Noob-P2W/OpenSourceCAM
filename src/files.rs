//! Protect source files when exporting a different representation of their data.
use std::path::Path;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
struct PendingFile {
    path: PathBuf,
    file: Option<File>,
    committed: bool,
}
impl Drop for PendingFile {
    fn drop(&mut self) {
        self.file.take();
        if !self.committed {
            let _ = fs::remove_file(&self.path);
        }
    }
}
/// Replace a file only after the new contents have been written and synced.
/// A same-directory temporary file keeps failed writes from truncating saved jobs.
pub fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    write_atomic_with(path, |file| file.write_all(data))
}
pub fn write_atomic_with(
    path: &Path,
    write: impl FnOnce(&mut File) -> io::Result<()>,
) -> io::Result<()> {
    // Follow an existing destination symlink without replacing the symlink itself.
    let resolved = if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        fs::canonicalize(path)?
    } else {
        path.to_path_buf()
    };
    let path = resolved.as_path();
    let existing = match fs::metadata(path) {
        Ok(m) => {
            if !m.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Save destination must be a regular file",
                ));
            }
            if m.permissions().readonly() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Save destination is read-only",
                ));
            }
            Some(m.permissions())
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    path.file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Missing save filename"))?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut pending = None;
    for _ in 0..32 {
        let name = format!(
            ".opensourcecam-save-{}-{stamp}-{}.tmp",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        );
        let temp = parent.join(name);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&temp) {
            Ok(file) => {
                pending = Some(PendingFile {
                    path: temp,
                    file: Some(file),
                    committed: false,
                });
                break;
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
    }
    let mut pending = pending.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Cannot reserve an atomic save file",
        )
    })?;
    let file = pending.file.as_mut().unwrap();
    write(file)?;
    if let Some(permissions) = existing {
        file.set_permissions(permissions)?;
    }
    file.sync_all()?;
    pending.file.take();
    fs::rename(&pending.path, path)?;
    pending.committed = true;
    Ok(())
}

pub fn ensure_distinct(input: &Path, output: &Path) -> Result<(), String> {
    if input == output {
        return Err("Input and output must refer to different files".into());
    }
    match std::fs::symlink_metadata(output) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("Cannot inspect output file: {error}")),
        Ok(_) => {}
    }
    // Canonical names resolve symlinks but cannot detect hard links. Compare
    // filesystem identities on both Windows and Unix, and fail closed on errors.
    if same_file::is_same_file(input, output)
        .map_err(|e| format!("Cannot compare input/output files: {e}"))?
    {
        return Err("Input and output must refer to different files".into());
    }
    Ok(())
}
