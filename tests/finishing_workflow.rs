use opensourcecam::{
    cam::{self, Motion},
    commands,
    features::{self, Detail, Feature, FeatureKind, Shape},
    model::{Project, Strategy},
    operation::MillingMode,
    tool_library::{LibraryTool, ToolKind},
};
fn job() -> Project {
    let mut p = Project {
        part: None,
        ..Default::default()
    };
    p.workspace.clear();
    p.operations.clear();
    p.tool_library.tools = vec![
        LibraryTool::new(1, 1, ToolKind::FlatEndMill),
        LibraryTool::new(2, 2, ToolKind::ChamferMill),
    ];
    p.active_tool = Some(1);
    p.active_preset = Some(1);
    p
}
fn ring(radius: f64, z: f64) -> Vec<[f64; 3]> {
    let mut points = Vec::new();
    for (cx, start) in [
        (30., std::f64::consts::FRAC_PI_2),
        (42., 3. * std::f64::consts::FRAC_PI_2),
    ] {
        for i in 0..=21 {
            let a = start + std::f64::consts::PI * i as f64 / 21.;
            points.push([cx + radius * a.cos(), 35. + radius * a.sin(), z]);
        }
    }
    points
}
#[test]
fn a_chamfered_slot_is_two_standard_operations_with_measured_chamfer() {
    let mut p = job();
    let f = Feature {
        kind: FeatureKind::RectangularPocket,
        min: [26.5, 31.5],
        max: [45.5, 38.5],
        center: [36., 35.],
        diameter: None,
        opening_z: -2.,
        bottom_z: -8.,
        through: true,
        outline: ring(3.5, -2.),
        detail: Detail {
            shape: Shape::ChamferedSlot,
            profiles: vec![ring(3.5, -2.), ring(3., -2.5), ring(3., -8.)],
            ..Default::default()
        },
    };
    let geometry = features::slot::geometry(&f).unwrap();
    assert!(
        (geometry.centerline[0][0] - 30.).abs() < 1e-7
            && (geometry.centerline[1][0] - 42.).abs() < 1e-7
    );
    let slot = features::draft(&p, &f, Strategy::Slot).unwrap();
    assert!((slot.height - 6.).abs() < 1e-7);
    assert_eq!(slot.depth, 8.);
    p.operations = vec![slot];
    let path = cam::generate(&p).unwrap();
    assert!((path.moves.iter().map(|m| m.to.z).fold(0., f64::min) + 8.).abs() < 1e-7);
    let mut chamfer = features::draft(&p, &f, Strategy::Chamfer).unwrap();
    chamfer.tool_id = Some(2);
    assert_eq!(chamfer.settings.chamfer_width, Some(0.5));
    assert!((chamfer.settings.chamfer_angle.unwrap() - 45.).abs() < 1e-7);
    p.operations = vec![chamfer];
    cam::generate(&p).unwrap();
    assert!(features::draft(&p, &f, Strategy::Pocket)
        .unwrap_err()
        .contains("separate Chamfer"));
}
#[test]
fn zero_and_positive_finishing_offsets_never_cut_below_selected_edge() {
    for offset in [0., 0.5, 2.] {
        let mut p = job();
        let mut op = commands::operation_template(&p, Strategy::OutsideProfile);
        op.settings.path = vec![
            [20., 20., -8.],
            [40., 20., -8.],
            [40., 40., -8.],
            [20., 40., -8.],
        ];
        op.settings.closed = true;
        op.settings.height_offset = Some(offset);
        p.operations.push(op);
        let path = cam::generate(&p).unwrap();
        for m in path.moves.iter().filter(|m| m.motion == Motion::Cut) {
            assert!((m.to.z - (-8. + offset)).abs() < 1e-7);
        }
    }
}
#[test]
fn planar_face_smoothing_stays_above_floor_and_clear_of_islands() {
    let mut p = job();
    let mut op = commands::operation_template(&p, Strategy::SurfaceSmooth);
    op.settings.path = vec![
        [10., 10., -5.],
        [80., 10., -5.],
        [80., 60., -5.],
        [10., 60., -5.],
    ];
    op.settings.islands = vec![vec![
        [35., 25., -5.],
        [50., 25., -5.],
        [50., 45., -5.],
        [35., 45., -5.],
    ]];
    op.settings.precleared = true;
    op.settings.height_offset = Some(0.25);
    p.operations.push(op);
    let path = cam::generate(&p).unwrap();
    let rings = vec![
        p.operations[0].settings.path.clone(),
        p.operations[0].settings.islands[0].clone(),
    ];
    for m in path.moves.iter().filter(|m| m.motion == Motion::Cut) {
        assert!((m.to.z + 4.75).abs() < 1e-7);
        for i in 0..=20 {
            let t = i as f64 / 20.;
            let q = [
                m.from.x + (m.to.x - m.from.x) * t,
                m.from.y + (m.to.y - m.from.y) * t,
            ];
            assert!(
                opensourcecam::planar::inside(q, &rings[0])
                    && !opensourcecam::planar::inside(q, &rings[1])
            );
            for r in &rings {
                for j in 0..r.len() {
                    assert!(
                        opensourcecam::planar::distance(q, r[j], r[(j + 1) % r.len()]) >= 3. - 1e-6
                    );
                }
            }
        }
    }
}
#[test]
fn climb_milling_resolves_spindle_rotation_without_changing_compensation() {
    for outside in [true, false] {
        for clockwise in [true, false] {
            for mode in [MillingMode::Climb, MillingMode::Conventional] {
                let mut p = job();
                p.tool_library.tools[0].clockwise = clockwise;
                let mut op = commands::operation_template(&p, Strategy::Circular);
                op.x = 50.;
                op.y = 40.;
                op.width = 20.;
                op.depth = 1.;
                op.settings.precleared = true;
                op.settings.outside = outside;
                op.settings.milling_mode = Some(mode);
                p.operations.push(op);
                let path = cam::generate(&p).unwrap();
                let sweep: f64 = path
                    .moves
                    .iter()
                    .filter(|m| m.motion == Motion::Cut && (m.to.z + 1.).abs() < 1e-6)
                    .map(|m| {
                        let a = (m.from.y - 40.).atan2(m.from.x - 50.);
                        let b = (m.to.y - 40.).atan2(m.to.x - 50.);
                        (b - a + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU)
                            - std::f64::consts::PI
                    })
                    .sum();
                assert_eq!(
                    sweep < 0.,
                    (clockwise == (mode == MillingMode::Climb)) == outside
                );
            }
        }
    }
}
#[test]
fn connected_edges_join_at_either_end_and_close_only_when_complete() {
    let a = [0., 0., 0.];
    let b = [10., 0., 0.];
    let c = [10., 10., 0.];
    let (p, closed) = opensourcecam::faces::join_chain(vec![a, b], vec![c, b]).unwrap();
    assert_eq!(p, vec![a, b, c]);
    assert!(!closed);
    let (p, closed) = opensourcecam::faces::join_chain(p, vec![c, a]).unwrap();
    assert_eq!(p, vec![a, b, c]);
    assert!(closed);
    assert!(opensourcecam::faces::join_chain(p, vec![[50., 50., 0.], [60., 50., 0.]]).is_err());
}

