use opensourcecam::{
    cam::{generate, Motion},
    model::{Operation, Project, Strategy},
    post::gcode,
};

fn one(strategy: Strategy) -> Project {
    let mut project = Project {
        operations: vec![Operation {
            strategy,
            ..Operation::default()
        }],
        ..Project::default()
    };
    if strategy == Strategy::Facing {
        let op = &mut project.operations[0];
        op.x = 0.0;
        op.y = 0.0;
        op.width = project.stock.width;
        op.height = project.stock.height;
    }
    if strategy == Strategy::CircularPocket {
        project.operations[0].x = project.stock.width / 2.0;
        project.operations[0].y = project.stock.height / 2.0;
        project.operations[0].width = 20.0;
    }
    project
}

#[test]
fn travel_retracts_before_xy_and_finishes_at_clearance() {
    for strategy in Strategy::ALL.into_iter().filter(|s| !s.is_surface()) {
        let p = one(strategy);
        let path = generate(&p).unwrap();
        assert!(!path.moves.is_empty());
        for m in &path.moves {
            assert!(m.to.x.is_finite() && m.to.y.is_finite() && m.to.z.is_finite());
            if m.motion == Motion::Rapid {
                if m.from.x != m.to.x || m.from.y != m.to.y {
                    assert_eq!(m.from.z, p.clearance);
                    assert_eq!(m.to.z, p.clearance);
                } else {
                    assert!(m.to.z >= m.from.z);
                }
            }
        }
        assert_eq!(path.moves.last().unwrap().to.z, p.clearance);
        assert!(path.estimated_seconds > 0.0);
    }
}

#[test]
fn depth_passes_stop_exactly_at_requested_depth() {
    let mut p = one(Strategy::Pocket);
    p.operations[0].depth = 2.4;
    p.tool.step_down = 0.7;
    let path = generate(&p).unwrap();
    let depths: Vec<_> = path
        .moves
        .iter()
        .filter(|m| m.motion == Motion::Plunge)
        .map(|m| m.to.z)
        .collect();
    assert_eq!(depths.len(), 4);
    assert_eq!(*depths.last().unwrap(), -2.4);
    assert!(path.moves.iter().all(|m| m.to.z >= -2.4));
}

#[test]
fn profile_centerline_uses_tool_radius_on_correct_side() {
    for (strategy, sign) in [
        (Strategy::InsideProfile, 1.0),
        (Strategy::OutsideProfile, -1.0),
    ] {
        let p = one(strategy);
        let op = &p.operations[0];
        let r = p.tool.diameter / 2.0 * sign;
        let path = generate(&p).unwrap();
        let cuts: Vec<_> = path
            .moves
            .iter()
            .filter(|m| m.motion == Motion::Cut)
            .collect();
        assert_eq!(cuts.len(), 12);
        for m in cuts {
            assert!(m.to.x >= op.x + r - 1e-8);
            assert!(m.to.x <= op.x + op.width - r + 1e-8);
            assert!(m.to.y >= op.y + r - 1e-8);
            assert!(m.to.y <= op.y + op.height - r + 1e-8);
        }
        assert_eq!(
            path.moves
                .iter()
                .find(|m| m.motion == Motion::Plunge)
                .unwrap()
                .to
                .x,
            op.x + r
        );
    }
}

#[test]
fn pocket_rows_cover_width_and_obey_maximum_stepover() {
    let mut p = one(Strategy::Pocket);
    p.operations[0].depth = 1.0;
    let path = generate(&p).unwrap();
    let op = &p.operations[0];
    let r = p.tool.diameter / 2.0;
    let rows: Vec<_> = path
        .moves
        .iter()
        .filter(|m| {
            m.motion == Motion::Cut && (m.to.x - m.from.x).abs() > 1e-8 && m.to.y == m.from.y
        })
        .collect();
    assert!(rows.len() > 2);
    for m in &rows {
        assert!(((m.to.x - m.from.x).abs() - (op.width - p.tool.diameter)).abs() < 1e-8);
    }
    let mut ys: Vec<_> = rows.iter().map(|m| m.to.y).collect();
    ys.sort_by(f64::total_cmp);
    ys.dedup();
    assert_eq!(ys[0], op.y + r);
    assert_eq!(*ys.last().unwrap(), op.y + op.height - r);
    assert!(ys
        .windows(2)
        .all(|w| w[1] - w[0] <= p.tool.diameter * p.tool.step_over + 1e-8));
}

