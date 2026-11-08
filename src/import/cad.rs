use super::{mesh, Mesh, SourceFormat, MAX_IMPORT_BYTES};
use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub fn executable() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(root) = std::env::var_os("OPENSOURCECAM_CAD_RUNTIME") {
        candidates.push(PathBuf::from(root));
    }
    if let Ok(exe) = std::env::current_exe() {
        for parent in exe.ancestors().skip(1).take(4) {
            candidates.push(parent.join("cad-runtime"));
        }
    }
    candidates
        .into_iter()
        .map(|root| interpreter(&root))
        .find(|python| python.is_file())
}

fn interpreter(root: &Path) -> PathBuf {
    if cfg!(windows) {
        root.join("python/python.exe")
    } else {
        root.join("python/bin/python3.12")
    }
}

struct Job {
    root: PathBuf,
}
impl Job {
    fn new() -> Result<Self, String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("opensourcecam-cad-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&root).map_err(|e| e.to_string())?;
        Ok(Self { root })
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        // Only our fixed job files; never recursively delete a computed directory.
        for name in [
            "source.step",
            "source.iges",
            "bridge.py",
            "request.json",
            "result.stl",
            "error.txt",
            "stderr.txt",
        ] {
            let _ = std::fs::remove_file(self.root.join(name));
        }
        let _ = std::fs::remove_dir(&self.root);
    }
}

pub fn convert(data: &[u8], format: SourceFormat, cancel: &AtomicBool) -> Result<Mesh, String> {
    let python = executable().ok_or("Bundled CAD runtime is missing. Use the full OpenSourceCAM package, or run scripts/bundle-cad.py when building from source.")?;
    let runtime = if cfg!(windows) {
        python.parent().and_then(Path::parent)
    } else {
        python
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
    }
    .ok_or("Invalid CAD runtime layout")?;
    let job = Job::new()?;
    let extension = if format == SourceFormat::Step {
        "step"
    } else {
        "iges"
    };
    let source = job.root.join(format!("source.{extension}"));
    std::fs::write(&source, data).map_err(|e| e.to_string())?;
    std::fs::write(
        job.root.join("bridge.py"),
        include_str!("../../scripts/cad_bridge.py"),
    )
    .map_err(|e| e.to_string())?;
    let config = serde_json::json!({
        "source": source, "format": extension, "output": job.root.join("result.stl"),
        "error": job.root.join("error.txt"), "max_triangles": super::MAX_TRIANGLES,
    });
    std::fs::write(
        job.root.join("request.json"),
        serde_json::to_vec(&config).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let stderr = std::fs::File::create(job.root.join("stderr.txt")).map_err(|e| e.to_string())?;
    let mut command = Command::new(&python);
    command
        .arg("-I")
        .arg(job.root.join("bridge.py"))
        .arg(job.root.join("request.json"))
        .arg(runtime.join("packages"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(stderr);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW: CAD worker has no console UI.
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("Cannot start bundled CAD importer: {e}"))?;
    let start = Instant::now();
    loop {
        if cancel.load(Ordering::Relaxed) || start.elapsed() > Duration::from_secs(120) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(if cancel.load(Ordering::Relaxed) {
                "Import cancelled"
            } else {
                "CAD import timed out after 120 seconds"
            }
            .into());
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    let message = bounded_text(&job.root.join("error.txt"))
                        .or_else(|| bounded_text(&job.root.join("stderr.txt")))
                        .unwrap_or_else(|| status.to_string());
                    return Err(format!("CAD import failed: {message}"));
                }
                break;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(40)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error.to_string());
            }
        }
    }
    let file = std::fs::File::open(job.root.join("result.stl"))
        .map_err(|e| format!("CAD importer returned no mesh: {e}"))?;
    let mut result = Vec::new();
    file.take(MAX_IMPORT_BYTES + 1)
        .read_to_end(&mut result)
        .map_err(|e| e.to_string())?;
    if result.len() as u64 > MAX_IMPORT_BYTES {
        return Err("CAD mesh exceeds the import limit".into());
    }
    mesh::stl(&result)
}

fn bounded_text(path: &Path) -> Option<String> {
    let mut result = String::new();
    std::fs::File::open(path)
        .ok()?
        .take(4000)
        .read_to_string(&mut result)
        .ok()?;
    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}