#[test]
fn silhouette_edges_snap_even_when_projection_lands_just_outside_the_face() {
    use opensourcecam::{
        faces,
        import::{self, Units},
    };
    let p = Project::from_part(
        import::import_file(
            std::path::Path::new("examples/models/mount.stl"),
            Units::Millimetres,
        )
        .unwrap(),
    )
    .unwrap();
    let (edge, closed) = faces::machining_edges(
        p.part.as_ref().unwrap(),
        [30., 4.999997895, 100.],
        [0., 0., -1.],
        true,
        Some(1.),
    )
    .unwrap();
    assert!(!closed);
    assert!(edge
        .iter()
        .all(|p| (p[1] - 5.).abs() < 1e-7 && p[2].abs() < 1e-7));
    assert!(faces::machining_edges(
        p.part.as_ref().unwrap(),
        [30., 2., 100.],
        [0., 0., -1.],
        true,
        Some(1.)
    )
    .is_err());
}

#[test]
fn outside_roughing_encloses_a_part_with_internal_features_and_never_clears_its_inside() {
    use opensourcecam::import::{self, Units};
    let mut p = Project::from_part(
        import::import_file(
            std::path::Path::new("examples/models/mount.stl"),
            Units::Millimetres,
        )
        .unwrap(),
    )
    .unwrap();
    p.ensure_tool_library();
    let op = commands::operation_template(&p, Strategy::StockRough);
    let b = p.part.as_ref().unwrap().bounds();
    assert!((op.x - b.min[0]).abs() < 1e-7 && (op.y - b.min[1]).abs() < 1e-7);
    assert!(
        (op.x + op.width - b.max[0]).abs() < 1e-7 && (op.y + op.height - b.max[1]).abs() < 1e-7
    );
    p.operations = vec![op];
    let radius = p.operation_tool(&p.operations[0]).unwrap().diameter / 2.;
    let path = cam::generate(&p).unwrap();
    for m in path
        .moves
        .iter()
        .filter(|m| m.motion != Motion::Rapid && m.to.z < 0.)
    {
        let dx = (b.min[0] - m.to.x).max(0.).max(m.to.x - b.max[0]);
        let dy = (b.min[1] - m.to.y).max(0.).max(m.to.y - b.max[1]);
        assert!(dx.hypot(dy) >= radius - 0.011);
    }
    p.operations[0].model_link = None;
    p.operations[0].width /= 2.;
    assert!(cam::generate(&p)
        .unwrap_err()
        .contains("complete workpiece"));
}

