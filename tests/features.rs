use opensourcecam::{
    features::{self, FeatureKind},
    import::{self, Units},
    model::Project,
};
use std::path::Path;
use std::sync::{atomic::AtomicBool, Arc};

fn model(path: &str) -> import::Part {
    let mut part = import::import_file(Path::new(path), Units::Millimetres).unwrap();
    part.align_to_top(5.0);
    part
}

#[test]
fn mounting_plate_mesh_recognizes_four_holes_and_one_pocket() {
    for filename in ["examples/models/mount.stl", "examples/models/mount.obj"] {
        let mut part = import::import_file(Path::new(filename), Units::Millimetres).unwrap();
        part.align_to_top(5.0);
        let report = features::recognize(&part).unwrap();
        assert_eq!(report.features.len(), 5, "{filename}: {report:?}");
        assert_eq!(report.skipped_openings, 0);
        let pocket = report
            .features
            .iter()
            .find(|f| f.kind == FeatureKind::RectangularPocket)
            .unwrap();
        assert_eq!(pocket.min, [25.0, 20.0]);
        assert_eq!(pocket.max, [65.0, 40.0]);
        assert!((pocket.depth() - 5.0).abs() < features::TOLERANCE);
        for hole in report
            .features
            .iter()
            .filter(|f| f.kind == FeatureKind::CircularHole)
        {
            assert!(hole.through);
            assert!((hole.diameter.unwrap() - 6.0).abs() < features::TOLERANCE);
            assert_eq!(hole.depth(), 12.0);
        }
    }
}

#[test]
fn candidates_copy_to_disabled_operations_with_explicit_tools_and_presets() {
    let mut part =
        import::import_file(Path::new("examples/models/mount.stl"), Units::Millimetres).unwrap();
    part.align_to_top(5.0);
    let report = features::recognize(&part).unwrap();
    let mut project = Project::from_part(part).unwrap();
    project.ensure_tool_library();
    let pocket = report
        .features
        .iter()
        .find(|f| f.kind == FeatureKind::RectangularPocket)
        .unwrap();
    let operation = features::operation(&project, pocket).unwrap();
    assert!(!operation.enabled);
    assert_eq!(operation.tool_id, project.active_tool);
    assert_eq!(operation.preset_id, Some(1));
    assert_eq!(operation.depth, 5.0);
    assert!(project.operations.is_empty());
}

#[test]
fn counterbores_and_pockets_with_islands_are_skipped() {
    let report = features::recognize(&model("tests/fixtures/features-unsupported.stl")).unwrap();
    assert!(report.features.is_empty(), "{report:?}");
    assert_eq!(report.skipped_openings, 2);
}

#[test]
fn rotated_pockets_are_skipped_but_round_holes_remain_candidates() {
    let mut part = model("examples/models/mount.stl");
    let geometry = Arc::make_mut(&mut part.geometry);
    let (sin, cos) = 0.5_f64.sin_cos();
    for v in &mut geometry.mesh.vertices {
        let [x, y, z] = *v;
        *v = [x * cos - y * sin, x * sin + y * cos, z];
    }
    part.align_to_top(5.0);
    let report = features::recognize(&part).unwrap();
    assert_eq!(report.features.len(), 4);
    assert_eq!(report.skipped_openings, 1);
    assert!(report
        .features
        .iter()
        .all(|f| f.kind == FeatureKind::CircularHole));
}

#[test]
fn placement_and_inch_units_are_applied_once_in_work_coordinates() {
    let mut part = model("examples/models/mount.stl");
    for v in &mut Arc::make_mut(&mut part.geometry).mesh.vertices {
        for x in v {
            *x /= 25.4;
        }
    }
    part.units = Units::Inches;
    part.offset = [11.0, 17.0, -12.0];
    let report = features::recognize(&part).unwrap();
    let pocket = report
        .features
        .iter()
        .find(|f| f.kind == FeatureKind::RectangularPocket)
        .unwrap();
    assert!((pocket.min[0] - 31.0).abs() < 1e-6);
    assert!((pocket.min[1] - 32.0).abs() < 1e-6);
    assert_eq!(pocket.depth(), 5.0);
    assert_eq!(report.features.len(), 5);
}

