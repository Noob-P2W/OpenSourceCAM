use opensourcecam::{
    cam::{self, Motion},
    commands::{self, GeometryQuery},
    faces, features,
    import::{self, Units},
    model::{Operation, Project, Strategy},
    session::{Edit, Session},
    stock,
    tool_library::{LibraryTool, ToolKind},
};
use std::{
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};
fn mount() -> Project {
    let part =
        import::import_file(Path::new("examples/models/mount.stl"), Units::Millimetres).unwrap();
    let mut p = Project::from_part(part).unwrap();
    p.ensure_tool_library();
    p
}
fn two_levels(roof: bool) -> Project {
    let mut p = mount();
    let part = p.part.as_mut().unwrap();
    let mesh = &mut Arc::make_mut(&mut part.geometry).mesh;
    let original = mesh.clone();
    for (x, z) in if roof {
        vec![(100., -5.), (100., 15.)]
    } else {
        vec![(100., -5.)]
    } {
        let base = mesh.vertices.len() as u32;
        mesh.vertices
            .extend(original.vertices.iter().map(|v| [v[0] + x, v[1], v[2] + z]));
        mesh.triangles
            .extend(original.triangles.iter().map(|t| t.map(|i| i + base)));
    }
    let part = p.part.take().unwrap();
    let mut p = Project::from_part(part).unwrap();
    p.ensure_tool_library();
    p
}
#[test]
fn face_selection_reorients_the_setup_and_survives_undo_and_recovery() {
    let p = mount();
    let side = faces::list(p.part.as_ref().unwrap())
        .unwrap()
        .into_iter()
        .find(|f| f.normal[0] > 0.99)
        .unwrap();
    let before = p.clone();
    let mut s = Session::new(p);
    s.apply(Edit::SetMachiningFace {
        triangle: side.id,
        align: true,
    })
    .unwrap();
    let after = s.project().clone();
    assert_eq!(after.setup_face, Some(side.id));
    assert!(
        faces::face(after.part.as_ref().unwrap(), side.id)
            .unwrap()
            .horizontal
    );
    assert!(faces::level(&after).unwrap().unwrap() < 0.);
    opensourcecam::blank::contains_part(&after.stock, after.part.as_ref().unwrap()).unwrap();
    let checkpoint = s.history_checkpoint().unwrap();
    let mut recovered = Session::new(after.clone());
    recovered.restore_history(&checkpoint).unwrap();
    recovered.apply(Edit::Undo).unwrap();
    assert_eq!(recovered.project(), &before);
    recovered.apply(Edit::Redo).unwrap();
    assert_eq!(recovered.project(), &after);
    let part = before.part.as_ref().unwrap();
    let b = part.bounds();
    let hit = faces::pick(
        part,
        [b.min[0] + 1., b.min[1] + 1., b.max[2] + 10.],
        [0., 0., -1.],
    )
    .unwrap();
    assert!(hit.horizontal);
    assert!(!hit.segments.is_empty());
}
#[test]
fn facing_stops_at_the_model_and_never_below_it() {
    let mut p = mount();
    p.operations = vec![Operation {
        strategy: Strategy::Facing,
        depth: 1.,
        ..Default::default()
    }];
    assert!(cam::generate(&p).unwrap_err().contains("No facing stock"));
    p.part.as_mut().unwrap().offset[2] -= 1.;
    p.stock.thickness += 1.;
    let id = faces::list(p.part.as_ref().unwrap())
        .unwrap()
        .into_iter()
        .find(|f| f.horizontal && (f.z + 1.).abs() < 1e-6)
        .unwrap()
        .id;
    let op: Operation = serde_json::from_value(
        commands::geometry_query(&p, GeometryQuery::FacingDraft { triangle: id }).unwrap(),
    )
    .unwrap();
    assert_eq!(op.depth, 1.);
    p.operations = vec![op];
    let path = cam::generate(&p).unwrap();
    assert!(path
        .moves
        .iter()
        .filter(|m| m.motion == Motion::Cut)
        .all(|m| m.to.z >= -1. - 1e-7));
    p.operations[0].model_link = None;
    p.operations[0].depth = 2.;
    assert!(cam::generate(&p)
        .unwrap_err()
        .contains("cut into the target"));
}
#[test]
fn recognition_uses_the_selected_level_without_skipping_uncleared_stock() {
    let mut p = two_levels(false);
    let top = features::recognize_project(&p).unwrap();
    assert_eq!(top.features.len(), 1); // Upper through holes do not reach the complete model bottom.
    let part = p.part.as_ref().unwrap();
    let level = part.bounds().max[2] - 5.;
    let face = faces::list(part)
        .unwrap()
        .into_iter()
        .find(|f| f.horizontal && (f.z - level).abs() < 1e-6 && f.area > 100.)
        .unwrap();
    faces::setup(&mut p, face.id, false).unwrap();
    let report = features::recognize_project(&p).unwrap();
    assert_eq!(report.features.len(), 5);
    assert!(report
        .features
        .iter()
        .all(|f| f.center[0] > 100. && (f.opening_z - level).abs() < 1e-6));
    let feature = report
        .features
        .iter()
        .find(|f| f.kind == features::FeatureKind::RectangularPocket)
        .unwrap();
    let mut op = features::operation(&p, feature).unwrap();
    assert_eq!(op.settings.start_depth, Some(-level));
    op.enabled = true;
    p.operations = vec![op];
    let path = cam::generate(&p).unwrap();
    let first = path
        .moves
        .iter()
        .find(|m| m.motion == Motion::Plunge)
        .unwrap();
    assert_eq!(
        first.to.z, -1.,
        "Raw stock above the selected plane must be cleared in the tool step-down"
    );
}
#[test]
fn a_feature_below_an_overhang_cannot_be_programmed_as_accessible() {
    let mut p = two_levels(true);
    let part = p.part.as_ref().unwrap();
    let level = part.bounds().max[2] - 20.;
    let face = faces::list(part)
        .unwrap()
        .into_iter()
        .find(|f| f.horizontal && (f.z - level).abs() < 1e-6 && f.area > 100.)
        .unwrap();
    faces::setup(&mut p, face.id, false).unwrap();
    let report = features::recognize_project(&p).unwrap();
    let feature = report
        .features
        .iter()
        .find(|f| f.kind == features::FeatureKind::RectangularPocket)
        .unwrap();
    assert!(features::operation(&p, feature)
        .unwrap_err()
        .contains("obstructed"));
}
#[test]
fn adding_tools_preserves_existing_definitions_and_operation_defaults() {
    let mut p = Project::default();
    p.ensure_tool_library();
    let before = p.clone();
    let mut s = Session::new(p);
    s.apply(Edit::AddLibraryTool {
        tool_kind: ToolKind::Drill,
    })
    .unwrap();
    s.apply(Edit::AddLibraryTool {
        tool_kind: ToolKind::BallEndMill,
    })
    .unwrap();
    let p = s.project();
    assert_eq!(p.tool_library.tools.len(), 3);
    assert_eq!(p.tool_library.tools[0], before.tool_library.tools[0]);
    assert_eq!(p.active_tool, before.active_tool);
    assert_eq!(p.operations, before.operations);
    assert_eq!(
        p.operation_tool(&p.operations[0]).unwrap(),
        before.operation_tool(&before.operations[0]).unwrap()
    );
}

