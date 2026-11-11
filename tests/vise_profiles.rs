use opensourcecam::{
    faces,
    model::Project,
    session::{Edit, Session},
    vise,
    workholding::{place_example, ViseSpec},
};
fn job() -> (Project, usize, usize) {
    let mut p = Project::default();
    place_example(&mut p, None, ViseSpec::default()).unwrap();
    p.workspace[0].vise = None; // Exercise imported geometry, independent of the built-in vise settings.
    let list = faces::list(&p.workspace[0].part).unwrap();
    let fixed = list
        .iter()
        .find(|f| f.normal[1] > 0.99 && f.center[1] < 0.)
        .unwrap()
        .id;
    let moving = list
        .iter()
        .find(|f| f.normal[1] < -0.99 && f.center[1] > p.stock.height)
        .unwrap()
        .id;
    (p, fixed, moving)
}
#[test]
fn configure_close_open_undo_and_profile_reuse() {
    let (p, fixed, moving) = job();
    let original = p.workspace[0].part.geometry.mesh.clone();
    let mut session = Session::new(p);
    session
        .apply(Edit::ConfigureVise {
            id: 1,
            fixed,
            moving,
            max_closure: 30.,
        })
        .unwrap();
    session.apply(Edit::ClampVise { id: 1, grip: 3. }).unwrap();
    let p = session.project();
    assert!((p.workspace[0].clamp.as_ref().unwrap().closed_by - 0.5).abs() < 1e-6);
    assert_ne!(p.workspace[0].part.geometry.mesh, original);
    let data = vise::export(p, 1).unwrap();
    let restored = Project::from_json(&serde_json::to_vec(p).unwrap()).unwrap();
    let mut r = Session::new(restored);
    r.apply(Edit::OpenVise { id: 1 }).unwrap();
    assert_eq!(r.project().workspace[0].part.geometry.mesh, original);
    let checkpoint = session.history_checkpoint().unwrap();
    let mut restored_session = Session::new(session.project().clone());
    restored_session.restore_history(&checkpoint).unwrap();
    assert_eq!(
        restored_session.project().workspace[0].clamp,
        session.project().workspace[0].clamp
    );
    session.apply(Edit::Undo).unwrap();
    assert_eq!(session.project().workspace[0].part.geometry.mesh, original);
    let mut next = Session::new(Project::default());
    next.apply(Edit::AddViseProfile { data }).unwrap();
    next.apply(Edit::ClampVise { id: 1, grip: 4. }).unwrap();
    assert!(
        next.project().workspace[0]
            .clamp
            .as_ref()
            .unwrap()
            .closed_by
            > 0.
    );
}
#[test]
fn rejects_bad_faces_travel_and_fused_bodies_atomically() {
    let (p, fixed, moving) = job();
    let mut session = Session::new(p);
    let before = serde_json::to_string(session.project()).unwrap();
    assert!(session
        .apply(Edit::ConfigureVise {
            id: 1,
            fixed,
            moving: fixed,
            max_closure: 30.
        })
        .is_err());
    assert_eq!(serde_json::to_string(session.project()).unwrap(), before);
    session
        .apply(Edit::ConfigureVise {
            id: 1,
            fixed,
            moving,
            max_closure: 0.1,
        })
        .unwrap();
    let before = serde_json::to_string(session.project()).unwrap();
    assert!(session.apply(Edit::ClampVise { id: 1, grip: 3. }).is_err());
    assert_eq!(serde_json::to_string(session.project()).unwrap(), before);
    assert!(session.apply(Edit::ClampVise { id: 1, grip: 99. }).is_err());
}

