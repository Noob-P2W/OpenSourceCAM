use opensourcecam::{
    cam::{self, Motion, Point3},
    commands,
    model::{Operation, Project, Strategy},
    operation::{Direction, Raster},
    post::{self, PostProcessor},
    tool_library::{FeedMode, LibraryTool, ToolKind},
};
fn job(strategy: Strategy) -> Project {
    let mut p = Project {
        operations: vec![Operation {
            strategy,
            x: 25.,
            y: 25.,
            width: 40.,
            height: 30.,
            depth: 3.4,
            ..Default::default()
        }],
        ..Default::default()
    };
    p.ensure_tool_library();
    if strategy.is_drilling() {
        p.tool_library.tools = vec![LibraryTool::new(1, 1, ToolKind::Drill)]
    }
    p.operations[0].settings.step_down = Some(1.);
    p.operations[0].settings.retract = Some(2.);
    p
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-7, "{a} != {b}")
}
#[test]
fn standard_drill_feeds_once_and_peck_drill_reenters_only_cleared_material() {
    for strategy in [Strategy::DrillOnce, Strategy::Drill] {
        let p = job(strategy);
        let path = post::checked_path(&p).unwrap();
        let feeds: Vec<_> = path
            .moves
            .iter()
            .enumerate()
            .filter(|(_, m)| m.motion == Motion::Plunge)
            .collect();
        assert_eq!(
            feeds.len(),
            if strategy == Strategy::DrillOnce {
                1
            } else {
                4
            }
        );
        let mut previous = 0.;
        for (n, (index, m)) in feeds.iter().enumerate() {
            close(m.from.x, 25.);
            close(m.to.x, 25.);
            close(m.from.y, 25.);
            close(m.to.y, 25.);
            close(m.from.z, if n == 0 { 2. } else { previous + 0.254 });
            let depth = if strategy == Strategy::DrillOnce {
                -3.4
            } else {
                -((n + 1) as f64).min(3.4)
            };
            close(m.to.z, depth);
            previous = depth;
            let retract = &path.moves[index + 1];
            assert_eq!(retract.motion, Motion::Rapid);
            close(retract.to.z, 2.);
        }
        close(path.moves.last().unwrap().to.z, p.travel_clearance());
        for m in &path.moves {
            if m.motion == Motion::Rapid && (m.from.x != m.to.x || m.from.y != m.to.y) {
                close(m.from.z, 5.);
                close(m.to.z, 5.);
            }
        }
        for processor in [
            PostProcessor::Generic,
            PostProcessor::Grbl,
            PostProcessor::Mach3,
            PostProcessor::LinuxCnc,
        ] {
            let mut p = p.clone();
            p.post.processor = processor;
            let code = post::gcode(&p).unwrap();
            assert!(!code
                .lines()
                .any(|s| s.starts_with("G81") || s.starts_with("G83")));
            assert!(code
                .lines()
                .any(|line| line.starts_with("G1 ") && line.contains("Z-3.4000")));
        }
    }
}
#[test]
fn single_feed_ignores_peck_count_but_validates_tool_planes_and_limits() {
    let mut p = job(Strategy::DrillOnce);
    p.operations[0].settings.step_down = Some(0.001);
    assert!(cam::generate(&p).is_ok());
    p.operations[0].strategy = Strategy::Drill;
    assert!(cam::generate(&p).unwrap_err().contains("2,000"));
    p.operations[0].settings.step_down = Some(1.);
    p.operations[0].settings.retract = Some(6.);
    assert!(cam::generate(&p).unwrap_err().contains("Retract"));
    p.operations[0].settings.retract = Some(2.);
    p.operations[0].settings.peck_clearance = Some(0.);
    assert!(cam::generate(&p).is_err());
    p.operations[0].settings.peck_clearance = None;
    p.tool_library.tools[0] = LibraryTool::new(1, 1, ToolKind::FlatEndMill);
    assert!(cam::generate(&p).unwrap_err().contains("drill tool"));
}
#[test]
fn drilling_per_revolution_feed_tracks_rpm_and_explicit_axial_override() {
    let mut p = job(Strategy::DrillOnce);
    let preset = &mut p.tool_library.tools[0].presets[0];
    preset.feed_mode = FeedMode::PerRevolution;
    preset.values.insert("feed_per_revolution".into(), 0.03);
    p.operations[0].settings.spindle_rpm = Some(5000);
    let path = cam::generate(&p).unwrap();
    close(
        path.moves
            .iter()
            .find(|m| m.motion == Motion::Plunge)
            .unwrap()
            .feed,
        150.,
    );
    p.operations[0].settings.plunge = Some(80.);
    close(
        cam::generate(&p)
            .unwrap()
            .moves
            .iter()
            .find(|m| m.motion == Motion::Plunge)
            .unwrap()
            .feed,
        80.,
    );
}
fn segment_distance(p: [f64; 2], a: Point3, b: Point3) -> f64 {
    let d = [b.x - a.x, b.y - a.y];
    let len = d[0] * d[0] + d[1] * d[1];
    let t = if len > 0. {
        (((p[0] - a.x) * d[0] + (p[1] - a.y) * d[1]) / len).clamp(0., 1.)
    } else {
        0.
    };
    (p[0] - a.x - t * d[0]).hypot(p[1] - a.y - t * d[1])
}
#[test]
fn outside_corners_follow_the_cutter_offset_without_gouging_either_direction() {
    for strategy in [Strategy::OutsideProfile, Strategy::StockRough] {
        for direction in [Direction::Clockwise, Direction::Counterclockwise] {
            let mut p = job(strategy);
            p.operations[0].depth = 1.;
            p.operations[0].settings.direction = direction;
            p.operations[0].settings.radial_allowance = 0.4;
            p.operations[0].settings.path_tolerance = Some(0.005);
            let op = &p.operations[0];
            let offset = p.operation_tool(op).unwrap().diameter / 2. + 0.4;
            let path = cam::generate(&p).unwrap();
            let cuts: Vec<_> = path
                .moves
                .iter()
                .filter(|m| m.motion == Motion::Cut)
                .collect();
            for (x, y, sx, sy) in [
                (25., 25., -1., -1.),
                (65., 25., 1., -1.),
                (65., 55., 1., 1.),
                (25., 55., -1., 1.),
            ] {
                let ideal = [
                    x + sx * offset / 2_f64.sqrt(),
                    y + sy * offset / 2_f64.sqrt(),
                ];
                assert!(
                    cuts.iter()
                        .map(|m| segment_distance(ideal, m.from, m.to))
                        .fold(f64::INFINITY, f64::min)
                        < 0.0051,
                    "Outside corner retains excessive material"
                );
                for m in &cuts {
                    assert!(
                        segment_distance([x, y], m.from, m.to) >= offset - 1e-7,
                        "Corner gouge in {m:?}"
                    );
                }
            }
            let code = post::gcode(&p).unwrap();
            assert!(code.contains("G40"));
            assert!(!code
                .lines()
                .any(|s| s.starts_with("G41") || s.starts_with("G42")));
        }
    }
}
#[test]
fn rounded_stock_clearing_reaches_the_original_blanks_diagonal_corners() {
    let mut p = job(Strategy::StockRough);
    p.operations[0].depth = 1.;
    let radius = p.operation_tool(&p.operations[0]).unwrap().diameter / 2.;
    let path = cam::generate(&p).unwrap();
    for corner in [
        [0., 0.],
        [p.stock.width, 0.],
        [p.stock.width, p.stock.height],
        [0., p.stock.height],
    ] {
        let distance = path
            .moves
            .iter()
            .filter(|m| m.motion == Motion::Cut)
            .map(|m| segment_distance(corner, m.from, m.to))
            .fold(f64::INFINITY, f64::min);
        assert!(distance < radius, "Uncleared stock corner {corner:?}");
    }
}
#[test]
fn inward_offsets_and_raster_reversals_preserve_all_four_sides() {
    for strategy in [Strategy::Pocket, Strategy::InsideProfile] {
        for direction in [Direction::Clockwise, Direction::Counterclockwise] {
            for raster in [Raster::Bidirectional, Raster::PositiveX, Raster::NegativeX] {
                let mut p = job(strategy);
                let op = &mut p.operations[0];
                op.depth = 1.;
                op.settings.direction = direction;
                op.settings.raster = raster;
                op.settings.radial_allowance = 0.4;
                let r = p.operation_tool(&p.operations[0]).unwrap().diameter / 2. + 0.4;
                let path = cam::generate(&p).unwrap();
                for m in path.moves.iter().filter(|m| m.motion == Motion::Cut) {
                    for v in [m.from, m.to] {
                        assert!(v.x >= 25. + r - 1e-7 && v.x <= 65. - r + 1e-7);
                        assert!(v.y >= 25. + r - 1e-7 && v.y <= 55. - r + 1e-7);
                    }
                }
            }
        }
    }
}
#[test]
fn stock_playback_uses_the_same_time_prefix_and_rewinds_to_uncut_stock() {
    let p = job(Strategy::Pocket);
    let preview = commands::PlaybackPreview::new(&p).unwrap();
    let timeline = commands::path_value(&preview.path);
    let mut previous = 0.;
    for m in timeline["moves"].as_array().unwrap() {
        close(m["start_seconds"].as_f64().unwrap(), previous);
        let end = m["end_seconds"].as_f64().unwrap();
        assert!(end > previous);
        previous = end;
    }
    close(previous, timeline["seconds"].as_f64().unwrap());
    let start = preview.snapshot(&p, 0., 32).unwrap();
    let middle = preview.snapshot(&p, 0.5, 32).unwrap();
    let end = preview.snapshot(&p, 1., 32).unwrap();
    assert_eq!(start["summary"]["removed_volume_mm3"].as_f64().unwrap(), 0.);
    assert!(middle["summary"]["removed_volume_mm3"].as_f64().unwrap() > 0.);
    assert!(
        end["summary"]["removed_volume_mm3"].as_f64().unwrap()
            > middle["summary"]["removed_volume_mm3"].as_f64().unwrap()
    );
    assert_eq!(
        preview.snapshot(&p, 0., 32).unwrap()["heights"],
        start["heights"]
    );
    let full = commands::simulate_project(&p, 1., 32).unwrap();
    assert_eq!(full["heights"], end["heights"]);
}
