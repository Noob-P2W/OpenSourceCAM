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
