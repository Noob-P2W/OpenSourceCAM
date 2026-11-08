use opensourcecam::{
    cam,
    import::{self, Units},
    model::{Project, Strategy},
    operation::Raster,
    post,
    workspace::{Frame, ModelKind, WorkspaceModel},
};
use std::path::Path;

fn job() -> Project {
    let mut p = Project::default();
    p.ensure_tool_library();
    p.operations.truncate(1);
    p
}

#[test]
fn operation_overrides_allowances_and_clearances_drive_real_moves_and_post() {
    let mut p = job();
    let op = &mut p.operations[0];
    op.depth = 3.0;
    op.settings.feed = Some(250.0);
    op.settings.plunge = Some(70.0);
    op.settings.spindle_rpm = Some(6000);
    op.settings.step_down = Some(0.75);
    op.settings.clearance = Some(9.0);
    op.settings.radial_allowance = 0.5;
    op.settings.axial_allowance = 0.5;
    let path = cam::generate(&p).unwrap();
    assert_eq!(p.travel_clearance(), 9.0);
    assert_eq!(
        path.moves
            .iter()
            .map(|m| m.to.z)
            .fold(f64::INFINITY, f64::min),
        -2.5
    );
    let cuts: Vec<_> = path
        .moves
        .iter()
        .filter(|m| m.motion == cam::Motion::Cut)
        .collect();
    assert!(cuts.iter().all(|m| m.feed == 250.0
        && m.to.x >= 23.5
        && m.to.x <= 76.5
        && m.to.y >= 21.5
        && m.to.y <= 54.5));
    let code = post::gcode(&p).unwrap();
    assert!(code.contains("G0 Z9.0000"));
    assert!(code.contains("M3 S6000"));
    assert!(code.contains("F70.0000"));
}

#[test]
fn one_way_rows_spring_passes_and_peck_retracts_are_effective() {
    let mut p = job();
    p.operations[0].settings.raster = Raster::NegativeX;
    let first = cam::generate(&p).unwrap();
    let row = first
        .moves
        .iter()
        .find(|m| m.motion == cam::Motion::Cut)
        .unwrap();
    assert!(row.to.x < row.from.x);
    p.operations[0].settings.spring_passes = 2;
    p.operations[0].settings.finish_feed = Some(150.0);
    let extra = cam::generate(&p).unwrap();
    assert!(extra.moves.len() > first.moves.len());
    assert_eq!(
        extra
            .moves
            .iter()
            .filter(|m| m.motion == cam::Motion::Cut && m.feed == 150.0)
            .count(),
        8
    );
    p.operations[0].strategy = Strategy::Drill;
    p.operations[0].settings.spring_passes = 0;
    p.operations[0].settings.retract = Some(2.0);
    let drill = cam::generate(&p).unwrap();
    assert!(drill
        .moves
        .iter()
        .any(|m| m.motion == cam::Motion::Rapid && m.to.z == 2.0));
    assert_eq!(drill.moves.last().unwrap().to.z, p.clearance);
}

#[test]
fn machine_limits_reject_overrides_and_axis_specific_feed_excesses() {
    let mut p = job();
    p.machine.as_mut().unwrap().configuration.axis_feed[0] = 100.0;
    assert!(cam::generate(&p).unwrap_err().contains("X axis"));
    p.machine.as_mut().unwrap().configuration.axis_feed[0] = 1e6;
    p.machine.as_mut().unwrap().configuration.minimum_rpm = 13000;
    assert!(post::gcode(&p).is_err());
    p.machine.as_mut().unwrap().configuration.minimum_rpm = 0;
    p.machine.as_mut().unwrap().configuration.reverse_spindle = false;
    p.tool_library.tools[0].clockwise = false;
    assert!(post::gcode(&p).unwrap_err().contains("reverse"));
    p.tool_library.tools[0].clockwise = true;
    p.operations[0].settings.spindle_rpm = Some(200000);
    assert!(post::gcode(&p).is_err());
    p.operations[0].settings.spindle_rpm = None;
    p.operations[0].settings.retract = Some(100.0);
    assert!(cam::generate(&p).unwrap_err().contains("Retract"));
}

#[test]
fn spindle_overrides_keep_chip_load_when_feed_is_derived() {
    let mut p = job();
    let preset = &mut p.tool_library.tools[0].presets[0];
    preset.feed_mode = opensourcecam::tool_library::FeedMode::PerTooth;
    let before = p.operation_tool(&p.operations[0]).unwrap();
    p.operations[0].settings.spindle_rpm = Some(before.spindle_rpm / 2);
    assert_eq!(
        p.operation_tool(&p.operations[0]).unwrap().feed,
        before.feed / 2.0
    );
    p.operations[0].settings.feed = Some(200.0);
    assert_eq!(p.operation_tool(&p.operations[0]).unwrap().feed, 200.0);
}

#[test]
fn workspace_transform_frames_and_project_roundtrip_preserve_the_part() {
    let mut p = job();
    let part =
        import::import_file(Path::new("examples/models/mount.stl"), Units::Millimetres).unwrap();
    p.part = Some(part.clone());
    let mut vise = WorkspaceModel::new(1, ModelKind::Vise, part);
    vise.rotation = [0.0, 0.0, 90.0];
    vise.part.offset = [10.0, 20.0, 30.0];
    let point = vise.point([1.0, 0.0, 0.0], p.machine.as_ref());
    assert!((point[0] - 10.0).abs() < 1e-10);
    assert_eq!(point[1], 21.0);
    vise.frame = Frame::Machine;
    p.machine.as_mut().unwrap().work_origin = [2.0, 3.0, 4.0];
    assert_eq!(vise.point([0.0; 3], p.machine.as_ref()), [8.0, 17.0, 26.0]);
    p.workspace.push(vise);
    let before = p.part.clone();
    cam::validate_setup(&p).unwrap();
    let file = std::env::temp_dir().join(format!("osc-scene-{}.json", std::process::id()));
    p.save(&file).unwrap();
    assert_eq!(Project::load(&file).unwrap(), p);
    std::fs::remove_file(file).unwrap();
    assert_eq!(p.part, before);
    let mut duplicate = p.workspace[0].clone();
    duplicate.name = "Second".into();
    p.workspace.push(duplicate);
    assert!(cam::validate_setup(&p).unwrap_err().contains("Duplicate"));
}
