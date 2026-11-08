use opensourcecam::{
    cam,
    model::{Project, FORMAT_VERSION},
    post,
    process::Process,
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct File(PathBuf);
impl File {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "osc-process-{}-{}.json",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }
}
impl Drop for File {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn every_process_can_be_saved_and_reopened_with_its_identity() {
    assert_eq!(Process::ALL.len(), 9);
    assert_eq!(Process::ALL.last(), Some(&Process::Mixed));
    for process in Process::ALL {
        let project = Project::new_for_process(process);
        assert!(project.operations.is_empty());
        cam::validate_setup(&project).unwrap();
        let file = File::new();
        project.save(&file.0).unwrap();
        assert_eq!(Project::load(&file.0).unwrap(), project);
    }
}

#[test]
fn version_one_and_two_projects_default_to_milling() {
    for version in [1, 2] {
        let mut json = serde_json::to_value(Project::default()).unwrap();
        json["format_version"] = version.into();
        json.as_object_mut().unwrap().remove("process");
        let file = File::new();
        std::fs::write(&file.0, serde_json::to_vec(&json).unwrap()).unwrap();
        let project = Project::load(&file.0).unwrap();
        assert_eq!(project.format_version, FORMAT_VERSION);
        assert_eq!(project.process, Process::Milling);
        assert!(post::gcode(&project).is_ok());
    }
}

#[test]
fn planned_processes_never_export_milling_code() {
    for process in Process::ALL.into_iter().filter(|p| !p.supports_toolpaths()) {
        let project = Project::new_for_process(process);
        assert!(cam::generate(&project)
            .unwrap_err()
            .contains("not implemented"));
        assert!(post::gcode(&project).is_err());
        let mut invalid = project;
        invalid.operations = Project::default().operations;
        assert!(cam::validate_setup(&invalid).is_err());
        assert!(post::gcode(&invalid).is_err());
    }
}
