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
