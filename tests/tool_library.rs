use opensourcecam::{
    cam,
    machine::ChangeMode,
    model::{Operation, Project, Strategy},
    post,
    tool_library::{LibraryTool, ToolKind, ToolLibrary},
};

#[test]
fn job_coolant_override_can_force_off_even_when_a_preset_requests_flood() {
    let mut p = Project::default();
    p.ensure_tool_library();
    p.tool_library.tools[0].presets[0].coolant = post::Coolant::Flood;
    assert!(post::gcode(&p).unwrap().contains("\nM8\n"));
    p.post.use_tool_coolant = false;
    p.post.coolant = post::Coolant::Off;
    assert!(!post::gcode(&p).unwrap().contains("\nM8\n"));
}

#[test]
fn head_settings_enforce_temperature_flow_current_and_rated_speed() {
    for (kind, geometry, parameter, bad) in [
        (ToolKind::PrintingHead, false, "nozzle_temperature", 400.0),
        (ToolKind::PrintingHead, false, "print_speed", 1000.0),
        (ToolKind::PrintingHead, false, "first_layer_speed", 1000.0),
        (ToolKind::Plasma, false, "current", 500.0),
        (ToolKind::Waterjet, false, "pressure", 5000.0),
        (ToolKind::FlatEndMill, false, "spindle_rpm", 70000.0),
        (ToolKind::FlatEndMill, true, "corner_radius", 4.0),
    ] {
        let mut tool = LibraryTool::new(1, 1, kind);
        if geometry {
            tool.geometry.insert(parameter.into(), bad);
        } else {
            tool.presets[0].values.insert(parameter.into(), bad);
        }
        assert!(
            ToolLibrary { tools: vec![tool] }.validate().is_err(),
            "{kind:?} accepted invalid {parameter}"
        );
    }
}

#[test]
fn tool_file_roundtrip_preserves_multi_mode_bindings_and_rejects_units_and_size() {
    let mut tool = LibraryTool::new(1, 1, ToolKind::PrintingHead);
    tool.uses.push(opensourcecam::process::Process::Mixed);
    tool.post.pocket = Some(4);
    tool.post.mount_offset = [20.0, -2.0, 50.0];
    let library = ToolLibrary { tools: vec![tool] };
    let file = std::env::temp_dir().join(format!("osc-library-{}.json", std::process::id()));
    library.save(&file).unwrap();
    assert_eq!(ToolLibrary::load(&file).unwrap(), library);
    let text = std::fs::read_to_string(&file)
        .unwrap()
        .replace("\"units\": \"mm\"", "\"units\": \"in\"");
    std::fs::write(&file, text).unwrap();
    assert!(ToolLibrary::load(&file).unwrap_err().contains("units"));
    std::fs::write(&file, vec![b' '; 8 * 1024 * 1024 + 1]).unwrap();
    assert!(ToolLibrary::load(&file).unwrap_err().contains("8 MiB"));
    std::fs::remove_file(file).unwrap();
}

#[test]
fn mixed_heads_and_turning_turrets_store_data_without_enabling_unimplemented_motion() {
    use opensourcecam::process::Process;
    let mut p = Project::new_for_process(Process::Mixed);
    let mut library = ToolLibrary::default();
    for kind in [
        ToolKind::FlatEndMill,
        ToolKind::Laser,
        ToolKind::PrintingHead,
    ] {
        let mut tool = LibraryTool::new(
            library.next_id(),
            library.next_number_for(&[kind.process(), Process::Mixed]),
            kind,
        );
        tool.uses.push(Process::Mixed);
        library.tools.push(tool);
    }
    p.tool_library = library;
    p.active_tool = Some(1);
    cam::validate_setup(&p).unwrap();
    assert!(post::gcode(&p).is_err());
    let mut turning = Project::new_for_process(Process::Turning);
    turning.ensure_tool_library();
    let changer = &mut turning.machine.as_mut().unwrap().tool_changer;
    changer.mode = ChangeMode::TurningTurret;
    changer.capacity = 8;
    assert!(cam::validate_setup(&turning)
        .unwrap_err()
        .contains("station"));
    turning.tool_library.tools[0].post.pocket = Some(2);
    turning.tool_library.tools[0].post.turning_offset = Some(2);
    cam::validate_setup(&turning).unwrap();
    assert!(post::gcode(&turning).is_err());
}