#[test]
fn closed_vise_pose_is_used_by_clearance_checks_and_interference_is_atomic() {
    use opensourcecam::{
        blank,
        cam::{Motion, Move, Point3, Toolpath},
        collision,
    };
    let (p, fixed, moving) = job();
    let mut s = Session::new(p);
    s.apply(Edit::ConfigureVise {
        id: 1,
        fixed,
        moving,
        max_closure: 30.,
    })
    .unwrap();
    s.apply(Edit::ClampVise { id: 1, grip: 3. }).unwrap();
    let path = Toolpath {
        moves: vec![Move {
            from: Point3 {
                x: 60.,
                y: 85.,
                z: -11.,
            },
            to: Point3 {
                x: 61.,
                y: 85.,
                z: -11.,
            },
            feed: 100.,
            motion: Motion::Cut,
            operation: 0,
        }],
        estimated_seconds: 1.,
        cut_length: 1.,
        rapid_length: 0.,
    };
    assert!(collision::check(s.project(), &path)
        .unwrap_err()
        .contains("fixture"));
    let mut p = s.project().clone();
    vise::open(&mut p, 1).unwrap();
    let extra = blank::mesh_from_boxes(&[([30., 30., -10.], [50., 50., 0.])]);
    let mesh = &mut std::sync::Arc::make_mut(&mut p.workspace[0].part.geometry).mesh;
    let offset = mesh.vertices.len() as u32;
    mesh.vertices.extend(extra.vertices);
    mesh.triangles
        .extend(extra.triangles.into_iter().map(|t| t.map(|i| i + offset)));
    let mut session = Session::new(p);
    let before = serde_json::to_string(session.project()).unwrap();
    assert!(session
        .apply(Edit::ClampVise { id: 1, grip: 3. })
        .unwrap_err()
        .contains("penetrates"));
    assert_eq!(serde_json::to_string(session.project()).unwrap(), before);
}

#[test]
fn quarter_turn_and_profile_vertex_validation() {
    let (mut p, fixed, moving) = job();
    p.operations.clear();
    p.stock.width = 80.;
    p.stock.height = 120.;
    p.workspace[0].rotation[2] = 90.;
    let mut s = Session::new(p);
    s.apply(Edit::ConfigureVise {
        id: 1,
        fixed,
        moving,
        max_closure: 30.,
    })
    .unwrap();
    s.apply(Edit::ClampVise { id: 1, grip: 3. }).unwrap();
    assert!((s.project().workspace[0].clamp.as_ref().unwrap().closed_by - 0.5).abs() < 1e-6);
    let mut broken = s.project().clone();
    broken.workspace[0]
        .clamp
        .as_mut()
        .unwrap()
        .moving_vertices
        .pop();
    assert!(Project::from_json(&serde_json::to_vec(&broken).unwrap()).is_err());
    let mut original = Session::new(job().0);
    assert!(original
        .apply(Edit::ConfigureVise {
            id: 1,
            fixed: usize::MAX,
            moving: 0,
            max_closure: 30.
        })
        .is_err());
}

#[test]
fn malformed_profile_in_recovery_history_is_rejected() {
    let (p, fixed, moving) = job();
    let mut s = Session::new(p);
    s.apply(Edit::ConfigureVise {
        id: 1,
        fixed,
        moving,
        max_closure: 30.,
    })
    .unwrap();
    s.apply(Edit::ClampVise { id: 1, grip: 3. }).unwrap();
    let mut history: serde_json::Value =
        serde_json::from_str(&s.history_checkpoint().unwrap()).unwrap();
    let mut changed = false;
    for snapshot in history["undo"].as_array_mut().unwrap() {
        if let Some(profile) = snapshot["workspace"][0]["clamp"].as_object_mut() {
            profile
                .get_mut("moving_vertices")
                .unwrap()
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!(99999999));
            changed = true;
        }
    }
    assert!(changed);
    let mut restored = Session::new(s.project().clone());
    assert!(restored.restore_history(&history.to_string()).is_err());
}

