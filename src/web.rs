//! Browser boundary: all CAM and export decisions stay in the shared Rust core.
use crate::{
    import,
    model::{Operation, Project},
};
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
#[wasm_bindgen]
pub fn run(data: &str, action: &str) -> Result<String, JsValue> {
    action
        .parse()
        .and_then(|command| crate::commands::execute(data, command))
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
