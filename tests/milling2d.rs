use opensourcecam::{
    cam::{self, Motion},
    commands,
    model::{Project, Strategy},
    operation::{Direction, Entry, PocketPattern},
    tool_library::{LibraryTool, ToolKind},
};
use std::sync::atomic::AtomicBool;
fn job(strategy: Strategy) -> Project {
    let mut p = Project {
        part: None,
        ..Default::default()
    };
    p.workspace.clear();
    p.operations.clear();
    let kind = match strategy {
        Strategy::Thread => ToolKind::ThreadMill,
        Strategy::Engrave | Strategy::Chamfer => ToolKind::ChamferMill,
        _ => ToolKind::FlatEndMill,
    };
    p.tool_library.tools = vec![LibraryTool::new(1, 1, kind)];
    p.active_tool = Some(1);
    p.active_preset = Some(1);
    let mut op = commands::operation_template(&p, strategy);
    op.enabled = true;
    op.depth = 3.;
    op.x = 40.;
    op.y = 35.;
    op.width = 20.;
    op.height = 6.;
    op.settings.step_down = Some(1.);
    op.settings.precleared = true;
    if matches!(strategy, Strategy::SmoothRough | Strategy::AreaRough) {
        op.width = 30.;
        op.height = 25.;
    }
    if strategy.is_curve() {
        op.settings.path = vec![[20., 20., 0.], [45., 20., 0.], [45., 40., 0.]];
        if matches!(strategy, Strategy::Engrave | Strategy::Chamfer) {
            op.depth = 0.5;
        }
    }
    p.operations.push(op);
    p
}
#[test]
fn each_new_cycle_has_finite_feed_moves_reaches_depth_and_retracts() {
    for strategy in Strategy::ALL.into_iter().filter(|s| s.is_new_2d()) {
        let p = job(strategy);
        let path = cam::generate(&p).unwrap_or_else(|e| panic!("{strategy:?}: {e}"));
        assert!(path.moves.iter().any(|m| m.motion == Motion::Cut));
        let depth = path.moves.iter().map(|m| m.to.z).fold(0., f64::min);
        assert!(
            (depth
                + if strategy == Strategy::SurfaceSmooth {
                    0.
                } else {
                    p.operations[0].depth
                })
            .abs()
                < 1e-7,
            "{strategy:?}"
        );
        for m in &path.moves {
            assert!(m.to.x.is_finite() && m.to.y.is_finite() && m.to.z.is_finite());
            if m.motion == Motion::Rapid && (m.from.x != m.to.x || m.from.y != m.to.y) {
                assert!(m.from.z >= p.clearance && m.to.z >= p.clearance)
            }
        }
        assert!(path.moves.last().unwrap().to.z >= p.clearance);
        assert!(cam::generate_with_cancel(&p, &AtomicBool::new(true)).is_err());
    }
}

