use opensourcecam::{
    cam::{self, Motion},
    commands::{self, PlaybackPreview},
    machine::ChangeMode,
    model::{Project, Strategy},
    operation::{MillingMode, PocketPattern},
    post, stock,
    tool_library::{LibraryTool, ToolKind},
};
use std::sync::atomic::AtomicBool;
fn job() -> Project {
    let mut p = Project {
        part: None,
        operations: vec![],
        ..Default::default()
    };
    p.workspace.clear();
    p.stock.width = 50.;
    p.stock.height = 40.;
    p.stock.thickness = 8.;
    p.tool_library.tools = vec![
        LibraryTool::new(1, 1, ToolKind::FlatEndMill),
        LibraryTool::new(2, 2, ToolKind::Drill),
    ];
    p.active_tool = Some(1);
    p.active_preset = Some(1);
    p
}
fn rough(p: &Project) -> opensourcecam::model::Operation {
    let mut o = commands::operation_template(p, Strategy::Pocket);
    o.x = 5.;
    o.y = 5.;
    o.width = 40.;
    o.height = 30.;
    o.depth = 3.;
    o.settings.step_down = Some(1.);
    o.settings.pocket_pattern = PocketPattern::Offset;
    o
}
#[test]
fn planar_finish_covers_the_floor_and_stays_down_after_roughing() {
    let mut p = job();
    let mut f = commands::operation_template(&p, Strategy::SurfaceSmooth);
    f.x = 13.;
    f.y = 12.;
    f.width = 24.;
    f.height = 16.;
    f.settings.height_offset = Some(-3.);
    p.operations = vec![rough(&p), f];
    let path = cam::generate(&p).unwrap();
    let moves: Vec<_> = path.moves.iter().filter(|m| m.operation == 1).collect();
    assert!(
        moves.iter().filter(|m| m.motion == Motion::Plunge).count() <= 1,
        "Finishing repeatedly plunged through air"
    );
    let cuts: Vec<_> = moves.iter().filter(|m| m.motion == Motion::Cut).collect();
    assert!(cuts.len() >= 5);
    for x in 16..=34 {
        for y in 15..=25 {
            assert!(
                cuts.iter().any(|m| opensourcecam::planar::distance(
                    [x as f64, y as f64],
                    [m.from.x, m.from.y, 0.],
                    [m.to.x, m.to.y, 0.]
                ) <= 3. + 1e-6),
                "Unfinished floor {x},{y}"
            )
        }
    }
    assert!(
        moves
            .iter()
            .filter(|m| m.motion == Motion::Plunge)
            .map(|m| m.from.distance(m.to))
            .sum::<f64>()
            <= 0.51
    );
    // Conservative full-clearance mode remains available and has more travel.
    for o in &mut p.operations {
        o.settings.milling_mode = Some(MillingMode::Climb)
    }
    let full = cam::generate(&p).unwrap();
    assert!(path.estimated_seconds < full.estimated_seconds);
}
#[test]
fn tool_changes_travel_to_station_wait_and_return_without_removing_stock() {
    let mut p = job();
    let mut drill = commands::operation_template(&p, Strategy::DrillOnce);
    drill.tool_id = Some(2);
    drill.x = 25.;
    drill.y = 20.;
    drill.depth = 3.;
    p.operations = vec![rough(&p), drill];
    let changer = &mut p.machine.as_mut().unwrap().tool_changer;
    changer.mode = ChangeMode::Manual;
    changer.manual_position = Some([10., 10., 20.]);
    changer.change_seconds = 25.;
    for (i, t) in p.tool_library.tools.iter_mut().enumerate() {
        t.post.length_offset = Some(i as u32 + 1);
        t.post.pocket = Some(i as u32 + 1)
    }
    let preview = PlaybackPreview::new(&p).unwrap();
    let path = &preview.path;
    let (i, pause) = path
        .moves
        .iter()
        .enumerate()
        .find(|(_, m)| matches!(m.motion, Motion::ToolChange { .. }))
        .unwrap();
    assert_eq!([pause.from.x, pause.from.y, pause.from.z], [10., 10., 20.]);
    assert_eq!(pause.from, pause.to);
    assert_eq!(pause.seconds(), 25.);
    assert_eq!(path.moves[i - 1].operation, 0);
    assert_eq!(path.moves[i + 1].operation, 1);
    let before = path.moves[..i].iter().map(|m| m.seconds()).sum::<f64>() / path.estimated_seconds;
    let after =
        (path.moves[..i].iter().map(|m| m.seconds()).sum::<f64>() + 25.) / path.estimated_seconds;
    let a = stock::simulate(&p, path, before, 64, &AtomicBool::new(false)).unwrap();
    let b = stock::simulate(&p, path, after, 64, &AtomicBool::new(false)).unwrap();
    assert_eq!(a.heights, b.heights);
    let nc = post::gcode(&p).unwrap();
    assert!(nc.contains("G0 Z20.0000\nG0 X10.0000 Y10.0000"));
    assert!(nc.contains("T2\nM0"));
    p.machine.as_mut().unwrap().tool_changer.mode = ChangeMode::MillingAtc;
    p.machine.as_mut().unwrap().tool_changer.controller_managed = true;
    p.post.processor = post::PostProcessor::Mach3;
    let nc = post::gcode(&p).unwrap();
    let station = nc.find("G0 X10.0000 Y10.0000").unwrap();
    let change = nc[station..].find("T2 M6").unwrap();
    assert!(nc.contains("T2 M6\nG21\nG90\nG17\nG91.1\nG94\nG40\nG80\nG54\nG43 H2"));
    assert!(
        !nc[station..station + change].contains("G0 Z5.0000"),
        "ATC lowered before exchange"
    );
}
#[test]
fn same_tool_does_not_insert_exchange_events() {
    let mut p = job();
    let a = rough(&p);
    p.operations = vec![a.clone(), a];
    let path = PlaybackPreview::new(&p).unwrap().path;
    assert!(!path
        .moves
        .iter()
        .any(|m| matches!(m.motion, Motion::ToolChange { .. })));
}
#[test]
fn simulation_colors_record_finish_contact_even_at_unchanged_height() {
    let mut p = job();
    let mut f = commands::operation_template(&p, Strategy::SurfaceSmooth);
    f.x = 13.;
    f.y = 12.;
    f.width = 24.;
    f.height = 16.;
    f.settings.height_offset = Some(-3.);
    p.operations = vec![rough(&p), f];
    let path = cam::generate(&p).unwrap();
    let sim =
        stock::simulate_mode(&p, &path, 1., 96, &AtomicBool::new(false), 50_000_000, true).unwrap();
    assert!(sim.surface_roles.contains(&0));
    assert!(sim.surface_roles.contains(&1));
    assert!(sim.surface_roles.contains(&2));
    assert_eq!(sim.surface_roles.len(), sim.geometry.mesh.triangles.len());
}

