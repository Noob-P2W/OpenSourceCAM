//! Interpret posted centre-format arcs independently of the generator.
use opensourcecam::{
    cam::{self, ArcSpan, Motion, Move, Point3, Toolpath},
    commands,
    model::{Project, Strategy},
    operation::{Direction, Entry, MillingMode, PocketPattern},
    post::{self, PostProcessor},
    tool_library::{LibraryTool, ToolKind},
};
fn job(strategy: Strategy) -> Project {
    let mut p = Project {
        part: None,
        operations: vec![],
        ..Project::default()
    };
    p.workspace.clear();
    let kind = if strategy == Strategy::Thread {
        ToolKind::ThreadMill
    } else {
        ToolKind::FlatEndMill
    };
    p.tool_library.tools = vec![LibraryTool::new(1, 1, kind)];
    p.active_tool = Some(1);
    p.active_preset = Some(1);
    let mut o = commands::operation_template(&p, strategy);
    o.x = 40.123456;
    o.y = 30.987654;
    o.width = 24.;
    o.height = 20.;
    o.depth = 2.3;
    o.settings.step_down = Some(1.);
    o.settings.precleared = true;
    o.settings.pocket_pattern = PocketPattern::Offset;
    o.settings.entry = Entry::Helix;
    p.operations = vec![o];
    p
}
fn words(line: &str) -> std::collections::BTreeMap<char, f64> {
    line.split_whitespace()
        .filter_map(|w| {
            let (letter, number) = w.split_at(1);
            number
                .parse()
                .ok()
                .map(|n| (letter.chars().next().unwrap(), n))
        })
        .collect()
}
fn check_nc(p: &Project) {
    let path = cam::generate(p).unwrap();
    let nc = post::gcode(p).unwrap();
    assert!(nc.contains("G17\nG91.1\n"));
    assert!(!path.arcs.is_empty());
    let mut pos = [0.; 3];
    let mut arc_index = 0;
    for line in nc.lines() {
        if !["G0 ", "G1 ", "G2 ", "G3 "]
            .iter()
            .any(|s| line.starts_with(s))
        {
            continue;
        }
        let w = words(line);
        let mut end = pos;
        for (i, c) in ['X', 'Y', 'Z'].iter().enumerate() {
            if let Some(v) = w.get(c) {
                end[i] = *v;
            }
        }
        if line.starts_with("G2 ") || line.starts_with("G3 ") {
            let arc = &path.arcs[arc_index];
            arc_index += 1;
            let centre = [pos[0] + w[&'I'], pos[1] + w[&'J']];
            assert!((centre[0] - arc.center[0]).abs() < 1.1e-6);
            assert!((centre[1] - arc.center[1]).abs() < 1.1e-6);
            let r = (pos[0] - centre[0]).hypot(pos[1] - centre[1]);
            let end_r = (end[0] - centre[0]).hypot(end[1] - centre[1]);
            assert!((r - end_r).abs() < 0.0001, "Posted radii mismatch: {line}");
            let a = (pos[1] - centre[1]).atan2(pos[0] - centre[0]);
            let b = (end[1] - centre[1]).atan2(end[0] - centre[0]);
            let ccw = line.starts_with("G3 ");
            assert_eq!(ccw, arc.sweep > 0.);
            let sweep = if ccw {
                (b - a).rem_euclid(std::f64::consts::TAU)
            } else {
                -(a - b).rem_euclid(std::f64::consts::TAU)
            };
            assert!((sweep - arc.sweep).abs() < 0.0001);
            let samples = &path.moves[arc.start..arc.end];
            assert!((w[&'F'] - samples[0].feed).abs() < 0.000051);
            for (i, m) in samples.iter().enumerate() {
                let t = (i + 1) as f64 / samples.len() as f64;
                let theta = a + sweep * t;
                let reconstructed = Point3 {
                    x: centre[0] + r * theta.cos(),
                    y: centre[1] + r * theta.sin(),
                    z: pos[2] + (end[2] - pos[2]) * t,
                };
                assert!(
                    reconstructed.distance(m.to) < 0.00015,
                    "Posted helix disagrees with preview"
                );
            }
        }
        pos = end;
    }
    assert_eq!(arc_index, path.arcs.len());
    let mut linear = p.clone();
    linear.post.linearize_arcs = true;
    let g1 = post::gcode(&linear).unwrap();
    assert!(!g1
        .lines()
        .any(|l| l.starts_with("G2 ") || l.starts_with("G3 ")));
    assert!(nc.lines().count() < g1.lines().count());
}
#[test]
fn all_supported_posts_preserve_planar_and_fractional_helical_arcs() {
    for processor in [
        PostProcessor::Generic,
        PostProcessor::Grbl,
        PostProcessor::Mach3,
        PostProcessor::LinuxCnc,
    ] {
        for strategy in [
            Strategy::Bore,
            Strategy::Circular,
            Strategy::Thread,
            Strategy::CircularPocket,
            Strategy::Pocket,
        ] {
            for clockwise in [false, true] {
                for mode in [MillingMode::Climb, MillingMode::Conventional] {
                    let mut p = job(strategy);
                    p.post.processor = processor;
                    p.tool_library.tools[0].clockwise = clockwise;
                    p.operations[0].settings.milling_mode = Some(mode);
                    p.operations[0].settings.direction = if clockwise {
                        Direction::Clockwise
                    } else {
                        Direction::Counterclockwise
                    };
                    check_nc(&p);
                }
            }
        }
    }
}
#[test]
fn conical_spirals_are_not_misrepresented_as_constant_radius_arcs() {
    let mut p = job(Strategy::Bore);
    p.operations[0].settings.taper_angle = Some(5.);
    let path = cam::generate(&p).unwrap();
    assert!(path
        .arcs
        .iter()
        .all(|a| (path.moves[a.end - 1].to.z - path.moves[a.start].from.z).abs() < 1e-7));
}
fn short_arc(phase: f64, sweep: f64) -> Toolpath {
    let r = 10.;
    let c = [30., 30.];
    let point = |a: f64| Point3 {
        x: c[0] + r * a.cos(),
        y: c[1] + r * a.sin(),
        z: -2.,
    };
    let moves = (0..3)
        .map(|i| Move {
            from: point(phase + sweep * i as f64 / 3.),
            to: point(phase + sweep * (i + 1) as f64 / 3.),
            feed: 100.,
            motion: Motion::Cut,
            operation: 0,
        })
        .collect();
    Toolpath {
        arcs: vec![ArcSpan {
            start: 0,
            end: 3,
            center: c,
            sweep,
        }],
        moves,
        estimated_seconds: 1.,
        cut_length: 1.,
        rapid_length: 0.,
    }
}
#[test]
fn arc_extrema_and_tangents_have_stronger_preflight_than_sample_chords() {
    let mut p = job(Strategy::Bore);
    let path = short_arc(-0.1, 0.2);
    let m = p.machine.as_mut().unwrap();
    m.enforce_limits = true;
    m.work_origin = [0.; 3];
    m.axis_max[0] = 39.999;
    for s in &path.moves {
        m.check_point(s.to).unwrap();
    }
    assert!(
        !opensourcecam::arcs::preflight(&p, &path).unwrap(),
        "The true curve crosses X40 between samples"
    );
    let m = p.machine.as_mut().unwrap();
    m.axis_max[0] = 100.;
    m.configuration.axis_feed = [1000., 99.99, 1000.];
    let feed_path = short_arc(-0.075, 0.2);
    for s in &feed_path.moves {
        m.check_move(s).unwrap();
    }
    assert!(
        !opensourcecam::arcs::preflight(&p, &feed_path).unwrap(),
        "True tangent reaches the full Y feed"
    );
    let mut malformed = path.clone();
    malformed.moves[1].feed += 1.;
    assert!(opensourcecam::arcs::preflight(&p, &malformed).is_err());
}