#[test]
fn machine_only_profiles_preserve_the_library_and_legacy_tool_profiles_add_definitions() {
    let mut p = Project::default();
    p.ensure_tool_library();
    let original = p.tool_library.clone();
    let mut profile = opensourcecam::machine::SetupProfile::from_project(&p).unwrap();
    assert_eq!(profile.format_version, 3);
    profile.machine.name = "Second machine".into();
    profile.apply(&mut p).unwrap();
    assert_eq!(p.tool_library, original);
    let mut legacy = Project::default();
    legacy.tool.diameter = 3.0;
    let profile = opensourcecam::machine::SetupProfile::from_project(&legacy).unwrap();
    assert_eq!(profile.format_version, 1);
    profile.apply(&mut p).unwrap();
    assert_eq!(p.tool_library.tools.len(), 2);
    assert_eq!(p.tool_library.tools[0], original.tools[0]);
    assert_eq!(p.operation_tool(&p.operations[0]).unwrap().diameter, 3.0);
}

#[test]
fn atc_never_guesses_head_offsets_or_reuses_calibration_registers() {
    let mut p = two_tools();
    p.post.processor = post::PostProcessor::LinuxCnc;
    p.machine.as_mut().unwrap().tool_changer.mode = ChangeMode::MillingAtc;
    p.machine.as_mut().unwrap().tool_changer.controller_managed = true;
    p.tool_library.tools[1].post.length_offset = Some(1);
    assert!(post::gcode(&p).unwrap_err().contains("separate calibrated"));
    p.tool_library.tools[1].post.length_offset = Some(2);
    p.tool_library.tools[1].post.mount_offset[0] = 10.0;
    assert!(post::gcode(&p).unwrap_err().contains("kinematics"));
    p.tool_library.tools[1].post.mount_offset = [0.0; 3];
    let code = post::gcode(&p).unwrap();
    assert!(code.find("T1 M6").unwrap() < code.find("G43 H1").unwrap());
    assert!(code.find("G43 H1").unwrap() < code.find("G0 Z").unwrap());
}

#[test]
fn every_catalogue_type_has_complete_valid_parameters_and_roundtrips() {
    assert_eq!(ToolKind::ALL.len(), 37);
    let mut library = ToolLibrary::default();
    for kind in ToolKind::ALL {
        let tool = LibraryTool::new(library.next_id(), library.next_number(kind.process()), kind);
        library.tools.push(tool);
        library
            .validate()
            .unwrap_or_else(|e| panic!("{}: {e}", kind.label()));
    }
    let encoded = serde_json::to_string(&library).unwrap();
    assert_eq!(
        serde_json::from_str::<ToolLibrary>(&encoded).unwrap(),
        library
    );
    for tool in &library.tools {
        assert_eq!(tool.geometry.len(), tool.kind.geometry().len());
        assert_eq!(tool.presets[0].values.len(), tool.kind.cutting().len());
    }
}

#[test]
fn unknown_missing_and_nonfinite_tool_data_cannot_be_saved() {
    let valid = LibraryTool::new(1, 1, ToolKind::FlatEndMill);
    let mut tool = valid.clone();
    tool.geometry.remove("diameter");
    assert!(ToolLibrary { tools: vec![tool] }.validate().is_err());
    let mut tool = valid.clone();
    tool.geometry.insert("unknown".into(), 1.0);
    assert!(ToolLibrary { tools: vec![tool] }.validate().is_err());
    let mut tool = valid;
    tool.presets[0].values.insert("cut_feed".into(), f64::NAN);
    assert!(ToolLibrary { tools: vec![tool] }.validate().is_err());
}