fn assembly_job() -> (Project, usize, usize) {
    use opensourcecam::import::Body;
    use std::sync::Arc;
    let (mut p, fixed, moving) = job();
    let geometry = Arc::make_mut(&mut p.workspace[0].part.geometry);
    // A separate accessory in the same CAD file, rigidly attached to the moving jaw.
    let accessory =
        opensourcecam::blank::mesh_from_boxes(&[([50., 100., -35.], [70., 110., -30.])]);
    let base = geometry.mesh.vertices.len() as u32;
    geometry.mesh.vertices.extend(accessory.vertices);
    geometry
        .mesh
        .triangles
        .extend(accessory.triangles.into_iter().map(|t| t.map(|i| i + base)));
    geometry.bodies = ["Base", "Fixed jaw", "Movable jaw", "Jaw accessory"]
        .into_iter()
        .enumerate()
        .map(|(i, name)| Body {
            name: name.into(),
            first_triangle: i * 12,
            triangle_count: 12,
        })
        .collect();
    Project::from_json(&serde_json::to_vec(&p).unwrap()).unwrap();
    (p, fixed, moving)
}

#[test]
fn cad_components_move_as_a_rigid_group_and_survive_save_undo_and_library() {
    let (p, fixed, moving) = assembly_job();
    let before = p.workspace[0].part.geometry.clone();
    let mut s = Session::new(p);
    s.apply(Edit::ConfigureViseGroup {
        id: 1,
        fixed,
        moving,
        max_closure: 30.,
        bodies: vec![3],
    })
    .unwrap();
    let profile = s.project().workspace[0].clamp.as_ref().unwrap();
    assert_eq!(profile.moving_vertices.len(), 16);
    assert!(profile.moving_vertices.contains(&24));
    assert!(!profile.moving_vertices.contains(&0));
    s.apply(Edit::ClampVise { id: 1, grip: 3. }).unwrap();
    let after = &s.project().workspace[0].part.geometry;
    // Jaw and accessory receive the identical displacement, independently of the base.
    for axis in 0..3 {
        assert!(
            (after.mesh.vertices[24][axis]
                - after.mesh.vertices[16][axis]
                - before.mesh.vertices[24][axis]
                + before.mesh.vertices[16][axis])
                .abs()
                < 1e-8
        );
    }
    assert!(
        (after.mesh.vertices[16][1] - before.mesh.vertices[16][1] - after.mesh.vertices[0][1]
            + before.mesh.vertices[0][1]
            + 0.5)
            .abs()
            < 1e-8
    );
    let restored = Project::from_json(&serde_json::to_vec(s.project()).unwrap()).unwrap();
    assert_eq!(restored.workspace[0].part.geometry.bodies, before.bodies);
    let checkpoint = s.history_checkpoint().unwrap();
    let mut recovered = Session::new(restored);
    recovered.restore_history(&checkpoint).unwrap();
    recovered.apply(Edit::Undo).unwrap();
    assert_eq!(
        recovered.project().workspace[0].part.geometry.mesh,
        before.mesh
    );
    let data = vise::export(s.project(), 1).unwrap();
    let mut reused = Session::new(Project::default());
    reused.apply(Edit::AddViseProfile { data }).unwrap();
    assert_eq!(reused.project().workspace[0].part.geometry.bodies.len(), 4);
    reused.apply(Edit::ClampVise { id: 1, grip: 3. }).unwrap();
}

#[test]
fn cad_fixed_body_and_invalid_body_selection_are_rejected_atomically() {
    let (p, fixed, moving) = assembly_job();
    let mut s = Session::new(p);
    let before = serde_json::to_string(s.project()).unwrap();
    for bodies in [vec![1], vec![usize::MAX]] {
        assert!(s
            .apply(Edit::ConfigureViseGroup {
                id: 1,
                fixed,
                moving,
                max_closure: 30.,
                bodies
            })
            .is_err());
        assert_eq!(serde_json::to_string(s.project()).unwrap(), before);
    }
}

