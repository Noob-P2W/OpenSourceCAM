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

#[wasm_bindgen]
pub struct WebSession {
    inner: crate::session::Session,
}
#[wasm_bindgen]
impl WebSession {
    #[wasm_bindgen(constructor)]
    pub fn new(data: &str) -> Result<WebSession, JsValue> {
        if data.len() as u64 > crate::model::MAX_PROJECT_BYTES {
            return Err(JsValue::from_str("Draft exceeds project size limit"));
        }
        let p: Project =
            serde_json::from_str(data).map_err(|e| JsValue::from_str(&e.to_string()))?;
        if p.format_version != crate::model::FORMAT_VERSION {
            return Err(JsValue::from_str("Unsupported draft version"));
        }
        Ok(Self {
            inner: crate::session::Session::new(p),
        })
    }
    pub fn view(&self) -> Result<String, JsValue> {
        let preview: Vec<_>=self.inner.project().workspace.iter().filter(|m|m.part.visible).map(|m|serde_json::json!({
            "kind":m.kind,"vertices":m.part.geometry.mesh.vertices.iter().map(|p|m.point(*p,self.inner.project().machine.as_ref())).collect::<Vec<_>>(),"triangles":m.part.geometry.mesh.triangles
        })).collect();
        let mut value = serde_json::to_value(self.inner.view())
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        value["workspace_preview"] = serde_json::json!(preview);
        serde_json::to_string(&value).map_err(|e| JsValue::from_str(&e.to_string()))
    }
    pub fn edit(&mut self, data: &str) -> Result<String, JsValue> {
        if data.len() as u64 > crate::model::MAX_PROJECT_BYTES {
            return Err(JsValue::from_str("Edit exceeds project size limit"));
        }
        let edit = serde_json::from_str(data).map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.inner.apply(edit).map_err(|e| JsValue::from_str(&e))?;
        self.view()
    }
    pub fn run(&self, action: &str) -> Result<String, JsValue> {
        action
            .parse()
            .and_then(|c| self.inner.run(c))
            .and_then(|v| serde_json::to_string(&v).map_err(|e| e.to_string()))
            .map_err(|e| JsValue::from_str(&e))
    }
}

/// Validate a browser-kernel result through the same Part and Project boundaries as native import.
#[wasm_bindgen]
pub fn cad_project(name: &str, source: &str, format: &str, mesh: &str) -> Result<String, JsValue> {
    fn build(name: &str, source: &str, format: &str, mesh: &str) -> Result<String, String> {
        if source.len() as u64 > import::MAX_IMPORT_BYTES
            || mesh.len() as u64 > crate::model::MAX_PROJECT_BYTES
        {
            return Err("CAD input exceeds import limits".into());
        }
        let format = match format {
            "step" => import::SourceFormat::Step,
            "iges" => import::SourceFormat::Iges,
            _ => return Err("Unsupported CAD format".into()),
        };
        let part = import::Part {
            geometry: std::sync::Arc::new(import::Geometry {
                source_name: name.into(),
                format,
                mesh: serde_json::from_str(mesh).map_err(|e| e.to_string())?,
                cad_source: Some(source.into()),
            }),
            units: import::Units::Millimetres,
            offset: [0.0; 3],
            visible: true,
        };
        let project = Project::from_part(part)?;
        serde_json::to_string(&project).map_err(|e| e.to_string())
    }
    build(name, source, format, mesh).map_err(|e| JsValue::from_str(&e))
}