fn two_tools() -> Project {
    let mut project = Project::default();
    project.ensure_tool_library();
    let mut drill = LibraryTool::new(2, 2, ToolKind::Drill);
    drill.geometry.insert("diameter".into(), 3.0);
    drill.presets[0].values.insert("plunge_feed".into(), 80.0);
    drill.post.pocket = Some(2);
    drill.post.length_offset = Some(2);
    project.tool_library.tools[0].post.pocket = Some(1);
    project.tool_library.tools[0].post.length_offset = Some(1);
    project.tool_library.tools.push(drill);
    project.operations = vec![
        Operation {
            tool_id: Some(1),
            ..Default::default()
        },
        Operation {
            tool_id: Some(2),
            strategy: Strategy::Drill,
            x: 90.0,
            y: 60.0,
            ..Default::default()
        },
    ];
    project
}

#[test]
fn atc_restores_xy_after_m6_even_when_the_next_tool_uses_the_same_hole() {
    let mut p = two_tools();
    let mut first_drill = LibraryTool::new(1, 1, ToolKind::Drill);
    first_drill.post = p.tool_library.tools[0].post.clone();
    p.tool_library.tools[0] = first_drill;
    p.post.processor = post::PostProcessor::Mach3;
    let changer = &mut p.machine.as_mut().unwrap().tool_changer;
    changer.mode = ChangeMode::MillingAtc;
    changer.controller_managed = true;
    for op in &mut p.operations {
        op.strategy = Strategy::Drill;
        op.x = 20.0;
        op.y = 20.0;
    }
    let code = post::gcode(&p).unwrap();
    assert!(code.contains("T1 M6\nG43 H1\nG0 Z5.0000\nG0 X0.0000 Y0.0000\n"));
    assert!(code.contains("T2 M6\nG43 H2\nG0 Z5.0000\nG0 X20.0000 Y20.0000\n"));
}

#[test]
fn operation_tools_drive_radius_pecks_feeds_and_tool_changes() {
    let mut p = two_tools();
    let path = cam::generate(&p).unwrap();
    assert!(path
        .moves
        .iter()
        .any(|m| m.operation == 1 && m.feed == 80.0));
    assert!(post::gcode(&p).unwrap_err().contains("Multiple tools"));
    p.post.processor = post::PostProcessor::Mach3;
    let changer = &mut p.machine.as_mut().unwrap().tool_changer;
    changer.mode = ChangeMode::MillingAtc;
    changer.controller_managed = true;
    let code = post::gcode(&p).unwrap();
    assert_eq!(code.matches(" M6\n").count(), 2);
    assert!(code.contains("T1 M6\nG43 H1\n"));
    assert!(code.contains("T2 M6\nG43 H2\n"));
    assert!(code.contains("F80.0000"));
    p.post.processor = post::PostProcessor::Grbl;
    assert!(post::gcode(&p)
        .unwrap_err()
        .contains("does not support ATC"));
}

#[test]
fn atc_requires_real_capacity_offsets_and_controller_configuration() {
    let mut p = two_tools();
    p.post.processor = post::PostProcessor::LinuxCnc;
    p.machine.as_mut().unwrap().tool_changer.mode = ChangeMode::MillingAtc;
    assert!(post::gcode(&p).unwrap_err().contains("safe M6"));
    p.machine.as_mut().unwrap().tool_changer.controller_managed = true;
    p.machine.as_mut().unwrap().tool_changer.capacity = 1;
    assert!(post::gcode(&p).unwrap_err().contains("capacity"));
    p.machine.as_mut().unwrap().tool_changer.capacity = 20;
    p.tool_library.tools[1].post.length_offset = None;
    assert!(post::gcode(&p).unwrap_err().contains("length-offset"));
}

#[test]
fn unsupported_tools_and_missing_references_never_export_as_flat_mills() {
    let mut p = two_tools();
    p.tool_library.tools[1] = LibraryTool::new(2, 2, ToolKind::BallEndMill);
    p.operations[1].strategy = Strategy::Pocket;
    assert!(cam::generate(&p)
        .unwrap_err()
        .contains("3D roughing or finishing"));
    p.operations[1].tool_id = Some(999);
    assert!(post::gcode(&p).is_err());
}