#[test]
fn touching_cad_bodies_are_not_welded_by_face_selection_or_jaw_detection() {
    use opensourcecam::import::Body;
    use std::sync::Arc;
    let (mut p, _, _) = job();
    let geometry = Arc::make_mut(&mut p.workspace[0].part.geometry);
    geometry.mesh = opensourcecam::blank::mesh_from_boxes(&[
        ([0., 0., -10.], [10., 30., 0.]),
        ([0., 0., 0.], [10., 10., 10.]),
        ([0., 20., 0.], [10., 30., 10.]),
    ]);
    geometry.bodies = (0..3)
        .map(|i| Body {
            name: format!("Body {i}"),
            first_triangle: i * 12,
            triangle_count: 12,
        })
        .collect();
    let list = faces::list(&p.workspace[0].part).unwrap();
    assert_eq!(
        list.len(),
        18,
        "Touching coplanar faces must remain separate CAD body faces"
    );
    let g = &p.workspace[0].part.geometry;
    let fixed = list
        .iter()
        .find(|f| g.body_at(f.id) == Some(1) && f.normal[1] > 0.99)
        .unwrap()
        .id;
    let moving = list
        .iter()
        .find(|f| g.body_at(f.id) == Some(2) && f.normal[1] < -0.99)
        .unwrap()
        .id;
    vise::configure(&mut p, 1, fixed, moving, 30.).unwrap();
    assert_eq!(
        p.workspace[0].clamp.as_ref().unwrap().moving_vertices.len(),
        8
    );
}

#[test]
fn saved_profile_cannot_move_only_one_shell_of_a_cad_component() {
    use std::sync::Arc;
    let (mut p, fixed, moving) = assembly_job();
    vise::configure_group(&mut p, 1, fixed, moving, 30., &[3]).unwrap();
    let geometry = Arc::make_mut(&mut p.workspace[0].part.geometry);
    geometry.bodies.truncate(3);
    geometry.bodies[2].triangle_count = 24; // One CAD component with two separate shells.
    p.workspace[0]
        .clamp
        .as_mut()
        .unwrap()
        .moving_vertices
        .retain(|i| *i < 24);
    assert!(Project::from_json(&serde_json::to_vec(&p).unwrap())
        .unwrap_err()
        .contains("whole rigid body"));
}

#[test]
fn touching_assembly_components_are_validated_separately_for_clamping_and_clearance() {
    use opensourcecam::{blank, collision, import::Body};
    use std::sync::Arc;
    let (mut p, _, _) = job();
    let geometry = Arc::make_mut(&mut p.workspace[0].part.geometry);
    geometry.mesh = blank::mesh_from_boxes(&[
        ([-10., -20., -37.], [130., 110., -27.]),
        ([-10., -20., -27.], [130., 0., -7.]),
        ([-10., 100., -27.], [130., 110., -7.]),
        ([-10., 90., -27.], [130., 100., -7.]),
    ]);
    geometry.bodies = (0..4)
        .map(|i| Body {
            name: format!("Component {i}"),
            first_triangle: i * 12,
            triangle_count: 12,
        })
        .collect();
    let list = faces::list(&p.workspace[0].part).unwrap();
    let g = &p.workspace[0].part.geometry;
    let fixed = list
        .iter()
        .find(|f| g.body_at(f.id) == Some(1) && f.normal[1] > 0.99)
        .unwrap()
        .id;
    let moving = list
        .iter()
        .find(|f| g.body_at(f.id) == Some(3) && f.normal[1] < -0.99)
        .unwrap()
        .id;
    let mut session = Session::new(p);
    session
        .apply(Edit::ConfigureViseGroup {
            id: 1,
            fixed,
            moving,
            max_closure: 30.,
            bodies: vec![2],
        })
        .unwrap();
    session.apply(Edit::ClampVise { id: 1, grip: 3. }).unwrap();
    collision::check_stock_in_vise(session.project(), 1).unwrap();
    let mut flat = session.project().clone();
    Arc::make_mut(&mut flat.workspace[0].part.geometry)
        .bodies
        .clear();
    assert!(collision::check_stock_in_vise(&flat, 1)
        .unwrap_err()
        .contains("manifold"));
    let mut opened = session.project().clone();
    vise::open(&mut opened, 1).unwrap();
    // The same body-aware closure check is used by NC tool/shaft/holder preflight.
    collision::check(
        &opened,
        &opensourcecam::cam::Toolpath {
            moves: vec![],
            estimated_seconds: 0.,
            cut_length: 0.,
            rapid_length: 0.,
        },
    )
    .unwrap();
}
