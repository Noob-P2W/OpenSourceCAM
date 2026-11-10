//! Temporary worker-recovery history. Current geometry is referenced rather than
//! serialized again; unique older meshes appear once and retain shared ownership.
use super::*;
use crate::{
    import::{Geometry, Part, Units},
    machine::MachineSetup,
    model::{Stock, Tool},
    post::PostSettings,
    process::Process,
    tool_library::ToolLibrary,
    workspace::{Frame, ModelKind, WorkspaceModel},
};
use std::{collections::HashMap, sync::Arc};

#[derive(Serialize, Deserialize)]
#[serde(tag = "source", content = "value", rename_all = "snake_case")]
enum GeometryEntry {
    CurrentPart(String),
    CurrentWorkspace { index: usize, fingerprint: String },
    Retained(Arc<Geometry>),
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PartRef {
    rotation: [f64; 3],
    geometry: usize,
    units: Units,
    offset: [f64; 3],
    visible: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceRef {
    vise: Option<crate::workholding::ViseSpec>,
    id: u64,
    name: String,
    kind: ModelKind,
    frame: Frame,
    part: PartRef,
    rotation: [f64; 3],
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectRef {
    #[serde(default)]
    setup_face: Option<usize>,
    workspace: Vec<WorkspaceRef>,
    tool_library: ToolLibrary,
    active_tool: Option<u64>,
    active_preset: Option<u64>,
    format_version: u32,
    name: String,
    stock: Stock,
    process: Process,
    post: PostSettings,
    machine: Option<MachineSetup>,
    tool: Tool,
    clearance: f64,
    operations: Vec<Operation>,
    part: Option<PartRef>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Checkpoint {
    version: u32,
    geometry: Vec<GeometryEntry>,
    undo: Vec<ProjectRef>,
    redo: Vec<ProjectRef>,
}
struct Pool {
    indices: HashMap<usize, usize>,
    entries: Vec<GeometryEntry>,
}
impl Pool {
    fn insert(&mut self, geometry: &Arc<Geometry>, entry: GeometryEntry) -> usize {
        let key = Arc::as_ptr(geometry) as usize;
        if let Some(id) = self.indices.get(&key) {
            return *id;
        }
        let id = self.entries.len();
        self.entries.push(entry);
        self.indices.insert(key, id);
        id
    }
    fn part(&mut self, part: &Part) -> PartRef {
        let geometry = self.insert(
            &part.geometry,
            GeometryEntry::Retained(part.geometry.clone()),
        );
        PartRef {
            rotation: part.rotation,
            geometry,
            units: part.units,
            offset: part.offset,
            visible: part.visible,
        }
    }
    fn project(&mut self, p: &Project) -> ProjectRef {
        ProjectRef {
            setup_face: p.setup_face,
            workspace: p
                .workspace
                .iter()
                .map(|m| WorkspaceRef {
                    vise: m.vise.clone(),
                    id: m.id,
                    name: m.name.clone(),
                    kind: m.kind,
                    frame: m.frame,
                    part: self.part(&m.part),
                    rotation: m.rotation,
                })
                .collect(),
            tool_library: p.tool_library.clone(),
            active_tool: p.active_tool,
            active_preset: p.active_preset,
            format_version: p.format_version,
            name: p.name.clone(),
            stock: p.stock.clone(),
            process: p.process,
            post: p.post.clone(),
            machine: p.machine.clone(),
            tool: p.tool.clone(),
            clearance: p.clearance,
            operations: p.operations.clone(),
            part: p.part.as_ref().map(|part| self.part(part)),
        }
    }
}
impl PartRef {
    fn restore(self, pool: &[Arc<Geometry>]) -> Result<Part, String> {
        let geometry = pool
            .get(self.geometry)
            .ok_or("Recovery history references missing geometry")?
            .clone();
        let part = Part {
            rotation: self.rotation,
            geometry,
            units: self.units,
            offset: self.offset,
            visible: self.visible,
        };
        part.validate()?;
        Ok(part)
    }
}
impl ProjectRef {
    fn restore(self, pool: &[Arc<Geometry>]) -> Result<Project, String> {
        if self.format_version != FORMAT_VERSION {
            return Err("Unsupported recovery project version".into());
        }
        Ok(Project {
            setup_face: self.setup_face,
            workspace: self
                .workspace
                .into_iter()
                .map(|m| {
                    Ok(WorkspaceModel {
                        vise: m.vise,
                        id: m.id,
                        name: m.name,
                        kind: m.kind,
                        frame: m.frame,
                        part: m.part.restore(pool)?,
                        rotation: m.rotation,
                    })
                })
                .collect::<Result<_, String>>()?,
            tool_library: self.tool_library,
            active_tool: self.active_tool,
            active_preset: self.active_preset,
            format_version: self.format_version,
            name: self.name,
            stock: self.stock,
            process: self.process,
            post: self.post,
            machine: self.machine,
            tool: self.tool,
            clearance: self.clearance,
            operations: self.operations,
            part: self.part.map(|p| p.restore(pool)).transpose()?,
        })
    }
}
impl Session {
    pub fn history_checkpoint(&self) -> Result<String, String> {
        let mut pool = Pool {
            indices: HashMap::new(),
            entries: Vec::new(),
        };
        if let Some(part) = &self.project.part {
            pool.insert(
                &part.geometry,
                GeometryEntry::CurrentPart(crate::machining::model_fingerprint(part)),
            );
        }
        for (index, m) in self.project.workspace.iter().enumerate() {
            pool.insert(
                &m.part.geometry,
                GeometryEntry::CurrentWorkspace {
                    index,
                    fingerprint: crate::machining::model_fingerprint(&m.part),
                },
            );
        }
        let undo = self.undo.iter().map(|s| pool.project(&s.project)).collect();
        let redo = self.redo.iter().map(|s| pool.project(&s.project)).collect();
        let checkpoint = Checkpoint {
            version: 1,
            geometry: pool.entries,
            undo,
            redo,
        };
        serde_json::to_writer(&mut SizeLimit(0), &checkpoint).map_err(|e| e.to_string())?;
        serde_json::to_string(&checkpoint).map_err(|e| e.to_string())
    }
    pub fn restore_history(&mut self, data: &str) -> Result<(), String> {
        if data.len() as u64 > MAX_PROJECT_BYTES {
            return Err("Recovery history exceeds its size limit".into());
        }
        let checkpoint: Checkpoint = serde_json::from_str(data).map_err(|e| e.to_string())?;
        if checkpoint.version != 1 || checkpoint.undo.len() + checkpoint.redo.len() > 30 {
            return Err("Invalid recovery history version or length".into());
        }
        let mut pool = Vec::new();
        for entry in checkpoint.geometry {
            let geometry = match entry {
                GeometryEntry::CurrentPart(fingerprint) => {
                    let part = self
                        .project
                        .part
                        .as_ref()
                        .ok_or("Current part missing during recovery")?;
                    if crate::machining::model_fingerprint(part) != fingerprint {
                        return Err("Current part differs from the recovery checkpoint".into());
                    }
                    part.geometry.clone()
                }
                GeometryEntry::CurrentWorkspace { index, fingerprint } => {
                    let model = self
                        .project
                        .workspace
                        .get(index)
                        .ok_or("Current reference model missing during recovery")?;
                    if crate::machining::model_fingerprint(&model.part) != fingerprint {
                        return Err("Current reference differs from the recovery checkpoint".into());
                    }
                    model.part.geometry.clone()
                }
                GeometryEntry::Retained(g) => {
                    g.mesh.validate()?;
                    g
                }
            };
            pool.push(geometry);
        }
        let undo = checkpoint
            .undo
            .into_iter()
            .map(|p| p.restore(&pool).map(Snapshot::new))
            .collect::<Result<_, _>>()?;
        let redo = checkpoint
            .redo
            .into_iter()
            .map(|p| p.restore(&pool).map(Snapshot::new))
            .collect::<Result<_, _>>()?;
        self.undo = undo;
        self.redo = redo;
        self.trim();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn session() -> Session {
        let part = crate::import::import_file(
            std::path::Path::new("examples/models/mount.stl"),
            Units::Millimetres,
        )
        .unwrap();
        Session::new(Project::from_part(part).unwrap())
    }
    #[test]
    fn checkpoint_omits_current_mesh_and_restores_both_history_stacks() {
        let mut original = session();
        for i in 0..20 {
            let mut p = original.project().clone();
            p.name = format!("revision {i}");
            original
                .apply(Edit::Replace {
                    project: Box::new(p),
                })
                .unwrap();
        }
        original.apply(Edit::Undo).unwrap();
        let data = original.history_checkpoint().unwrap();
        assert!(!data.contains("\"vertices\""));
        assert!(data.len() < 100_000);
        let current: Project = serde_json::from_str(&original.json().unwrap()).unwrap();
        let mut recovered = Session::new(current);
        recovered.restore_history(&data).unwrap();
        assert!(recovered.view().can_undo && recovered.view().can_redo);
        assert!(Arc::ptr_eq(
            &recovered.project.part.as_ref().unwrap().geometry,
            &recovered
                .undo
                .back()
                .unwrap()
                .project
                .part
                .as_ref()
                .unwrap()
                .geometry
        ));
        for edit in [Edit::Redo, Edit::Undo, Edit::Undo] {
            let other = if matches!(edit, Edit::Redo) {
                Edit::Redo
            } else {
                Edit::Undo
            };
            original.apply(edit).unwrap();
            recovered.apply(other).unwrap();
            assert_eq!(original.json().unwrap(), recovered.json().unwrap());
        }
    }
    #[test]
    fn checkpoint_retains_old_geometry_once_and_rejects_bad_references_atomically() {
        let mut s = session();
        let old = s.json().unwrap();
        let part = crate::import::import_file(
            std::path::Path::new("examples/models/mount.obj"),
            Units::Millimetres,
        )
        .unwrap();
        s.apply(Edit::ImportPart { part }).unwrap();
        let point = s.history_checkpoint().unwrap();
        let mut restored = Session::new(s.project().clone());
        restored.restore_history(&point).unwrap();
        restored.apply(Edit::Undo).unwrap();
        assert_eq!(restored.json().unwrap(), old);
        let mut bad: serde_json::Value = serde_json::from_str(&point).unwrap();
        bad["undo"][0]["part"]["geometry"] = serde_json::json!(9999);
        let before = restored.json().unwrap();
        let can_redo = restored.view().can_redo;
        assert!(restored.restore_history(&bad.to_string()).is_err());
        assert_eq!(restored.json().unwrap(), before);
        assert_eq!(restored.view().can_redo, can_redo);
    }
}
