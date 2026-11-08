use opensourcecam::files::ensure_distinct;
use std::{
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "osc-files-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn output_guard_detects_identity_and_allows_distinct_files() {
    let dir = Directory::new();
    let source = dir.0.join("source.json");
    let alias = dir.0.join("hardlink.nc");
    let other = dir.0.join("other.nc");
    assert!(ensure_distinct(&other, &other).is_err());
    std::fs::write(&source, b"preserve me").unwrap();
    std::fs::hard_link(&source, &alias).unwrap();
    assert!(ensure_distinct(&source, &source).is_err());
    assert!(ensure_distinct(&source, &alias).is_err());
    assert!(ensure_distinct(&source, &other).is_ok());
    std::fs::write(&other, b"different").unwrap();
    assert!(ensure_distinct(&source, &other).is_ok());
    assert!(ensure_distinct(&dir.0.join("missing"), &other).is_err());
}

#[test]
fn cli_exports_and_imports_never_overwrite_hardlinked_sources() {
    let dir = Directory::new();
    let source = dir.0.join("source.cam.json");
    let alias = dir.0.join("output.nc");
    opensourcecam::model::Project::default()
        .save(&source)
        .unwrap();
    std::fs::hard_link(&source, &alias).unwrap();
    let before = std::fs::read(&source).unwrap();
    for args in [
        vec!["--export"],
        vec!["--export-post", "mach3"],
        vec!["--plan-stock"],
        vec!["--simulate-stock"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_opensourcecam"))
            .args(args)
            .arg(&source)
            .arg(&alias)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("different files"));
        assert_eq!(std::fs::read(&source).unwrap(), before);
    }
    let model = dir.0.join("part.stl");
    let model_alias = dir.0.join("imported.cam.json");
    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/models/mount.stl");
    std::fs::copy(fixture, &model).unwrap();
    std::fs::hard_link(&model, &model_alias).unwrap();
    let before = std::fs::read(&model).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_opensourcecam"))
        .arg("--import")
        .arg(&model)
        .arg(&model_alias)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(std::fs::read(&model).unwrap(), before);
}

#[test]
fn atomic_saves_preserve_existing_files_on_partial_write_errors_and_panics() {
    use std::io::Write;
    let dir = Directory::new();
    let path = dir.0.join("saved.json");
    std::fs::write(&path, b"original contents").unwrap();
    let result = opensourcecam::files::write_atomic_with(&path, |file| {
        file.write_all(b"partial update")?;
        Err(std::io::Error::other("injected disk/write failure"))
    });
    assert!(result.is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"original contents");
    let result = std::panic::catch_unwind(|| {
        opensourcecam::files::write_atomic_with(&path, |file| {
            file.write_all(b"unfinished")?;
            panic!("injected interrupted writer");
        })
    });
    assert!(result.is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"original contents");
    assert_eq!(
        std::fs::read_dir(&dir.0).unwrap().count(),
        1,
        "Temporary files must be removed after errors and unwinding"
    );
    opensourcecam::files::write_atomic(&path, b"complete replacement").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"complete replacement");
    let missing = dir.0.join("never-created.nc");
    assert!(
        opensourcecam::files::write_atomic_with(&missing, |_| Err(std::io::Error::other(
            "failure"
        )))
        .is_err()
    );
    assert!(!missing.exists());
}
#[test]
fn atomic_saves_respect_readonly_destinations() {
    let dir = Directory::new();
    let path = dir.0.join("readonly.json");
    std::fs::write(&path, b"preserved").unwrap();
    let original = std::fs::metadata(&path).unwrap().permissions();
    let mut readonly = original.clone();
    readonly.set_readonly(true);
    std::fs::set_permissions(&path, readonly).unwrap();
    let result = opensourcecam::files::write_atomic(&path, b"replacement");
    std::fs::set_permissions(&path, original).unwrap();
    assert_eq!(
        result.unwrap_err().kind(),
        std::io::ErrorKind::PermissionDenied
    );
    assert_eq!(std::fs::read(path).unwrap(), b"preserved");
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 1);
}
#[cfg(unix)]
#[test]
fn atomic_saves_preserve_symlinks_and_private_file_modes() {
    use std::os::unix::{fs::symlink, fs::PermissionsExt};
    let dir = Directory::new();
    let target = dir.0.join("private.json");
    let link = dir.0.join("link.json");
    std::fs::write(&target, b"old").unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
    symlink(&target, &link).unwrap();
    opensourcecam::files::write_atomic(&link, b"new").unwrap();
    assert!(std::fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(
        std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(std::fs::read(target).unwrap(), b"new");
}

#[test]
fn atomic_saves_support_long_filenames_without_lengthening_the_temp_basename() {
    let dir = Directory::new();
    let path = dir.0.join(format!("{}.json", "p".repeat(180)));
    opensourcecam::files::write_atomic(&path, b"new").unwrap();
    opensourcecam::files::write_atomic(&path, b"updated").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"updated");
}
