use opensourcecam::{model::Project, post};
use std::process::Command;

#[test]
fn cli_shared_export_matches_postprocessor() {
    let file = format!("{}/examples/tray.cam.json", env!("CARGO_MANIFEST_DIR"));
    let result = Command::new(env!("CARGO_BIN_EXE_opensourcecam"))
        .args(["--command", "export", &file])
        .output()
        .unwrap();
    assert!(result.status.success(), "{:?}", result.stderr);
    let gcode: String = serde_json::from_slice(&result.stdout).unwrap();
    let project = Project::load(std::path::Path::new(&file)).unwrap();
    assert_eq!(gcode, post::gcode(&project).unwrap());
}

#[test]
fn unknown_cli_command_does_not_emit_a_result() {
    let result = Command::new(env!("CARGO_BIN_EXE_opensourcecam"))
        .args(["--command", "unknown", "examples/tray.cam.json"])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    assert!(String::from_utf8_lossy(&result.stderr).contains("Unknown application command"));
}