#[test]
fn external_arcs_and_manual_tool_changes_preserve_centres_and_feed_boundaries() {
    for strategy in [Strategy::Bore, Strategy::Circular, Strategy::Thread] {
        let mut p = job(strategy);
        p.operations[0].settings.outside = true;
        check_nc(&p);
    }
    let mut p = job(Strategy::Bore);
    p.post.processor = PostProcessor::Mach3;
    let mut second = LibraryTool::new(2, 2, ToolKind::FlatEndMill);
    second.geometry.insert("diameter".into(), 3.);
    second.post.length_offset = Some(2);
    p.tool_library.tools[0].post.length_offset = Some(1);
    p.tool_library.tools.push(second);
    let mut op = p.operations[0].clone();
    op.tool_id = Some(2);
    op.settings.feed = Some(333.);
    op.settings.finish_feed = Some(250.);
    p.operations.push(op);
    let c = &mut p.machine.as_mut().unwrap().tool_changer;
    c.mode = opensourcecam::machine::ChangeMode::Manual;
    c.manual_position = Some([10., 10., 25.]);
    check_nc(&p);
    let nc = post::gcode(&p).unwrap();
    assert!(nc.contains("M0\nG21\nG90\nG17\nG91.1\n"));
}

#[test]
fn incremental_centres_use_the_exact_posted_cursor_at_decimal_ties() {
    let mut p = job(Strategy::Bore);
    p.operations[0].x = 40.03125;
    p.operations[0].y = 30.03125;
    check_nc(&p);
}
