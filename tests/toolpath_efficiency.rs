#[path = "../examples/support/toolpath_cases.rs"]
mod cases;
use opensourcecam::{
    cam::{self, Motion},
    collision, commands,
    import::{Geometry, Mesh, Part, SourceFormat, Units},
    model::Strategy,
    operation::{MillingMode, Raster},
    post, stock,
    workspace::{ModelKind, WorkspaceModel},
};
use std::sync::{atomic::AtomicBool, Arc};

#[test]
fn every_strategy_generates_and_posts_finite_bounded_motion() {
    let cases = cases::cases();
    assert_eq!(cases.len(), Strategy::ALL.len() + 4);
    for case in cases {
        let path = cam::generate(&case.project).unwrap_or_else(|e| panic!("{}: {e}", case.name));
        assert!(!path.moves.is_empty(), "{}", case.name);
        assert!(path.moves.len() <= 200_000);
        for m in &path.moves {
            assert!(
                m.seconds().is_finite() && m.seconds() > 0.,
                "{}: {m:?}",
                case.name
            );
            assert!(m.to.x.is_finite() && m.to.y.is_finite() && m.to.z.is_finite());
        }
        assert_eq!(
            path.moves.last().unwrap().to.z,
            case.project.travel_clearance()
        );
        let nc = post::gcode(&case.project).unwrap_or_else(|e| panic!("{}: {e}", case.name));
        assert!(!nc.contains("NaN") && !nc.contains("inf"));
        if !path.arcs.is_empty() {
            assert!(
                nc.contains("G2 ") || nc.contains("G3 "),
                "{} lost native arcs",
                case.name
            );
        }
    }
}

#[test]
fn numeric_outside_trims_air_without_changing_remaining_stock() {
    let p = cases::numeric(Strategy::StockRough);
    let optimized = cam::generate(&p).unwrap();
    let mut conservative = p.clone();
    conservative.operations[0].settings.milling_mode = Some(MillingMode::Climb);
    let original = cam::generate(&conservative).unwrap();
    assert!(optimized.estimated_seconds < original.estimated_seconds * 0.8);
    let a = stock::simulate(&p, &optimized, 1., 96, &AtomicBool::new(false)).unwrap();
    let b = stock::simulate(&conservative, &original, 1., 96, &AtomicBool::new(false)).unwrap();
    for (i, (a, b)) in a.heights.iter().zip(&b.heights).enumerate() {
        assert!(
            (a - b).abs() < 1e-7,
            "Air trimming changed stock column {i}: {a} / {b}"
        );
    }
    // Independently check all low XY rapids against earlier flat cutter sweeps.
    let width = p.stock.width as usize;
    let height = p.stock.height as usize;
    let mut heights = vec![0_f64; width * height];
    let mut low = 0;
    let radius = p.operation_tool(&p.operations[0]).unwrap().diameter / 2.;
    for m in &optimized.moves {
        if m.motion == Motion::Rapid
            && (m.from.x != m.to.x || m.from.y != m.to.y)
            && m.to.z < 0.5 - 1e-8
        {
            low += 1;
        }
        for y in 0..height {
            for x in 0..width {
                let q = [x as f64 + 0.5, y as f64 + 0.5];
                if opensourcecam::planar::distance(
                    q,
                    [m.from.x, m.from.y, 0.],
                    [m.to.x, m.to.y, 0.],
                ) > radius
                {
                    continue;
                }
                let h = &mut heights[y * width + x];
                if m.motion == Motion::Rapid && (m.from.x != m.to.x || m.from.y != m.to.y) {
                    assert!(
                        *h + 0.49 <= m.from.z.min(m.to.z) + 1e-8,
                        "Rapid crosses uncut stock at {q:?}: {m:?}"
                    );
                } else if matches!(m.motion, Motion::Cut | Motion::Plunge) {
                    assert!(
                        (m.from.z - m.to.z).abs() < 1e-8
                            || (m.from.x == m.to.x && m.from.y == m.to.y)
                    );
                    *h = h.min(m.from.z.min(m.to.z));
                }
            }
        }
    }
    assert!(low > 0);
}

