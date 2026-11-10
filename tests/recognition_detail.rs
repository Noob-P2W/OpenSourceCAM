use opensourcecam::{
    commands::{self, GeometryQuery},
    features::{self, Shape},
    import::{self, Units},
    model::{Operation, Project, Strategy},
};
use std::path::Path;
fn load(name: &str) -> Project {
    let part = import::import_file(
        Path::new(&format!("tests/fixtures/{name}.stl")),
        Units::Millimetres,
    )
    .unwrap();
    let mut p = Project::from_part(part).unwrap();
    p.ensure_tool_library();
    p
}
fn shapes(p: &Project) -> Vec<serde_json::Value> {
    features::survey(p).unwrap()["features"]
        .as_array()
        .unwrap()
        .clone()
}
#[test]
fn chamfered_and_filleted_entries_retain_the_bore_and_actual_surfaces() {
    for (name, shape) in [
        ("recognition-chamfer", "chamfered_bore"),
        ("recognition-fillet", "filleted_bore"),
    ] {
        let p = load(name);
        let entries = shapes(&p);
        let entry = entries
            .iter()
            .find(|e| e["current_orientation"] == true && e["feature"]["detail"]["shape"] == shape)
            .unwrap_or_else(|| panic!("{name}: {entries:?}"));
        let f: features::Feature = serde_json::from_value(entry["feature"].clone()).unwrap();
        assert!((f.diameter.unwrap() - 6.).abs() < 0.002);
        assert!(f.detail.triangles.len() > 20);
        assert!(f.detail.profiles.len() >= 3);
        let bounds = p.part.as_ref().unwrap().bounds();
        let hit = features::pick_feature(
            &p,
            [f.center[0], f.center[1], bounds.max[2] + 50.],
            [0., 0., -1.],
        )
        .unwrap();
        assert_eq!(hit.detail.shape, f.detail.shape);
        let op: Operation = serde_json::from_value(
            commands::geometry_query(
                &p,
                GeometryQuery::FeatureGeometry {
                    feature: f,
                    strategy: Strategy::DrillOnce,
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(op.strategy, Strategy::DrillOnce);
    }
}
#[test]
fn tapers_islands_and_open_pockets_are_recognized_but_not_flattened_by_simple_paths() {
    for (name, shape) in [
        ("recognition-tapers", Shape::TaperedHole),
        ("recognition-tapers", Shape::TaperedPocket),
        ("features-unsupported", Shape::PocketWithIslands),
        ("recognition-open-pocket", Shape::OpenPocket),
    ] {
        let p = load(name);
        let entries = shapes(&p);
        let f = entries
            .iter()
            .filter_map(|e| serde_json::from_value::<features::Feature>(e["feature"].clone()).ok())
            .find(|f| f.detail.shape == shape)
            .unwrap_or_else(|| panic!("{name} missing {shape:?}: {entries:?}"));
        assert!(!f.detail.triangles.is_empty());
        assert!(features::draft(&p, &f, Strategy::Pocket)
            .unwrap_err()
            .contains("3D"));
        if shape == Shape::PocketWithIslands {
            assert!(!f.detail.islands.is_empty());
        }
    }
}
