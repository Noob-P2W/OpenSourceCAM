use opensourcecam::{
    cam::generate,
    model::Project,
    post::{gcode, Coolant, PostProcessor, WorkOffset},
    process::Process,
};

#[test]
fn controller_dialects_preserve_all_planned_moves() {
    let mut project = Project::default();
    let expected = generate(&project).unwrap();
    let baseline = gcode(&project).unwrap();
    let baseline_moves: Vec<_> = baseline
        .lines()
        .filter(|line| line.starts_with("G0 ") || line.starts_with("G1 "))
        .collect();
    for processor in PostProcessor::ALL
        .into_iter()
        .filter(|p| p.ready(Process::Milling))
    {
        project.post.processor = processor;
        let code = gcode(&project).unwrap();
        let moves: Vec<_> = code
            .lines()
            .filter(|line| line.starts_with("G0 ") || line.starts_with("G1 "))
            .collect();
        // One initial clearance and one final retract surround the planner's moves.
        assert_eq!(moves.len(), expected.moves.len() + 2);
        assert_eq!(moves[0], "G0 Z5.0000");
        assert_eq!(moves, baseline_moves);
        assert!(!code.contains("M6"));
        assert!(!code.contains("G43"));
        match processor {
            PostProcessor::Mach3 | PostProcessor::LinuxCnc => {
                assert!(code.starts_with("%\n"));
                assert!(code.contains("\nG61\n"));
                assert!(code.ends_with("M30\n%\n"));
            }
            PostProcessor::Grbl => {
                assert!(!code.contains('%'));
                assert!(!code.contains("G61"));
                assert!(code.ends_with("M30\n"));
                assert!(code.lines().all(|line| line.len() < 80));
            }
            PostProcessor::Generic => assert!(code.ends_with("M2\n")),
            _ => unreachable!(),
        }
    }
}

#[test]
fn offsets_coolant_and_saved_post_roundtrip() {
    let mut project = Project::default();
    project.post.processor = PostProcessor::Mach3;
    project.post.coolant = Coolant::Flood;
    for offset in WorkOffset::ALL {
        project.post.work_offset = offset;
        let code = gcode(&project).unwrap();
        assert!(code.contains(&format!("\n{}\n", offset.code())));
        assert_eq!(code.lines().filter(|line| *line == "M8").count(), 1);
        assert!(code.ends_with("M5\nM9\nM30\n%\n"));
        let saved = serde_json::to_string(&project).unwrap();
        let loaded: Project = serde_json::from_str(&saved).unwrap();
        assert_eq!(loaded, project);
        assert_eq!(gcode(&loaded).unwrap(), code);
    }
    project.post.coolant = Coolant::Off;
    assert!(!gcode(&project).unwrap().contains("\nM8\n"));
}

#[test]
fn planned_and_incompatible_posts_cannot_export_milling_as_printing() {
    let printing = Project::new_for_process(Process::Printing3d);
    assert_eq!(printing.post.processor, PostProcessor::Klipper);
    assert!(gcode(&printing).unwrap_err().contains("not implemented"));
    let mut milling = Project::default();
    milling.post.processor = PostProcessor::Klipper;
    assert!(gcode(&milling).unwrap_err().contains("not compatible"));
    assert!(generate(&milling).is_err());
    assert!(PostProcessor::parse("unknown").is_err());
}

#[test]
fn version_three_projects_migrate_with_generic_post_defaults() {
    let mut value = serde_json::to_value(Project::default()).unwrap();
    value["format_version"] = 3.into();
    value.as_object_mut().unwrap().remove("post");
    let dir = std::env::temp_dir().join(format!("osc-v3-{}.cam.json", std::process::id()));
    std::fs::write(&dir, serde_json::to_vec(&value).unwrap()).unwrap();
    let result = Project::load(&dir);
    std::fs::remove_file(&dir).unwrap();
    let project = result.unwrap();
    assert_eq!(project.format_version, opensourcecam::model::FORMAT_VERSION);
    assert_eq!(project.post.processor, PostProcessor::Generic);
    assert_eq!(project.post.work_offset, WorkOffset::G54);
    assert!(gcode(&project).is_ok());
}
