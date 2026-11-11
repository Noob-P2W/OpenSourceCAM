use opensourcecam::{
    cam,
    import::{self, Units, MAX_IMPORT_BYTES, MAX_TRIANGLES},
    model::{Project, FORMAT_VERSION},
    post,
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct File {
    path: PathBuf,
}
impl File {
    fn new(extension: &str, bytes: &[u8]) -> Self {
        let path = std::env::temp_dir().join(format!(
            "osc-test-{}-{}.{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
            extension
        ));
        std::fs::write(&path, bytes).unwrap();
        Self { path }
    }
}
impl Drop for File {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

const ASCII: &str = "solid sample\nfacet normal 0 0 1\n outer loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid sample\n";

fn binary() -> Vec<u8> {
    let mut bytes = b"solid is also a valid binary header".to_vec();
    bytes.resize(80, 0);
    bytes.extend(1_u32.to_le_bytes());
    for number in [0_f32, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
        bytes.extend(number.to_le_bytes());
    }
    bytes.extend(0_u16.to_le_bytes());
    bytes
}

#[test]
fn ascii_and_binary_stl_match_even_with_solid_binary_header() {
    let ascii = File::new("STL", ASCII.as_bytes());
    let bin = File::new("stl", &binary());
    let a = import::import_file(&ascii.path, Units::Millimetres).unwrap();
    let b = import::import_file(&bin.path, Units::Millimetres).unwrap();
    assert_eq!(a.geometry.mesh, b.geometry.mesh);
    assert_eq!(a.bounds().size(), [1.0, 1.0, 0.0]);
    assert!(a.geometry.cad_source.is_none());
}

#[test]
fn stl_supports_multiple_ascii_solids_without_losing_faces() {
    let input = File::new("stl", format!("{ASCII}{ASCII}").as_bytes());
    let part = import::import_file(&input.path, Units::Millimetres).unwrap();
    assert_eq!(part.geometry.mesh.triangles.len(), 2);
}

#[test]
fn malformed_stl_never_becomes_partial_geometry() {
    let mut nan = binary();
    nan[96..100].copy_from_slice(&f32::NAN.to_le_bytes());
    let mut huge = binary();
    huge[80..84].copy_from_slice(&u32::MAX.to_le_bytes());
    let invalid = [
        Vec::new(),
        binary()[..120].to_vec(),
        nan,
        huge,
        ASCII.replace("vertex 0 1 0", "vertex 1 0 0").into_bytes(),
        ASCII.replace("endsolid sample", "").into_bytes(),
        format!("{ASCII}garbage\n").into_bytes(),
        ASCII.replace("endfacet", "endwrong").into_bytes(),
        ASCII.replace("vertex 1 0 0", "vertex 1 0 0 2").into_bytes(),
    ];
    for bytes in invalid {
        let file = File::new("stl", &bytes);
        assert!(import::import_file(&file.path, Units::Millimetres).is_err());
    }
}

#[test]
fn obj_quads_negative_indices_and_slash_indices_are_triangulated() {
    let file = File::new("obj", b"# no materials are fetched\nmtllib unavailable.mtl\nv 0 0 0\nv 2 0 0\nv 2 1 0\nv 0 1 0\nf -4/1/1 -3/2/1 -2/3/1 -1/4/1 # quad\n");
    let part = import::import_file(&file.path, Units::Millimetres).unwrap();
    assert_eq!(part.geometry.mesh.triangles, [[0, 1, 2], [0, 2, 3]]);
    assert_eq!(part.bounds().size(), [2.0, 1.0, 0.0]);
}

#[test]
fn obj_rejects_bad_indices_concavity_and_nonplanar_polygons() {
    for text in [
        "v 0 0 0\nf 1 2 3",
        "v 0 0 0\nf 0 0 0",
        "v 0 0 0\nf -2 -1 -1",
        "v 0 0 0\nv 1 0 0\nv 1 1 1\nv 0 1 0\nf 1 2 3 4",
        "v 0 0 0\nv 2 0 0\nv 1 0.2 0\nv 2 2 0\nv 0 2 0\nf 1 2 3 4 5",
        "v inf 0 0\nf 1 1 1",
        "v 0 0 0\ncurv 0 1 1",
        "v 0 0 0 1",
    ] {
        let file = File::new("obj", text.as_bytes());
        assert!(
            import::import_file(&file.path, Units::Millimetres).is_err(),
            "Accepted {text}"
        );
    }
}

#[test]
fn input_limits_and_cancellation_are_enforced() {
    let file = File::new("stl", ASCII.as_bytes());
    assert!(
        import::import_with_cancel(&file.path, Units::Millimetres, &AtomicBool::new(true))
            .unwrap_err()
            .contains("cancelled")
    );
    std::fs::OpenOptions::new()
        .write(true)
        .open(&file.path)
        .unwrap()
        .set_len(MAX_IMPORT_BYTES + 1)
        .unwrap();
    assert!(import::import_file(&file.path, Units::Millimetres)
        .unwrap_err()
        .contains("32 MiB"));
    let mut mesh = import::Mesh {
        vertices: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        triangles: vec![[0, 1, 2]; MAX_TRIANGLES + 1],
    };
    assert!(mesh.validate().is_err());
    mesh.triangles = vec![[0, 1, 8]];
    assert!(mesh.validate().is_err());
}

#[test]
fn inch_scaling_placement_and_portable_project_roundtrip() {
    let file = File::new("stl", ASCII.as_bytes());
    let part = import::import_file(&file.path, Units::Inches).unwrap();
    assert_eq!(part.bounds().size(), [25.4, 25.4, 0.0]);
    let project = Project::from_part(part).unwrap();
    assert_eq!(project.stock.width, 35.4);
    assert_eq!(project.part.as_ref().unwrap().bounds().min, [5.0, 5.0, 0.0]);
    assert!(project.operations.is_empty());
    assert!(post::gcode(&project).is_err()); // No invented machining from a mesh.
    let output = File::new("json", b"");
    project.save(&output.path).unwrap();
    std::fs::remove_file(&file.path).unwrap(); // Project must not depend on the source file.
    let loaded = Project::load(&output.path).unwrap();
    assert_eq!(project, loaded);
    cam::validate_setup(&loaded).unwrap();
}

#[test]
fn old_projects_upgrade_and_invalid_embedded_meshes_are_rejected() {
    let mut value = serde_json::to_value(Project::default()).unwrap();
    value["format_version"] = 1.into();
    let file = File::new("json", serde_json::to_string(&value).unwrap().as_bytes());
    let loaded = Project::load(&file.path).unwrap();
    assert_eq!(loaded.format_version, FORMAT_VERSION);
    let stl = File::new("stl", ASCII.as_bytes());
    let part = import::import_file(&stl.path, Units::Millimetres).unwrap();
    let mut value = serde_json::to_value(Project::from_part(part).unwrap()).unwrap();
    value["part"]["geometry"]["mesh"]["triangles"][0][0] = 999.into();
    let invalid = File::new("json", serde_json::to_string(&value).unwrap().as_bytes());
    assert!(Project::load(&invalid.path).is_err());
}

#[test]
fn supplied_mesh_examples_have_the_same_dimensions() {
    for extension in ["stl", "obj"] {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(format!("examples/models/mount.{extension}"));
        let part = import::import_file(&path, Units::Millimetres).unwrap();
        assert_eq!(part.geometry.mesh.triangles.len(), 716);
        assert_eq!(part.bounds().size(), [80.0, 50.0, 12.0]);
    }
}

#[test]
#[ignore = "requires the bundled runtime; packaging CI runs ignored tests explicitly"]
fn bundled_kernel_imports_step_iges_and_file_units() {
    assert!(
        import::cad_backend().is_some(),
        "Build the bundled CAD runtime first"
    );
    for (name, size) in [
        ("mount.step", [80.0, 50.0, 12.0]),
        ("mount.iges", [80.0, 50.0, 12.0]),
        ("inch-box.step", [25.4, 50.8, 12.7]),
    ] {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("examples/models")
            .join(name);
        let part = import::import_file(&path, Units::Millimetres).unwrap();
        for (actual, expected) in part.bounds().size().into_iter().zip(size) {
            assert!((actual - expected).abs() < 1e-4);
        }
        assert!(
            part.geometry
                .cad_source
                .as_ref()
                .unwrap()
                .contains("ISO-10303")
                || name.ends_with("iges")
        );
        let file = File::new("json", b"");
        let project = Project::from_part(part).unwrap();
        project.save(&file.path).unwrap();
        assert!(
            Project::load(&file.path).unwrap() == project,
            "CAD project roundtrip changed {name}"
        );
    }
    let broken = File::new(
        "step",
        b"ISO-10303-21;\nnot a CAD shape;\nEND-ISO-10303-21;",
    );
    assert!(import::import_file(&broken.path, Units::Millimetres).is_err());
    assert!(import::import_file(&broken.path, Units::Inches)
        .unwrap_err()
        .contains("units"));
}

#[test]
fn model_import_removes_only_zero_area_triangles_and_records_the_count() {
    let obj=b"v 0 0 0\nv 1 0 0\nv 0 1 0\nv 0.000001 0 0\nv 0 0.000001 0\nf 1 2 3\nf 1 1 1\nf 1 2 4\nf 1 4 5\n";
    let part = import::import_bytes(
        std::path::Path::new("repair.obj"),
        Units::Millimetres,
        obj,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(part.geometry.removed_degenerate_triangles, 2);
    assert_eq!(part.geometry.mesh.triangles, vec![[0, 1, 2], [0, 3, 4]]);
    part.validate().unwrap();
    let roundtrip: import::Part =
        serde_json::from_str(&serde_json::to_string(&part).unwrap()).unwrap();
    assert_eq!(part, roundtrip);
    let mut broken = (*part.geometry).clone();
    broken.mesh.triangles.push([0, 0, 0]);
    assert!(
        broken.mesh.validate().unwrap_err().contains("degenerate"),
        "Existing project validation must remain strict"
    );
}

#[test]
fn ascii_and_binary_stl_cleanup_preserve_valid_facets() {
    let text=ASCII.replace("endsolid sample", "facet normal 0 0 0\nouter loop\nvertex 0 0 0\nvertex 0 0 0\nvertex 0 0 0\nendloop\nendfacet\nendsolid sample");
    let part = import::import_bytes(
        std::path::Path::new("repair.stl"),
        Units::Millimetres,
        text.as_bytes(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(part.geometry.removed_degenerate_triangles, 1);
    assert_eq!(part.geometry.mesh.triangles.len(), 1);
    let mut binary = vec![0u8; 184];
    binary[80..84].copy_from_slice(&2u32.to_le_bytes());
    for (i, value) in [0f32, 0., 0., 1., 0., 0., 0., 1., 0.]
        .into_iter()
        .enumerate()
    {
        binary[96 + i * 4..100 + i * 4].copy_from_slice(&value.to_le_bytes());
    }
    let other = import::import_bytes(
        std::path::Path::new("binary.stl"),
        Units::Millimetres,
        &binary,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(other.geometry.removed_degenerate_triangles, 1);
    assert_eq!(other.geometry.mesh, part.geometry.mesh);
}

#[test]
fn repair_rejects_unusable_or_invalid_meshes_before_discarding_faces() {
    let mut mesh = import::Mesh {
        vertices: vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]],
        triangles: vec![[0, 0, 0]],
    };
    assert!(mesh
        .repair_degenerate_triangles()
        .unwrap_err()
        .contains("no usable"));
    mesh.triangles = vec![[0, 1, 99]];
    assert!(mesh
        .repair_degenerate_triangles()
        .unwrap_err()
        .contains("invalid vertex"));
    mesh.triangles = vec![[0, 1, 2]];
    mesh.vertices.push([f64::NAN, 0., 0.]);
    assert!(mesh.repair_degenerate_triangles().is_err());
    mesh.vertices.pop();
    mesh.triangles = vec![[0, 0, 0]; MAX_TRIANGLES + 1];
    assert!(mesh
        .repair_degenerate_triangles()
        .unwrap_err()
        .contains("250,000"));
}

#[test]
fn cleanup_drops_orphan_vertices_so_camera_fit_cannot_include_bad_face_outliers() {
    let mut mesh = import::Mesh {
        vertices: vec![
            [99999., 99999., 99999.],
            [0., 0., 0.],
            [1., 0., 0.],
            [0., 1., 0.],
        ],
        triangles: vec![[0, 0, 0], [1, 2, 3]],
    };
    assert_eq!(mesh.repair_degenerate_triangles().unwrap(), 1);
    assert_eq!(
        mesh.vertices,
        vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]]
    );
    assert_eq!(mesh.triangles, vec![[0, 1, 2]]);
    assert_eq!(mesh.repair_degenerate_triangles().unwrap(), 0);
    mesh.validate().unwrap();
}

#[test]
fn imports_above_the_previous_triangle_cap_remain_bounded() {
    let mesh = import::Mesh {
        vertices: vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]],
        triangles: vec![[0, 1, 2]; 150_000],
    };
    mesh.validate().unwrap();
    let mut over = mesh;
    over.triangles.resize(MAX_TRIANGLES + 1, [0, 1, 2]);
    assert!(over.validate().is_err());
}

