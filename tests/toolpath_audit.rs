//! Geometric method checks, independent of the toolpath construction algorithms.
use opensourcecam::{
    cam::{self, Motion, Point3},
    commands,
    model::{Project, Strategy},
    operation::{ClearingBoundary, Entry, PocketPattern, Raster},
    tool_library::{LibraryTool, ToolKind},
};
fn job(strategy: Strategy) -> Project {
    let mut p = Project {
        part: None,
        operations: Vec::new(),
        ..Default::default()
    };
    p.workspace.clear();
    let kind = match strategy {
        Strategy::Thread => ToolKind::ThreadMill,
        Strategy::Chamfer | Strategy::Engrave => ToolKind::ChamferMill,
        Strategy::Drill | Strategy::DrillOnce => ToolKind::Drill,
        _ => ToolKind::FlatEndMill,
    };
    p.tool_library.tools = vec![LibraryTool::new(1, 1, kind)];
    p.active_tool = Some(1);
    p.active_preset = Some(1);
    let mut op = commands::operation_template(&p, strategy);
    op.x = 20.;
    op.y = 20.;
    op.width = 30.;
    op.height = 24.;
    op.depth = 3.;
    op.settings.step_down = Some(1.);
    op.settings.precleared = true;
    if strategy == Strategy::Facing {
        op.x = 0.;
        op.y = 0.;
        op.width = p.stock.width;
        op.height = p.stock.height;
    }
    if strategy.is_round() {
        op.x = 45.;
        op.y = 40.;
        op.width = 24.;
    }
    if strategy == Strategy::Thread {
        op.settings.pitch = Some(p.tool_library.tools[0].dimension("thread_pitch"));
    }
    if strategy == Strategy::SurfaceSmooth {
        op.settings.height_offset = Some(-3.);
    }
    if strategy.is_curve() {
        op.settings.path = vec![[20., 20., 0.], [50., 20., 0.], [50., 44., 0.]];
        op.height = p.operation_tool(&op).unwrap().diameter;
        if matches!(strategy, Strategy::Chamfer | Strategy::Engrave) {
            op.depth = 0.5;
        }
    }
    p.operations.push(op);
    p
}
fn coverage(path: &cam::Toolpath, q: [f64; 2], z: f64) -> f64 {
    path.moves
        .iter()
        .filter(|m| {
            m.motion == Motion::Cut && (m.to.z - z).abs() < 1e-6 && (m.from.z - z).abs() < 1e-6
        })
        .map(|m| opensourcecam::planar::distance(q, [m.from.x, m.from.y, z], [m.to.x, m.to.y, z]))
        .fold(f64::INFINITY, f64::min)
}
#[test]
fn linked_clearing_covers_stock_edges_and_corners_beyond_the_selected_face() {
    use opensourcecam::{
        blank,
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
    blank::fit(&mut p, 5., 3., 0.).unwrap();
    let op = serde_json::from_value(
        commands::geometry_query(
            &p,
            commands::GeometryQuery::SmoothFaceDraft {
                origin: [6., 6., 20.],
                direction: [0., 0., -1.],
                roughing: true,
            },
        )
        .unwrap(),
    )
    .unwrap();
    p.operations = vec![op];
    let path = cam::generate(&p).unwrap();
    let r = p.operation_tool(&p.operations[0]).unwrap().diameter / 2.;
    for x in (0..=p.stock.width as usize).step_by(2) {
        for y in (0..=p.stock.height as usize).step_by(2) {
            assert!(
                coverage(&path, [x as f64, y as f64], -3.) <= r + 1e-6,
                "Uncleared stock at {x},{y}"
            );
        }
    }
    assert!(
        path.moves.iter().any(|m| m.to.x < 0. || m.to.y < 0.),
        "Open stock edges need cutter overrun"
    );
    p.operations[0].settings.clearing_boundary = Some(ClearingBoundary::Selected);
    let confined = cam::generate(&p).unwrap();
    assert!(coverage(&confined, [0., p.stock.height / 2.], -3.) > r);
    let saved = serde_json::to_vec(&p).unwrap();
    let restored = Project::from_json(&saved).unwrap();
    assert_eq!(
        restored.operations[0].settings.clearing_boundary,
        Some(ClearingBoundary::Selected)
    );
}
#[test]
fn clearing_and_facing_sweep_their_full_cutter_accessible_regions() {
    for strategy in [
        Strategy::Pocket,
        Strategy::CircularPocket,
        Strategy::Facing,
        Strategy::StockRough,
        Strategy::SmoothRough,
        Strategy::SurfaceSmooth,
        Strategy::AreaRough,
    ] {
        for step in [20., 90.] {
            if strategy == Strategy::SmoothRough && step > 40. {
                continue;
            }
            for pattern in [PocketPattern::Offset, PocketPattern::Raster] {
                let mut p = job(strategy);
                p.operations[0].settings.pocket_pattern = pattern;
                p.operations[0].settings.step_over_percent = Some(step);
                if strategy == Strategy::Facing {
                    p.operations[0].x = 0.;
                    p.operations[0].y = 0.;
                    p.operations[0].width = p.stock.width;
                    p.operations[0].height = p.stock.height;
                }
                let op = &p.operations[0];
                let r = p.operation_tool(op).unwrap().diameter / 2.;
                let path = cam::generate(&p).unwrap_or_else(|e| panic!("{strategy:?}: {e}"));
                for x in (0..=p.stock.width as usize).step_by(2) {
                    for y in (0..=p.stock.height as usize).step_by(2) {
                        let q = [x as f64, y as f64];
                        let dx = (op.x + r - q[0]).max(q[0] - (op.x + op.width - r)).max(0.);
                        let dy = (op.y + r - q[1]).max(q[1] - (op.y + op.height - r)).max(0.);
                        let required = match strategy {
                            Strategy::Facing => true,
                            Strategy::StockRough => {
                                q[0] < op.x - 0.05
                                    || q[0] > op.x + op.width + 0.05
                                    || q[1] < op.y - 0.05
                                    || q[1] > op.y + op.height + 0.05
                            }
                            Strategy::CircularPocket => {
                                (q[0] - op.x).hypot(q[1] - op.y) < op.width / 2. - 0.03
                            }
                            // Grid roughing deliberately leaves a resolution-sized boundary residual.
                            Strategy::AreaRough => {
                                q[0] > op.x + 1.
                                    && q[0] < op.x + op.width - 1.
                                    && q[1] > op.y + 1.
                                    && q[1] < op.y + op.height - 1.
                            }
                            _ => dx.hypot(dy) < r - 0.03,
                        };
                        if required {
                            assert!(
                                coverage(&path, q, -3.) <= r + 0.03,
                                "{strategy:?}, {pattern:?}, {step}% left material at {q:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}
#[test]
fn finishing_feed_applies_without_requiring_spring_passes() {
    for strategy in [
        Strategy::Facing,
        Strategy::Pocket,
        Strategy::CircularPocket,
        Strategy::InsideProfile,
        Strategy::OutsideProfile,
        Strategy::StockRough,
        Strategy::SmoothRough,
        Strategy::Slot,
        Strategy::Trace,
        Strategy::Engrave,
        Strategy::Chamfer,
        Strategy::Bore,
        Strategy::Circular,
        Strategy::SurfaceSmooth,
    ] {
        let mut p = job(strategy);
        p.operations[0].settings.finish_feed = Some(123.);
        p.operations[0].settings.spring_passes = 0;
        let path = cam::generate(&p).unwrap_or_else(|e| panic!("{strategy:?}: {e}"));
        assert!(
            path.moves
                .iter()
                .any(|m| m.motion == Motion::Cut && (m.feed - 123.).abs() < 1e-6),
            "{strategy:?} ignored finishing feed"
        );
    }
}
#[test]
fn a_selected_contour_below_stock_top_obeys_axial_stepdowns() {
    let mut p = job(Strategy::OutsideProfile);
    let op = &mut p.operations[0];
    op.settings.path = vec![
        [20., 20., -8.],
        [40., 20., -8.],
        [40., 40., -8.],
        [20., 40., -8.],
    ];
    op.settings.closed = true;
    op.settings.height_offset = Some(0.5);
    op.settings.precleared = false;
    op.settings.step_down = Some(2.);
    let path = cam::generate(&p).unwrap();
    let mut levels: Vec<_> = path
        .moves
        .iter()
        .filter(|m| m.motion == Motion::Cut)
        .map(|m| m.to.z)
        .collect();
    levels.sort_by(f64::total_cmp);
    levels.dedup();
    assert_eq!(levels, vec![-7.5, -6., -4., -2.]);
    p.operations[0].settings.precleared = true;
    let finished = cam::generate(&p).unwrap();
    assert!(finished
        .moves
        .iter()
        .filter(|m| m.motion == Motion::Cut)
        .all(|m| (m.to.z + 7.5).abs() < 1e-6));
}
#[test]
fn buried_edge_finishing_needs_an_exposed_surface_and_unsupported_options_fail() {
    for strategy in [Strategy::Engrave, Strategy::Chamfer, Strategy::Trace] {
        let mut p = job(strategy);
        for q in &mut p.operations[0].settings.path {
            q[2] = -4.;
        }
        p.operations[0].settings.precleared = false;
        assert!(cam::generate(&p).unwrap_err().contains("exposed"));
        p.operations[0].settings.precleared = true;
        cam::generate(&p).unwrap();
    }
    for strategy in [
        Strategy::AreaRough,
        Strategy::Thread,
        Strategy::Drill,
        Strategy::DrillOnce,
    ] {
        let mut p = job(strategy);
        p.operations[0].settings.finish_feed = Some(123.);
        assert!(
            cam::generate(&p).is_err(),
            "{strategy:?} silently accepted unsupported finish feed"
        );
    }
}
#[test]
fn drilling_and_peck_drilling_and_entry_modes_keep_rapids_in_proven_air() {
    for strategy in [Strategy::Drill, Strategy::DrillOnce] {
        let p = job(strategy);
        let path = cam::generate(&p).unwrap();
        assert!(path
            .moves
            .iter()
            .filter(|m| m.motion == Motion::Rapid && (m.to.x != m.from.x || m.to.y != m.from.y))
            .all(|m| m.to.z >= p.clearance && m.from.z >= p.clearance));
        let pecks = path
            .moves
            .iter()
            .filter(|m| m.motion == Motion::Plunge)
            .count();
        assert_eq!(
            pecks,
            if strategy == Strategy::DrillOnce {
                1
            } else {
                3
            }
        );
    }
    for entry in [Entry::Plunge, Entry::Ramp, Entry::Helix] {
        let mut p = job(Strategy::Pocket);
        p.operations[0].settings.entry = entry;
        p.operations[0].settings.raster = Raster::Bidirectional;
        let path = cam::generate(&p).unwrap();
        assert!(path
            .moves
            .iter()
            .filter(|m| m.motion == Motion::Rapid)
            .all(|m| m.to.z >= p.clearance - 1e-6 || m.to.x == m.from.x && m.to.y == m.from.y));
        if entry != Entry::Plunge {
            for m in path
                .moves
                .iter()
                .filter(|m| m.motion == Motion::Cut && m.from.z > m.to.z)
            {
                let xy = Point3 {
                    x: m.to.x,
                    y: m.to.y,
                    z: m.from.z,
                }
                .distance(m.from);
                assert!((m.from.z - m.to.z) <= xy * 3_f64.to_radians().tan() + 1e-6);
            }
        }
    }
}

#[test]
fn bore_and_thread_helices_obey_pitch_and_circular_passes_lead_off_before_plunging() {
    for strategy in [Strategy::Bore, Strategy::Thread] {
        let p = job(strategy);
        let tool = p.operation_tool(&p.operations[0]).unwrap();
        let pitch = if strategy == Strategy::Bore {
            tool.step_down
        } else {
            p.operations[0].settings.pitch.unwrap()
        };
        let path = cam::generate(&p).unwrap();
        let mut count = 0;
        for m in path
            .moves
            .iter()
            .filter(|m| m.motion == Motion::Cut && m.from.z > m.to.z + 1e-7)
        {
            let a = [m.from.x - 45., m.from.y - 40.];
            let b = [m.to.x - 45., m.to.y - 40.];
            let angle = (a[0] * b[1] - a[1] * b[0])
                .atan2(a[0] * b[0] + a[1] * b[1])
                .abs();
            assert!(((m.from.z - m.to.z) / angle - pitch / std::f64::consts::TAU).abs() < 1e-6);
            count += 1;
        }
        assert!(count >= 24);
        assert!((path.moves.iter().map(|m| m.to.z).fold(0., f64::min) + 3.).abs() < 1e-6);
    }
    for outside in [false, true] {
        let mut p = job(Strategy::Circular);
        p.operations[0].settings.outside = outside;
        let r = p.operations[0].width / 2. + if outside { 3. } else { -3. };
        let path = cam::generate(&p).unwrap();
        let plunges: Vec<_> = path
            .moves
            .iter()
            .filter(|m| m.motion == Motion::Plunge)
            .collect();
        assert_eq!(plunges.len(), 3);
        for m in plunges {
            let radius = (m.to.x - 45.).hypot(m.to.y - 40.);
            assert!(
                if outside {
                    radius > r + 0.1
                } else {
                    radius < r - 0.1
                },
                "Circular milling must change depth in cleared air, off the wall"
            );
        }
        for m in path.moves.iter().filter(|m| m.motion == Motion::Cut) {
            let radius = (m.to.x - 45.).hypot(m.to.y - 40.);
            assert!(if outside {
                radius >= r - 1e-6
            } else {
                radius <= r + 1e-6
            });
        }
    }
}

#[test]
fn inferred_modes_and_generation_use_the_same_spindle_and_compensation_rules() {
    use opensourcecam::operation::{self, Direction, MillingMode};
    for strategy in [
        Strategy::Pocket,
        Strategy::InsideProfile,
        Strategy::OutsideProfile,
        Strategy::StockRough,
        Strategy::AreaRough,
        Strategy::SmoothRough,
        Strategy::SurfaceRough,
        Strategy::Bore,
        Strategy::Circular,
        Strategy::Chamfer,
    ] {
        for spin in [false, true] {
            for mode in [MillingMode::Climb, MillingMode::Conventional] {
                let mut p = job(strategy);
                p.tool_library.tools[0].clockwise = spin;
                p.operations[0].settings.direction =
                    if operation::clockwise_for(&p.operations[0], mode, spin) {
                        Direction::Clockwise
                    } else {
                        Direction::Counterclockwise
                    };
                assert_eq!(
                    commands::operation_defaults(&p, &p.operations[0]).unwrap()["milling_mode"],
                    serde_json::to_value(mode).unwrap(),
                    "{strategy:?}"
                );
            }
        }
    }
}

#[test]
fn measured_preset_ramp_limits_are_used_instead_of_hidden_constants() {
    for entry in [Entry::Ramp, Entry::Helix] {
        let mut p = job(Strategy::Pocket);
        p.operations[0].settings.entry = entry;
        p.tool_library.tools[0].presets[0]
            .values
            .insert("ramp_angle".into(), 1.25);
        p.tool_library.tools[0].presets[0]
            .values
            .insert("ramp_feed".into(), 77.);
        let path = cam::generate(&p).unwrap();
        let mut ramps = 0;
        for m in path
            .moves
            .iter()
            .filter(|m| m.motion == Motion::Cut && m.from.z > m.to.z + 1e-7)
        {
            let xy = (m.to.x - m.from.x).hypot(m.to.y - m.from.y);
            assert!(m.from.z - m.to.z <= xy * 1.25_f64.to_radians().tan() + 1e-7);
            assert!(m.feed <= 77. + 1e-7);
            ramps += 1;
        }
        assert!(ramps > 0);
        assert!(path
            .moves
            .iter()
            .any(|m| m.motion == Motion::Cut && m.feed > 77.));
    }
}

#[test]
fn tapered_wall_finishing_uses_a_continuous_bore_strategy() {
    let mut p = job(Strategy::Circular);
    p.operations[0].settings.taper_angle = Some(5.);
    assert!(cam::generate(&p).unwrap_err().contains("Bore milling"));
    p.operations[0].strategy = Strategy::Bore;
    cam::generate(&p).unwrap();
}
