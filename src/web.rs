//! Browser boundary: all CAM and export decisions stay in the shared Rust core.
use crate::{
    cam, import,
    model::{Operation, Project},
    post, stock,
};
use serde_json::{json, Value};
use std::sync::atomic::AtomicBool;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn sample() -> String {
    serde_json::to_string(&Project::default()).unwrap()
}
#[wasm_bindgen]
pub fn operation() -> String {
    serde_json::to_string(&Operation::default()).unwrap()
}
#[wasm_bindgen]
pub fn normalize(data: &str) -> Result<String, JsValue> {
    Project::from_json(data.as_bytes())
        .and_then(|p| serde_json::to_string(&p).map_err(|e| e.to_string()))
        .map_err(|e| JsValue::from_str(&e))
}
fn execute(data: &str, action: &str) -> Result<Value, String> {
    let p = Project::from_json(data.as_bytes())?;
    match action {
        "generate" => {
            let path = post::checked_path(&p)?;
            Ok(
                json!({"moves":path.moves.iter().map(|m| json!({"from":[m.from.x,m.from.y,m.from.z],"to":[m.to.x,m.to.y,m.to.z],"rapid":m.motion==cam::Motion::Rapid})).collect::<Vec<_>>(),"seconds":path.estimated_seconds,"cut":path.cut_length}),
            )
        }
        "export" => Ok(json!(post::gcode(&p)?)),
        "simulate" => {
            let path = post::checked_path(&p)?;
            let sim = stock::simulate(&p, &path, 1.0, 96, &AtomicBool::new(false))?;
            Ok(json!({"summary":sim.summary,"heights":sim.heights}))
        }
        "plan" | "surface" => {
            let plan = if action == "surface" {
                crate::surface::from_stock(&p, &Default::default(), &AtomicBool::new(false))?
            } else {
                crate::machining::from_stock(&p, &Default::default(), &AtomicBool::new(false))?
            };
            Ok(json!(plan.project))
        }
        _ => Err("Unknown browser action".into()),
    }
}
#[wasm_bindgen]
pub fn run(data: &str, action: &str) -> Result<String, JsValue> {
    execute(data, action)
        .and_then(|v| serde_json::to_string(&v).map_err(|e| e.to_string()))
        .map_err(|e| JsValue::from_str(&e))
}
#[wasm_bindgen]
pub fn import_model(name: &str, data: &[u8], inches: bool) -> Result<String, JsValue> {
    let units = if inches {
        import::Units::Inches
    } else {
        import::Units::Millimetres
    };
    import::import_bytes(
        std::path::Path::new(name),
        units,
        data,
        &AtomicBool::new(false),
    )
    .and_then(Project::from_part)
    .and_then(|p| serde_json::to_string(&p).map_err(|e| e.to_string()))
    .map_err(|e| JsValue::from_str(&e))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_export_matches_native_and_rejects_invalid_paths() {
        let p = Project::default();
        assert_eq!(
            execute(&sample(), "export").unwrap(),
            json!(post::gcode(&p).unwrap())
        );
        assert!(!execute(&sample(), "generate").unwrap()["moves"]
            .as_array()
            .unwrap()
            .is_empty());
        let mut bad = p;
        bad.operations[0].depth = -1.0;
        assert!(execute(&serde_json::to_string(&bad).unwrap(), "export").is_err());
    }
}