#[test]
fn open_or_inconsistently_wound_meshes_are_rejected_without_mutation() {
    let part = model("examples/models/mount.stl");
    for remove in [true, false] {
        let mut broken = part.clone();
        let geometry = Arc::make_mut(&mut broken.geometry);
        if remove {
            geometry.mesh.triangles.pop();
        } else {
            geometry.mesh.triangles[0].swap(0, 1);
        }
        let before = broken.clone();
        assert!(features::recognize(&broken).unwrap_err().contains("closed"));
        assert_eq!(broken, before);
    }
}

#[test]
fn recognition_can_be_cancelled_before_processing() {
    assert!(features::recognize_with_cancel(
        &model("examples/models/mount.stl"),
        &AtomicBool::new(true)
    )
    .unwrap_err()
    .contains("cancelled"));
}

#[test]
fn slightly_tilted_walls_are_not_accepted_as_vertical_extrusions() {
    let mut part = model("examples/models/mount.stl");
    for v in &mut Arc::make_mut(&mut part.geometry).mesh.vertices {
        v[0] += v[2] / 12.0 * 0.1;
    }
    let report = features::recognize(&part).unwrap();
    assert!(report.features.is_empty());
    assert_eq!(report.skipped_openings, 5);
}

#[test]
fn excessive_candidates_are_bounded_without_returning_a_partial_report() {
    let mut part = model("examples/models/mount.stl");
    let source = part.geometry.mesh.clone();
    let mesh = &mut Arc::make_mut(&mut part.geometry).mesh;
    mesh.vertices.clear();
    mesh.triangles.clear();
    for copy in 0..101 {
        let offset = mesh.vertices.len() as u32;
        mesh.vertices.extend(
            source
                .vertices
                .iter()
                .map(|v| [v[0] + copy as f64 * 90.0, v[1], v[2]]),
        );
        mesh.triangles
            .extend(source.triangles.iter().map(|t| t.map(|i| i + offset)));
    }
    assert!(features::recognize(&part).unwrap_err().contains("500"));
}

#[test]
fn conversion_rejects_wrong_tools_offsets_and_machine_limits() {
    let part = model("examples/models/mount.stl");
    let report = features::recognize(&part).unwrap();
    let mut project = Project::from_part(part).unwrap();
    project.ensure_tool_library();
    let hole = report
        .features
        .iter()
        .find(|f| f.kind == FeatureKind::CircularHole)
        .unwrap();
    project.tool_library.tools[0]
        .geometry
        .insert("diameter".into(), 9.0);
    assert!(features::operation(&project, hole)
        .unwrap_err()
        .contains("larger"));
    project.tool_library.tools[0]
        .geometry
        .insert("diameter".into(), 6.0);
    let mut displaced = hole.clone();
    displaced.opening_z = 2.0;
    assert!(features::operation(&project, &displaced)
        .unwrap_err()
        .contains("stock top"));
    project.machine.as_mut().unwrap().enforce_limits = true;
    project.machine.as_mut().unwrap().axis_max[0] = 1.0;
    assert!(features::operation(&project, hole).is_err());
    project.machine.as_mut().unwrap().enforce_limits = false;
    project.process = opensourcecam::process::Process::Turning;
    assert!(features::operation(&project, hole)
        .unwrap_err()
        .contains("Milling"));
}

#[test]
fn flat_bottom_holes_require_a_supported_flat_bottom_tool() {
    let part = model("tests/fixtures/features-blind-hole.stl");
    let report = features::recognize(&part).unwrap();
    assert_eq!(report.features.len(), 1);
    let hole = &report.features[0];
    assert!(!hole.through);
    assert_eq!(hole.depth(), 8.0);
    let mut project = Project::from_part(part).unwrap();
    project.ensure_tool_library();
    assert!(features::operation(&project, hole).is_ok());
    project.tool_library.tools[0] = opensourcecam::tool_library::LibraryTool::new(
        1,
        1,
        opensourcecam::tool_library::ToolKind::Drill,
    );
    assert!(features::operation(&project, hole)
        .unwrap_err()
        .contains("flat-bottom"));
}

