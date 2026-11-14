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
pub fn blank() -> String {
    serde_json::to_string(&Project::new_for_process(crate::process::Process::Milling)).unwrap()
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

#[derive(serde::Serialize)]
struct ReferencePreview {
    id: u64,
    name: String,
    kind: crate::workspace::ModelKind,
    moving_faces: Vec<usize>,
    jaw_axis: [f64; 3],
    vertices: Vec<[f64; 3]>,
    triangles: Vec<[u32; 3]>,
    geometry_key: String,
    groups_key: String,
    transform: [[f64; 4]; 4],
}
type PreviewCache = Vec<(
    crate::workspace::WorkspaceModel,
    [f64; 3],
    std::sync::Arc<ReferencePreview>,
)>;
type GeometryKeys = Vec<(std::sync::Arc<import::Geometry>, String)>;

#[wasm_bindgen]
pub struct WebSession {
    inner: crate::session::Session,
    playback: Option<crate::commands::PlaybackPreview>,
    frame_vertices: Vec<f32>,
    previews: std::cell::RefCell<PreviewCache>,
    geometry_keys: std::cell::RefCell<GeometryKeys>,
    part_key: std::cell::RefCell<Option<(import::Part, String)>>,
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
            frame_vertices: Vec::new(),
            previews: std::cell::RefCell::new(Vec::new()),
            geometry_keys: std::cell::RefCell::new(Vec::new()),
            part_key: std::cell::RefCell::new(None),
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
        struct BrowserView<'a> {
            part_fingerprint: Option<String>,
            part_geometry_key: Option<String>,
            setup_allowances: [f64; 2],
            #[serde(flatten)]
            view: crate::session::View<'a>,
            workspace_preview: Vec<std::sync::Arc<ReferencePreview>>,
            part_transform: Option<[[f64; 4]; 4]>,
            stock_outline: Vec<[f64; 3]>,
            stock_definition: crate::blank::StockDefinition,
            setup_issue: Option<String>,
            clamping_stock: Option<crate::import::Mesh>,
        }
        // Keep only current geometries: cached keys never retain old history meshes.
        self.geometry_keys.borrow_mut().retain(|(g, _)| {
            self.inner
                .project()
                .part
                .iter()
                .chain(self.inner.project().workspace.iter().map(|m| &m.part))
                .any(|p| std::sync::Arc::ptr_eq(g, &p.geometry))
        });
        let old = self.previews.borrow();
        let preview: Vec<std::sync::Arc<ReferencePreview>> = self
            .inner
            .project()
            .workspace
            .iter()
            .filter(|m| m.part.visible)
            .map(|m| {
                let offset = m.offset(self.inner.project().machine.as_ref());
                if let Some((_, _, cached)) = old
                    .iter()
                    .find(|(model, origin, _)| model == m && *origin == offset)
                {
                    return cached.clone();
                }
                let mesh = &m.part.geometry.mesh;
                let mut used = vec![false; mesh.vertices.len()];
                for face in &mesh.triangles {
                    for index in face {
                        used[*index as usize] = true;
                    }
                }
                let mut remap = vec![0; mesh.vertices.len()];
                let mut vertices = Vec::new();
                for (index, vertex) in mesh.vertices.iter().enumerate() {
                    if used[index] {
                        remap[index] = vertices.len() as u32;
                        vertices.push(m.point(*vertex, self.inner.project().machine.as_ref()));
                    }
                }
                let mut moving = vec![false; mesh.vertices.len()];
                let mut jaw_axis = [0.; 3];
                if let Some(profile) = &m.clamp {
                    for index in &profile.moving_vertices {
                        moving[*index] = true;
                    }
                    let points = mesh.triangles[profile.fixed].map(|i| {
                        m.point(
                            mesh.vertices[i as usize],
                            self.inner.project().machine.as_ref(),
                        )
                    });
                    let normal = crate::import::normal(points);
                    let length = normal.iter().map(|v| v * v).sum::<f64>().sqrt();
                    jaw_axis = normal.map(|v| v / length);
                }
                use std::hash::{Hash, Hasher};
                let mut group_hash = std::collections::hash_map::DefaultHasher::new();
                m.clamp
                    .as_ref()
                    .map(|c| &c.moving_vertices)
                    .hash(&mut group_hash);
                let zero = m.point([0.; 3], self.inner.project().machine.as_ref());
                let transform = std::array::from_fn(|i| {
                    if i == 3 {
                        [zero[0], zero[1], zero[2], 1.]
                    } else {
                        let mut p = [0.; 3];
                        p[i] = 1.;
                        let q = m.point(p, self.inner.project().machine.as_ref());
                        [q[0] - zero[0], q[1] - zero[1], q[2] - zero[2], 0.]
                    }
                });
                std::sync::Arc::new(ReferencePreview {
                    geometry_key: self.geometry_key(&m.part),
                    groups_key: format!("{:016x}", group_hash.finish()),
                    transform,
                    moving_faces: mesh
                        .triangles
                        .iter()
                        .enumerate()
                        .filter(|(_, t)| moving[t[0] as usize])
                        .map(|(i, _)| i)
                        .collect(),
                    jaw_axis,
                    id: m.id,
                    name: m.name.clone(),
                    kind: m.kind,
                    vertices,
                    triangles: mesh
                        .triangles
                        .iter()
                        .map(|face| face.map(|i| remap[i as usize]))
                        .collect(),
                })
            })
            .collect();
        drop(old);
        *self.previews.borrow_mut() = self
            .inner
            .project()
            .workspace
            .iter()
            .filter(|m| m.part.visible)
            .zip(preview.iter())
            .map(|(m, p)| {
                (
                    m.clone(),
                    m.offset(self.inner.project().machine.as_ref()),
                    p.clone(),
                )
            })
            .collect();
        serde_json::to_string(&BrowserView {
            clamping_stock: if self
                .inner
                .project()
                .workspace
                .iter()
                .any(|m| m.clamp.is_some())
                && matches!(
                    self.inner.project().stock.profile,
                    crate::blank::StockProfile::Rectangular
                ) {
                let s = &self.inner.project().stock;
                Some(crate::blank::mesh_from_boxes(&[(
                    [0., 0., -s.thickness],
                    [s.width, s.height, 0.],
                )]))
            } else {
                None
            },
            setup_allowances: self.inner.project().part.as_ref().map_or([0.; 2], |part| {
                let b = part.bounds();
                [
                    (-b.max[2]).max(0.),
                    (self.inner.project().stock.thickness + b.min[2]).max(0.),
                ]
            }),
            part_fingerprint: self.cached_part_key(),
            part_geometry_key: self
                .inner
                .project()
                .part
                .as_ref()
                .map(|p| self.geometry_key(p)),
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
    fn geometry_key(&self, part: &import::Part) -> String {
        let mut keys = self.geometry_keys.borrow_mut();
        if let Some((_, key)) = keys
            .iter()
            .find(|(g, _)| std::sync::Arc::ptr_eq(g, &part.geometry))
        {
            return key.clone();
        }
        let neutral = import::Part {
            geometry: part.geometry.clone(),
            rotation: [0.; 3],
            offset: [0.; 3],
            units: import::Units::Millimetres,
            visible: true,
        };
        let key = crate::machining::model_fingerprint(&neutral);
        keys.push((part.geometry.clone(), key.clone()));
        key
    }
    fn cached_part_key(&self) -> Option<String> {
        let mut cache = self.part_key.borrow_mut();
        let Some(part) = self.inner.project().part.as_ref() else {
            *cache = None;
            return None;
        };
        if let Some((old, key)) = cache.as_ref() {
            if old == part {
                return Some(key.clone());
            }
        }
        let key = crate::machining::model_fingerprint(part);
        *cache = Some((part.clone(), key.clone()));
        Some(key)
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
    pub fn simulate(
        &self,
        progress: f64,
        resolution: usize,
        budget: u32,
    ) -> Result<String, JsValue> {
        crate::commands::simulate_project_budget(
            self.inner.project(),
            progress,
            resolution,
            u64::from(budget),
        )
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
    pub fn stock_obj(
        &self,
        progress: f64,
        resolution: usize,
        budget: u32,
    ) -> Result<String, JsValue> {
        crate::commands::stock_obj_budget(
            self.inner.project(),
            progress,
            resolution,
            u64::from(budget),
        )
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
    pub fn vise_query(&self, data: &str) -> Result<String, JsValue> {
        #[derive(serde::Deserialize)]
        #[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
        enum Query {
            Placement {
                id: u64,
                body: Option<usize>,
            },
            Pick {
                id: u64,
                origin: [f64; 3],
                direction: [f64; 3],
            },
            Export {
                id: u64,
            },
            Info {
                id: u64,
            },
        }
        let result = (|| -> Result<String, String> {
            match serde_json::from_str(data).map_err(|e| e.to_string())? {
                Query::Placement { id, body } => serde_json::to_string(
                    &crate::workspace::placement_info(self.inner.project(), id, body)?,
                )
                .map_err(|e| e.to_string()),
                Query::Pick {
                    id,
                    origin,
                    direction,
                } => serde_json::to_string(&crate::vise::pick(
                    self.inner.project(),
                    id,
                    origin,
                    direction,
                )?)
                .map_err(|e| e.to_string()),
                Query::Export { id } => crate::vise::export(self.inner.project(), id),
                Query::Info { id } => {
                    serde_json::to_string(&crate::vise::info(self.inner.project(), id)?)
                        .map_err(|e| e.to_string())
                }
            }
        })();
        result.map_err(|e| JsValue::from_str(&e))
    }
    pub fn operation_defaults(&self, data: &str) -> Result<String, JsValue> {
        serde_json::from_str(data)
            .map_err(|e| e.to_string())
            .and_then(|op| crate::commands::operation_defaults(self.inner.project(), &op))
            .and_then(|v| serde_json::to_string(&v).map_err(|e| e.to_string()))
            .map_err(|e| JsValue::from_str(&e))
    }
    pub fn stock_frame(
        &mut self,
        progress: f64,
        resolution: usize,
        epoch: u32,
        limit: u32,
    ) -> Result<String, JsValue> {
        if self.playback.is_none() {
            self.playback = Some(
                crate::commands::PlaybackPreview::new(self.inner.project())
                    .map_err(|e| JsValue::from_str(&e))?,
            );
        }
        let frame = self
            .playback
            .as_ref()
            .unwrap()
            .frame(
                self.inner.project(),
                progress,
                resolution,
                epoch,
                u64::from(limit),
            )
            .map_err(|e| JsValue::from_str(&e))?;
        self.frame_vertices = frame.vertices;
        serde_json::to_string(&frame.metadata).map_err(|e| JsValue::from_str(&e.to_string()))
    }
    pub fn take_stock_vertices(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.frame_vertices)
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
pub fn cad_project(
    name: &str,
    source: &str,
    format: &str,
    mesh: &str,
    bodies: Option<String>,
) -> Result<String, JsValue> {
    fn build(
        name: &str,
        source: &str,
        format: &str,
        mesh: &str,
        bodies: &str,
    ) -> Result<String, String> {
        if source.len() as u64 > import::MAX_IMPORT_BYTES
            || mesh.len() as u64 + bodies.len() as u64 > crate::model::MAX_PROJECT_BYTES
        {
            return Err("CAD input exceeds import limits".into());
        }
        let format = match format {
            "step" => import::SourceFormat::Step,
            "iges" => import::SourceFormat::Iges,
            _ => return Err("Unsupported CAD format".into()),
        };
        let mesh: import::Mesh = serde_json::from_str(mesh).map_err(|e| e.to_string())?;
        let bodies: Vec<import::Body> = serde_json::from_str(bodies).map_err(|e| e.to_string())?;
        let mut geometry = import::Geometry {
            bodies,
            removed_degenerate_triangles: 0,
            source_name: name.into(),
            format,
            mesh,
            cad_source: Some(source.into()),
        };
        geometry.repair_import()?;
        let part = import::Part {
            rotation: [0.; 3],
            geometry: std::sync::Arc::new(geometry),
            units: import::Units::Millimetres,
            offset: [0.0; 3],
            visible: true,
        };
        let project = Project::from_part(part)?;
        serde_json::to_string(&project).map_err(|e| e.to_string())
    }
    build(
        name,
        source,
        format,
        mesh,
        bodies.as_deref().unwrap_or("[]"),
    )
    .map_err(|e| JsValue::from_str(&e))
}

#[cfg(test)]
mod preview_cache_tests {
    use super::*;
    #[test]
    fn cached_reference_survives_metadata_edits_but_not_placement_changes() {
        let mut project = Project::default();
        crate::workholding::place_example(
            &mut project,
            None,
            crate::workholding::ViseSpec::default(),
        )
        .unwrap();
        let mut session = WebSession::new(&serde_json::to_string(&project).unwrap()).unwrap();
        session.view().unwrap();
        let original = session.previews.borrow()[0].2.clone();
        session.edit(r#"{"kind":"add_operation"}"#).unwrap();
        assert!(std::sync::Arc::ptr_eq(
            &original,
            &session.previews.borrow()[0].2
        ));
        session.edit(r#"{"kind":"position_workspace","id":1,"offset":[10,20,30],"rotation":[0,0,90],"frame":"work"}"#).unwrap();
        let moved = session.previews.borrow()[0].2.clone();
        assert!(!std::sync::Arc::ptr_eq(&original, &moved));
        assert_eq!(
            moved.geometry_key, original.geometry_key,
            "placement must reuse the raw GPU mesh"
        );
        assert_ne!(moved.transform, original.transform);
        let p = session.inner.project();
        let model = &p.workspace[0];
        for (actual, expected) in moved.transform[3]
            .iter()
            .zip(model.point([0.; 3], p.machine.as_ref()))
        {
            assert!((actual - expected).abs() < 1e-8)
        }
    }
}
