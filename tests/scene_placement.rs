use opensourcecam::{
    import::Body,
    model::Project,
    session::{Edit, Session},
    workholding::{place_example, ViseSpec},
    workspace::{self, Frame},
};
fn job() -> Project {
    let mut p = Project::default();
    place_example(&mut p, None, ViseSpec::default()).unwrap();
    let g = std::sync::Arc::make_mut(&mut p.workspace[0].part.geometry);
    g.bodies = (0..3)
        .map(|i| Body {
            name: format!("Body {i}"),
            first_triangle: i * 12,
            triangle_count: 12,
        })
        .collect();
    p
}
#[test]
fn whole_model_position_frames_centering_and_undo() {
    let mut p = job();
    p.machine.as_mut().unwrap().work_origin = [200., -40., 50.];
    let mut s = Session::new(p);
    s.apply(Edit::PositionWorkspace {
        id: 1,
        offset: [210., 10., 55.],
        rotation: [20., 30., 45.],
        frame: Frame::Machine,
    })
    .unwrap();
    assert_eq!(
        s.project().workspace[0].point([0.; 3], s.project().machine.as_ref()),
        [10., 50., 5.]
    );
    s.apply(Edit::CenterWorkspace { id: 1 }).unwrap();
    let info = workspace::placement_info(s.project(), 1, None).unwrap();
    assert!((info["center"][0].as_f64().unwrap() - s.project().stock.width / 2.).abs() < 1e-7);
    assert!((info["center"][1].as_f64().unwrap() - s.project().stock.height / 2.).abs() < 1e-7);
    s.apply(Edit::Undo).unwrap();
    assert_eq!(s.project().workspace[0].part.offset, [210., 10., 55.]);
}
#[test]
fn component_position_uses_world_axes_and_keeps_other_bodies_fixed() {
    let mut p = job();
    p.workspace[0].rotation = [15., 25., 70.];
    p.workspace[0].part.units = opensourcecam::import::Units::Inches;
    let original = p.workspace[0].part.geometry.clone();
    let mut s = Session::new(p);
    let info = workspace::placement_info(s.project(), 1, Some(2)).unwrap();
    let old: [f64; 3] = std::array::from_fn(|i| info["center"][i].as_f64().unwrap());
    let target = [old[0] + 15., old[1] - 20., old[2] + 8.];
    s.apply(Edit::PositionComponent {
        id: 1,
        body: 2,
        center: target,
        rotate: [0., 0., 90.],
    })
    .unwrap();
    assert_eq!(
        &s.project().workspace[0].part.geometry.mesh.vertices[..16],
        &original.mesh.vertices[..16]
    );
    let info = workspace::placement_info(s.project(), 1, Some(2)).unwrap();
    for (i, value) in target.iter().enumerate() {
        assert!((info["center"][i].as_f64().unwrap() - value).abs() < 1e-6)
    }
    let restored = Project::from_json(&serde_json::to_vec(s.project()).unwrap()).unwrap();
    assert_eq!(
        restored.workspace[0].part.geometry,
        s.project().workspace[0].part.geometry
    );
    s.apply(Edit::Undo).unwrap();
    assert_eq!(s.project().workspace[0].part.geometry, original);
}
#[test]
fn invalid_component_and_nonfinite_pose_are_atomic() {
    let mut s = Session::new(job());
    let before = serde_json::to_string(s.project()).unwrap();
    assert!(s
        .apply(Edit::PositionComponent {
            id: 1,
            body: 999,
            center: [0.; 3],
            rotate: [0.; 3]
        })
        .is_err());
    assert!(s
        .apply(Edit::PositionWorkspace {
            id: 1,
            offset: [f64::NAN, 0., 0.],
            rotation: [0.; 3],
            frame: Frame::Work
        })
        .is_err());
    assert_eq!(serde_json::to_string(s.project()).unwrap(), before);
}

#[test]
fn closed_jaw_placement_is_blocked_and_orientation_changes_clear_the_profile() {
    let p = job();
    let faces = opensourcecam::faces::list(&p.workspace[0].part).unwrap();
    let fixed = faces
        .iter()
        .find(|f| f.normal[1] > 0.99 && f.center[1] < 0.)
        .unwrap()
        .id;
    let moving = faces
        .iter()
        .find(|f| f.normal[1] < -0.99 && f.center[1] > p.stock.height)
        .unwrap()
        .id;
    let mut s = Session::new(p);
    s.apply(Edit::ClampViseSetup {
        id: 1,
        fixed,
        moving,
        max_closure: 30.,
        bodies: vec![],
        grip: 3.,
    })
    .unwrap();
    let before = serde_json::to_string(s.project()).unwrap();
    assert!(s
        .apply(Edit::CenterWorkspace { id: 1 })
        .unwrap_err()
        .contains("Open the jaw"));
    assert_eq!(serde_json::to_string(s.project()).unwrap(), before);
    s.apply(Edit::OpenVise { id: 1 }).unwrap();
    s.apply(Edit::PositionWorkspace {
        id: 1,
        offset: [0.; 3],
        rotation: [0., 0., 45.],
        frame: Frame::Work,
    })
    .unwrap();
    assert!(s.project().workspace[0].clamp.is_none());
}

#[test]
fn scene_center_ignores_unused_source_vertices() {
    let mut p = job();
    let before = workspace::placement_info(&p, 1, None).unwrap();
    std::sync::Arc::make_mut(&mut p.workspace[0].part.geometry)
        .mesh
        .vertices
        .push([900000., 900000., 900000.]);
    assert_eq!(workspace::placement_info(&p, 1, None).unwrap(), before);
}

#[test]
fn asymmetric_component_keeps_its_requested_center_after_rotation() {
    let mut p = job();
    let g = std::sync::Arc::make_mut(&mut p.workspace[0].part.geometry);
    g.mesh.vertices[16][0] += 3.; // An asymmetric closed jaw, not a centered box.
    let mut s = Session::new(p);
    let target = [100., 50., -20.];
    s.apply(Edit::PositionComponent {
        id: 1,
        body: 2,
        center: target,
        rotate: [25., 15., 60.],
    })
    .unwrap();
    let info = workspace::placement_info(s.project(), 1, Some(2)).unwrap();
    for (i, value) in target.iter().enumerate() {
        assert!((info["center"][i].as_f64().unwrap() - value).abs() < 1e-7)
    }
}