#[test]
fn new_cycles_export_through_each_implemented_milling_post() {
    use opensourcecam::post::{self, PostProcessor};
    for strategy in Strategy::ALL.into_iter().filter(|s| s.is_new_2d()) {
        for processor in [
            PostProcessor::Generic,
            PostProcessor::Mach3,
            PostProcessor::Grbl,
            PostProcessor::LinuxCnc,
        ] {
            let mut p = job(strategy);
            p.post.processor = processor;
            let nc =
                post::gcode(&p).unwrap_or_else(|e| panic!("{strategy:?} / {processor:?}: {e}"));
            assert!(nc.contains("G1") && nc.contains("M5"));
            assert!(!nc.contains("NaN") && !nc.contains("inf"));
        }
    }
}
#[test]
fn internal_and_external_round_paths_stay_on_the_correct_wall_side() {
    for strategy in [Strategy::Bore, Strategy::Circular, Strategy::Thread] {
        for outside in [false, true] {
            let mut p = job(strategy);
            p.operations[0].settings.outside = outside;
            let op = &p.operations[0];
            let tool = p.operation_tool(op).unwrap();
            let path = cam::generate(&p).unwrap();
            let radius = op.width / 2.
                + if outside {
                    tool.diameter / 2.
                } else {
                    -tool.diameter / 2.
                }
                + if strategy == Strategy::Thread {
                    if outside {
                        -0.3
                    } else {
                        0.3
                    }
                } else {
                    0.
                };
            assert!(path
                .moves
                .iter()
                .any(|m| m.to.z < -2.9
                    && ((m.to.x - op.x).hypot(m.to.y - op.y) - radius).abs() < 1e-6));
            if outside {
                assert!(path
                    .moves
                    .iter()
                    .filter(|m| m.to.z < 0.)
                    .all(|m| (m.to.x - op.x).hypot(m.to.y - op.y) >= radius - 1e-5));
            }
        }
    }
}
#[test]
fn helix_pitch_and_hand_are_preserved() {
    for direction in [Direction::Clockwise, Direction::Counterclockwise] {
        let mut p = job(Strategy::Thread);
        p.operations[0].settings.direction = direction;
        let path = cam::generate(&p).unwrap();
        let op = &p.operations[0];
        let points: Vec<_> = path
            .moves
            .iter()
            .filter(|m| m.motion == Motion::Cut && (m.to.z - m.from.z).abs() > 1e-8)
            .collect();
        let mut sweep = 0.;
        let mut dz = 0.;
        for m in points {
            let a = (m.from.y - op.y).atan2(m.from.x - op.x);
            let b = (m.to.y - op.y).atan2(m.to.x - op.x);
            let d = (b - a + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU)
                - std::f64::consts::PI;
            sweep += d;
            dz += m.from.z - m.to.z;
            assert_eq!(d < 0., direction == Direction::Clockwise);
        }
        assert!((dz - 3.).abs() < 1e-7);
        assert!((sweep.abs() / std::f64::consts::TAU - 3.).abs() < 1e-6);
    }
}
#[test]
fn cycles_reject_uncleared_bores_bad_tools_pitch_and_unsupported_chains() {
    let mut p = job(Strategy::Bore);
    p.operations[0].settings.precleared = false;
    assert!(cam::generate(&p).unwrap_err().contains("not proven clear"));
    let mut p = job(Strategy::Thread);
    p.operations[0].settings.pitch = Some(2.);
    assert!(cam::generate(&p).unwrap_err().contains("pitch"));
    let mut p = job(Strategy::Slot);
    p.operations[0].height = 8.;
    assert!(cam::generate(&p).unwrap_err().contains("matching"));
    let mut p = job(Strategy::Chamfer);
    p.operations[0].settings.path[1][2] = 1.;
    assert!(cam::generate(&p).unwrap_err().contains("flat"));
    let mut p = job(Strategy::Trace);
    p.operations[0].settings.path[0][0] = 0.;
    assert!(cam::generate(&p).is_err());
}
#[test]
fn offset_pockets_keep_compensated_bounds_with_each_entry() {
    for entry in [Entry::Plunge, Entry::Ramp, Entry::Helix] {
        let mut p = job(Strategy::Pocket);
        p.operations[0].x = 20.;
        p.operations[0].y = 20.;
        p.operations[0].width = 40.;
        p.operations[0].height = 25.;
        p.operations[0].settings.pocket_pattern = PocketPattern::Offset;
        p.operations[0].settings.entry = entry;
        let path = cam::generate(&p).unwrap();
        let r = p.operation_tool(&p.operations[0]).unwrap().diameter / 2.;
        for m in path.moves.iter().filter(|m| m.to.z <= 0.) {
            assert!(
                m.to.x >= 20. + r - 1e-6
                    && m.to.x <= 60. - r + 1e-6
                    && m.to.y >= 20. + r - 1e-6
                    && m.to.y <= 45. - r + 1e-6
            )
        }
        assert!(path.cut_length > 100.);
    }
}
#[test]
fn planar_offsets_preserve_clearance_and_reject_collapsed_or_crossing_chains() {
    let square = [
        [20., 20., 0.],
        [40., 20., 0.],
        [40., 40., 0.],
        [20., 40., 0.],
    ];
    assert_eq!(
        opensourcecam::contour::offset(&square, true, 3., true).unwrap(),
        vec![
            [23., 23., 0.],
            [37., 23., 0.],
            [37., 37., 0.],
            [23., 37., 0.]
        ]
    );
    assert!(opensourcecam::contour::offset(&square, true, 11., true).is_err());
    let crossed = [square[0], square[2], square[1], square[3]];
    assert!(opensourcecam::contour::offset(&crossed, true, 1., true).is_err());
    let mut p = job(Strategy::InsideProfile);
    p.operations[0].settings.path = square.to_vec();
    p.operations[0].settings.closed = true;
    let path = cam::generate(&p).unwrap();
    assert!(path
        .moves
        .iter()
        .filter(|m| m.to.z < 0.)
        .all(|m| m.to.x >= 23. - 1e-6
            && m.to.x <= 37. + 1e-6
            && m.to.y >= 23. - 1e-6
            && m.to.y <= 37. + 1e-6));
}

