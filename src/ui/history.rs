use super::*;
use std::collections::HashSet;
fn text_focus(ctx: &egui::Context) -> Option<egui::Id> {
    ctx.memory(|m| m.focused())
        .filter(|id| egui::TextEdit::load_state(ctx, *id).is_some())
}

struct State {
    project: Project,
    selected: usize,
    metadata: usize,
}
impl State {
    fn new(project: Project, selected: usize) -> Self {
        let metadata = serde_json::to_vec(&(
            &project.name,
            &project.operations,
            &project.tool_library,
            &project.machine,
            &project.tool,
            &project.post,
            &project.stock,
        ))
        .map_or(64 * 1024 * 1024, |v| v.len() * 2)
            + project.workspace.len() * 4096;
        Self {
            project,
            selected,
            metadata,
        }
    }
}
#[derive(Default)]
pub(super) struct History {
    undo: Vec<State>,
    redo: Vec<State>,
    group: Option<egui::Id>,
}
impl History {
    pub(super) fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub(super) fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub(super) fn record(
        &mut self,
        before: Project,
        selected: usize,
        current: &Project,
        group: Option<egui::Id>,
    ) {
        if before != *current {
            if group.is_none() || self.group != group {
                self.undo.push(State::new(before, selected));
            }
            self.redo.clear();
            self.trim(current);
        }
        self.group = group;
    }
    fn trim(&mut self, current: &Project) {
        while self.undo.len() + self.redo.len() > 32
            || self.retained_bytes(current) > 64 * 1024 * 1024
        {
            if !self.undo.is_empty() {
                self.undo.remove(0);
            } else if !self.redo.is_empty() {
                self.redo.remove(0);
            } else {
                break;
            }
        }
    }
    fn retained_bytes(&self, current: &Project) -> usize {
        fn geometry(p: &Project, seen: &mut HashSet<usize>) -> usize {
            p.part
                .iter()
                .chain(p.workspace.iter().map(|m| &m.part))
                .map(|part| {
                    if !seen.insert(Arc::as_ptr(&part.geometry) as usize) {
                        return 0;
                    }
                    let g = &part.geometry;
                    g.mesh.vertices.len() * 24
                        + g.mesh.triangles.len() * 12
                        + g.cad_source.as_ref().map_or(0, String::len)
                        + g.source_name.len()
                })
                .sum()
        }
        let mut seen = HashSet::new();
        geometry(current, &mut seen);
        self.undo
            .iter()
            .chain(&self.redo)
            .map(|s| s.metadata + geometry(&s.project, &mut seen))
            .sum()
    }
}
impl Workbench {
    pub(super) fn undo_edit(&mut self, redo: bool) {
        let state = if redo {
            self.history.redo.pop()
        } else {
            self.history.undo.pop()
        };
        if let Some(state) = state {
            let current = State::new(
                std::mem::replace(&mut self.project, state.project),
                self.selected,
            );
            if redo {
                self.history.undo.push(current);
            } else {
                self.history.redo.push(current);
            }
            self.history.group = None;
            self.reveal_selection = true;
            self.machining_ui = machining::MachiningState::for_project(&self.project);
            self.selected = state
                .selected
                .min(self.project.operations.len().saturating_sub(1));
            self.history_restored = true;
            self.history.trim(&self.project);
            self.notice = if redo {
                "Change redone"
            } else {
                "Change undone"
            }
            .into();
            self.regenerate();
        }
    }
    pub(super) fn finish_edit(
        &mut self,
        before: Project,
        selected: usize,
        epoch: u64,
        ctx: &egui::Context,
    ) {
        if before != self.project {
            if !self.history_restored && epoch == self.history_epoch {
                let group = ctx.dragged_id().or_else(|| text_focus(ctx));
                self.history.record(before, selected, &self.project, group);
            }
            self.regenerate();
        }
        self.history_restored = false;
        if text_focus(ctx).is_none() && ctx.dragged_id().is_none() {
            self.history.group = None;
        }
    }
    pub(super) fn operation_shortcuts(&mut self, ctx: &egui::Context) {
        if self.main_menu
            || self.library_ui.open
            || self.editors.is_open()
            || self.pending.is_some()
            || ctx.dragged_id().is_some()
            || ctx.memory(|m| m.any_popup_open())
            || self.import_choice.is_some()
            || text_focus(ctx).is_some()
        {
            return;
        }
        if ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::Z)
        }) || ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Y))
        {
            self.undo_edit(true);
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Z)) {
            self.undo_edit(false);
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::D)) {
            self.duplicate_selected();
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Delete)) {
            self.delete_selected();
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowUp)) {
            self.move_selected(true);
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowDown)) {
            self.move_selected(false);
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F2)) {
            self.editors.open_operation(self.selected, &self.project);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_coalesces_a_field_gesture_but_separates_discrete_actions() {
        let mut project = Project::default();
        let mut history = History::default();
        let group = Some(egui::Id::new("field"));
        let original = project.clone();
        project.operations[0].x += 1.0;
        history.record(original.clone(), 0, &project, group);
        let previous = project.clone();
        project.operations[0].x += 1.0;
        history.record(previous, 0, &project, group);
        assert_eq!(history.undo.len(), 1);
        assert_eq!(history.undo[0].project, original);
        let previous = project.clone();
        project.operations.pop();
        history.record(previous, 0, &project, None);
        assert_eq!(history.undo.len(), 2);
    }
    #[test]
    fn history_is_bounded_and_new_edits_discard_redo() {
        let mut app = Workbench::new();
        for _ in 0..40 {
            let before = app.project.clone();
            app.project.operations[0].x += 0.01;
            app.history.record(before, 0, &app.project, None);
        }
        assert_eq!(app.history.undo.len(), 32);
        app.undo_edit(false);
        assert!(app.history.can_redo());
        let before = app.project.clone();
        app.project.operations[0].depth = 1.5;
        app.history.record(before, 0, &app.project, None);
        assert!(!app.history.can_redo());
        app.install_project(Project::default(), None);
        assert!(!app.history.can_undo());
    }
}