#[test]
fn facing_preparation_skips_completed_layers_and_3d_roughing_reaches_its_floor() {
    for strategy in [
        Strategy::Pocket,
        Strategy::StockRough,
        Strategy::AreaRough,
        Strategy::SurfaceRough,
    ] {
        let mut p = if strategy.is_surface() {
            cases::cases()
                .into_iter()
                .find(|c| c.name == "SurfaceRough")
                .unwrap()
                .project
        } else {
            cases::numeric(strategy)
        };
        let mut face = commands::operation_template(&p, Strategy::Facing);
        face.depth = p.part.as_ref().map_or(1., |part| -part.bounds().max[2]);
        face.settings.step_down = Some(1.);
        p.operations[0].settings.step_down = Some(1.);
        let bottom = -p.operations[0].depth + p.operations[0].settings.axial_allowance;
        let face_bottom = -face.depth;
        p.operations.insert(0, face);
        let path = cam::generate(&p).unwrap();
        let cuts: Vec<_> = path
            .moves
            .iter()
            .filter(|m| m.operation == 1 && m.motion == Motion::Cut)
            .collect();
        let tolerance = if strategy.is_surface() { 0.0002 } else { 0. };
        assert!(
            cuts.iter()
                .all(|m| m.to.z <= (face_bottom - 1.).max(bottom) + tolerance + 1e-7),
            "{strategy:?} repeated faced layers"
        );
        assert!(
            cuts.iter()
                .any(|m| (m.to.z - bottom - tolerance).abs() < 1e-7),
            "{strategy:?} stopped above the floor"
        );
    }
}

fn block() -> Part {
    Part {
        rotation: [0.; 3],
        offset: [0.; 3],
        units: Units::Millimetres,
        visible: false,
        geometry: Arc::new(Geometry {
            bodies: vec![],
            removed_degenerate_triangles: 0,
            source_name: "Test clamp".into(),
            format: SourceFormat::Obj,
            cad_source: None,
            mesh: Mesh {
                vertices: vec![
                    [27., 10., -1.],
                    [33., 10., -1.],
                    [33., 30., -1.],
                    [27., 30., -1.],
                    [27., 10., 3.],
                    [33., 10., 3.],
                    [33., 30., 3.],
                    [27., 30., 3.],
                ],
                triangles: vec![
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
                ],
            },
        }),
    }
}

#[test]
fn implicit_optimized_clearance_includes_hidden_fixtures() {
    let mut p = cases::numeric(Strategy::Pocket);
    let op = &mut p.operations[0];
    op.x = 5.;
    op.y = 10.;
    op.width = 15.;
    op.height = 20.;
    let mut second = op.clone();
    second.x = 40.;
    p.operations.push(second);
    p.workspace
        .push(WorkspaceModel::new(1, ModelKind::Fixture, block()));
    assert!(p
        .operations
        .iter()
        .all(|o| o.settings.stock_clearance.is_none()));
    let path = cam::generate(&p).unwrap();
    collision::check(&p, &path).unwrap();
    assert!(path.moves.iter().any(|m| m.operation == 1
        && m.motion == Motion::Rapid
        && m.from.x < 27.
        && m.to.x > 33.
        && m.from.z > 3.5));
}

#[test]
fn optimized_facing_uses_fewer_passes_but_preserves_one_way_direction() {
    let p = cases::numeric(Strategy::Facing);
    let path = cam::generate(&p).unwrap();
    let mut full = p.clone();
    full.operations[0].settings.milling_mode = Some(MillingMode::Climb);
    let original = cam::generate(&full).unwrap();
    assert!(path.moves.len() < original.moves.len());
    assert!(path.estimated_seconds < original.estimated_seconds);
    for mode in [Raster::PositiveX, Raster::NegativeX] {
        let mut one = p.clone();
        one.operations[0].settings.raster = mode;
        for m in cam::generate(&one)
            .unwrap()
            .moves
            .iter()
            .filter(|m| m.motion == Motion::Cut)
        {
            assert!(if mode == Raster::PositiveX {
                m.to.x >= m.from.x - 1e-8
            } else {
                m.to.x <= m.from.x + 1e-8
            });
        }
    }
}