#[test]
fn peck_drill_retracts_between_every_feed() {
    let p = one(Strategy::Drill);
    let path = generate(&p).unwrap();
    let mut pecks = 0;
    for (index, m) in path.moves.iter().enumerate() {
        if m.motion == Motion::Plunge {
            pecks += 1;
            assert_eq!(m.from.x, m.to.x);
            assert_eq!(m.from.y, m.to.y);
            assert_eq!(m.from.z, p.clearance);
            assert_eq!(path.moves[index + 1].motion, Motion::Rapid);
            assert_eq!(path.moves[index + 1].to.z, p.clearance);
        }
    }
    assert_eq!(pecks, 3);
}

#[test]
fn invalid_numbers_dimensions_depths_and_path_explosion_are_rejected() {
    let base = one(Strategy::Pocket);
    let cases: Vec<fn(&mut Project)> = vec![
        |p| p.tool.feed = f64::NAN,
        |p| p.clearance = f64::INFINITY,
        |p| p.tool.plunge = 0.0,
        |p| p.tool.step_over = 0.0,
        |p| p.operations[0].depth = 100.0,
        |p| p.operations[0].x = -1.0,
        |p| p.operations[0].width = 1000.0,
        |p| p.tool.diameter = 60.0,
        |p| p.tool.step_down = 0.001,
        |p| {
            p.tool.diameter = 0.001;
            p.tool.step_over = 0.01;
        },
        |p| p.operations.clear(),
        |p| p.format_version = 100,
    ];
    for mutate in cases {
        let mut p = base.clone();
        mutate(&mut p);
        assert!(generate(&p).is_err(), "Invalid project accepted: {p:?}");
        assert!(gcode(&p).is_err());
    }
    let mut p = base;
    p.operations[0].depth = 12.0;
    p.tool.step_down = 0.01;
    p.tool.step_over = 0.01;
    assert!(generate(&p).unwrap_err().contains("200,000"));
}

#[test]
fn disabled_operations_are_skipped_and_order_is_preserved() {
    let mut p = Project::default();
    p.operations[0].enabled = false;
    p.operations[0].depth = -100.0;
    let path = generate(&p).unwrap();
    assert!(path.moves.iter().all(|m| m.operation == 1));
    p.operations[1].enabled = false;
    assert!(generate(&p).is_err());
}

#[test]
fn post_has_explicit_modes_clearance_first_and_no_comment_injection() {
    let mut p = one(Strategy::Pocket);
    p.name = "part)\nM3 S99999\n(unsafe".into();
    let code = gcode(&p).unwrap();
    assert!(code.contains("G21\nG90\nG17\nG94\nG40\nG49\nG80\nG54\n"));
    let lines: Vec<_> = code.lines().collect();
    let first_motion = lines
        .iter()
        .position(|line| line.starts_with("G0 ") || line.starts_with("G1 "))
        .unwrap();
    assert_eq!(lines[first_motion], "G0 Z5.0000");
    assert_eq!(
        lines.iter().filter(|line| line.starts_with("M3 ")).count(),
        1
    );
    assert!(!code.contains("NaN"));
    assert!(code.ends_with("G0 Z5.0000\nM5\nM9\nM2\n"));
}

#[test]
fn json_roundtrip_and_schema_are_stable() {
    let p = Project::default();
    let json = serde_json::to_string_pretty(&p).unwrap();
    let roundtrip: Project = serde_json::from_str(&json).unwrap();
    assert_eq!(p, roundtrip);
    assert_eq!(gcode(&p).unwrap(), gcode(&roundtrip).unwrap());
    let bad = json.replacen("\"format_version\"", "\"unknown_field\"", 1);
    assert!(serde_json::from_str::<Project>(&bad).is_err());
}

#[test]
fn varied_tools_and_passes_preserve_pocket_envelope() {
    for diameter in [0.5, 3.175, 6.0, 12.0] {
        for step_over in [0.1, 0.4, 1.0] {
            let mut p = one(Strategy::Pocket);
            p.tool.diameter = diameter;
            p.tool.step_over = step_over;
            p.tool.step_down = 0.8;
            let op = &p.operations[0];
            let path = generate(&p).unwrap();
            for m in path.moves.iter().filter(|m| m.motion != Motion::Rapid) {
                assert!(m.to.x >= op.x + diameter / 2.0 - 1e-8);
                assert!(m.to.x <= op.x + op.width - diameter / 2.0 + 1e-8);
                assert!(m.to.y >= op.y + diameter / 2.0 - 1e-8);
                assert!(m.to.y <= op.y + op.height - diameter / 2.0 + 1e-8);
                assert!(m.to.z >= -op.depth);
            }
        }
    }
}