#[test]
fn opening_depth_only_skips_stock_proven_cleared_by_enabled_facing() {
    let mut p = mount();
    p.part.as_mut().unwrap().offset[2] -= 2.;
    p.stock.thickness += 2.;
    let feature = features::recognize_project(&p)
        .unwrap()
        .features
        .into_iter()
        .find(|f| f.kind == features::FeatureKind::RectangularPocket)
        .unwrap();
    let mut pocket = features::operation(&p, &feature).unwrap();
    pocket.enabled = true;
    let facing = commands::operation_template(&p, Strategy::Facing);
    p.operations = vec![facing, pocket];
    let first = |p: &Project| {
        cam::generate(p)
            .unwrap()
            .moves
            .into_iter()
            .find(|m| m.operation == 1 && m.motion == Motion::Plunge)
            .unwrap()
            .to
            .z
    };
    assert_eq!(
        first(&p),
        -3.,
        "A faced opening can start one step below its plane"
    );
    p.operations[0].enabled = false;
    assert_eq!(
        first(&p),
        -1.,
        "Disabling facing must restore stock-top depth passes"
    );
    p.operations[0].enabled = true;
    p.operations[0].settings.axial_allowance = 1.;
    assert_eq!(
        first(&p),
        -2.,
        "Facing stock allowance must remain in the next operation's material estimate"
    );
    p.operations[1].strategy = Strategy::Drill;
    p.operations[1].model_link = None;
    p.operations[1].x = 20.;
    p.operations[1].y = 20.;
    p.operations[1].settings.step_down = Some(1.);
    p.operations[0].enabled = false;
    p.tool_library.tools[0] = LibraryTool::new(1, 1, ToolKind::Drill);
    assert_eq!(
        first(&p),
        -1.,
        "Peck drilling must not treat model opening depth as pre-drilled stock"
    );
}
#[test]
fn desktop_detail_resolves_small_removal_with_bounded_resolution() {
    let mut p = Project::default();
    p.stock.width = 40.;
    p.stock.height = 40.;
    p.ensure_tool_library();
    p.tool_library.tools[0] = LibraryTool::new(1, 1, ToolKind::Drill);
    p.operations = vec![Operation {
        strategy: Strategy::DrillOnce,
        x: 20.,
        y: 20.,
        depth: 3.,
        ..Default::default()
    }];
    let path = cam::generate(&p).unwrap();
    let cancel = AtomicBool::new(false);
    let low = stock::simulate(&p, &path, 1., 32, &cancel).unwrap();
    let high = stock::simulate(&p, &path, 1., 1024, &cancel).unwrap();
    assert_eq!(high.summary.resolution, [1024, 1024]);
    assert!(high.summary.removed_volume_mm3 > 0.);
    assert!(high.geometry.mesh.triangles.len() > low.geometry.mesh.triangles.len());
    assert!(stock::simulate(&p, &path, 1., 1025, &cancel).is_err());
}

