#![cfg(feature = "web")]
use opensourcecam::{
    model::Project,
    web::WebSession,
    workholding::{place_example, ViseSpec},
};
#[test]
fn browser_reference_preview_keeps_all_faces_and_shared_vertices() {
    let mut p = Project::default();
    place_example(&mut p, None, ViseSpec::default()).unwrap();
    let model = &mut p.workspace[0];
    // Exceed the former display threshold without changing the fixture's extent.
    let faces = model.part.geometry.mesh.triangles.clone();
    let geometry = std::sync::Arc::make_mut(&mut model.part.geometry);
    geometry.mesh.triangles = faces.iter().copied().cycle().take(25020).collect();
    std::sync::Arc::make_mut(&mut model.part.geometry)
        .mesh
        .vertices
        .push([99999.; 3]);
    model.part.offset = [7., -3., 2.];
    let model = &p.workspace[0];
    let expected = &model.part.geometry.mesh;
    let view: serde_json::Value = serde_json::from_str(
        &WebSession::new(&serde_json::to_string(&p).unwrap())
            .unwrap()
            .view()
            .unwrap(),
    )
    .unwrap();
    let preview = &view["workspace_preview"][0];
    assert_eq!(preview["triangles"], serde_json::json!(expected.triangles));
    assert_eq!(
        preview["vertices"].as_array().unwrap().len(),
        expected.vertices.len() - 1
    );
    for (i, v) in expected.vertices[..expected.vertices.len() - 1]
        .iter()
        .enumerate()
    {
        assert_eq!(
            preview["vertices"][i],
            serde_json::json!(model.point(*v, p.machine.as_ref()))
        );
    }
}
