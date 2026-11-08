use opensourcecam::{
    cam::{generate, validate_setup, Point3},
    machine::{CutterKind, MachineSetup, SetupProfile, Tooling},
    model::{Project, Strategy, FORMAT_VERSION},
    post::gcode,
    process::Process,
};

#[test]
fn every_standard_process_has_distinct_valid_tooling_and_mixed_has_none() {
    for process in Process::ALL {
        let project = Project::new_for_process(process);
        if process == Process::Mixed {
            assert!(project.machine.is_none());
            continue;
        }
        let machine = project.machine.as_ref().unwrap();
        assert_eq!(machine.tooling.process(), process);
        machine.validate(process).unwrap();
        let profile = SetupProfile::from_project(&project).unwrap();
        let restored: SetupProfile =
            serde_json::from_str(&serde_json::to_string(&profile).unwrap()).unwrap();
        assert_eq!(restored, profile);
        let mut destination = Project::new_for_process(process);
        restored.apply(&mut destination).unwrap();
        assert_eq!(destination, project);
    }
}

#[test]
fn profile_application_is_atomic_and_preserves_job_geometry_and_post() {
    let mut source = Project::default();
    source.machine.as_mut().unwrap().name = "Bench mill".into();
    source.tool.diameter = 3.0;
    let profile = SetupProfile::from_project(&source).unwrap();
    let mut job = Project {
        name: "My part".into(),
        ..Default::default()
    };
    job.post.processor = opensourcecam::post::PostProcessor::Mach3;
    let stock = job.stock.clone();
    let ops = job.operations.clone();
    let post = job.post.clone();
    profile.apply(&mut job).unwrap();
    assert_eq!(job.machine.as_ref().unwrap().name, "Bench mill");
    assert_eq!(job.tool.diameter, 3.0);
    assert_eq!(job.stock, stock);
    assert_eq!(job.operations, ops);
    assert_eq!(job.post, post);
    assert_eq!(job.name, "My part");
    let before = job.clone();
    let mut bad = profile.clone();
    bad.machine.max_spindle_rpm = 1;
    assert!(bad.apply(&mut job).is_err());
    assert_eq!(job, before);
    let wrong =
        SetupProfile::from_project(&Project::new_for_process(Process::LaserCutting)).unwrap();
    assert!(wrong.apply(&mut job).is_err());
    assert_eq!(job, before);
    bad = profile;
    bad.format_version = 99;
    assert!(bad.apply(&mut job).is_err());
    assert_eq!(job, before);
}

#[test]
fn travel_checks_include_radius_offsets_clearance_and_work_origin() {
    let mut project = Project {
        operations: vec![opensourcecam::model::Operation {
            strategy: Strategy::OutsideProfile,
            x: 0.0,
            y: 0.0,
            width: 40.0,
            height: 30.0,
            ..Default::default()
        }],
        ..Default::default()
    };
    project.machine.as_mut().unwrap().enforce_limits = true;
    assert!(generate(&project).unwrap_err().contains("X travel"));
    assert!(gcode(&project).is_err());
    project.machine.as_mut().unwrap().work_origin = [10.0, 10.0, 0.0];
    generate(&project).unwrap();
    project.machine.as_mut().unwrap().axis_max[2] = 4.0;
    assert!(generate(&project).unwrap_err().contains("Z travel"));
    project.machine.as_mut().unwrap().axis_max[2] = 100.0;
    project.machine.as_mut().unwrap().axis_min[0] = 11.0;
    // The first tool-tip clearance at work X=0 is outside even though cuts start further in.
    assert!(generate(&project).unwrap_err().contains("X travel"));
    project.machine.as_mut().unwrap().enforce_limits = false;
    assert!(generate(&project).is_ok());
}

#[test]
fn travel_rounding_matches_the_exact_export_formatter_at_ties() {
    let mut machine = MachineSetup::for_process(Process::Milling).unwrap();
    machine.enforce_limits = true;
    machine.axis_min[0] = 0.03124;
    let x = 0.03125;
    assert_eq!(format!("{x:.4}"), "0.0312");
    assert!(
        machine.check_point(Point3 { x, y: 0.0, z: 0.0 }).is_err(),
        "Export rounds below the configured minimum"
    );
}

#[test]
fn valid_profiles_can_be_reused_while_job_operations_need_repair() {
    let mut source = Project::default();
    source.operations[0].width = 1.0;
    assert!(generate(&source).is_err());
    let profile =
        SetupProfile::from_project(&source).expect("A valid setup must not depend on job geometry");
    let mut target = source.clone();
    profile
        .apply(&mut target)
        .expect("Apply setup, then let the job report its incompatible operation");
    assert!(gcode(&target).is_err());
}

#[test]
fn travel_checks_include_posted_coordinate_rounding() {
    let mut machine = MachineSetup::for_process(Process::Milling).unwrap();
    machine.enforce_limits = true;
    machine.axis_min[0] = 1.00004;
    assert!(machine
        .check_point(Point3 {
            x: 1.000041,
            y: 0.0,
            z: 0.0
        })
        .is_err());
    assert!(machine
        .check_point(Point3 {
            x: 1.0001,
            y: 0.0,
            z: 0.0
        })
        .is_ok());
}

