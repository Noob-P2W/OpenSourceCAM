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

#[test]
fn issues_keep_colliding_moves_inspectable_and_export_checked() {
    use opensourcecam::{
        commands::{self, Command},
        import::{self, Units},
        model::Strategy,
        workspace::{ModelKind, WorkspaceModel},
    };
    let mut p = Project::default();
    p.workspace.clear();
    p.operations.truncate(1);
    let op = &mut p.operations[0];
    op.strategy = Strategy::OutsideProfile;
    op.x = 20.;
    op.y = 1.;
    op.width = 20.;
    op.height = 10.;
    let fixture = import::import_file(
        std::path::Path::new("examples/models/example-vise.obj"),
        Units::Millimetres,
    )
    .unwrap();
    p.workspace
        .push(WorkspaceModel::new(3, ModelKind::Fixture, fixture));
    let review = commands::execute_project(&p, Command::Review).unwrap();
    let moves = review["moves"].as_array().unwrap();
    assert!(!moves.is_empty());
    let issue = review["issues"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["title"] == "Fixture contact")
        .unwrap();
    let location = &issue["location"];
    let ordinal = location["move_index"].as_u64().unwrap() as usize;
    let time = location["seconds"].as_f64().unwrap();
    assert_eq!(moves[ordinal]["operation"], location["operation"]);
    assert!(time >= moves[ordinal]["start_seconds"].as_f64().unwrap());
    assert!(time <= moves[ordinal]["end_seconds"].as_f64().unwrap());
    assert_eq!(location["contact"]["fixture"], 3);
    assert!(location["contact"]["patch"].as_array().unwrap().len() > 2);
    assert!(post::gcode(&p).unwrap_err().contains("contacts fixture"));
}

#[test]
fn machine_limit_issues_locate_playback_and_the_machine_field() {
    use opensourcecam::commands::{self, Command};
    let mut p = Project::default();
    p.workspace.clear();
    let machine = p.machine.as_mut().unwrap();
    machine.enforce_limits = true;
    machine.axis_min = [-500., -500., -200.];
    machine.axis_max[0] = 15.;
    let preview = commands::execute_project(&p, Command::Review).unwrap();
    assert!(!preview["moves"].as_array().unwrap().is_empty());
    let issue = preview["issues"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["title"] == "Machine travel limit")
        .unwrap();
    assert_eq!(issue["target"]["label"], "axis max X · mm");
    assert!(issue["location"]["seconds"].is_number());
    assert!(post::gcode(&p).unwrap_err().contains("travel exceeds"));
}

#[test]
fn independent_invalid_operations_get_settings_targets() {
    use opensourcecam::commands::{self, Command};
    let mut p = Project::default();
    p.operations[0].depth = -2.;
    p.operations[1].depth = -3.;
    let results = commands::execute_project(&p, Command::Issues).unwrap();
    let results = results.as_array().unwrap();
    assert_eq!(results.len(), 2);
    for (i, issue) in results.iter().enumerate() {
        assert_eq!(issue["target"]["index"], i);
        assert_eq!(issue["target"]["field"], "depth");
    }
}
