//! Repeatable generated-motion measurements; no external projects are uploaded.
#[path = "support/toolpath_cases.rs"]
mod cases;
use opensourcecam::cam::{self, Motion};
fn measure(case: cases::Case) -> serde_json::Value {
    let path = cam::generate(&case.project).unwrap_or_else(|e| panic!("{}: {e}", case.name));
    let approach: f64 = path
        .moves
        .iter()
        .filter(|m| m.motion == Motion::Plunge)
        .map(|m| m.from.distance(m.to))
        .sum();
    serde_json::json!({
        "case": case.name, "moves": path.moves.len(), "cut_mm": path.cut_length,
        "rapid_mm": path.rapid_length, "approach_mm": approach,
        "seconds": path.estimated_seconds, "arcs": path.arcs.len()
    })
}
fn main() {
    let cases = if let Some(path) = std::env::args_os().nth(1) {
        vec![cases::Case {
            name: "Input project".into(),
            project: opensourcecam::model::Project::load(std::path::Path::new(&path)).unwrap(),
        }]
    } else {
        cases::cases()
    };
    let results: Vec<_> = cases.into_iter().map(measure).collect();
    println!("{}", serde_json::to_string_pretty(&results).unwrap());
}