#[test]
fn invalid_ranges_and_process_specific_tools_are_rejected() {
    let mut project = Project::default();
    project.machine.as_mut().unwrap().axis_min[0] = 500.0;
    assert!(validate_setup(&project).is_err());
    project = Project::default();
    project.machine.as_mut().unwrap().work_origin[1] = f64::NAN;
    assert!(validate_setup(&project).is_err());
    project = Project::default();
    project.machine.as_mut().unwrap().max_feed = 100.0;
    assert!(gcode(&project).unwrap_err().contains("maximum feed"));
    project = Project::default();
    project.machine.as_mut().unwrap().max_spindle_rpm = 100;
    assert!(gcode(&project).unwrap_err().contains("maximum RPM"));
    for process in Process::ALL.into_iter().filter(|p| *p != Process::Mixed) {
        let mut p = Project::new_for_process(process);
        match &mut p.machine.as_mut().unwrap().tooling {
            Tooling::Milling { flute_length, .. } => *flute_length = 1000.0,
            Tooling::Turning { insert_angle, .. } => *insert_angle = 180.0,
            Tooling::Printing { layer_height, .. } => *layer_height = 1.0,
            Tooling::Laser { power_watts, .. } => *power_watts = -1.0,
            Tooling::Plasma { pierce_height, .. } => *pierce_height = 0.5,
            Tooling::Grinding { bore_diameter, .. } => *bore_diameter = 200.0,
            Tooling::WireEdm { taper_angle, .. } => *taper_angle = 90.0,
            Tooling::SinkerEdm { spark_gap, .. } => *spark_gap = -1.0,
        }
        assert!(
            validate_setup(&p).is_err(),
            "Invalid tool accepted for {process:?}"
        );
    }
}

#[test]
fn milling_tool_reach_and_drill_strategy_are_enforced() {
    let mut project = Project::default();
    if let Tooling::Milling { flute_length, .. } = &mut project.machine.as_mut().unwrap().tooling {
        *flute_length = 2.0;
    }
    assert!(gcode(&project).unwrap_err().contains("flute length"));
    project.machine.as_mut().unwrap().check_tool_reach = false;
    gcode(&project).unwrap();
    if let Tooling::Milling { kind, .. } = &mut project.machine.as_mut().unwrap().tooling {
        *kind = CutterKind::Drill;
    }
    assert!(gcode(&project)
        .unwrap_err()
        .contains("only be used for drilling"));
    project.operations.truncate(1);
    project.operations[0].strategy = Strategy::Drill;
    gcode(&project).unwrap();
}

#[test]
fn rapid_feed_affects_estimate_without_changing_motion_or_posted_feed() {
    let mut project = Project::default();
    let normal = generate(&project).unwrap();
    let code = gcode(&project).unwrap();
    project.machine.as_mut().unwrap().rapid_feed = 600.0;
    let slow = generate(&project).unwrap();
    let delta = normal.rapid_length * (1.0 / 600.0 - 1.0 / 3000.0) * 60.0;
    assert!((slow.estimated_seconds - normal.estimated_seconds - delta).abs() < 1e-7);
    assert_eq!(slow.moves.len(), normal.moves.len());
    assert_eq!(gcode(&project).unwrap(), code);
}

#[test]
fn old_machine_less_projects_migrate_without_inventing_measured_reach() {
    let path = std::env::temp_dir().join(format!("osc-machine-{}.json", std::process::id()));
    for process in Process::ALL {
        let mut value = serde_json::to_value(Project::new_for_process(process)).unwrap();
        value["format_version"] = 4.into();
        value.as_object_mut().unwrap().remove("machine");
        if process == Process::Milling {
            value["stock"]["thickness"] = 40.into();
            value["operations"] = serde_json::to_value(vec![opensourcecam::model::Operation {
                depth: 30.0,
                ..Default::default()
            }])
            .unwrap();
        }
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        let loaded = Project::load(&path).unwrap();
        assert_eq!(loaded.format_version, FORMAT_VERSION);
        assert_eq!(loaded.machine.is_none(), process == Process::Mixed);
        if process == Process::Milling {
            assert!(!loaded.machine.as_ref().unwrap().check_tool_reach);
            assert!(gcode(&loaded).is_ok());
        }
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn reusable_profile_file_is_bounded_and_validated_on_load() {
    let path = std::env::temp_dir().join(format!("osc-profile-{}.json", std::process::id()));
    let profile = SetupProfile::from_project(&Project::default()).unwrap();
    profile.save(&path).unwrap();
    assert_eq!(SetupProfile::load(&path).unwrap(), profile);
    let example = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples/bench-mill.cam-machine.json");
    let supplied = SetupProfile::load(&example).unwrap();
    let mut job = Project::default();
    supplied.apply(&mut job).unwrap();
    gcode(&job).unwrap();
    std::fs::write(&path, vec![b' '; 128 * 1024 + 1]).unwrap();
    assert!(SetupProfile::load(&path).unwrap_err().contains("128 KiB"));
    std::fs::remove_file(path).unwrap();
}
