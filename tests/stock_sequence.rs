//! Sequence and simulation checks against independently sampled cutter sweeps.
use opensourcecam::{
    cam::{self, Motion},
    commands,
    model::{Operation, Project, Strategy},
    operation::{Entry, PocketPattern},
    stock,
    tool_library::{LibraryTool, ToolKind},
};
use std::sync::atomic::AtomicBool;
fn job() -> Project {
    let mut p = Project {
        part: None,
        operations: vec![],
        ..Project::default()
    };
    p.workspace.clear();
    p.stock.width = 50.;
    p.stock.height = 40.;
    p.stock.thickness = 10.;
    p.tool_library.tools = vec![LibraryTool::new(1, 1, ToolKind::FlatEndMill)];
    p.active_tool = Some(1);
    p.active_preset = Some(1);
    p
}
fn pocket(p: &Project) -> Operation {
    let mut o = commands::operation_template(p, Strategy::Pocket);
    o.x = 5.;
    o.y = 5.;
    o.width = 40.;
    o.height = 30.;
    o.depth = 4.;
    o.settings.step_down = Some(1.);
    o.settings.pocket_pattern = PocketPattern::Offset;
    o.settings.entry = Entry::Plunge;
    o
}
#[test]
fn bore_preparation_follows_order_enabled_state_depth_and_real_tool_sweeps() {
    let mut p = job();
    let mut rough = commands::operation_template(&p, Strategy::CircularPocket);
    rough.x = 25.;
    rough.y = 20.;
    rough.width = 23.5;
    rough.depth = 4.;
    rough.settings.step_down = Some(1.);
    let mut bore = rough.clone();
    bore.strategy = Strategy::Bore;
    bore.width = 24.;
    bore.settings.precleared = false;
    p.operations = vec![rough, bore];
    cam::validate(&p).unwrap();
    for change in 0..3 {
        let mut bad = p.clone();
        match change {
            0 => bad.operations[0].enabled = false,
            1 => bad.operations.swap(0, 1),
            _ => bad.operations[0].depth = 2.,
        }
        assert!(cam::generate(&bad)
            .unwrap_err()
            .contains("not proven clear"));
    }
    let mut smaller = p.tool_library.tools[0].clone();
    smaller.id = 2;
    smaller.post.number = 2;
    smaller.geometry.insert("diameter".into(), 3.);
    p.tool_library.tools.push(smaller);
    p.operations[1].tool_id = Some(2);
    cam::generate(&p).unwrap();
    p.operations.remove(0);
    // An explicit external-stock assertion remains available, but never seeds
    // imaginary cleared cells into the shared sequence.
    p.operations[0].settings.precleared = true;
    cam::generate(&p).unwrap();
}
fn sampled_height(q: [f64; 2], previous: &[&cam::Move], radius: f64) -> f64 {
    previous
        .iter()
        .filter_map(|m| {
            let vx = m.to.x - m.from.x;
            let vy = m.to.y - m.from.y;
            let length = vx * vx + vy * vy;
            let t = if length > 1e-15 {
                ((q[0] - m.from.x) * vx + (q[1] - m.from.y) * vy) / length
            } else {
                0.
            };
            let t = t.clamp(0., 1.);
            let d = (q[0] - m.from.x - vx * t).hypot(q[1] - m.from.y - vy * t);
            (d <= radius).then_some(m.from.z + (m.to.z - m.from.z) * t)
        })
        .fold(0., f64::min)
}
#[test]
fn local_rapid_links_only_cross_previously_removed_material_and_save_travel() {
    let mut p = job();
    p.operations = vec![pocket(&p)];
    let mut interior = pocket(&p);
    interior.x = 13.;
    interior.y = 12.;
    interior.width = 24.;
    interior.height = 16.;
    interior.depth = 3.;
    interior.settings.pocket_pattern = PocketPattern::Raster;
    interior.settings.raster = opensourcecam::operation::Raster::PositiveX;
    p.operations.push(interior);
    let full = cam::generate(&p).unwrap();
    for op in &mut p.operations {
        op.settings.stock_clearance = Some(0.5);
    }
    let local = cam::generate(&p).unwrap();
    assert!(
        local.rapid_length + 2. < full.rapid_length,
        "Local {} / full {}",
        local.rapid_length,
        full.rapid_length
    );
    assert_eq!(local.moves.last().unwrap().to.z, p.travel_clearance());
    let mut prior = Vec::new();
    let radius = p.operation_tool(&p.operations[0]).unwrap().diameter / 2.;
    let mut low = 0;
    for m in &local.moves {
        if m.motion != Motion::Rapid {
            prior.push(m);
            continue;
        }
        // Check the full XY swept cutter disc against an independent dense
        // point model of all earlier cutter sweeps, never later cuts.
        if m.from.z.min(m.to.z) < 0. && (m.from.x != m.to.x || m.from.y != m.to.y) {
            low += 1;
            for i in 0..=24 {
                let t = i as f64 / 24.;
                let c = [
                    m.from.x + (m.to.x - m.from.x) * t,
                    m.from.y + (m.to.y - m.from.y) * t,
                ];
                for k in 0..48 {
                    let a = k as f64 * std::f64::consts::TAU / 48.;
                    for r in [0., radius / 2., radius] {
                        let q = [c[0] + r * a.cos(), c[1] + r * a.sin()];
                        if q[0] >= 0.
                            && q[0] <= p.stock.width
                            && q[1] >= 0.
                            && q[1] <= p.stock.height
                        {
                            assert!(
                                sampled_height(q, &prior, radius) + 0.49 <= m.from.z.min(m.to.z),
                                "Unsafe rapid {m:?} at {q:?}"
                            );
                        }
                    }
                }
            }
        }
    }
    assert!(
        low > 0,
        "Expected at least one certified link below stock top"
    );
    p.operations[0].settings.stock_clearance = Some(p.clearance + 1.);
    assert!(cam::generate(&p).is_err());
}
#[test]
fn conservative_simulation_bounds_removal_and_resets_mode_after_seeking() {
    let mut p = job();
    p.operations = vec![pocket(&p)];
    let playback = commands::PlaybackPreview::new(&p).unwrap();
    let sampled = stock::simulate(&p, &playback.path, 1., 128, &AtomicBool::new(false)).unwrap();
    let bound = stock::simulate_mode(
        &p,
        &playback.path,
        1.,
        128,
        &AtomicBool::new(false),
        50_000_000,
        true,
    )
    .unwrap();
    assert!(bound.summary.conservative);
    assert!(bound.summary.remaining_volume_mm3 >= sampled.summary.remaining_volume_mm3);
    assert_eq!(
        bound.summary.remaining_upper_bound_mm3,
        Some(bound.summary.remaining_volume_mm3)
    );
    let dx = bound.summary.cell_size[0];
    let dy = bound.summary.cell_size[1];
    let nx = bound.summary.resolution[0];
    let previous = playback
        .path
        .moves
        .iter()
        .filter(|m| m.motion != Motion::Rapid)
        .collect::<Vec<_>>();
    for y in 0..bound.summary.resolution[1] {
        for x in 0..nx {
            let h = bound.heights[y * nx + x];
            for (u, v) in [
                (0.01, 0.01),
                (0.99, 0.01),
                (0.01, 0.99),
                (0.99, 0.99),
                (0.5, 0.5),
            ] {
                assert!(
                    h + 1e-8
                        >= sampled_height(
                            [(x as f64 + u) * dx, (y as f64 + v) * dy],
                            &previous,
                            3.
                        )
                        .max(-p.stock.thickness)
                );
            }
        }
    }
    let mut previous_volume = f64::INFINITY;
    for progress in [0., 0.12, 0.5, 1.] {
        let frame = playback
            .frame_mode(&p, progress, 128, 1, 50_000_000, true)
            .unwrap();
        let v = frame.metadata["summary"]["remaining_upper_bound_mm3"]
            .as_f64()
            .unwrap();
        assert!(v <= previous_volume + 1e-6);
        previous_volume = v;
    }
    let changed = playback
        .frame_mode(&p, 1., 128, 1, 50_000_000, false)
        .unwrap();
    assert_eq!(changed.metadata["reset"], true);
    assert!(
        (changed.metadata["summary"]["remaining_volume_mm3"]
            .as_f64()
            .unwrap()
            - sampled.summary.remaining_volume_mm3)
            .abs()
            < 1e-7
    );
    let rewind = playback
        .frame_mode(&p, 0., 128, 1, 50_000_000, true)
        .unwrap();
    assert_eq!(rewind.metadata["reset"], true);
    assert_eq!(rewind.metadata["summary"]["removed_volume_mm3"], 0.);
}
#[test]
fn low_edge_exposure_is_checked_along_the_whole_chain() {
    let mut p = job();
    let mut first = pocket(&p);
    first.x = 3.;
    first.width = 15.;
    first.height = 25.;
    let mut second = first.clone();
    second.x = 32.;
    let mut e = commands::operation_template(&p, Strategy::Trace);
    e.settings.path = vec![[10., 15., -3.], [39., 15., -3.]];
    e.depth = 0.5;
    p.operations = vec![first, second, e];
    assert!(cam::generate(&p).unwrap_err().contains("exposed"));
}
#[test]
fn external_clearing_uses_the_silhouette_and_leaves_enclosed_holes() {
    use opensourcecam::{
        import::{Geometry, Mesh, Part, SourceFormat, Units},
        machining,
    };
    use std::sync::Arc;
    let mut p = job();
    // Closed polygonal annulus: independent outer and inner wall loops.
    let n = 48;
    let mut vertices = vec![];
    let mut triangles = vec![];
    for z in [-6., -1.] {
        for radius in [11., 4.] {
            for i in 0..n {
                let a = i as f64 * std::f64::consts::TAU / n as f64;
                vertices.push([25. + radius * a.cos(), 20. + radius * a.sin(), z]);
            }
        }
    }
    for i in 0..n {
        let j = (i + 1) % n;
        let [a, b, c, d, e, f, g, h] = [
            i,
            j,
            n + i,
            n + j,
            2 * n + i,
            2 * n + j,
            3 * n + i,
            3 * n + j,
        ]
        .map(|v| v as u32);
        triangles.extend([
            [a, d, b],
            [a, c, d],
            [e, f, h],
            [e, h, g],
            [a, b, f],
            [a, f, e],
            [c, h, d],
            [c, g, h],
        ]);
    }
    p.part = Some(Part {
        geometry: Arc::new(Geometry {
            bodies: vec![],
            removed_degenerate_triangles: 0,
            source_name: "original-annulus.obj".into(),
            format: SourceFormat::Obj,
            mesh: Mesh {
                vertices,
                triangles,
            },
            cad_source: None,
        }),
        units: Units::Millimetres,
        offset: [0.; 3],
        rotation: [0.; 3],
        visible: true,
    });
    let mut o = commands::operation_template(&p, Strategy::StockRough);
    o.x = 14.;
    o.y = 9.;
    o.width = 22.;
    o.height = 22.;
    o.depth = 3.;
    o.settings.step_down = Some(1.);
    machining::link(&mut o, p.part.as_ref().unwrap());
    p.operations = vec![o];
    let path = cam::generate(&p).unwrap();
    let mut corner = false;
    for m in path.moves.iter().filter(|m| m.motion == Motion::Cut) {
        for i in 0..=8 {
            let t = i as f64 / 8.;
            let x = m.from.x + (m.to.x - m.from.x) * t;
            let y = m.from.y + (m.to.y - m.from.y) * t;
            assert!(
                (x - 25.).hypot(y - 20.) > 13.8,
                "Exterior clearing entered a retained wall or enclosed hole"
            );
            if (x - 15.).hypot(y - 10.) < 3. {
                corner = true;
            }
        }
    }
    assert!(
        corner,
        "The circular silhouette must clear corners inside its bounding rectangle"
    );
}

