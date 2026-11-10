use opensourcecam::{
    cam, features,
    import::{self, Units},
    machining::{self, PlanOptions},
    model::{Project, Strategy},
    post, stock,
    tool_library::{LibraryTool, ToolKind},
};
use std::{path::Path, sync::atomic::AtomicBool};
fn imported() -> Project {
    let part =
        import::import_file(Path::new("examples/models/mount.stl"), Units::Millimetres).unwrap();
    let mut p = Project::from_part(part).unwrap();
    p.ensure_tool_library();
    p.tool_library.tools[0]
        .geometry
        .insert("diameter".into(), 3.0);
    p.tool_library.tools[0].name = "3 mm end mill".into();
    p
}

#[test]
fn stock_to_model_generates_real_facing_clearing_and_cavity_operations() {
    let original = imported();
    let before = original.clone();
    let plan =
        machining::from_stock(&original, &PlanOptions::default(), &AtomicBool::new(false)).unwrap();
    assert_eq!(original, before);
    assert_eq!(plan.project.operations.len(), 7);
    assert!(plan.report.exterior.is_some());
    assert!(plan.report.machining_error.is_none());
    assert!(plan
        .project
        .operations
        .iter()
        .all(|op| op.enabled && op.model_link.is_some()));
    assert_eq!(plan.project.operations[0].strategy, Strategy::Facing);
    assert_eq!(
        plan.project.operations.last().unwrap().strategy,
        Strategy::StockRough
    );
    assert_eq!(plan.project.stock.thickness, 13.5);
    let path = cam::generate(&plan.project).unwrap();
    assert!(path.moves.len() > 1000);
    for movement in &path.moves {
        let op = &plan.project.operations[movement.operation];
        if op.strategy == Strategy::CircularPocket && movement.motion == cam::Motion::Cut {
            assert!(
                (movement.to.x - op.x).hypot(movement.to.y - op.y) + 1.5 <= op.width / 2.0 + 0.002
            );
        }
    }
    let code = post::gcode(&plan.project).unwrap();
    assert!(code.contains("linked machining"));
    assert!(code.contains("G1"));
    let result = stock::simulate(&plan.project, &path, 1.0, 192, &AtomicBool::new(false)).unwrap();
    assert!(result.summary.removed_volume_mm3 > 1000.0);
    assert!(result.summary.sampled_gouge_mm3.unwrap() < 1.0);
    assert_eq!(result.summary.backing_mm, Some(0.5));
    assert!(
        result.summary.excess_material_mm3.unwrap() > 0.0,
        "Sharp pocket corners should retain cutter-radius material"
    );
    result.geometry.mesh.validate().unwrap();
}

#[test]
fn linked_operations_detect_changed_models_and_edited_geometry() {
    let mut p = machining::from_stock(&imported(), &Default::default(), &AtomicBool::new(false))
        .unwrap()
        .project;
    p.part.as_mut().unwrap().offset[0] += 1.0;
    assert!(post::gcode(&p).unwrap_err().contains("placement changed"));
    p.part.as_mut().unwrap().offset[0] -= 1.0;
    p.operations[1].x += 1.0;
    assert!(post::gcode(&p)
        .unwrap_err()
        .contains("linked geometry changed"));
    p.operations[1].x -= 1.0;
    p.operations[1].settings.feed = Some(200.0);
    assert!(post::gcode(&p).is_ok());
    let file = std::env::temp_dir().join(format!("osc-linked-{}.json", std::process::id()));
    p.save(&file).unwrap();
    let loaded = Project::load(&file).unwrap();
    assert_eq!(loaded, p);
    std::fs::remove_file(file).unwrap();
}

#[test]
fn model_linked_paths_require_defined_tool_reach_even_for_legacy_machine_settings() {
    let mut p = machining::from_stock(&imported(), &Default::default(), &AtomicBool::new(false))
        .unwrap()
        .project;
    p.machine.as_mut().unwrap().check_tool_reach = false;
    p.tool_library.tools[0]
        .geometry
        .insert("flute_length".into(), 2.0);
    assert!(post::gcode(&p)
        .unwrap_err()
        .contains("defined flute length or stickout"));
}

