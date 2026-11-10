use opensourcecam::{
    cam::{Motion, Move, Point3, Toolpath},
    model::{Operation, Project, Strategy},
    stock, tool_preview,
};
use std::sync::atomic::AtomicBool;
#[test]
fn six_mm_cutter_slot_and_display_agree_at_all_preview_resolutions() {
    let mut p = Project::default();
    p.stock.width = 60.;
    p.stock.height = 40.;
    p.stock.thickness = 10.;
    p.ensure_tool_library();
    let tool = &mut p.tool_library.tools[0];
    tool.geometry.insert("diameter".into(), 6.);
    tool.geometry.insert("shaft_diameter".into(), 8.);
    p.operations = vec![Operation {
        x: 10.,
        y: 10.,
        width: 40.,
        height: 20.,
        depth: 2.,
        strategy: Strategy::Pocket,
        ..Default::default()
    }];
    let path = Toolpath {
        moves: vec![Move {
            from: Point3 {
                x: 10.,
                y: 20.,
                z: -2.,
            },
            to: Point3 {
                x: 40.,
                y: 20.,
                z: -2.,
            },
            feed: 600.,
            motion: Motion::Cut,
            operation: 0,
        }],
        cut_length: 30.,
        rapid_length: 0.,
        estimated_seconds: 3.,
    };
    let display = tool_preview::playback(&p, 0).unwrap();
    assert_eq!(display.cutting_diameter, 6.);
    assert_eq!(display.shaft_diameter, 8.);
    let radius = display.meshes[1]
        .vertices
        .iter()
        .map(|p| p[0].hypot(p[1]))
        .fold(0., f64::max);
    assert!((radius - 3.).abs() < 1e-9);
    for resolution in [32, 128, 512] {
        let sim = stock::preview(&p, &path, 1., resolution, &AtomicBool::new(false)).unwrap();
        let [nx, ny] = sim.summary.resolution;
        let [dx, dy] = sim.summary.cell_size;
        let x = (25. / dx) as usize;
        let rows: Vec<_> = (0..ny).filter(|&y| sim.heights[y * nx + x] < -1.).collect();
        let width = rows.len() as f64 * dy;
        assert!(
            (width - 6.).abs() <= 2. * dy,
            "{resolution}: {width} mm slot from 6 mm tool"
        );
        for y in 0..ny {
            let cy = (y as f64 + 0.5) * dy;
            if (cy - 20.).abs() > 3. + 1e-9 {
                assert_eq!(sim.heights[y * nx + x], 0.);
            }
        }
    }
}