#[test]
fn rest_clearing_skips_only_regions_proven_air_by_preceding_cuts() {
    let mut p = job();
    let first = pocket(&p);
    let mut rest = commands::operation_template(&p, Strategy::AreaRough);
    rest.x = 13.;
    rest.y = 12.;
    rest.width = 24.;
    rest.height = 16.;
    rest.depth = 3.;
    p.operations = vec![first, rest];
    let path = cam::generate(&p).unwrap();
    assert!(!path
        .moves
        .iter()
        .any(|m| m.operation == 1 && m.motion == Motion::Cut));
    p.operations[0].enabled = false;
    let uncut = cam::generate(&p).unwrap();
    assert!(uncut
        .moves
        .iter()
        .any(|m| m.operation == 1 && m.motion == Motion::Cut));
}

#[test]
fn conservative_nonrectangular_stock_retains_physical_sampled_columns() {
    let mut p = job();
    p.stock = opensourcecam::blank::StockDefinition {
        kind: opensourcecam::blank::StockKind::Cylinder,
        size: [40., 40., 10.],
        rotation: [0.; 3],
    }
    .build()
    .unwrap();
    let mut o = commands::operation_template(&p, Strategy::CircularPocket);
    o.x = 20.;
    o.y = 20.;
    o.width = 20.;
    o.depth = 3.;
    p.operations = vec![o];
    let path = cam::generate(&p).unwrap();
    let sim = stock::simulate_mode(
        &p,
        &path,
        1.,
        128,
        &AtomicBool::new(false),
        50_000_000,
        true,
    )
    .unwrap();
    assert!(
        !sim.summary.conservative,
        "An enclosing-box certificate must not replace physical cylinder corners"
    );
    assert!(sim.summary.remaining_upper_bound_mm3.unwrap() > sim.summary.remaining_volume_mm3);
    assert_eq!(sim.heights[0], -p.stock.thickness);
}