#[test]
fn unsupported_complete_parts_are_not_replaced_by_bounding_box_paths() {
    let part = import::import_file(
        Path::new("tests/fixtures/features-unsupported.stl"),
        Units::Millimetres,
    )
    .unwrap();
    let p = Project::from_part(part).unwrap();
    assert!(
        machining::from_stock(&p, &Default::default(), &AtomicBool::new(false))
            .unwrap_err()
            .contains("unsupported")
    );
    let mut p = imported();
    p.operations.push(Default::default());
    assert!(machining::from_stock(&p, &Default::default(), &AtomicBool::new(false)).is_err());
}

#[test]
fn fixture_checks_use_real_triangles_and_hidden_fixtures_are_still_checked() {
    use opensourcecam::workspace::{ModelKind, WorkspaceModel};
    let mut p = Project::default();
    p.ensure_tool_library();
    p.clearance = 10.0;
    p.operations.truncate(1);
    p.operations[0].strategy = Strategy::Drill;
    p.tool_library.tools[0] = LibraryTool::new(1, 1, ToolKind::Drill);
    p.operations[0].x = 45.0;
    p.operations[0].y = 30.0;
    let part = import::import_file(
        Path::new("examples/models/example-vise.obj"),
        Units::Millimetres,
    )
    .unwrap();
    p.workspace
        .push(WorkspaceModel::new(1, ModelKind::Vise, part));
    assert!(post::gcode(&p).is_ok(),"The opening between jaws must not collide just because their combined AABB overlaps the tool");
    p.operations[0].strategy = Strategy::OutsideProfile;
    p.tool_library.tools[0] = LibraryTool::new(1, 1, ToolKind::FlatEndMill);
    p.operations[0].x = 20.0;
    p.operations[0].y = 1.0;
    p.operations[0].width = 20.0;
    p.operations[0].height = 10.0;
    p.workspace[0].part.visible = false;
    assert!(post::gcode(&p).unwrap_err().contains("contacts fixture"));
}

#[test]
fn fixture_checks_include_a_library_holder_above_the_cutting_tool() {
    use opensourcecam::workspace::{ModelKind, WorkspaceModel};
    let mut p = Project::default();
    p.ensure_tool_library();
    p.operations.truncate(1);
    p.operations[0].strategy = Strategy::Drill;
    p.tool_library.tools[0] = LibraryTool::new(1, 1, ToolKind::Drill);
    p.operations[0].x = 45.0;
    p.operations[0].y = 30.0;
    p.clearance = 10.0;
    let part = import::import_file(
        Path::new("examples/models/example-vise.obj"),
        Units::Millimetres,
    )
    .unwrap();
    let mut fixture = WorkspaceModel::new(1, ModelKind::Fixture, part);
    fixture.part.offset = [70.0, 0.0, 70.0];
    p.workspace.push(fixture);
    assert!(post::gcode(&p).is_ok());
    let holder = LibraryTool::new(2, 2, ToolKind::Holder);
    p.tool_library.tools.push(holder);
    p.tool_library.tools[0].holder = Some(2);
    assert!(post::gcode(&p).unwrap_err().contains("contacts fixture"));
}

#[test]
fn stock_preview_uses_drill_point_geometry_and_playback_prefix() {
    let mut p = Project::default();
    p.operations.truncate(1);
    p.operations[0].strategy = Strategy::Drill;
    p.operations[0].x = 10.5;
    p.operations[0].y = 10.5;
    p.operations[0].depth = 4.0;
    p.stock.width = 20.0;
    p.stock.height = 20.0;
    p.tool_library.tools = vec![LibraryTool::new(1, 1, ToolKind::Drill)];
    p.active_tool = Some(1);
    p.active_preset = Some(1);
    let path = cam::generate(&p).unwrap();
    let empty = stock::simulate(&p, &path, 0.0, 20, &AtomicBool::new(false)).unwrap();
    assert_eq!(empty.summary.removed_volume_mm3, 0.0);
    let cut = stock::simulate(&p, &path, 1.0, 20, &AtomicBool::new(false)).unwrap();
    assert!((cut.heights[10 * 20 + 10] + 4.0).abs() < 1e-9);
    assert!(cut.heights[10 * 20 + 12] > -3.0);
    assert!(stock::simulate(&p, &path, 1.0, 1000, &AtomicBool::new(false)).is_err());
}

