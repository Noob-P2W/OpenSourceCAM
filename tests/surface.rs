use opensourcecam::{
    cam::{self, Motion},
    import::{Geometry, Mesh, Part, SourceFormat, Units},
    machining::{self, PlanOptions},
    model::{Project, Strategy},
    post, surface,
    tool_library::ToolKind,
};
use std::sync::{atomic::AtomicBool, Arc};

fn wedge() -> Project {
    let mesh = Mesh {
        vertices: vec![
            [0., 0., 0.],
            [8., 0., 0.],
            [8., 6., 0.],
            [0., 6., 0.],
            [0., 0., 2.],
            [8., 0., 6.],
            [8., 6., 6.],
            [0., 6., 2.],
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
    };
    let part = Part {
        geometry: Arc::new(Geometry {
            source_name: "ramp.obj".into(),
            format: SourceFormat::Obj,
            mesh,
            cad_source: None,
        }),
        units: Units::Millimetres,
        offset: [0.; 3],
        visible: true,
    };
    let mut project = Project::from_part(part).unwrap();
    project.ensure_tool_library();
    project.tool_library.tools[0]
        .geometry
        .insert("diameter".into(), 2.);
    project
}
fn plan(ball: bool) -> Project {
    let mut p = wedge();
    if ball {
        p.tool_library.tools[0].kind = ToolKind::BallEndMill;
    }
    surface::from_stock(
        &p,
        &PlanOptions {
            xy_margin: 2.,
            top_allowance: 1.,
            backing: 1.,
            ..Default::default()
        },
        &AtomicBool::new(false),
    )
    .unwrap()
    .project
}

#[test]
fn ramp_roughs_then_finishes_with_real_xyz_and_safe_travel() {
    for ball in [false, true] {
        let p = plan(ball);
        let path = cam::generate(&p).unwrap();
        assert_eq!(p.operations.len(), 2);
        assert!(p.operations.iter().all(|o| o.model_link.is_some()));
        assert!(path.moves.iter().any(|m| m.operation == 1
            && m.motion == Motion::Cut
            && (m.from.z - m.to.z).abs() > 0.01));
        for m in &path.moves {
            if m.motion == Motion::Rapid && (m.from.x != m.to.x || m.from.y != m.to.y) {
                assert_eq!(m.from.z, p.clearance);
                assert_eq!(m.to.z, p.clearance);
            }
            if m.motion == Motion::Cut {
                // Dense points on the analytic ramp, against each swept cutter.
                let r = p
                    .operation_tool(&p.operations[m.operation])
                    .unwrap()
                    .diameter
                    / 2.;
                for k in 0..=8 {
                    let t = k as f64 / 8.;
                    let round = |v: f64| (v * 10000.).round() / 10000.;
                    let x = round(m.from.x) + t * (round(m.to.x) - round(m.from.x));
                    let y = round(m.from.y) + t * (round(m.to.y) - round(m.from.y));
                    let z = round(m.from.z) + t * (round(m.to.z) - round(m.from.z));
                    for ix in 0..=16 {
                        for iy in 0..=12 {
                            let px = 2. + ix as f64 * 0.5;
                            let py = 2. + iy as f64 * 0.5;
                            let d = (x - px).hypot(y - py);
                            if d <= r {
                                let profile = if ball {
                                    r - (r * r - d * d).max(0.).sqrt()
                                } else {
                                    0.
                                };
                                let target = -5. + 0.5 * (px - 2.);
                                assert!(
                                    z + profile >= target - 0.00001,
                                    "cutter below ramp: {z} + {profile} < {target}"
                                );
                            }
                        }
                    }
                }
            }
        }
        let nc = post::gcode(&p).unwrap();
        assert!(nc.contains("G1"));
    }
}

#[test]
fn rejects_missing_roughing_open_mesh_stale_model_and_invalid_grid() {
    let p = plan(false);
    let mut bad = p.clone();
    bad.operations[0].enabled = false;
    assert!(cam::generate(&bad).unwrap_err().contains("earlier enabled"));
    let mut bad = p.clone();
    bad.part.as_mut().unwrap().offset[0] += 0.1;
    assert!(cam::generate(&bad)
        .unwrap_err()
        .contains("placement changed"));
    let mut bad = p.clone();
    bad.operations[0].settings.surface_step = Some(0.001);
    assert!(cam::generate(&bad).unwrap_err().contains("grid step"));
    let mut bad = p.clone();
    Arc::make_mut(&mut bad.part.as_mut().unwrap().geometry)
        .mesh
        .triangles
        .pop();
    for op in &mut bad.operations {
        machining::link(op, bad.part.as_ref().unwrap());
    }
    assert!(cam::generate(&bad).is_err());
    assert!(surface::from_stock(&wedge(), &Default::default(), &AtomicBool::new(true)).is_err());
}

#[test]
fn grid_refinement_reduces_retained_height_and_persists() {
    let mut p = plan(false);
    let coarse = cam::generate(&p).unwrap();
    for op in &mut p.operations {
        op.settings.surface_step = Some(0.25);
    }
    let fine = cam::generate(&p).unwrap();
    assert!(fine.moves.len() > coarse.moves.len());
    let json = serde_json::to_string(&p).unwrap();
    let restored: Project = serde_json::from_str(&json).unwrap();
    assert_eq!(p, restored);
    assert_eq!(restored.operations[1].strategy, Strategy::ParallelFinish);
}

#[test]
fn narrow_peak_between_grid_centres_survives_the_complete_swept_path() {
    let mut p = wedge();
    let mesh = &mut Arc::make_mut(&mut p.part.as_mut().unwrap().geometry).mesh;
    *mesh = Mesh {
        vertices: vec![
            [0., 0., 0.],
            [8., 0., 0.],
            [8., 6., 0.],
            [0., 6., 0.],
            [3.13, 2.17, 5.],
        ],
        triangles: vec![
            [0, 2, 1],
            [0, 3, 2],
            [0, 1, 4],
            [1, 2, 4],
            [2, 3, 4],
            [3, 0, 4],
        ],
    };
    let p = surface::from_stock(&p, &Default::default(), &AtomicBool::new(false))
        .unwrap()
        .project;
    let peak = p.part.as_ref().unwrap().point([3.13, 2.17, 5.]);
    let path = cam::generate(&p).unwrap();
    for m in path.moves.iter().filter(|m| m.motion == Motion::Cut) {
        for i in 0..=64 {
            let t = i as f64 / 64.;
            let x = m.from.x + t * (m.to.x - m.from.x);
            let y = m.from.y + t * (m.to.y - m.from.y);
            let z = m.from.z + t * (m.to.z - m.from.z);
            if (x - peak[0]).hypot(y - peak[1]) < 1. {
                assert!(z >= peak[2]);
            }
        }
    }
}

#[test]
fn invalid_reach_bounds_and_excessive_grid_never_generate() {
    let p = plan(false);
    let mut bad = p.clone();
    bad.stock.width = 9.;
    assert!(cam::generate(&bad).is_err());
    let mut bad = p.clone();
    bad.tool_library.tools[0]
        .geometry
        .insert("flute_length".into(), 1.);
    assert!(cam::generate(&bad).is_err());
    let mut bad = p.clone();
    bad.stock.width = 200.;
    bad.stock.height = 200.;
    for op in &mut bad.operations {
        op.width = 200.;
        op.height = 200.;
        op.settings.surface_step = Some(0.05);
        machining::link(op, bad.part.as_ref().unwrap());
    }
    assert!(cam::generate(&bad).unwrap_err().contains("grid exceeds"));
}

#[test]
fn curved_model_ball_paths_produce_a_bounded_stock_preview() {
    let part = opensourcecam::import::import_file(
        std::path::Path::new("examples/models/curved-surface.obj"),
        Units::Millimetres,
    )
    .unwrap();
    let mut p = Project::from_part(part).unwrap();
    p.ensure_tool_library();
    p.tool_library.tools[0].kind = ToolKind::BallEndMill;
    let p = surface::from_stock(&p, &Default::default(), &AtomicBool::new(false))
        .unwrap()
        .project;
    let path = cam::generate(&p).unwrap();
    let simulation =
        opensourcecam::stock::simulate(&p, &path, 1., 128, &AtomicBool::new(false)).unwrap();
    assert!(simulation.summary.removed_volume_mm3 > 0.);
    assert!(simulation.summary.sampled_gouge_mm3.is_none());
    assert!(simulation.geometry.mesh.triangles.len() <= 100000);
}