#[test]
fn cad_component_ranges_survive_degenerate_cleanup_and_reject_cross_body_welding() {
    use import::{Body, Geometry, SourceFormat};
    let mut geometry = Geometry {
        bodies: vec![
            Body {
                name: "Empty component".into(),
                first_triangle: 0,
                triangle_count: 1,
            },
            Body {
                name: "Jaw".into(),
                first_triangle: 1,
                triangle_count: 2,
            },
            Body {
                name: "Insert".into(),
                first_triangle: 3,
                triangle_count: 1,
            },
        ],
        mesh: import::Mesh {
            vertices: vec![
                [9., 9., 9.],
                [0., 0., 0.],
                [1., 0., 0.],
                [0., 1., 0.],
                [0., 0., 0.],
                [1., 0., 0.],
                [0., 1., 0.],
            ],
            triangles: vec![[0, 0, 0], [1, 2, 3], [1, 1, 1], [4, 5, 6]],
        },
        removed_degenerate_triangles: 0,
        source_name: "assembly.step".into(),
        format: SourceFormat::Step,
        cad_source: None,
    };
    geometry.repair_import().unwrap();
    assert_eq!(geometry.removed_degenerate_triangles, 2);
    assert_eq!(geometry.bodies.len(), 2);
    assert_eq!(geometry.bodies[0].name, "Jaw");
    assert_eq!(geometry.bodies[0].first_triangle, 0);
    assert_eq!(geometry.bodies[0].triangle_count, 1);
    assert_eq!(geometry.bodies[1].first_triangle, 1);
    assert_eq!(geometry.body_at(1), Some(1));
    assert_eq!(geometry.body_owners().unwrap(), vec![0, 0, 0, 1, 1, 1]);
    geometry.mesh.triangles[1][0] = 0;
    assert!(geometry
        .body_owners()
        .unwrap_err()
        .contains("must not share"));
    geometry.mesh.triangles[1][0] = 3;
    geometry.bodies[1].first_triangle = usize::MAX;
    assert!(geometry.body_owners().is_err());
}