#[test]
fn full_model_recognition_accounts_for_all_extruded_surfaces() {
    let p = imported();
    let report = features::recognize(p.part.as_ref().unwrap()).unwrap();
    assert!(report.exterior.is_some(), "{report:?}");
    assert!(report.machining_error.is_none());
}

#[test]
fn drilling_recognized_through_holes_accounts_for_point_length() {
    let mut p = imported();
    let f = features::recognize(p.part.as_ref().unwrap())
        .unwrap()
        .features
        .into_iter()
        .find(|f| f.diameter.is_some())
        .unwrap();
    p.tool_library.tools = vec![LibraryTool::new(1, 1, ToolKind::Drill)];
    p.active_tool = Some(1);
    p.active_preset = Some(1);
    assert!(features::operation(&p, &f)
        .unwrap_err()
        .contains("additional blank thickness"));
    p.stock.thickness += 3.0;
    let op = features::operation(&p, &f).unwrap();
    let point = 3.0
        / (p.tool_library.tools[0]
            .dimension("point_angle")
            .to_radians()
            / 2.0)
            .tan();
    assert!((op.depth + f.bottom_z - point).abs() < 1e-9);
}

#[test]
fn facing_accepts_non_center_cutting_face_mills_and_obeys_one_way_rows() {
    use opensourcecam::operation::Raster;
    let mut p = imported();
    p.part.as_mut().unwrap().offset[2] -= 1.;
    p.stock.thickness += 1.;
    p.operations = vec![opensourcecam::model::Operation {
        strategy: Strategy::Facing,
        x: 0.0,
        y: 0.0,
        width: p.stock.width,
        height: p.stock.height,
        depth: 1.0,
        ..Default::default()
    }];
    let mut tool = LibraryTool::new(1, 1, ToolKind::FaceMill);
    tool.center_cutting = false;
    p.tool_library.tools = vec![tool];
    p.active_tool = Some(1);
    p.active_preset = Some(1);
    p.operations[0].settings.raster = Raster::NegativeX;
    p.operations[0].settings.spring_passes = 1;
    let path = cam::generate(&p).unwrap();
    for m in &path.moves {
        if m.motion == cam::Motion::Plunge {
            assert!(m.to.x > p.stock.width);
        }
        if m.motion == cam::Motion::Cut {
            assert!(m.to.x < m.from.x);
        }
    }
    p.operations[0].strategy = Strategy::Pocket;
    assert!(cam::generate(&p).is_err());
    assert!(
        machining::from_stock(&imported(), &Default::default(), &AtomicBool::new(true))
            .unwrap_err()
            .contains("cancelled")
    );
}