#[test]
fn legacy_tool_wall_finishing_colors_the_contact_height() {
    let mut p = job();
    p.tool_library.tools.clear();
    p.active_tool = None;
    p.active_preset = None;
    let mut f = commands::operation_template(&p, Strategy::InsideProfile);
    f.x = 5.;
    f.y = 5.;
    f.width = 40.;
    f.height = 30.;
    f.depth = 3.;
    p.operations = vec![rough(&p), f];
    let path = cam::generate(&p).unwrap();
    let sim = stock::simulate(&p, &path, 1., 96, &AtomicBool::new(false)).unwrap();
    assert!(
        sim.geometry
            .mesh
            .triangles
            .iter()
            .zip(&sim.surface_roles)
            .any(|(t, role)| {
                let p = t.map(|i| sim.geometry.mesh.vertices[i as usize]);
                *role == 2
                    && opensourcecam::import::normal(p)[2].abs() < 1e-9
                    && p.iter().all(|v| v[2] >= -3. - 1e-6 && v[2] <= 1e-6)
            }),
        "Legacy cutter flute length was omitted from wall-finish contact"
    );
}

#[test]
fn planar_outside_layers_keep_the_island_and_share_same_tool_clearance() {
    let mut p = job();
    let mut a = commands::operation_template(&p, Strategy::StockRough);
    a.settings.closed = true;
    a.settings.height_offset = Some(0.);
    a.settings.step_down = Some(1.);
    a.settings.path = vec![
        [15., 12., -2.],
        [30., 12., -2.],
        [30., 26., -2.],
        [15., 26., -2.],
    ];
    a.depth = 2.;
    let mut b = a.clone();
    for q in &mut b.settings.path {
        q[2] = -3.
    }
    b.depth = 3.;
    p.operations = vec![a, b];
    let optimized = cam::generate(&p).unwrap();
    let end = optimized
        .moves
        .iter()
        .rev()
        .find(|m| m.operation == 0)
        .unwrap();
    assert!(
        end.to.z < p.clearance,
        "Same-tool boundary forced a full retract"
    );
    assert!(optimized.moves.iter().any(|m| m.operation == 1
        && m.motion == Motion::Rapid
        && m.to.z < p.clearance
        && (m.from.x != m.to.x || m.from.y != m.to.y)));
    for m in optimized.moves.iter().filter(|m| m.motion == Motion::Cut) {
        for i in 0..=10 {
            let t = i as f64 / 10.;
            let x = m.from.x + (m.to.x - m.from.x) * t;
            let y = m.from.y + (m.to.y - m.from.y) * t;
            let dx = (15. - x).max(x - 30.).max(0.);
            let dy = (12. - y).max(y - 26.).max(0.);
            assert!(
                dx.hypot(dy) >= 3. - 1e-6,
                "Clearing crossed the retained island"
            );
        }
    }
    for op in &mut p.operations {
        op.settings.milling_mode = Some(MillingMode::Climb)
    }
    let full = cam::generate(&p).unwrap();
    assert!(optimized.rapid_length < full.rapid_length);
}

#[test]
fn one_way_finishing_does_not_drag_backwards_across_the_finished_floor() {
    let mut p = job();
    let mut f = commands::operation_template(&p, Strategy::SurfaceSmooth);
    f.x = 13.;
    f.y = 12.;
    f.width = 24.;
    f.height = 16.;
    f.settings.height_offset = Some(-3.);
    f.settings.raster = opensourcecam::operation::Raster::PositiveX;
    p.operations = vec![rough(&p), f];
    let path = cam::generate(&p).unwrap();
    for m in path
        .moves
        .iter()
        .filter(|m| m.operation == 1 && m.motion == Motion::Cut)
    {
        assert!(
            m.to.x >= m.from.x - 1e-8,
            "One-way finish made a backwards contact move"
        );
    }
}
