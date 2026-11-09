use opensourcecam::{
    commands::{self, Command},
    import::{self, Units},
    machining::PlanOptions,
    model::Project,
    process::Process,
    session::{Edit, Session},
    tool_library::ToolKind,
};
use std::path::Path;

fn part() -> import::Part {
    import::import_file(Path::new("examples/models/mount.stl"), Units::Millimetres).unwrap()
}

#[test]
fn browser_import_preserves_the_selected_desktop_process() {
    for process in Process::ALL {
        let mut session = Session::new(Project::default());
        session.apply(Edit::NewProject { process }).unwrap();
        session.apply(Edit::ImportPart { part: part() }).unwrap();
        let project = session.project();
        assert_eq!(project.process, process);
        assert!(project.operations.is_empty());
        assert!(project.part.is_some());
        opensourcecam::cam::validate_setup(project).unwrap();
        if process != Process::Milling {
            assert!(session.run(Command::PlanSurface).is_err());
        }
        let saved = session.json().unwrap();
        assert_eq!(
            Project::from_json(saved.as_bytes()).unwrap().process,
            process
        );
    }
}

#[test]
fn tool_management_is_undoable_and_protects_operation_assignments() {
    let mut session = Session::new(Project::new_for_process(Process::Milling));
    session
        .apply(Edit::AddLibraryTool {
            tool_kind: ToolKind::FlatEndMill,
        })
        .unwrap();
    let id = session.project().active_tool.unwrap();
    session.apply(Edit::AddPreset { tool_id: id }).unwrap();
    assert_eq!(session.project().active_preset, Some(2));
    session.apply(Edit::AddOperation).unwrap();
    let before = session.json().unwrap();
    assert!(session.apply(Edit::RemoveTool { id }).is_err());
    assert_eq!(session.json().unwrap(), before);
    session.apply(Edit::DuplicateTool { id }).unwrap();
    assert_eq!(session.project().tool_library.tools.len(), 2);
    assert_ne!(
        session.project().tool_library.tools[0].post.number,
        session.project().tool_library.tools[1].post.number
    );
    session.apply(Edit::Undo).unwrap();
    assert_eq!(session.json().unwrap(), before);
    session
        .apply(Edit::AddLibraryTool {
            tool_kind: ToolKind::Holder,
        })
        .unwrap();
    assert_eq!(session.project().active_tool, Some(id));
    session.project().tool_library.validate().unwrap();
}

#[test]
fn explicit_planner_allowances_and_replacement_match_desktop_behavior() {
    let project = Project::from_part(part()).unwrap();
    let before = project.clone();
    let options = PlanOptions {
        xy_margin: 7.,
        top_allowance: 2.,
        backing: 1.,
        ..Default::default()
    };
    let plan = commands::plan_project(&project, Command::PlanSurface, &options).unwrap();
    assert_eq!(project, before);
    assert_eq!(plan.project.stock.width, 94.);
    assert_eq!(plan.project.stock.height, 64.);
    assert_eq!(plan.project.stock.thickness, 15.);
    assert_eq!(plan.project.operations.len(), 2);
    assert!(!plan.notes.is_empty());
    assert!(commands::plan_project(&plan.project, Command::PlanSurface, &options).is_err());
    let replacement = commands::plan_project(
        &plan.project,
        Command::PlanSurface,
        &PlanOptions {
            replace_operations: true,
            ..options
        },
    )
    .unwrap();
    assert_eq!(replacement.project.operations.len(), 2);
    assert!(commands::plan_project(&project, Command::Generate, &Default::default()).is_err());
}

#[test]
fn shared_library_import_remaps_holders_and_preserves_current_assignments() {
    use opensourcecam::tool_library::{LibraryTool, ToolLibrary};
    let mut project = Project::new_for_process(Process::Milling);
    project.ensure_tool_library();
    let active = project.active_tool;
    let mut cutter = LibraryTool::new(10, 10, ToolKind::FlatEndMill);
    let holder = LibraryTool::new(11, 11, ToolKind::Holder);
    cutter.holder = Some(11);
    cutter.post.pocket = Some(10);
    let source = ToolLibrary {
        tools: vec![cutter, holder],
    };
    opensourcecam::tool_library::import_into(&mut project, source, false).unwrap();
    assert_eq!(project.active_tool, active);
    assert_eq!(project.tool_library.tools.len(), 3);
    assert_eq!(
        project.tool_library.tools[1].holder,
        Some(project.tool_library.tools[2].id)
    );
    assert_eq!(project.tool_library.tools[1].post.pocket, None);
    project.tool_library.validate().unwrap();
}

#[test]
fn simulated_stock_obj_roundtrips_through_the_shared_importer() {
    let project = Project::default();
    let path = opensourcecam::post::checked_path(&project).unwrap();
    let simulation = opensourcecam::stock::simulate(
        &project,
        &path,
        0.5,
        32,
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    let data = opensourcecam::stock::obj_data(&simulation).unwrap();
    let part = import::import_bytes(
        Path::new("stock.obj"),
        Units::Millimetres,
        data.as_bytes(),
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(
        part.geometry.mesh.triangles,
        simulation.geometry.mesh.triangles
    );
    assert_eq!(
        part.geometry.mesh.vertices,
        simulation.geometry.mesh.vertices
    );
}

#[test]
fn duplicated_tools_clear_controller_register_bindings() {
    let mut project = Project::new_for_process(Process::Milling);
    project.ensure_tool_library();
    let id = project.active_tool.unwrap();
    let tool = &mut project.tool_library.tools[0];
    tool.post.pocket = Some(1);
    tool.post.length_offset = Some(1);
    tool.post.diameter_offset = Some(1);
    let mut session = Session::new(project);
    session.apply(Edit::DuplicateTool { id }).unwrap();
    let copy = session.project().tool_library.tools.last().unwrap();
    assert_eq!(copy.post.pocket, None);
    assert_eq!(copy.post.length_offset, None);
    assert_eq!(copy.post.diameter_offset, None);
}
