//! UI-independent editing transactions. Drafts can be incomplete; export remains checked.
mod checkpoint;
use crate::{
    commands,
    model::{Operation, Project, FORMAT_VERSION, MAX_PROJECT_BYTES},
};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Edit {
    Replace {
        project: Box<Project>,
    },
    AddOperation,
    ConfigureVise {
        id: u64,
        fixed: usize,
        moving: usize,
        max_closure: f64,
    },
    ClampVise {
        id: u64,
        grip: f64,
    },
    ClampStockInVise {
        id: u64,
        grip: f64,
    },
    ClampStockInViseSetup {
        id: u64,
        fixed: usize,
        moving: usize,
        max_closure: f64,
        bodies: Vec<usize>,
        grip: f64,
    },
    ClampViseSetup {
        id: u64,
        fixed: usize,
        moving: usize,
        max_closure: f64,
        bodies: Vec<usize>,
        grip: f64,
    },
    ConfigureViseGroup {
        id: u64,
        fixed: usize,
        moving: usize,
        max_closure: f64,
        bodies: Vec<usize>,
    },
    OpenVise {
        id: u64,
    },
    AddViseProfile {
        data: String,
    },
    PlaceExampleVise {
        id: Option<u64>,
        spec: crate::workholding::ViseSpec,
    },
    UseMachine {
        record: Box<crate::machine_library::MachineRecord>,
    },
    SaveOperation {
        index: Option<usize>,
        expected: Option<Box<Operation>>,
        operation: Box<Operation>,
    },
    ConfigureStock {
        definition: crate::blank::StockDefinition,
    },
    ConfigureSetup {
        definition: crate::blank::StockDefinition,
        rotation: [f64; 3],
        offset: Option<[f64; 3]>,
        clearance: f64,
        fit_margin: Option<f64>,
        #[serde(default)]
        fit_top: Option<f64>,
        #[serde(default)]
        fit_backing: Option<f64>,
    },
    OrientPart {
        rotation: [f64; 3],
    },
    CenterPart,

    AddStrategy {
        strategy: crate::model::Strategy,
    },
    NewProject {
        process: crate::process::Process,
    },
    ImportPart {
        part: crate::import::Part,
    },
    AddLibraryTool {
        tool_kind: crate::tool_library::ToolKind,
    },
    DuplicateTool {
        id: u64,
    },
    RemoveTool {
        id: u64,
    },
    AddPreset {
        tool_id: u64,
    },
    FitStock {
        margin: f64,
    },
    ApplyMachine {
        profile: Box<crate::machine::SetupProfile>,
    },
    ImportTools {
        data: String,
    },
    ImportLibrary {
        data: String,
        replace: bool,
    },
    RemovePreset {
        tool_id: u64,
        preset_id: u64,
    },
    InitializeTools,
    AddWorkspace {
        part: crate::import::Part,
        model_kind: crate::workspace::ModelKind,
    },
    RemoveWorkspace {
        id: u64,
    },
    PositionWorkspace {
        id: u64,
        offset: [f64; 3],
        rotation: [f64; 3],
        frame: crate::workspace::Frame,
    },
    PositionComponent {
        id: u64,
        body: usize,
        center: [f64; 3],
        rotate: [f64; 3],
    },
    CenterWorkspace {
        id: u64,
    },
    FeatureOperation {
        index: usize,
    },
    SetMachiningFace {
        triangle: usize,
        align: bool,
    },
    DeleteOperation {
        index: usize,
    },
    DuplicateOperation {
        index: usize,
    },
    MoveOperation {
        index: usize,
        target: usize,
    },
    Undo,
    Redo,
}
#[derive(Serialize)]
pub struct View<'a> {
    pub project: &'a Project,
    pub can_undo: bool,
    pub can_redo: bool,
}
struct Snapshot {
    project: Project,
    metadata: usize,
}
impl Snapshot {
    fn new(project: Project) -> Self {
        let metadata = serde_json::to_vec(&(
            &project.name,
            &project.operations,
            &project.tool_library,
            &project.machine,
            &project.tool,
            &project.post,
            &project.stock,
        ))
        .map_or(8 * 1024 * 1024, |data| data.len() * 2)
            + project.workspace.len() * 4096;
        Self { project, metadata }
    }
}
struct SizeLimit(usize);
impl std::io::Write for SizeLimit {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        if data.len() > MAX_PROJECT_BYTES as usize - self.0 {
            return Err(std::io::Error::other("Draft exceeds project size limit"));
        }
        self.0 += data.len();
        Ok(data.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub struct Session {
    project: Project,
    undo: VecDeque<Snapshot>,
    redo: VecDeque<Snapshot>,
}
impl Session {
    pub fn new(project: Project) -> Self {
        Self {
            project,
            undo: VecDeque::new(),
            redo: VecDeque::new(),
        }
    }
    pub fn view(&self) -> View<'_> {
        View {
            project: &self.project,
            can_undo: !self.undo.is_empty(),
            can_redo: !self.redo.is_empty(),
        }
    }
    pub fn project(&self) -> &Project {
        &self.project
    }
    pub fn json(&self) -> Result<String, String> {
        serde_json::to_string(&self.project).map_err(|e| e.to_string())
    }
    fn retained_bytes(&self) -> usize {
        fn geometry(p: &Project, seen: &mut std::collections::HashSet<usize>) -> usize {
            p.part
                .iter()
                .chain(p.workspace.iter().map(|m| &m.part))
                .map(|part| {
                    if !seen.insert(std::sync::Arc::as_ptr(&part.geometry) as usize) {
                        return 0;
                    }
                    let g = &part.geometry;
                    g.mesh.vertices.len() * 24
                        + g.mesh.triangles.len() * 12
                        + g.cad_source.as_ref().map_or(0, String::len)
                        + g.source_name.len()
                        + g.bodies
                            .iter()
                            .map(|b| b.name.len() + std::mem::size_of::<crate::import::Body>())
                            .sum::<usize>()
                })
                .sum()
        }
        let mut seen = std::collections::HashSet::new();
        geometry(&self.project, &mut seen);
        self.undo
            .iter()
            .chain(&self.redo)
            .map(|s| s.metadata + geometry(&s.project, &mut seen))
            .sum()
    }
    fn trim(&mut self) {
        while self.undo.len() + self.redo.len() > 30 || self.retained_bytes() > 8 * 1024 * 1024 {
            if self.undo.pop_front().is_none() {
                self.redo.pop_front();
            }
        }
    }
    pub fn apply(&mut self, edit: Edit) -> Result<(), String> {
        let before = self.project.clone();
        if matches!(edit, Edit::Undo | Edit::Redo) {
            let undo = matches!(edit, Edit::Undo);
            let source = if undo { &mut self.undo } else { &mut self.redo };
            let Some(data) = source.back() else {
                return Ok(());
            };
            let restored = data.project.clone();
            source.pop_back();
            if undo {
                self.redo.push_back(Snapshot::new(before))
            } else {
                self.undo.push_back(Snapshot::new(before))
            }
            self.project = restored;
            self.trim();
            return Ok(());
        }
        let mut candidate = self.project.clone();
        match edit {
            Edit::Replace { project } => candidate = *project,
            Edit::NewProject { process } => candidate = Project::new_for_process(process),
            Edit::ImportPart { part } => {
                let fitted = Project::from_part(part)?;
                candidate.name = fitted.name;
                candidate.stock = fitted.stock;
                candidate.part = fitted.part;
                // Face selections and operations (including model links) belong to
                // the old workpiece. References and machine/tool setup do not.
                candidate.setup_face = None;
                candidate.operations.clear();
            }
            Edit::AddLibraryTool { tool_kind } => {
                if candidate.tool_library.tools.is_empty() && !candidate.operations.is_empty() {
                    candidate.ensure_tool_library();
                }
                let library = &mut candidate.tool_library;
                let tool = crate::tool_library::LibraryTool::new(
                    library.next_id(),
                    library.next_number(tool_kind.process()),
                    tool_kind,
                );
                let id = tool.id;
                library.tools.push(tool);
                library.validate()?;
                if candidate.operations.is_empty()
                    && tool_kind.process() == candidate.process
                    && !matches!(
                        tool_kind,
                        crate::tool_library::ToolKind::Holder
                            | crate::tool_library::ToolKind::Probe
                    )
                {
                    candidate.active_tool = Some(id);
                    candidate.active_preset = Some(1);
                }
            }
            Edit::DuplicateTool { id } => {
                let library = &mut candidate.tool_library;
                let mut tool = library.get(id)?.clone();
                tool.id = library.next_id();
                tool.post.number = library.next_number_for(&tool.uses);
                tool.name = format!("{} copy", tool.name);
                tool.post.pocket = None;
                tool.post.length_offset = None;
                tool.post.diameter_offset = None;
                tool.post.turning_offset = None;
                let id = tool.id;
                let preset = tool.default_preset;
                let compatible = tool.uses.contains(&candidate.process)
                    && !matches!(
                        tool.kind,
                        crate::tool_library::ToolKind::Holder
                            | crate::tool_library::ToolKind::Probe
                    );
                library.tools.push(tool);
                library.validate()?;
                if compatible {
                    candidate.active_tool = Some(id);
                    candidate.active_preset = Some(preset);
                }
            }
            Edit::RemoveTool { id } => {
                candidate.tool_library.get(id)?;
                if candidate.operations.iter().any(|op| {
                    op.tool_id == Some(id)
                        || (op.tool_id.is_none() && candidate.active_tool == Some(id))
                }) || candidate
                    .tool_library
                    .tools
                    .iter()
                    .any(|tool| tool.holder == Some(id))
                {
                    return Err("This tool is assigned to an operation or used as a holder; remove those assignments first".into());
                }
                candidate.tool_library.tools.retain(|tool| tool.id != id);
                if candidate.active_tool == Some(id) {
                    candidate.active_tool = None;
                    candidate.active_preset = None;
                }
            }
            Edit::AddPreset { tool_id } => {
                let tool = candidate
                    .tool_library
                    .tools
                    .iter_mut()
                    .find(|tool| tool.id == tool_id)
                    .ok_or("Tool not found")?;
                let mut preset = tool.preset(None)?.clone();
                preset.id = tool.presets.iter().map(|p| p.id).max().unwrap_or(0) + 1;
                preset.name = format!("{} copy", preset.name);
                let id = preset.id;
                let compatible = tool.uses.contains(&candidate.process)
                    && !matches!(
                        tool.kind,
                        crate::tool_library::ToolKind::Holder
                            | crate::tool_library::ToolKind::Probe
                    );
                tool.presets.push(preset);
                candidate.tool_library.validate()?;
                if compatible {
                    candidate.active_tool = Some(tool_id);
                    candidate.active_preset = Some(id);
                }
            }
            Edit::AddStrategy { strategy } => candidate.operations.push(Operation {
                strategy,
                name: strategy.label().into(),
                tool_id: candidate.active_tool,
                preset_id: candidate.active_preset,
                ..Operation::default()
            }),
            Edit::AddOperation => candidate.operations.push(Operation {
                tool_id: candidate.active_tool,
                preset_id: candidate.active_preset,
                ..Operation::default()
            }),
            Edit::ConfigureVise {
                id,
                fixed,
                moving,
                max_closure,
            } => crate::vise::configure(&mut candidate, id, fixed, moving, max_closure)?,
            Edit::ConfigureViseGroup {
                id,
                fixed,
                moving,
                max_closure,
                bodies,
            } => crate::vise::configure_group(
                &mut candidate,
                id,
                fixed,
                moving,
                max_closure,
                &bodies,
            )?,
            Edit::ClampVise { id, grip } => crate::vise::clamp(&mut candidate, id, grip)?,
            Edit::ClampStockInVise { id, grip } => {
                crate::vise::clamp_stock_in_vise(&mut candidate, id, grip)?;
            }
            Edit::ClampStockInViseSetup {
                id,
                fixed,
                moving,
                max_closure,
                bodies,
                grip,
            } => {
                crate::vise::configure_group(
                    &mut candidate,
                    id,
                    fixed,
                    moving,
                    max_closure,
                    &bodies,
                )?;
                crate::vise::clamp_stock_in_vise(&mut candidate, id, grip)?;
            }
            Edit::ClampViseSetup {
                id,
                fixed,
                moving,
                max_closure,
                bodies,
                grip,
            } => {
                crate::vise::configure_group(
                    &mut candidate,
                    id,
                    fixed,
                    moving,
                    max_closure,
                    &bodies,
                )?;
                crate::vise::clamp(&mut candidate, id, grip)?;
            }
            Edit::OpenVise { id } => crate::vise::open(&mut candidate, id)?,
            Edit::AddViseProfile { data } => crate::vise::add(&mut candidate, &data)?,
            Edit::PlaceExampleVise { id, spec } => {
                crate::workholding::place_example(&mut candidate, id, spec)?
            }
            Edit::UseMachine { record } => record.apply(&mut candidate)?,
            Edit::SaveOperation {
                index,
                expected,
                operation,
            } => {
                operation.settings.validate()?;
                if let Some(index) = index {
                    let old = candidate
                        .operations
                        .get(index)
                        .ok_or("Operation no longer exists")?;
                    if expected.as_deref() != Some(old) {
                        return Err(
                            "Operation changed while its settings were open; reopen the settings"
                                .into(),
                        );
                    }
                    candidate.operations[index] = *operation;
                } else {
                    if candidate.operations.len() >= 1000 {
                        return Err("Maximum 1,000 operations".into());
                    }
                    candidate.operations.push(*operation);
                }
                crate::cam::validate_setup(&candidate)?;
            }
            Edit::ConfigureStock { definition } => candidate.stock = definition.build()?,
            Edit::ConfigureSetup {
                definition,
                rotation,
                offset,
                clearance,
                fit_margin,
                fit_top,
                fit_backing,
            } => {
                if !clearance.is_finite() || !(0.0..=10000.0).contains(&clearance) {
                    return Err("Clearance must be 0–10,000 mm".into());
                }
                candidate.stock = definition.build()?;
                candidate.clearance = clearance;
                if let Some(part) = &mut candidate.part {
                    part.orient_centered(rotation)?;
                    if let Some(offset) = offset {
                        part.offset = offset;
                    }
                    part.validate()?;
                }
                if let Some(margin) = fit_margin {
                    if !margin.is_finite() || !(0.0..=1000.0).contains(&margin) {
                        return Err("Stock margin must be 0–1000 mm".into());
                    }
                    crate::blank::fit(
                        &mut candidate,
                        margin,
                        fit_top.unwrap_or(0.),
                        fit_backing.unwrap_or(0.),
                    )?;
                }
            }
            Edit::OrientPart { rotation } => candidate
                .part
                .as_mut()
                .ok_or("Import a part first")?
                .orient_centered(rotation)?,
            Edit::CenterPart => {
                let stock = &candidate.stock;
                let part = candidate.part.as_mut().ok_or("Import a part first")?;
                let bounds = part.bounds();
                part.offset[0] += (stock.width - bounds.min[0] - bounds.max[0]) / 2.;
                part.offset[1] += (stock.height - bounds.min[1] - bounds.max[1]) / 2.;
                part.offset[2] -= bounds.max[2];
            }
            Edit::FitStock { margin } => {
                if !margin.is_finite() || !(0.0..=1000.0).contains(&margin) {
                    return Err("Stock margin must be 0–1000 mm".into());
                }
                crate::blank::fit(&mut candidate, margin, 0., 0.)?;
            }
            Edit::ApplyMachine { profile } => {
                crate::machine_library::apply_profile(&profile, &mut candidate)?;
            }
            Edit::ImportLibrary { data, replace } => {
                let library = crate::tool_library::ToolLibrary::from_json(data.as_bytes())?;
                crate::tool_library::import_into(&mut candidate, library, replace)?;
            }
            Edit::RemovePreset { tool_id, preset_id } => {
                let tool = candidate.tool_library.get(tool_id)?;
                tool.preset(Some(preset_id))?;
                if tool.presets.len() == 1 {
                    return Err("Keep at least one cutting preset".into());
                }
                if candidate.operations.iter().any(|op| {
                    op.tool_id.or(candidate.active_tool) == Some(tool_id)
                        && op.preset_id.unwrap_or(tool.default_preset) == preset_id
                }) {
                    return Err("This preset is assigned to an operation; choose another operation preset first".into());
                }
                let tool = candidate
                    .tool_library
                    .tools
                    .iter_mut()
                    .find(|t| t.id == tool_id)
                    .ok_or("Tool not found")?;
                tool.presets.retain(|preset| preset.id != preset_id);
                if tool.default_preset == preset_id {
                    tool.default_preset = tool.presets[0].id;
                }
                if candidate.active_tool == Some(tool_id)
                    && candidate.active_preset == Some(preset_id)
                {
                    candidate.active_preset = Some(tool.default_preset);
                }
            }
            Edit::ImportTools { data } => {
                let library = crate::tool_library::ToolLibrary::from_json(data.as_bytes())?;
                if !candidate.tool_library.tools.is_empty() {
                    return Err("This project already has a tool library; replacing assigned tools is not supported here".into());
                }
                candidate.tool_library = library;
                candidate.active_tool = None;
                candidate.active_preset = None;
            }
            Edit::InitializeTools => candidate.ensure_tool_library(),
            Edit::AddWorkspace {
                mut part,
                model_kind,
            } => {
                part.validate()?;
                part.offset = [0.0; 3];
                let id = candidate
                    .workspace
                    .iter()
                    .map(|m| m.id)
                    .max()
                    .unwrap_or(0)
                    .checked_add(1)
                    .ok_or("Workspace id exhausted")?;
                candidate
                    .workspace
                    .push(crate::workspace::WorkspaceModel::new(id, model_kind, part));
                crate::workspace::validate(&candidate.workspace, candidate.machine.as_ref())?;
            }
            Edit::PositionWorkspace {
                id,
                offset,
                rotation,
                frame,
            } => crate::workspace::position(&mut candidate, id, offset, rotation, frame)?,
            Edit::PositionComponent {
                id,
                body,
                center,
                rotate,
            } => crate::workspace::position_component(&mut candidate, id, body, center, rotate)?,
            Edit::CenterWorkspace { id } => crate::workspace::center(&mut candidate, id)?,
            Edit::RemoveWorkspace { id } => {
                let index = candidate
                    .workspace
                    .iter()
                    .position(|m| m.id == id)
                    .ok_or("Workspace model not found")?;
                candidate.workspace.remove(index);
            }
            Edit::FeatureOperation { index } => {
                let report = crate::features::recognize_project(&candidate)?;
                let feature = report
                    .features
                    .get(index)
                    .ok_or("Recognized feature is no longer available")?;
                let operation = crate::features::operation(&candidate, feature)?;
                candidate.operations.push(operation);
            }
            Edit::SetMachiningFace { triangle, align } => {
                crate::faces::setup(&mut candidate, triangle, align)?
            }
            Edit::DeleteOperation { index } => {
                if index >= candidate.operations.len() {
                    return Err("Select an existing operation".into());
                }
                candidate.operations.remove(index);
            }
            Edit::DuplicateOperation { index } => {
                let mut op = candidate
                    .operations
                    .get(index)
                    .ok_or("Select an existing operation")?
                    .clone();
                op.name = format!("{} copy", op.name);
                candidate.operations.insert(index + 1, op);
            }
            Edit::MoveOperation { index, target } => {
                if index >= candidate.operations.len() || target >= candidate.operations.len() {
                    return Err("Select an existing operation position".into());
                }
                let op = candidate.operations.remove(index);
                candidate.operations.insert(target, op);
            }
            Edit::Undo | Edit::Redo => return Err("Unexpected history action".into()),
        }
        if candidate.format_version != FORMAT_VERSION {
            return Err("Unsupported draft version".into());
        }
        // Browser metadata edits deserialize geometry anew. Reuse immutable geometry
        // when unchanged, so history retains Arc references rather than whole meshes.
        if let (Some(new), Some(old)) = (&mut candidate.part, &before.part) {
            if new.geometry == old.geometry {
                new.geometry = old.geometry.clone()
            }
        }
        for new in &mut candidate.workspace {
            if let Some(old) = before.workspace.iter().find(|old| old.id == new.id) {
                if new.part.geometry == old.part.geometry {
                    new.part.geometry = old.part.geometry.clone()
                }
            }
        }
        if candidate != before {
            serde_json::to_writer(&mut SizeLimit(0), &candidate).map_err(|e| e.to_string())?;
            self.undo.push_back(Snapshot::new(before));
            self.redo.clear();
            self.project = candidate;
            self.trim()
        }
        Ok(())
    }
    pub fn run(&self, command: commands::Command) -> Result<serde_json::Value, String> {
        commands::execute_project(&self.project, command)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_history_reuses_geometry_after_browser_roundtrips() {
        let part = crate::import::import_file(
            std::path::Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/examples/models/mount.stl"
            )),
            crate::import::Units::Millimetres,
        )
        .unwrap();
        let mut s = Session::new(Project {
            part: Some(part),
            ..Project::default()
        });
        let geometry = s.project().part.as_ref().unwrap().geometry.clone();
        for i in 0..10 {
            let mut p: Project = serde_json::from_str(&s.json().unwrap()).unwrap();
            p.name = i.to_string();
            s.apply(Edit::Replace {
                project: Box::new(p),
            })
            .unwrap();
            assert!(std::sync::Arc::ptr_eq(
                &geometry,
                &s.project().part.as_ref().unwrap().geometry
            ));
        }
        for state in &s.undo {
            assert!(std::sync::Arc::ptr_eq(
                &geometry,
                &state.project.part.as_ref().unwrap().geometry
            ))
        }
        s.apply(Edit::Undo).unwrap();
        s.apply(Edit::Redo).unwrap();
        assert!(std::sync::Arc::ptr_eq(
            &geometry,
            &s.project().part.as_ref().unwrap().geometry
        ));
        assert!(s.retained_bytes() < 100_000);
    }
    #[test]
    fn new_operations_keep_the_selected_setup_tool_and_preset() {
        let mut p = Project::default();
        p.ensure_tool_library();
        let mut preset = p.tool_library.tools[0].presets[0].clone();
        preset.id = 2;
        preset.name = "Second preset".into();
        p.tool_library.tools[0].presets.push(preset);
        p.active_preset = Some(2);
        let active = p.active_tool;
        let mut s = Session::new(p);
        s.apply(Edit::AddOperation).unwrap();
        let op = s.project().operations.last().unwrap();
        assert_eq!(op.tool_id, active);
        assert_eq!(op.preset_id, Some(2));
    }
    #[test]
    fn setup_edits_preserve_operations_and_reject_invalid_inputs() {
        let part = crate::import::import_file(
            std::path::Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/examples/models/mount.stl"
            )),
            crate::import::Units::Millimetres,
        )
        .unwrap();
        let p = Project {
            part: Some(part.clone()),
            ..Project::default()
        };
        let mut s = Session::new(p);
        s.apply(Edit::FitStock { margin: 3.0 }).unwrap();
        assert_eq!(s.project().stock.width, 86.0);
        assert_eq!(s.project().operations.len(), 2);
        let before = s.json().unwrap();
        assert!(s.apply(Edit::FitStock { margin: -1.0 }).is_err());
        assert_eq!(s.json().unwrap(), before);
        s.apply(Edit::AddWorkspace {
            part,
            model_kind: crate::workspace::ModelKind::Vise,
        })
        .unwrap();
        assert_eq!(s.project().workspace.len(), 1);
        assert_eq!(s.project().workspace[0].part.offset, [0.0; 3]);
        let id = s.project().workspace[0].id;
        s.apply(Edit::RemoveWorkspace { id }).unwrap();
        assert!(s.project().workspace.is_empty());
        s.apply(Edit::Undo).unwrap();
        assert_eq!(s.project().workspace.len(), 1);
        let profile =
            serde_json::from_str(include_str!("../examples/bench-mill.cam-machine.json")).unwrap();
        s.apply(Edit::ApplyMachine { profile }).unwrap();
        assert_eq!(
            s.project().machine.as_ref().unwrap().name,
            "Example 3-axis bench mill"
        );
        s.apply(Edit::InitializeTools).unwrap();
        assert!(!s.project().tool_library.tools.is_empty());
    }
    #[test]
    fn library_import_validates_units_without_partial_changes() {
        let data = include_str!("../examples/tools.cam-tools.json");
        let mut s = Session::new(Project::default());
        let before = s.json().unwrap();
        assert!(s
            .apply(Edit::ImportTools {
                data: data.replace("\"units\": \"mm\"", "\"units\": \"inch\"")
            })
            .is_err());
        assert_eq!(s.json().unwrap(), before);
        s.apply(Edit::ImportTools { data: data.into() }).unwrap();
        assert!(!s.project().tool_library.tools.is_empty());
        let count = s.project().tool_library.tools.len();
        let profile =
            serde_json::from_str(include_str!("../examples/bench-mill.cam-machine.json")).unwrap();
        s.apply(Edit::ApplyMachine { profile }).unwrap();
        assert_eq!(s.project().tool_library.tools.len(), count);
        assert_eq!(s.project().active_tool, None);
    }
    #[test]
    fn transaction_history_and_export_preflight() {
        let mut s = Session::new(Project::default());
        let original = s.json().unwrap();
        s.apply(Edit::DuplicateOperation { index: 0 }).unwrap();
        assert_eq!(s.project().operations.len(), 3);
        s.apply(Edit::MoveOperation {
            index: 0,
            target: 2,
        })
        .unwrap();
        s.apply(Edit::Undo).unwrap();
        s.apply(Edit::Undo).unwrap();
        assert_eq!(s.json().unwrap(), original);
        s.apply(Edit::Redo).unwrap();
        assert_eq!(s.project().operations.len(), 3);
        let before = s.json().unwrap();
        assert!(s.apply(Edit::DeleteOperation { index: 999 }).is_err());
        assert_eq!(s.json().unwrap(), before);
        let mut p = s.project().clone();
        p.operations[0].depth = -1.;
        s.apply(Edit::Replace {
            project: Box::new(p),
        })
        .unwrap();
        assert!(!s.view().can_redo);
        assert!(s.run(commands::Command::Export).is_err());
        s.apply(Edit::Undo).unwrap();
        assert!(s.run(commands::Command::Export).is_ok());
    }
    #[test]
    fn history_is_bounded_and_new_edits_invalidate_redo() {
        let mut s = Session::new(Project::default());
        for i in 0..40 {
            let mut p = s.project().clone();
            p.name = i.to_string();
            s.apply(Edit::Replace {
                project: Box::new(p),
            })
            .unwrap()
        }
        assert_eq!(s.undo.len(), 30);
        s.apply(Edit::Undo).unwrap();
        s.apply(Edit::AddOperation).unwrap();
        assert!(!s.view().can_redo);
    }
}