#[test]
fn surface_smoothing_keeps_physical_stock_validation() {
    use opensourcecam::blank::{StockDefinition, StockKind};
    let mut p = job();
    p.stock = StockDefinition {
        kind: StockKind::Cylinder,
        size: [100., 100., 12.],
        rotation: [0.; 3],
    }
    .build()
    .unwrap();
    let mut op = commands::operation_template(&p, Strategy::SurfaceSmooth);
    op.x = 0.;
    op.y = 0.;
    op.width = 20.;
    op.height = 20.;
    op.settings.precleared = true;
    p.operations.push(op);
    assert!(cam::generate(&p).unwrap_err().contains("physical stock"));
}

#[test]
fn face_finishing_has_no_hidden_penetration_depth_from_setup_defaults() {
    use opensourcecam::{
        commands::GeometryQuery,
        import::{self, Units},
    };
    let mut p = Project::from_part(
        import::import_file(
            std::path::Path::new("examples/models/mount.stl"),
            Units::Millimetres,
        )
        .unwrap(),
    )
    .unwrap();
    p.ensure_tool_library();
    p.tool_library.tools[0]
        .geometry
        .insert("flute_length".into(), 1.);
    let op: opensourcecam::model::Operation = serde_json::from_value(
        commands::geometry_query(
            &p,
            GeometryQuery::SmoothFaceDraft {
                origin: [6., 6., 10.],
                direction: [0., 0., -1.],
            },
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(op.depth, 0.);
    assert_eq!(op.settings.height_offset, Some(0.));
    p.operations = vec![op];
    p.operations[0].settings.precleared = true;
    cam::generate(&p).unwrap();
}

#[test]
fn saved_cut_direction_is_named_without_rewriting_the_operation() {
    let p = job();
    let op = commands::operation_template(&p, Strategy::OutsideProfile);
    let before = op.clone();
    let refs = commands::operation_defaults(&p, &op).unwrap();
    assert_eq!(refs["milling_mode"], "conventional");
    assert_eq!(op, before);
}
