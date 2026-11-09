//! UI-independent editing transactions. Drafts can be incomplete; export remains checked.
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
    FitStock {
        margin: f64,
    },
    ApplyMachine {
        profile: Box<crate::machine::SetupProfile>,
    },
    ImportTools {
        data: String,
    },
    InitializeTools,
    AddWorkspace {
        part: crate::import::Part,
        model_kind: crate::workspace::ModelKind,
    },
    RemoveWorkspace {
        id: u64,
    },
    FeatureOperation {
        index: usize,
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
pub struct Session {
    project: Project,
    undo: VecDeque<String>,
    redo: VecDeque<String>,
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
    fn trim(&mut self) {
        while self.undo.len() + self.redo.len() > 30
            || self
                .undo
                .iter()
                .chain(&self.redo)
                .map(String::len)
                .sum::<usize>()
                > 8 * 1024 * 1024
        {
            if self.undo.pop_front().is_none() {
                self.redo.pop_front();
            }
        }
    }
    pub fn apply(&mut self, edit: Edit) -> Result<(), String> {
        let before = self.json()?;
        if matches!(edit, Edit::Undo | Edit::Redo) {
            let undo = matches!(edit, Edit::Undo);
            let source = if undo { &mut self.undo } else { &mut self.redo };
            let Some(data) = source.back() else {
                return Ok(());
            };
            let restored =
                serde_json::from_str(data).map_err(|e| format!("Cannot restore history: {e}"))?;
            source.pop_back();
            if undo {
                self.redo.push_back(before)
            } else {
                self.undo.push_back(before)
            }
            self.project = restored;
            self.trim();
            return Ok(());
        }
        let mut candidate = self.project.clone();
        match edit {
            Edit::Replace { project } => candidate = *project,
            Edit::AddOperation => candidate.operations.push(Operation {
                tool_id: candidate.active_tool,
                preset_id: candidate.active_preset,
                ..Operation::default()
            }),
            Edit::FitStock { margin } => {
                if !margin.is_finite() || !(0.0..=1000.0).contains(&margin) {
                    return Err("Stock margin must be 0–1000 mm".into());
                }
                let part = candidate
                    .part
                    .as_mut()
                    .ok_or("Import a part before fitting stock")?;
                part.validate()?;
                let size = part.bounds().size();
                if size[0] + 2.0 * margin < 0.001 || size[1] + 2.0 * margin < 0.001 {
                    return Err("Stock needs positive X/Y dimensions; use a larger margin".into());
                }
                if size[0] + 2.0 * margin > 100_000.0
                    || size[1] + 2.0 * margin > 100_000.0
                    || size[2] > 10_000.0
                {
                    return Err("Model exceeds supported stock limits".into());
                }
                part.align_to_top(margin);
                candidate.stock = crate::model::Stock {
                    width: size[0] + 2.0 * margin,
                    height: size[1] + 2.0 * margin,
                    thickness: size[2].max(0.001),
                };
            }
            Edit::ApplyMachine { profile } => {
                // Legacy profiles include a tool. During Import → Setup a separate
                // library may already be loaded without a selected tool; CNC selection
                // must not depend on choosing that tool first or replace the library.
                if profile.format_version == 1
                    && !candidate.tool_library.tools.is_empty()
                    && candidate.active_tool.is_none()
                {
                    let mut setup = Project::new_for_process(candidate.process);
                    profile.apply(&mut setup)?;
                    candidate.machine = setup.machine;
                    candidate.tool = setup.tool;
                } else {
                    profile.apply(&mut candidate)?;
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
            Edit::RemoveWorkspace { id } => {
                let index = candidate
                    .workspace
                    .iter()
                    .position(|m| m.id == id)
                    .ok_or("Workspace model not found")?;
                candidate.workspace.remove(index);
            }
            Edit::FeatureOperation { index } => {
                let part = candidate
                    .part
                    .as_ref()
                    .ok_or("Import a part before recognizing features")?;
                let report = crate::features::recognize(part)?;
                let feature = report
                    .features
                    .get(index)
                    .ok_or("Recognized feature is no longer available")?;
                let operation = crate::features::operation(&candidate, feature)?;
                candidate.operations.push(operation);
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
        let after = serde_json::to_string(&candidate).map_err(|e| e.to_string())?;
        if after.len() as u64 > MAX_PROJECT_BYTES {
            return Err("Draft exceeds project size limit".into());
        }
        if after != before {
            self.undo.push_back(before);
            self.redo.clear();
            self.project = candidate;
            self.trim()
        }
        Ok(())
    }
    pub fn run(&self, command: commands::Command) -> Result<serde_json::Value, String> {
        commands::execute(&self.json()?, command)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
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