#[test]
fn per_tooth_feed_and_spindle_direction_are_used() {
    let mut p = Project::default();
    p.ensure_tool_library();
    let t = &mut p.tool_library.tools[0];
    t.clockwise = false;
    t.flutes = 2;
    t.presets[0].feed_mode = opensourcecam::tool_library::FeedMode::PerTooth;
    t.presets[0].values.insert("feed_per_tooth".into(), 0.02);
    let code = post::gcode(&p).unwrap();
    assert!(code.contains("M4 S12000"));
    assert!(code.contains("F480.0000"));
}

#[test]
fn focused_tool_dimensions_match_supplied_geometry_in_assembly_coordinates() {
    let mut tool = opensourcecam::tool_library::LibraryTool::new(
        1,
        1,
        opensourcecam::tool_library::ToolKind::FlatEndMill,
    );
    for (key, value) in [
        ("diameter", 4.),
        ("shaft_diameter", 6.),
        ("overall_length", 63.),
        ("stickout", 16.5),
        ("shoulder_length", 14.),
        ("flute_length", 14.),
        ("gauge_length", 79.3396),
    ] {
        tool.geometry.insert(key.into(), value);
    }
    let dims = opensourcecam::tool_preview::dimensions(&tool);
    assert_eq!(dims.len(), 7);
    for d in dims {
        assert_eq!(d.value_mm, tool.dimension(&d.key));
        let distance = d
            .start
            .iter()
            .zip(d.end)
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f64>()
            .sqrt();
        assert!((distance - d.value_mm).abs() < 1e-9);
        assert!(d.start.into_iter().chain(d.end).all(f64::is_finite));
    }
}

#[test]
fn playback_uses_the_library_assembly_holder_and_effective_operation_binding() {
    use opensourcecam::tool_preview;
    let mut p = two_tools();
    p.operations[0].tool_id = None;
    p.active_tool = Some(1);
    let mut holder = LibraryTool::new(3, 3, ToolKind::Holder);
    holder.geometry.insert("body_diameter".into(), 30.);
    holder.geometry.insert("overall_length".into(), 80.);
    p.tool_library.tools.push(holder);
    let mill = &mut p.tool_library.tools[0];
    mill.holder = Some(3);
    mill.geometry.insert("stickout".into(), 35.);
    let original = p.clone();
    let first = tool_preview::playback(&p, 0).unwrap();
    assert_eq!(first.library_id, Some(1));
    assert_eq!(first.number, Some(1));
    assert_eq!(
        first.meshes,
        tool_preview::meshes(&p.tool_library.tools[0], &p.tool_library)
    );
    assert_eq!(first.min, [-15., -15., 0.]);
    assert_eq!(first.max, [15., 15., 115.]);
    let second = tool_preview::playback(&p, 1).unwrap();
    assert_eq!(second.library_id, Some(2));
    assert_eq!(second.number, Some(2));
    assert_eq!(
        second.meshes,
        tool_preview::meshes(&p.tool_library.tools[1], &p.tool_library)
    );
    assert_eq!(
        second.min[2], 0.,
        "The drill tip must stay at the path cursor"
    );
    assert_eq!(
        p, original,
        "Preview must not edit or initialize the library"
    );
    p.operations[1].tool_id = Some(999);
    assert!(tool_preview::playback(&p, 1).is_err());
    assert!(tool_preview::playback(&p, 1000).is_err());
}

#[test]
fn legacy_playback_uses_setup_dimensions_without_mutating_or_guessing_a_holder() {
    let mut p = Project::default();
    p.tool.diameter = 4.;
    if let opensourcecam::machine::Tooling::Milling {
        overall_length,
        shank_diameter,
        ..
    } = &mut p.machine.as_mut().unwrap().tooling
    {
        *overall_length = 73.;
        *shank_diameter = 6.;
    }
    let original = p.clone();
    let preview = opensourcecam::tool_preview::playback(&p, 0).unwrap();
    assert_eq!(preview.library_id, None);
    assert_eq!(preview.min, [-3., -3., 0.]);
    assert_eq!(preview.max, [3., 3., 73.]);
    assert!(preview.meshes[1]
        .vertices
        .iter()
        .all(|v| v[0].hypot(v[1]) <= 2. + 1e-8));
    assert_eq!(p, original);
}
