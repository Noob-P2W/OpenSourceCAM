use opensourcecam::view::Camera;

#[test]
fn top_view_is_xy_with_z_as_depth() {
    let projected = Camera::top().project([12.0, 30.0, 8.0]);
    for (actual, expected) in projected.into_iter().zip([12.0, -30.0, 8.0]) {
        assert!((actual - expected).abs() < 1e-12);
    }
}

#[test]
fn gpu_camera_basis_matches_cpu_projection() {
    for camera in [
        Camera::default(),
        Camera::top(),
        Camera {
            yaw: 2.1,
            pitch: -0.8,
        },
    ] {
        let point = [12.0, 30.0, -8.0];
        let matrix = camera.matrix();
        for (row, expected) in camera.project(point).into_iter().enumerate() {
            let actual: f64 = (0..3)
                .map(|column| matrix[column * 3 + row] as f64 * point[column])
                .sum();
            assert!((actual - expected).abs() < 1e-5);
        }
    }
}

#[test]
fn orbit_changes_projection_and_stays_finite_at_poles() {
    let mut camera = Camera::default();
    let original = camera.project([20.0, 5.0, 12.0]);
    camera.orbit(50.0, 20.0);
    assert_ne!(original, camera.project([20.0, 5.0, 12.0]));
    camera.orbit(1_000_000.0, 1_000_000.0);
    assert!(camera
        .project([20.0, 5.0, 12.0])
        .iter()
        .all(|v| v.is_finite()));
    assert!(camera.pitch < std::f64::consts::FRAC_PI_2);
    assert!((0.0..std::f64::consts::TAU).contains(&camera.yaw));
}