#[test]
fn optimized_facing_retains_x_passes_when_y_overrun_hits_a_clamp() {
    let mut p = cases::numeric(Strategy::Facing);
    let mut clamp = block();
    for v in &mut Arc::make_mut(&mut clamp.geometry).mesh.vertices {
        v[1] += 84.;
    }
    p.workspace
        .push(WorkspaceModel::new(1, ModelKind::Fixture, clamp));
    let path = cam::generate(&p).unwrap();
    collision::check(&p, &path).unwrap();
    assert!(path
        .moves
        .iter()
        .any(|m| m.motion == Motion::Cut && (m.to.x - m.from.x).abs() > p.stock.width));
    assert!(!path
        .moves
        .iter()
        .any(|m| m.motion == Motion::Cut && (m.to.y - m.from.y).abs() > p.stock.height));
}

#[test]
fn outside_clearing_keeps_each_requested_spring_pass_and_finish_feed() {
    for mode in [MillingMode::Optimized, MillingMode::Climb] {
        let mut p = cases::numeric(Strategy::StockRough);
        p.operations[0].settings.milling_mode = Some(mode);
        p.operations[0].settings.finish_feed = Some(123.);
        let base = cam::generate(&p).unwrap();
        let op = &p.operations[0];
        let y = op.y - p.operation_tool(op).unwrap().diameter / 2.;
        let passes = |path: &cam::Toolpath| {
            path.moves
                .iter()
                .filter(|m| {
                    m.motion == Motion::Cut
                        && (m.feed - 123.).abs() < 1e-8
                        && (m.from.z + op.depth).abs() < 1e-8
                        && (m.to.z + op.depth).abs() < 1e-8
                        && (m.from.y - y).abs() < 1e-8
                        && (m.to.y - y).abs() < 1e-8
                        && (m.to.x - m.from.x).abs() >= op.width - 1e-7
                })
                .count()
        };
        let before = passes(&base);
        let mut repeated = p.clone();
        repeated.operations[0].settings.spring_passes = 2;
        let path = cam::generate(&repeated).unwrap();
        assert_eq!(
            passes(&path),
            before + 2,
            "{mode:?} omitted a requested wall pass"
        );
    }
}

#[test]
fn physical_offset_spacing_covers_rotated_regions_and_islands() {
    let rectangle = |w: f64, h: f64, angle: f64| {
        let (s, c) = angle.to_radians().sin_cos();
        [
            (-w / 2., -h / 2.),
            (w / 2., -h / 2.),
            (w / 2., h / 2.),
            (-w / 2., h / 2.),
        ]
        .map(|(x, y)| [30. + x * c - y * s, 45. + x * s + y * c, -3.])
        .to_vec()
    };
    let inside = |q: [f64; 2], ring: &[[f64; 3]]| {
        (0..ring.len()).all(|i| {
            let a = ring[i];
            let b = ring[(i + 1) % ring.len()];
            (b[0] - a[0]) * (q[1] - a[1]) - (b[1] - a[1]) * (q[0] - a[0]) >= -1e-9
        })
    };
    let margin = |q: [f64; 2], ring: &[[f64; 3]]| {
        (0..ring.len())
            .map(|i| opensourcecam::planar::distance(q, ring[i], ring[(i + 1) % ring.len()]))
            .fold(f64::INFINITY, f64::min)
    };
    for angle in [0., 15., 30., 45., 75.] {
        for strategy in [Strategy::AreaRough, Strategy::StockRough] {
            let mut p = cases::numeric(strategy);
            let outer = rectangle(30., 40., angle);
            let island = rectangle(8., 8., angle);
            p.operations[0].settings.path = outer.clone();
            p.operations[0].settings.closed = true;
            p.operations[0].settings.height_offset = Some(0.);
            if strategy == Strategy::AreaRough {
                p.operations[0].settings.islands = vec![island.clone()];
            }
            let radius = p.operation_tool(&p.operations[0]).unwrap().diameter / 2.;
            let path = cam::generate(&p).unwrap();
            let cuts: Vec<_> = path
                .moves
                .iter()
                .filter(|m| {
                    m.motion == Motion::Cut
                        && (m.from.z + 3.).abs() < 1e-8
                        && (m.to.z + 3.).abs() < 1e-8
                })
                .collect();
            let mut checked = 0;
            for x in 0..60 {
                for y in 0..90 {
                    let q = [x as f64 + 0.5, y as f64 + 0.5];
                    let reachable = if strategy == Strategy::AreaRough {
                        inside(q, &outer)
                            && margin(q, &outer) > radius + 0.75
                            && !inside(q, &island)
                            && margin(q, &island) > radius + 0.75
                    } else {
                        !inside(q, &outer) && margin(q, &outer) > radius + 0.75
                    };
                    if !reachable {
                        continue;
                    }
                    checked += 1;
                    let d = cuts
                        .iter()
                        .map(|m| {
                            opensourcecam::planar::distance(
                                q,
                                [m.from.x, m.from.y, 0.],
                                [m.to.x, m.to.y, 0.],
                            )
                        })
                        .fold(f64::INFINITY, f64::min);
                    assert!(
                        d <= radius + 1e-7,
                        "{strategy:?}, {angle} deg left an uncut region at {q:?}: nearest cut {d}"
                    );
                }
            }
            assert!(checked > 100);
        }
    }
}

