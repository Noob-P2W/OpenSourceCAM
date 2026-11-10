//! Browser boundary: all CAM and export decisions stay in the shared Rust core.
use crate::{
    import,
    model::{Operation, Project},
};
use std::sync::atomic::AtomicBool;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn machine_records(data: &str) -> Result<String, JsValue> {
    crate::machine_library::parse(data)
        .and_then(|r| serde_json::to_string(&r).map_err(|e| e.to_string()))
        .map_err(|e| JsValue::from_str(&e))
}
/// Menu and editor metadata comes from the same definitions as the desktop UI.
#[wasm_bindgen]
pub fn catalog() -> String {
    use crate::{process::Process, tool_library::ToolKind};
    let processes: Vec<_> = Process::ALL.iter().map(|p| serde_json::json!({"id":p,"label":p.label(),"description":p.description(),"ready":p.supports_toolpaths(),"planned":p.planned_operations()})).collect();
    let tools: Vec<_> = ToolKind::ALL.iter().map(|kind| {
        let parameters = |specs: Vec<crate::tool_library::Parameter>| -> Vec<_> { specs.iter().map(|p| serde_json::json!({"key":p.key,"label":p.label,"unit":p.unit,"min":p.min,"max":p.max})).collect() };
        serde_json::json!({"id":kind,"label":kind.label(),"group":kind.group(),"process":kind.process(),"geometry":parameters(kind.geometry()),"cutting":parameters(kind.cutting())})
    }).collect();
    serde_json::json!({"processes":processes,"tools":tools,"machines":crate::machine_library::builtins(),"posts":crate::post::PostProcessor::ALL.iter().map(|p|serde_json::json!({"id":p,"label":p.label()})).collect::<Vec<_>>(),"strategies":crate::model::Strategy::ALL.iter().map(|s| serde_json::json!({"id":s,"label":s.label()})).collect::<Vec<_>>()}).to_string()
}
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
    playback: Option<crate::commands::PlaybackPreview>,
}
#[wasm_bindgen]
impl WebSession {
    #[wasm_bindgen(constructor)]
    pub fn new(data: &str) -> Result<WebSession, JsValue> {
        if data.len() as u64 > crate::model::MAX_PROJECT_BYTES {
            return Err(JsValue::from_str("Draft exceeds project size limit"));
        }
        let mut p: Project =
            serde_json::from_str(data).map_err(|e| JsValue::from_str(&e.to_string()))?;
        p.migrate().map_err(|e| JsValue::from_str(&e))?;
        Ok(Self {
            inner: crate::session::Session::new(p),
            playback: None,
        })
    }
    pub fn history_checkpoint(&self) -> Result<String, JsValue> {
        self.inner
            .history_checkpoint()
            .map_err(|e| JsValue::from_str(&e))
    }
    pub fn restore_history(&mut self, data: &str) -> Result<String, JsValue> {
        self.inner
            .restore_history(data)
            .map_err(|e| JsValue::from_str(&e))?;
        let view = self.inner.view();
        Ok(serde_json::json!({"can_undo":view.can_undo,"can_redo":view.can_redo}).to_string())
    }
    pub fn view(&self) -> Result<String, JsValue> {
        #[derive(serde::Serialize)]
        struct Preview {
            id: u64,
            name: String,
            kind: crate::workspace::ModelKind,
            vertices: Vec<[f64; 3]>,
            triangles: Vec<[u32; 3]>,
        }
        #[derive(serde::Serialize)]
        struct BrowserView<'a> {
            part_fingerprint: Option<String>,
            setup_allowances: [f64; 2],
            #[serde(flatten)]
            view: crate::session::View<'a>,
            workspace_preview: Vec<Preview>,
            part_transform: Option<[[f64; 4]; 4]>,
            stock_outline: Vec<[f64; 3]>,
            stock_definition: crate::blank::StockDefinition,
            setup_issue: Option<String>,
        }
        let visible = self
            .inner
            .project()
            .workspace
            .iter()
            .filter(|m| m.part.visible)
            .count()
            .max(1);
        let budget = (12000 / visible).max(1);
        let preview = self
            .inner
            .project()
            .workspace
            .iter()
            .filter(|m| m.part.visible)
            .map(|m| {
                let stride = m.part.geometry.mesh.triangles.len().div_ceil(budget).max(1);
                let mut p = Preview {
                    id: m.id,
                    name: m.name.clone(),
                    kind: m.kind,
                    vertices: Vec::new(),
                    triangles: Vec::new(),
                };
                for t in m.part.geometry.mesh.triangles.iter().step_by(stride) {
                    let base = p.vertices.len() as u32;
                    for i in t {
                        p.vertices.push(m.point(
                            m.part.geometry.mesh.vertices[*i as usize],
                            self.inner.project().machine.as_ref(),
                        ))
                    }
                    p.triangles.push([base, base + 1, base + 2]);
                }
                p
            })
            .collect();
        serde_json::to_string(&BrowserView {
            setup_allowances: self.inner.project().part.as_ref().map_or([0.; 2], |part| {
                let b = part.bounds();
                [
                    (-b.max[2]).max(0.),
                    (self.inner.project().stock.thickness + b.min[2]).max(0.),
                ]
            }),
            part_fingerprint: self
                .inner
                .project()
                .part
                .as_ref()
                .map(crate::machining::model_fingerprint),
            view: self.inner.view(),
            workspace_preview: preview,
            part_transform: self.inner.project().part.as_ref().map(|p| p.matrix()),
            stock_outline: self.inner.project().stock.outline(),
            stock_definition: self.inner.project().stock.definition(),
            setup_issue: self
                .inner
                .project()
                .part
                .as_ref()
                .and_then(|p| crate::blank::contains_part(&self.inner.project().stock, p).err()),
        })
        .map_err(|e| JsValue::from_str(&e.to_string()))
    }
    pub fn save_project(&self) -> Result<String, JsValue> {
        crate::cam::validate_setup(self.inner.project()).map_err(|e| JsValue::from_str(&e))?;
        self.inner.json().map_err(|e| JsValue::from_str(&e))
    }
    pub fn import_file(
        &mut self,
        name: &str,
        data: &[u8],
        inches: bool,
        role: &str,
    ) -> Result<String, JsValue> {
        let part = import::import_bytes(
            std::path::Path::new(name),
            if inches {
                import::Units::Inches
            } else {
                import::Units::Millimetres
            },
            data,
            &AtomicBool::new(false),
        )
        .map_err(|e| JsValue::from_str(&e))?;
        let edit = match role {
            "part" => crate::session::Edit::ImportPart { part },
            "machine" | "vise" | "fixture" => crate::session::Edit::AddWorkspace {
                part,
                model_kind: serde_json::from_str(&format!("\"{role}\""))
                    .map_err(|e| JsValue::from_str(&e.to_string()))?,
            },
            _ => return Err(JsValue::from_str("Unknown model role")),
        };
        self.inner.apply(edit).map_err(|e| JsValue::from_str(&e))?;
        self.playback = None;
        self.view()
    }
    pub fn edit(&mut self, data: &str) -> Result<String, JsValue> {
        if data.len() as u64 > crate::model::MAX_PROJECT_BYTES {
            return Err(JsValue::from_str("Edit exceeds project size limit"));
        }
        let edit = serde_json::from_str(data).map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.inner.apply(edit).map_err(|e| JsValue::from_str(&e))?;
        self.playback = None;
        self.view()
    }
    pub fn plan(&mut self, action: &str, options: &str) -> Result<String, JsValue> {
        let command = action.parse().map_err(|e: String| JsValue::from_str(&e))?;
        let options =
            serde_json::from_str(options).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let plan = crate::commands::plan_project(self.inner.project(), command, &options)
            .map_err(|e| JsValue::from_str(&e))?;
        self.inner
            .apply(crate::session::Edit::Replace {
                project: Box::new(plan.project),
            })
            .map_err(|e| JsValue::from_str(&e))?;
        self.playback = None;
        Ok(format!(
            "{{\"view\":{},\"notes\":{}}}",
            self.view()?,
            serde_json::to_string(&plan.notes).map_err(|e| JsValue::from_str(&e.to_string()))?
        ))
    }
    pub fn tool_preview(&self, id: u64) -> Result<String, JsValue> {
        let library = &self.inner.project().tool_library;
        let tool = library.get(id).map_err(|e| JsValue::from_str(&e))?;
        let meshes = crate::tool_preview::meshes(tool, library);
        serde_json::to_string(&serde_json::json!({"meshes":meshes,"dimensions":crate::tool_preview::dimensions(tool)})).map_err(|e| JsValue::from_str(&e.to_string()))
    }
    pub fn playback_tool(&self, index: usize) -> Result<String, JsValue> {
        crate::tool_preview::playback(self.inner.project(), index)
            .and_then(|tool| serde_json::to_string(&tool).map_err(|e| e.to_string()))
            .map_err(|e| JsValue::from_str(&e))
    }
    pub fn simulate(&self, progress: f64, resolution: usize) -> Result<String, JsValue> {
        crate::commands::simulate_project(self.inner.project(), progress, resolution)
            .and_then(|value| serde_json::to_string(&value).map_err(|e| e.to_string()))
            .map_err(|e| JsValue::from_str(&e))
    }
    pub fn operation_draft(&self, strategy: &str) -> Result<String, JsValue> {
        let strategy = serde_json::from_str(&format!("\"{strategy}\""))
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        serde_json::to_string(&crate::commands::operation_template(
            self.inner.project(),
            strategy,
        ))
        .map_err(|e| JsValue::from_str(&e.to_string()))
    }
    pub fn geometry(&self, query: &str) -> Result<String, JsValue> {
        serde_json::from_str(query)
            .map_err(|e| e.to_string())
            .and_then(|q| crate::commands::geometry_query(self.inner.project(), q))
            .and_then(|v| serde_json::to_string(&v).map_err(|e| e.to_string()))
            .map_err(|e| JsValue::from_str(&e))
    }
    pub fn machine_record(&self) -> Result<String, JsValue> {
        crate::machine_library::MachineRecord::from_project(self.inner.project())
            .and_then(|r| serde_json::to_string(&r).map_err(|e| e.to_string()))
            .map_err(|e| JsValue::from_str(&e))
    }
    pub fn stock_obj(&self, progress: f64, resolution: usize) -> Result<String, JsValue> {
        crate::commands::stock_obj(self.inner.project(), progress, resolution)
            .map_err(|e| JsValue::from_str(&e))
    }
    pub fn machine_profile(&self) -> Result<String, JsValue> {
        crate::machine::SetupProfile::from_project(self.inner.project())
            .and_then(|profile| serde_json::to_string(&profile).map_err(|e| e.to_string()))
            .map_err(|e| JsValue::from_str(&e))
    }
    pub fn tool_library(&self) -> Result<String, JsValue> {
        self.inner
            .project()
            .tool_library
            .validate()
            .map_err(|e| JsValue::from_str(&e))?;
        Ok(serde_json::json!({"format_version":1,"units":"mm","library":self.inner.project().tool_library}).to_string())
    }
    pub fn stock_preview(&mut self, progress: f64, resolution: usize) -> Result<String, JsValue> {
        if self.playback.is_none() {
            self.playback = Some(
                crate::commands::PlaybackPreview::new(self.inner.project())
                    .map_err(|e| JsValue::from_str(&e))?,
            )
        }
        self.playback
            .as_ref()
            .unwrap()
            .snapshot(self.inner.project(), progress, resolution)
            .and_then(|value| serde_json::to_string(&value).map_err(|e| e.to_string()))
            .map_err(|e| JsValue::from_str(&e))
    }
    pub fn run(&mut self, action: &str) -> Result<String, JsValue> {
        let action: crate::commands::Command =
            action.parse().map_err(|e: String| JsValue::from_str(&e))?;
        let result = if action == crate::commands::Command::Generate {
            let preview = crate::commands::PlaybackPreview::new(self.inner.project())
                .map_err(|e| JsValue::from_str(&e))?;
            let mut value = crate::commands::path_value(&preview.path);
            value["initial_stock"] = preview
                .snapshot(self.inner.project(), 0., 32)
                .map_err(|e| JsValue::from_str(&e))?;
            self.playback = Some(preview);
            Ok(value)
        } else {
            self.inner.run(action)
        };
        result
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
            rotation: [0.; 3],
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
