use opensourcecam::{
    import::{self, Units},
    model::Project,
    post::WorkOffset,
    process::Process,
    session::{Edit, Session},
    workspace::{Frame, ModelKind},
};
use std::{path::Path, sync::Arc};

fn part() -> import::Part {
    import::import_file(Path::new("examples/models/mount.stl"), Units::Millimetres).unwrap()
}

fn configured_project(process: Process) -> Project {
    let mut project = Project::new_for_process(process);
    project.ensure_tool_library();
    project.tool.name = "Measured setup tool".into();
    project.tool.feed = 321.;
    project.tool_library.tools[0].name = "Shop tool".into();
    let tool = &mut project.tool_library.tools[0];
    let mut preset = tool.presets[0].clone();
    preset.id = 2;
    preset.name = "Custom preset".into();
    tool.presets.push(preset);
    project.active_preset = Some(2);
    project.post.work_offset = WorkOffset::G55;
    project.clearance = 9.;
    if let Some(machine) = &mut project.machine {
        machine.name = "Shop CNC".into();
        machine.work_origin = [25., 30., -10.];
        machine.rapid_feed = 1234.;
    }
    project
}

#[test]
fn part_and_reference_imports_preserve_each_other_in_both_orders() {
    for kind in [ModelKind::Vise, ModelKind::Fixture, ModelKind::Machine] {
        for reference_first in [false, true] {
            let original = configured_project(Process::Milling);
            let mut session = Session::new(original.clone());
            if !reference_first {
                session.apply(Edit::ImportPart { part: part() }).unwrap();
                session.apply(Edit::AddOperation).unwrap();
            }
            let before_reference = session.project().clone();
            session
                .apply(Edit::AddWorkspace {
                    part: part(),
                    model_kind: kind,
                })
                .unwrap();
            let mut expected = before_reference;
            expected.workspace = session.project().workspace.clone();
            assert_eq!(
                session.project(),
                &expected,
                "adding {kind:?} replaced setup"
            );
            session
                .apply(Edit::PositionWorkspace {
                    id: 1,
                    offset: [150., 250., -20.],
                    rotation: [0., 0., 90.],
                    frame: Frame::Machine,
                })
                .unwrap();
            let reference = session.project().workspace[0].clone();
            if reference_first {
                session.apply(Edit::ImportPart { part: part() }).unwrap();
            }
            assert_eq!(session.project().workspace, vec![reference.clone()]);
            assert!(Arc::ptr_eq(
                &session.project().workspace[0].part.geometry,
                &reference.part.geometry
            ));
            assert!(session.project().part.is_some());
            let mut expected = original;
            let fitted = Project::from_part(part()).unwrap();
            expected.name = fitted.name;
            expected.stock = fitted.stock;
            expected.part = fitted.part;
            expected.workspace = vec![reference];
            if !reference_first {
                expected.operations = session.project().operations.clone();
                assert_eq!(expected.operations.len(), 1);
            }
            assert_eq!(session.project(), &expected);
        }
    }
}

#[test]
fn replacing_workpiece_refits_stock_and_clears_geometry_dependent_state_only() {
    for process in Process::ALL {
        let mut original = configured_project(process);
        original.part = Some(part());
        original.setup_face = Some(0);
        original.operations = Project::default().operations;
        let mut session = Session::new(original);
        session
            .apply(Edit::PlaceExampleVise {
                id: None,
                spec: Default::default(),
            })
            .unwrap();
        let before = session.project().clone();
        let mut replacement = part();
        Arc::make_mut(&mut replacement.geometry).source_name = "replacement.stl".into();
        for vertex in &mut Arc::make_mut(&mut replacement.geometry).mesh.vertices {
            vertex[0] *= 0.5;
        }
        let fitted = Project::from_part(replacement.clone()).unwrap();
        let mut expected = before.clone();
        expected.name = fitted.name;
        expected.stock = fitted.stock;
        expected.part = fitted.part;
        expected.setup_face = None;
        expected.operations.clear();
        session
            .apply(Edit::ImportPart { part: replacement })
            .unwrap();
        assert_eq!(session.project(), &expected);
        session.apply(Edit::Undo).unwrap();
        assert_eq!(session.project(), &before);
        session.apply(Edit::Redo).unwrap();
        assert_eq!(session.project(), &expected);
    }
}

#[test]
fn failed_imports_preserve_project_and_both_history_stacks() {
    let mut session = Session::new(configured_project(Process::Milling));
    session.apply(Edit::ImportPart { part: part() }).unwrap();
    let before_reference = session.project().clone();
    session
        .apply(Edit::AddWorkspace {
            part: part(),
            model_kind: ModelKind::Vise,
        })
        .unwrap();
    let before_operation = session.project().clone();
    session.apply(Edit::AddOperation).unwrap();
    let with_operation = session.project().clone();
    session.apply(Edit::Undo).unwrap();
    assert!(session.view().can_undo && session.view().can_redo);
    let mut invalid = part();
    Arc::make_mut(&mut invalid.geometry).mesh.triangles.clear();
    for edit in [
        Edit::ImportPart {
            part: invalid.clone(),
        },
        Edit::AddWorkspace {
            part: invalid,
            model_kind: ModelKind::Vise,
        },
    ] {
        assert!(session.apply(edit).is_err());
        assert_eq!(session.project(), &before_operation);
        assert!(session.view().can_undo && session.view().can_redo);
    }
    session.apply(Edit::Redo).unwrap();
    assert_eq!(session.project(), &with_operation);
    session.apply(Edit::Undo).unwrap();
    assert_eq!(session.project(), &before_operation);
    session.apply(Edit::Undo).unwrap();
    assert_eq!(session.project(), &before_reference);
    session.apply(Edit::Redo).unwrap();
    assert_eq!(session.project(), &before_operation);
    // A successful replacement is one edit and invalidates the old redo branch.
    let mut replacement = part();
    Arc::make_mut(&mut replacement.geometry).source_name = "replacement.stl".into();
    session
        .apply(Edit::ImportPart { part: replacement })
        .unwrap();
    assert!(!session.view().can_redo);
    session.apply(Edit::Undo).unwrap();
    assert_eq!(session.project(), &before_operation);
}