#[test]
fn straight_offsets_use_the_stepover_without_exceeding_its_maximum() {
    let mut p = cases::numeric(Strategy::AreaRough);
    // Exact contours use the full spacing budget. Optimized contours reserve
    // extra room for bevel deviation, tested on rotated walls below.
    p.operations[0].settings.milling_mode = Some(MillingMode::Climb);
    let tool = p.operation_tool(&p.operations[0]).unwrap();
    let path = cam::generate(&p).unwrap();
    // Compare successive offsets from one wall, rather than opposite sides
    // of the terminal core or the ends of nested rectangular loops.
    let mut rows: Vec<_> = path
        .moves
        .iter()
        .filter(|m| {
            m.motion == Motion::Cut
                && (m.to.z + 3.).abs() < 1e-8
                && (m.from.y - m.to.y).abs() < 1e-8
                && (m.to.x - m.from.x).abs() > 8.
                && m.to.y < p.operations[0].y + p.operations[0].height / 2.
        })
        .map(|m| m.to.y)
        .collect();
    rows.sort_by(f64::total_cmp);
    rows.dedup_by(|a, b| (*a - *b).abs() < 1e-7);
    let requested = tool.diameter * tool.step_over;
    let gaps: Vec<_> = rows.windows(2).map(|q| q[1] - q[0]).collect();
    assert!(
        gaps.iter().any(|d| *d >= requested * 0.8),
        "Offsets still underuse the requested stepover"
    );
    assert!(
        gaps.iter().all(|d| *d <= requested + 1e-6),
        "Offset spacing exceeds the requested maximum"
    );
    for angle in [15_f64, 30., 45., 75.] {
        let (s, c) = angle.to_radians().sin_cos();
        let mut rotated = p.clone();
        rotated.operations[0].settings.milling_mode = Some(MillingMode::Optimized);
        rotated.operations[0].settings.path = [(-15., -20.), (15., -20.), (15., 20.), (-15., 20.)]
            .map(|(x, y)| [30. + x * c - y * s, 45. + x * s + y * c, -3.])
            .to_vec();
        rotated.operations[0].settings.closed = true;
        let path = cam::generate(&rotated).unwrap();
        let local = |q: cam::Point3| {
            [
                (q.x - 30.) * c + (q.y - 45.) * s,
                -(q.x - 30.) * s + (q.y - 45.) * c,
            ]
        };
        let mut offsets = Vec::new();
        for m in path.moves.iter().filter(|m| {
            m.motion == Motion::Cut && (m.to.z + 3.).abs() < 1e-8 && (m.from.z + 3.).abs() < 1e-8
        }) {
            let a = local(m.from);
            let b = local(m.to);
            if (b[0] - a[0]).abs() < 1e-8 || a[0] * b[0] > 1e-8 {
                continue;
            }
            let t = (-a[0] / (b[0] - a[0])).clamp(0., 1.);
            let d = a[1] + t * (b[1] - a[1]);
            if d < 0. {
                offsets.push(d);
            }
        }
        offsets.sort_by(f64::total_cmp);
        offsets.dedup_by(|a, b| (*a - *b).abs() < 1e-7);
        assert!(offsets.len() > 2);
        assert!(
            offsets.windows(2).all(|q| q[1] - q[0] <= requested + 1e-6),
            "{angle} deg offsets exceed maximum: {offsets:?}"
        );
    }
}