#[test]
fn descending_traces_and_helices_respect_axial_feed() {
    for strategy in [Strategy::Trace, Strategy::Bore, Strategy::Thread] {
        let mut p = job(strategy);
        p.operations[0].settings.plunge = Some(1.);
        if strategy == Strategy::Trace {
            p.operations[0].settings.path[1][2] = -2.;
        }
        let path = cam::generate(&p).unwrap();
        for m in path
            .moves
            .iter()
            .filter(|m| m.motion == Motion::Cut && m.to.z < m.from.z)
        {
            assert!(m.feed * (m.from.z - m.to.z) / m.from.distance(m.to) <= 1. + 1e-6)
        }
    }
}

#[test]
fn fractional_external_helices_finish_and_exit_without_crossing_the_boss() {
    for strategy in [Strategy::Bore, Strategy::Thread] {
        let mut p = job(strategy);
        let op = &mut p.operations[0];
        op.depth = 2.375;
        op.settings.outside = true;
        let min_radius = op.width / 2. + p.tool_library.tools[0].dimension("diameter") / 2.
            - if strategy == Strategy::Thread {
                0.3
            } else {
                0.
            };
        let path = cam::generate(&p).unwrap();
        let op = &p.operations[0];
        for m in path
            .moves
            .iter()
            .filter(|m| m.to.z < 0. && m.motion != Motion::Rapid)
        {
            for i in 0..=16 {
                let t = i as f64 / 16.;
                let x = m.from.x + (m.to.x - m.from.x) * t;
                let y = m.from.y + (m.to.y - m.from.y) * t;
                assert!(
                    (x - op.x).hypot(y - op.y) >= min_radius - 0.011,
                    "{strategy:?}: {m:?}"
                );
            }
        }
    }
}

#[test]
fn thread_path_generation_is_independent_of_unsupported_sampled_stock() {
    let p = job(Strategy::Thread);
    assert!(commands::execute_project(&p, commands::Command::Generate).is_ok());
    let preview = commands::PlaybackPreview::new(&p).unwrap();
    assert!(preview
        .snapshot(&p, 0., 32)
        .unwrap_err()
        .contains("thread grooves"));
}