#[test]
fn survey_finds_multiple_levels_and_both_sides_without_changing_setup() {
    let p = two_levels(false);
    let original = p.clone();
    let survey = features::survey(&p).unwrap();
    let entries = survey["features"].as_array().unwrap();
    assert!(entries.len() >= 10);
    let top: Vec<_> = entries
        .iter()
        .filter(|e| e["current_orientation"] == true)
        .collect();
    assert!(top
        .iter()
        .any(|e| e["feature"]["opening_z"].as_f64().unwrap() < -4.));
    assert!(entries
        .iter()
        .any(|e| e["orientations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["normal"][2].as_f64().unwrap() < -0.99)));
    assert_eq!(p, original);
}
#[test]
fn feature_geometry_opens_before_choosing_a_small_enough_tool() {
    let mut p = mount();
    p.tool_library.tools[0]
        .geometry
        .insert("diameter".into(), 50.);
    let feature = features::recognize_project(&p)
        .unwrap()
        .features
        .into_iter()
        .find(|f| f.diameter.is_some())
        .unwrap();
    let op: Operation = serde_json::from_value(
        commands::geometry_query(
            &p,
            GeometryQuery::FeatureGeometry {
                feature: feature.clone(),
                strategy: Strategy::CircularPocket,
            },
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(op.width, feature.diameter.unwrap());
    assert!(!op.enabled);
    let hit = features::pick_feature(
        &p,
        [feature.center[0], feature.center[1], 100.],
        [0., 0., -1.],
    )
    .unwrap();
    assert_eq!(hit.center, feature.center);
}

#[test]
fn survey_groups_opposite_views_of_the_same_through_hole() {
    let survey = features::survey(&mount()).unwrap();
    let entries = survey["features"].as_array().unwrap();
    assert_eq!(
        entries.len(),
        5,
        "Cylinder facets and known pocket walls must not become extra open pockets"
    );
    assert_eq!(
        entries
            .iter()
            .filter(|e| e["feature"]["kind"] == "circular_hole" && e["feature"]["through"] == true)
            .count(),
        4
    );
}