fn cube_fixture() -> Project {
    use opensourcecam::workspace::{ModelKind, WorkspaceModel};
    let mut p = imported();
    p.operations = vec![Default::default()];
    p.operations[0].tool_id = p.active_tool;
    let mut part = p.part.clone().unwrap();
    part.offset = [0.0; 3];
    let g = std::sync::Arc::make_mut(&mut part.geometry);
    let (a, b) = (-1000.0, 1000.0);
    g.mesh.vertices = vec![
        [a, a, a],
        [b, a, a],
        [b, b, a],
        [a, b, a],
        [a, a, b],
        [b, a, b],
        [b, b, b],
        [a, b, b],
    ];
    g.mesh.triangles = vec![
        [0, 2, 1],
        [0, 3, 2],
        [4, 5, 6],
        [4, 6, 7],
        [0, 1, 5],
        [0, 5, 4],
        [1, 2, 6],
        [1, 6, 5],
        [2, 3, 7],
        [2, 7, 6],
        [3, 0, 4],
        [3, 4, 7],
    ];
    g.cad_source = None;
    p.workspace = vec![WorkspaceModel::new(1, ModelKind::Fixture, part)];
    p
}
#[test]
fn enclosed_tools_block_nc_and_invalid_fixtures_fail_closed() {
    let p = cube_fixture();
    assert!(post::gcode(&p)
        .unwrap_err()
        .contains("contained inside fixture"));
    let mut broken = p.clone();
    std::sync::Arc::make_mut(&mut broken.workspace[0].part.geometry)
        .mesh
        .triangles
        .pop();
    assert!(post::gcode(&broken)
        .unwrap_err()
        .contains("closed, consistently oriented"));
    let mut reversed = p;
    for face in &mut std::sync::Arc::make_mut(&mut reversed.workspace[0].part.geometry)
        .mesh
        .triangles
    {
        face.swap(0, 1);
    }
    assert!(post::gcode(&reversed)
        .unwrap_err()
        .contains("contained inside fixture"));
}
#[test]
fn a_real_through_bore_in_a_connected_fixture_remains_clear() {
    use opensourcecam::{
        collision,
        workspace::{ModelKind, WorkspaceModel},
    };
    let mut p = imported();
    let mut fixture = p.part.take().unwrap();
    fixture.offset = [0.0; 3];
    p.operations = vec![Default::default()];
    p.workspace = vec![WorkspaceModel::new(1, ModelKind::Fixture, fixture)];
    let path = cam::Toolpath {
        moves: vec![cam::Move {
            from: cam::Point3 {
                x: 10.0,
                y: 10.0,
                z: -3.0,
            },
            to: cam::Point3 {
                x: 10.0,
                y: 10.0,
                z: -2.0,
            },
            feed: 120.0,
            motion: cam::Motion::Plunge,
            operation: 0,
        }],
        cut_length: 1.0,
        rapid_length: 0.0,
        estimated_seconds: 0.5,
    };
    collision::check(&p, &path).unwrap();
}
#[test]
fn drill_point_clearance_does_not_gouge_through_hole_but_real_floor_overcut_does() {
    let mut p = imported();
    p.stock.thickness += 3.0;
    let f = features::recognize(p.part.as_ref().unwrap())
        .unwrap()
        .features
        .into_iter()
        .find(|f| f.diameter.is_some())
        .unwrap();
    p.tool_library.tools = vec![LibraryTool::new(1, 1, ToolKind::Drill)];
    p.active_tool = Some(1);
    p.active_preset = Some(1);
    let mut op = features::operation(&p, &f).unwrap();
    op.enabled = true;
    p.operations = vec![op];
    let path = cam::generate(&p).unwrap();
    let result = stock::simulate(&p, &path, 1.0, 192, &AtomicBool::new(false)).unwrap();
    assert_eq!(result.summary.sampled_gouge_mm3, Some(0.0));
    let mut p = imported();
    let f = features::recognize(p.part.as_ref().unwrap())
        .unwrap()
        .features
        .into_iter()
        .find(|f| f.diameter.is_none())
        .unwrap();
    let mut op = features::operation(&p, &f).unwrap();
    op.model_link = None;
    op.depth += 1.0;
    op.enabled = true;
    p.operations = vec![op];
    let path = cam::generate(&p).unwrap();
    let result = stock::simulate(&p, &path, 1.0, 192, &AtomicBool::new(false)).unwrap();
    assert!(
        result.summary.sampled_gouge_mm3.unwrap() > 100.0,
        "Actual pocket floor material must still be counted as gouged"
    );
}

#[test]
fn clipped_targets_do_not_receive_misleading_zero_gouge_comparisons() {
    let mut p = imported();
    p.operations = vec![Default::default()];
    p.part.as_mut().unwrap().offset[0] += p.stock.width;
    let path = cam::generate(&p).unwrap();
    let result = stock::simulate(&p, &path, 1.0, 64, &AtomicBool::new(false)).unwrap();
    assert!(result.summary.sampled_gouge_mm3.is_none());
    assert!(result.summary.excess_material_mm3.is_none());
    assert!(result
        .summary
        .notes
        .iter()
        .any(|n| n.contains("fully inside the blank")));
}
#[test]
fn cli_preflight_and_export_both_block_enclosed_fixtures_and_preserve_nc_files() {
    let p = cube_fixture();
    let dir = std::env::temp_dir().join(format!("osc-preflight-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let input = dir.join("fixture.cam.json");
    let output = dir.join("program.nc");
    p.save(&input).unwrap();
    std::fs::write(&output, b"previous verified program").unwrap();
    for command in ["--validate", "--export"] {
        let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_opensourcecam"));
        cmd.arg(command).arg(&input);
        if command == "--export" {
            cmd.arg(&output);
        }
        let result = cmd.output().unwrap();
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("contained inside fixture"));
        assert_eq!(
            std::fs::read(&output).unwrap(),
            b"previous verified program"
        );
    }
    std::fs::remove_file(output).unwrap();
    std::fs::remove_file(input).unwrap();
    std::fs::remove_dir(dir).unwrap();
}