#[test]
fn model_boundary_drafts_preserve_heights_and_invalidate_after_placement_changes() {
    use opensourcecam::{
        commands::GeometryQuery,
        faces,
        import::{self, Units},
    };
    let part = import::import_file(
        std::path::Path::new("examples/models/mount.stl"),
        Units::Millimetres,
    )
    .unwrap();
    let mut p = Project::from_part(part).unwrap();
    p.ensure_tool_library();
    let bounds = p.part.as_ref().unwrap().bounds();
    let recessed = faces::list(p.part.as_ref().unwrap())
        .unwrap()
        .into_iter()
        .find(|f| f.horizontal && f.z < bounds.max[2] - 1.)
        .unwrap();
    p.setup_face = Some(recessed.id);
    assert!(commands::operation_template(&p, Strategy::Trace)
        .settings
        .start_depth
        .is_none());
    p.setup_face = None;
    let origin = [bounds.min[0] + 1., bounds.min[1] + 1., bounds.max[2] + 10.];
    let direction = [0., 0., -1.];
    let chain = faces::pick_chain(p.part.as_ref().unwrap(), origin, direction).unwrap();
    assert_eq!(chain.len(), 4);
    assert!(chain.iter().all(|q| (q[2] - bounds.max[2]).abs() < 1e-6));
    let op: opensourcecam::model::Operation = serde_json::from_value(
        commands::geometry_query(
            &p,
            GeometryQuery::ChainDraft {
                origin,
                direction,
                strategy: Strategy::Trace,
                single_edge: false,
                existing_path: Vec::new(),
                edge_tolerance: None,
            },
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(op.settings.path, chain);
    assert!(op.settings.closed && op.model_link.is_some());
    p.operations = vec![op];
    cam::generate(&p).unwrap();
    p.part.as_mut().unwrap().offset[0] += 1.;
    assert!(cam::generate(&p).unwrap_err().contains("placement changed"));
}

#[test]
fn chamfer_width_changes_contact_offset_and_single_edge_stays_open() {
    let mut p = job(Strategy::Chamfer);
    p.operations[0].settings.path = vec![[20., 20., 0.], [45., 20., 0.]];
    p.operations[0].settings.outside = true;
    p.operations[0].settings.chamfer_width = Some(0.2);
    let first = cam::generate(&p).unwrap();
    p.operations[0].settings.chamfer_width = Some(0.4);
    let second = cam::generate(&p).unwrap();
    let y = |path: &cam::Toolpath| path.moves.iter().find(|m| m.to.z < 0.).unwrap().to.y;
    assert!((y(&second) - y(&first) - 0.2).abs() < 1e-6);
    p.operations[0].settings.chamfer_width = Some(10.);
    assert!(cam::generate(&p).unwrap_err().contains("tip depth"));
}

#[test]
fn picked_slot_tolerates_coordinate_roundoff_and_edges_are_real_boundaries() {
    use opensourcecam::{
        commands::GeometryQuery,
        faces, features,
        import::{self, Units},
    };
    let part = import::import_file(
        std::path::Path::new("examples/models/mount.stl"),
        Units::Millimetres,
    )
    .unwrap();
    let p = Project::from_part(part).unwrap();
    let mut feature = features::recognize_project(&p)
        .unwrap()
        .features
        .into_iter()
        .find(|f| f.kind == features::FeatureKind::RectangularPocket)
        .unwrap();
    feature.center[0] += 1e-7;
    let op = commands::geometry_query(
        &p,
        GeometryQuery::FeatureGeometry {
            feature,
            strategy: Strategy::Slot,
        },
    )
    .unwrap();
    assert_eq!(op["settings"]["path"].as_array().unwrap().len(), 2);
    let b = p.part.as_ref().unwrap().bounds();
    let edge = faces::pick_edges(
        p.part.as_ref().unwrap(),
        [b.min[0] + 1., b.min[1] + 1., 10.],
        [0., 0., -1.],
        true,
    )
    .unwrap();
    assert_eq!(edge.len(), 2);
    assert!(edge.iter().all(|q| (q[2] - b.max[2]).abs() < 1e-6));
}

#[test]
fn pocket_family_resolves_shape_and_contour_family_resolves_material_side() {
    use opensourcecam::{
        commands::GeometryQuery,
        features,
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
    let report = features::recognize_project(&p).unwrap();
    for kind in [
        features::FeatureKind::RectangularPocket,
        features::FeatureKind::CircularHole,
    ] {
        let f = report.features.iter().find(|f| f.kind == kind).unwrap();
        for requested in [Strategy::Pocket, Strategy::CircularPocket] {
            let op = features::draft(&p, f, requested).unwrap();
            assert_eq!(
                op.strategy,
                if kind == features::FeatureKind::CircularHole {
                    Strategy::CircularPocket
                } else {
                    Strategy::Pocket
                }
            );
            assert!(op.model_link.is_some());
        }
    }
    let bounds = p.part.as_ref().unwrap().bounds();
    for requested in [Strategy::InsideProfile, Strategy::OutsideProfile] {
        let op: opensourcecam::model::Operation = serde_json::from_value(
            commands::geometry_query(
                &p,
                GeometryQuery::ChainDraft {
                    origin: [bounds.min[0] + 1., bounds.min[1] + 1., bounds.max[2] + 10.],
                    direction: [0., 0., -1.],
                    strategy: requested,
                    single_edge: false,
                    existing_path: vec![],
                    edge_tolerance: None,
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            op.strategy,
            if op.settings.outside {
                Strategy::OutsideProfile
            } else {
                Strategy::InsideProfile
            }
        );
        assert!(op.settings.outside, "Selected the exterior boundary");
        p.operations = vec![op];
        opensourcecam::machining::validate_links(&p).unwrap();
    }
}
