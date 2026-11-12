use opensourcecam::{
    cam::{Motion, Move, Point3, Toolpath},
    commands::PlaybackPreview,
    model::{Operation, Project, Strategy},
    stock,
};
use std::sync::atomic::AtomicBool;
fn job() -> Project {
    let mut p = Project::default();
    p.ensure_tool_library();
    p.operations = vec![Operation {
        x: 10.,
        y: 10.,
        width: 70.,
        height: 40.,
        depth: 3.,
        strategy: Strategy::Pocket,
        ..Default::default()
    }];
    p
}
#[test]
fn incremental_frames_match_fresh_stock_after_seeks_and_resolution_changes() {
    let p = job();
    let preview = PlaybackPreview::new(&p).unwrap();
    for (epoch, resolution, progress) in [
        (1, 128, 0.),
        (1, 128, 0.2),
        (1, 128, 0.201),
        (1, 128, 0.8),
        (2, 128, 0.1),
        (2, 128, 1.),
        (3, 256, 0.5),
    ] {
        let frame = preview
            .frame(&p, progress, resolution, epoch, 50_000_000)
            .unwrap();
        let fresh = stock::preview(
            &p,
            &preview.path,
            progress,
            resolution,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(
            (frame.metadata["summary"]["removed_volume_mm3"]
                .as_f64()
                .unwrap()
                - fresh.summary.removed_volume_mm3)
                .abs()
                < 1e-7
        );
        for patch in frame.metadata["patches"].as_array().unwrap() {
            let end = patch["offset"].as_u64().unwrap() + patch["length"].as_u64().unwrap();
            assert!(end as usize <= frame.vertices.len());
        }
    }
    let repeat = preview.frame(&p, 0.5, 256, 3, 50_000_000).unwrap();
    assert_eq!(repeat.metadata["evaluations"], 0);
    assert!(repeat.vertices.is_empty());
}
#[test]
fn diagonal_repeated_passes_do_not_scan_the_whole_bounding_rectangle() {
    let mut p = job();
    p.stock.width = 100.;
    p.stock.height = 100.;
    let m = Move {
        from: Point3 {
            x: 5.,
            y: 5.,
            z: -2.,
        },
        to: Point3 {
            x: 95.,
            y: 95.,
            z: -2.,
        },
        feed: 600.,
        motion: Motion::Cut,
        operation: 0,
    };
    let seconds = m.from.distance(m.to) / 600. * 60.;
    let path = Toolpath {
        moves: vec![m.clone(); 1000],
        estimated_seconds: seconds * 1000.,
        cut_length: 1000. * m.from.distance(m.to),
        rapid_length: 0.,
    };
    // The previous rectangle scan hit its 50 million limit before completing this job.
    let result = stock::preview(&p, &path, 1., 512, &AtomicBool::new(false)).unwrap();
    assert!(result.summary.removed_volume_mm3 > 1000.);
    let mut playback = PlaybackPreview::new(&p).unwrap();
    playback.path = path;
    let frame = playback.frame(&p, 1., 512, 1, 50_000_000).unwrap();
    assert!(frame.metadata["evaluations"].as_u64().unwrap() < 100_000);
    assert!(frame.metadata["visited_cells"].as_u64().unwrap() < 30_000_000);
}
#[test]
fn budget_choices_are_bounded_and_cached_frames_are_transactional() {
    let p = job();
    let preview = PlaybackPreview::new(&p).unwrap();
    assert!(preview.frame(&p, 0.5, 128, 1, u64::MAX).is_err());
    let standard = preview.frame(&p, 0.5, 128, 1, 50_000_000).unwrap();
    let extended = preview.frame(&p, 0.5, 128, 2, 200_000_000).unwrap();
    assert_eq!(standard.vertices, extended.vertices);
    assert!(preview.frame(&p, f64::NAN, 128, 2, 50_000_000).is_err());
    let recovered = preview.frame(&p, 0.5, 128, 2, 50_000_000).unwrap();
    assert_eq!(recovered.metadata["reset"], true);
    assert_eq!(standard.vertices, recovered.vertices);
}

#[test]
fn high_resolution_inspection_playback_and_export_share_limits() {
    let mut p = job();
    p.stock.width = 100.;
    p.stock.height = 100.;
    p.operations[0].strategy = Strategy::DrillOnce;
    p.operations[0].x = 50.;
    p.operations[0].y = 50.;
    p.tool_library.tools[0] = opensourcecam::tool_library::LibraryTool::new(
        1,
        1,
        opensourcecam::tool_library::ToolKind::Drill,
    );
    let playback = PlaybackPreview::new(&p).unwrap();
    let cancel = AtomicBool::new(false);
    for resolution in [513, 768, 1024] {
        let detailed = stock::simulate(&p, &playback.path, 1., resolution, &cancel).unwrap();
        let fresh = stock::preview(&p, &playback.path, 1., resolution, &cancel).unwrap();
        assert_eq!(detailed.summary.resolution, [resolution, resolution]);
        assert_eq!(detailed.heights.len(), resolution * resolution);
        assert_eq!(detailed.heights, fresh.heights);
        assert!(detailed.summary.removed_volume_mm3 > 0.);
        assert!(detailed.geometry.mesh.triangles.len() <= 2_000_000);
        for progress in [0.5, 1.] {
            let frame = playback
                .frame(&p, progress, resolution, 1, 50_000_000)
                .unwrap();
            let reference =
                stock::preview(&p, &playback.path, progress, resolution, &cancel).unwrap();
            assert_eq!(
                frame.metadata["summary"]["resolution"],
                serde_json::json!([resolution, resolution])
            );
            assert!(
                (frame.metadata["summary"]["removed_volume_mm3"]
                    .as_f64()
                    .unwrap()
                    - reference.summary.removed_volume_mm3)
                    .abs()
                    < 1e-7
            );
        }
        let obj = opensourcecam::commands::stock_obj(&p, 1., resolution).unwrap();
        assert_eq!(obj, stock::obj_data(&detailed).unwrap());
        assert!(
            stock::simulate_budget(&p, &playback.path, 1., resolution, &cancel, u64::MAX).is_err()
        );
    }
    for resolution in [15, 1025, usize::MAX] {
        assert!(stock::simulate(&p, &playback.path, 1., resolution, &cancel).is_err());
        assert!(stock::preview(&p, &playback.path, 1., resolution, &cancel).is_err());
        assert!(
            stock::simulate_budget(&p, &playback.path, 1., resolution, &cancel, 200_000_000)
                .is_err()
        );
        assert!(playback.frame(&p, 1., resolution, 1, 50_000_000).is_err());
        assert!(opensourcecam::commands::stock_obj(&p, 1., resolution).is_err());
    }
}
