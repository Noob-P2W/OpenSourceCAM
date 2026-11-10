use opensourcecam::{
    blank::{self, StockDefinition, StockKind},
    cam, commands,
    import::{self, Units},
    machine_library,
    model::{Project, Strategy},
    post,
    session::{Edit, Session},
    workholding::{self, ViseSpec},
};
use std::{path::Path, sync::atomic::AtomicBool};
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-7, "{a} != {b}")
}
fn part_project() -> Project {
    let part =
        import::import_file(Path::new("examples/models/mount.stl"), Units::Millimetres).unwrap();
    let mut s = Session::new(Project::default());
    s.apply(Edit::ImportPart { part }).unwrap();
    s.project().clone()
}
#[test]
fn oriented_box_and_round_blank_have_physical_columns_and_volume() {
    let box_stock = StockDefinition {
        kind: StockKind::Rectangular,
        size: [20., 10., 5.],
        rotation: [90., 0., 0.],
    }
    .build()
    .unwrap();
    close(box_stock.width, 20.);
    close(box_stock.height, 5.);
    close(box_stock.thickness, 10.);
    assert_eq!(box_stock.column(10., 2.5), Some([-10., 0.]));
    let cylinder = StockDefinition {
        kind: StockKind::Cylinder,
        size: [20., 20., 10.],
        rotation: [0.; 3],
    }
    .build()
    .unwrap();
    assert!(cylinder.column(0., 0.).is_none());
    assert_eq!(cylinder.column(10., 10.), Some([-10., 0.]));
    assert!(cylinder.contains([10., 10., -5.], 0.));
    assert!(!cylinder.contains([0., 0., -5.], 0.));
    assert!(cylinder.contains_disc([10., 10., -5.], 9.));
    assert!(!cylinder.contains_disc([1., 1., -5.], 2.));
    let mut p = Project {
        stock: cylinder,
        ..Default::default()
    };
    p.operations.truncate(1);
    let op = &mut p.operations[0];
    op.x = 5.;
    op.y = 5.;
    op.width = 10.;
    op.height = 10.;
    op.depth = 1.;
    let path = cam::generate(&p).unwrap();
    let sim = opensourcecam::stock::simulate(&p, &path, 0., 64, &AtomicBool::new(false)).unwrap();
    assert_eq!(sim.summary.removed_volume_mm3, 0.);
    assert!((sim.summary.remaining_volume_mm3 - std::f64::consts::PI * 1000.).abs() < 25.);
    assert!(sim.summary.remaining_volume_mm3 < 4000.);
    let horizontal = StockDefinition {
        kind: StockKind::Cylinder,
        size: [20., 20., 30.],
        rotation: [90., 0., 0.],
    }
    .build()
    .unwrap();
    close(horizontal.height, 30.);
    close(horizontal.thickness, 20.);
    let interval = horizontal.column(10., 15.).unwrap();
    close(interval[0], -20.);
    close(interval[1], 0.);
}
#[test]
fn round_blank_rejects_pocket_outside_actual_material() {
    let mut p = Project {
        stock: StockDefinition {
            kind: StockKind::Cylinder,
            size: [100., 100., 12.],
            rotation: [0.; 3],
        }
        .build()
        .unwrap(),
        ..Default::default()
    };
    p.operations.truncate(1);
    let op = &mut p.operations[0];
    op.x = 0.;
    op.y = 0.;
    op.width = 20.;
    op.height = 20.;
    assert!(cam::generate(&p).is_err());
}
#[test]
fn centered_rotation_fit_and_roundtrip_preserve_shape() {
    let mut p = part_project();
    let original = p.part.as_ref().unwrap().bounds();
    p.part
        .as_mut()
        .unwrap()
        .orient_centered([90., 0., 0.])
        .unwrap();
    let rotated = p.part.as_ref().unwrap().bounds();
    for i in 0..3 {
        close(
            original.min[i] + original.max[i],
            rotated.min[i] + rotated.max[i],
        )
    }
    close(
        original.max[1] - original.min[1],
        rotated.max[2] - rotated.min[2],
    );
    p.stock = StockDefinition {
        kind: StockKind::Cylinder,
        size: [150., 150., 100.],
        rotation: [0., 0., 20.],
    }
    .build()
    .unwrap();
    blank::fit(&mut p, 5., 1., 4.).unwrap();
    blank::contains_part(&p.stock, p.part.as_ref().unwrap()).unwrap();
    assert_eq!(p.stock.definition().kind, StockKind::Cylinder);
    assert_eq!(p.stock.definition().rotation, [0., 0., 20.]);
    let data = serde_json::to_string(&p).unwrap();
    assert_eq!(Project::from_json(data.as_bytes()).unwrap(), p);
}
#[test]
fn setup_is_atomic_undoable_and_legacy_projects_migrate() {
    let mut s = Session::new(part_project());
    let before = s.project().clone();
    assert!(s
        .apply(Edit::ConfigureSetup {
            definition: StockDefinition {
                kind: StockKind::Cylinder,
                size: [100., 100., 30.],
                rotation: [0.; 3]
            },
            rotation: [0., 90., 0.],
            offset: Some([f64::NAN, 0., 0.]),
            clearance: 5.,
            fit_margin: Some(5.),
            fit_top: None,
            fit_backing: None,
        })
        .is_err());
    assert_eq!(s.project(), &before);
    s.apply(Edit::ConfigureSetup {
        definition: StockDefinition {
            kind: StockKind::Cylinder,
            size: [100., 100., 30.],
            rotation: [0.; 3],
        },
        rotation: [0., 90., 0.],
        offset: None,
        clearance: 6.,
        fit_margin: Some(5.),
        fit_top: None,
        fit_backing: None,
    })
    .unwrap();
    blank::contains_part(&s.project().stock, s.project().part.as_ref().unwrap()).unwrap();
    s.apply(Edit::Undo).unwrap();
    assert_eq!(s.project(), &before);
    let mut legacy = serde_json::to_value(Project::default()).unwrap();
    legacy["format_version"] = 9.into();
    legacy["stock"].as_object_mut().unwrap().remove("profile");
    assert_eq!(
        Project::from_json(legacy.to_string().as_bytes())
            .unwrap()
            .format_version,
        opensourcecam::model::FORMAT_VERSION
    );
}
#[test]
fn example_vise_reserves_backing_and_can_be_refitted() {
    let mut p = part_project();
    let before = p.part.clone();
    workholding::place_example(&mut p, None, ViseSpec::default()).unwrap();
    assert_eq!(p.part, before);
    let bottom = p.part.as_ref().unwrap().bounds().min[2];
    assert!(p.stock.thickness + bottom >= 4. - 1e-7);
    assert_eq!(p.workspace.len(), 1);
    assert_eq!(p.workspace[0].part.geometry.mesh.triangles.len(), 36);
    let id = p.workspace[0].id;
    workholding::place_example(
        &mut p,
        Some(id),
        ViseSpec {
            jaw_width: 140.,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(p.workspace.len(), 1);
    assert_eq!(p.workspace[0].vise.as_ref().unwrap().jaw_width, 140.);
    cam::validate_setup(&p).unwrap();
}
#[test]
fn machine_library_preserves_controller_and_validates_import() {
    let records = machine_library::builtins();
    assert_eq!(records.len(), 4);
    for r in records {
        r.validate().unwrap();
        let mut p = Project::default();
        r.apply(&mut p).unwrap();
        assert_eq!(p.post, r.post);
        post::gcode(&p).unwrap();
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(machine_library::parse(&json).unwrap(), vec![r]);
    }
    let mut record = machine_library::builtins()[0].clone();
    record.post.processor = post::PostProcessor::Klipper;
    assert!(record.validate().is_err());
}
#[test]
fn operation_window_drafts_commit_atomically_and_detect_stale_edits() {
    let mut s = Session::new(Project::default());
    let original = s.project().operations[0].clone();
    let mut draft = original.clone();
    draft.depth = -1.;
    let before = s.project().clone();
    assert!(s
        .apply(Edit::SaveOperation {
            index: Some(0),
            expected: Some(Box::new(original.clone())),
            operation: Box::new(draft)
        })
        .is_err());
    assert_eq!(s.project(), &before);
    let mut draft = original.clone();
    draft.settings.feed = Some(300.);
    s.apply(Edit::SaveOperation {
        index: Some(0),
        expected: Some(Box::new(original.clone())),
        operation: Box::new(draft),
    })
    .unwrap();
    assert!(s
        .apply(Edit::SaveOperation {
            index: Some(0),
            expected: Some(Box::new(original.clone())),
            operation: Box::new(original)
        })
        .is_err());
    let op = commands::operation_template(s.project(), Strategy::Drill);
    s.apply(Edit::SaveOperation {
        index: None,
        expected: None,
        operation: Box::new(op),
    })
    .unwrap();
    assert_eq!(s.project().operations.len(), 3);
}

#[test]
fn planning_can_preserve_a_measured_setup_and_orientation_changes_stale_links() {
    let mut p = part_project();
    p.stock = StockDefinition {
        kind: StockKind::Cylinder,
        size: [120., 120., 20.],
        rotation: [0., 0., 15.],
    }
    .build()
    .unwrap();
    let bounds = p.part.as_ref().unwrap().bounds();
    let stock_center = [p.stock.width / 2., p.stock.height / 2.];
    for (i, center) in stock_center.iter().enumerate() {
        p.part.as_mut().unwrap().offset[i] += center - (bounds.min[i] + bounds.max[i]) / 2.;
    }
    let stock = p.stock.clone();
    let part = p.part.clone();
    let plan = commands::plan_project(
        &p,
        commands::Command::PlanSurface,
        &opensourcecam::machining::PlanOptions {
            fit_stock: false,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(plan.project.stock, stock);
    assert_eq!(plan.project.part, part);
    let mut changed = plan.project;
    changed
        .part
        .as_mut()
        .unwrap()
        .orient_centered([0., 0., 90.])
        .unwrap();
    assert!(cam::generate(&changed)
        .unwrap_err()
        .to_lowercase()
        .contains("changed"));
}
