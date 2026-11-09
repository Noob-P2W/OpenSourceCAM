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
    Replace { project: Box<Project> },
    AddOperation,
    DeleteOperation { index: usize },
    DuplicateOperation { index: usize },
    MoveOperation { index: usize, target: usize },
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
            Edit::AddOperation => candidate.operations.push(Operation::default()),
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