#[test]
#[ignore = "requires the bundled CAD runtime"]
fn step_and_iges_use_the_same_rust_recognizer() {
    for path in ["examples/models/mount.step", "examples/models/mount.iges"] {
        let part = model(path);
        let report = features::recognize(&part).unwrap_or_else(|e| panic!("{path}: {e}"));
        assert_eq!(report.features.len(), 5, "{path}: {report:?}");
        assert_eq!(report.skipped_openings, 0);
    }
}

#[test]
fn closed_stl_with_sub_weld_edge_preserves_all_five_features() {
    let mut part = model("examples/models/mount.stl");
    let geometry = Arc::make_mut(&mut part.geometry);
    let [a, b, _] = geometry.mesh.triangles[0];
    let pa = geometry.mesh.vertices[a as usize];
    let pb = geometry.mesh.vertices[b as usize];
    let length = (0..3).map(|i| (pb[i] - pa[i]).powi(2)).sum::<f64>().sqrt();
    let inserted = geometry.mesh.vertices.len() as u32;
    geometry.mesh.vertices.push(std::array::from_fn(|i| {
        pa[i] + (pb[i] - pa[i]) * 0.00001 / length
    }));
    let original = std::mem::take(&mut geometry.mesh.triangles);
    let mut splits = 0;
    for t in original {
        if let Some(i) = (0..3).find(|i| {
            let (x, y) = (t[*i], t[(*i + 1) % 3]);
            (x == a && y == b) || (x == b && y == a)
        }) {
            let (x, y, z) = (t[i], t[(i + 1) % 3], t[(i + 2) % 3]);
            geometry
                .mesh
                .triangles
                .extend([[x, inserted, z], [inserted, y, z]]);
            splits += 1;
        } else {
            geometry.mesh.triangles.push(t);
        }
    }
    assert_eq!(splits, 2);
    // Exercise an actual binary STL import, including its f32 coordinates and
    // repeated vertices, rather than relying only on a constructed Part.
    let mut bytes = vec![0u8; 80];
    bytes.extend((geometry.mesh.triangles.len() as u32).to_le_bytes());
    for t in &geometry.mesh.triangles {
        bytes.extend([0u8; 12]);
        for index in t {
            for coordinate in geometry.mesh.vertices[*index as usize] {
                bytes.extend((coordinate as f32).to_le_bytes());
            }
        }
        bytes.extend([0u8; 2]);
    }
    let mesh = import::import_bytes(
        Path::new("fine-detail.stl"),
        Units::Millimetres,
        &bytes,
        &AtomicBool::new(false),
    )
    .unwrap()
    .geometry
    .mesh
    .clone();
    Arc::make_mut(&mut part.geometry).mesh = mesh;
    let before = part.clone();
    let report = features::recognize(&part).unwrap();
    assert_eq!(report.features.len(), 5);
    assert_eq!(part, before);
    // Preserving tiny faces must not allow an actual open seam through.
    Arc::make_mut(&mut part.geometry).mesh.triangles.pop();
    assert!(features::recognize(&part).unwrap_err().contains("closed"));
}

#[test]
fn almost_coincident_exchange_seams_still_use_bounded_welding() {
    let mut part = model("examples/models/mount.stl");
    let geometry = Arc::make_mut(&mut part.geometry);
    let original = geometry.mesh.clone();
    geometry.mesh.vertices.clear();
    geometry.mesh.triangles.clear();
    for (face, t) in original.triangles.iter().enumerate() {
        let start = geometry.mesh.vertices.len() as u32;
        for index in t {
            geometry
                .mesh
                .vertices
                .push(original.vertices[*index as usize].map(|x| x + (face % 3) as f64 * 1e-7));
        }
        geometry.mesh.triangles.push([start, start + 1, start + 2]);
    }
    assert_eq!(features::recognize(&part).unwrap().features.len(), 5);
}
