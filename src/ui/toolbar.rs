use super::icons::{self, Icon};
use super::*;

impl Workbench {
    pub(super) fn operations_toolbar(&mut self, ctx: &egui::Context) {
        let process = self.project.process;
        egui::TopBottomPanel::top("operations_toolbar").show(ctx,|ui|{
            ui.horizontal(|ui|{
                ui.add_space(142.0);
                ui.selectable_value(&mut self.ribbon_tab,RibbonTab::Process,process.label());
                ui.selectable_value(&mut self.ribbon_tab,RibbonTab::Inspect,"Inspection");
                ui.selectable_value(&mut self.ribbon_tab,RibbonTab::Utilities,"Utilities");
            });
            ui.separator();
            egui::ScrollArea::horizontal().id_salt("command_ribbon").auto_shrink([false,true]).show(ui,|ui|{
                ui.horizontal(|ui|{
                    ui.spacing_mut().item_spacing.x=8.0;
                    ui.allocate_ui_with_layout(Vec2::new(134.0,86.0),egui::Layout::top_down(egui::Align::Center),|ui|{
                        let menu=icons::button(ui,Icon::process(process),"Workspace","Return to the main menu; current work is preserved",self.import_receiver.is_none());
                        #[cfg(test)]
                        self.ui_hits.push(("menu".into(),menu.rect));
                        if menu.clicked() {self.main_menu=true;self.playing=false;}
                        ui.label(RichText::new(process.label().to_uppercase()).size(10.0).color(ACCENT).strong());
                    });
                    ui.separator();
                    match self.ribbon_tab {
                        RibbonTab::Process=>self.process_ribbon(ui),
                        RibbonTab::Inspect=>self.inspection_ribbon(ui),
                        RibbonTab::Utilities=>self.utilities_ribbon(ui),
                    }
                });
            });
            if !process.supports_toolpaths() {
                ui.label(RichText::new(if process==Process::Mixed {"Mixed custom machining will be developed last. Reference models can be imported and saved."}else{"Setup and tooling can be saved. Process-specific machining strategies are planned."}).size(11.0).color(MUTED));
            }
        });
    }

    fn process_ribbon(&mut self, ui: &mut egui::Ui) {
        let milling = self.project.process.supports_toolpaths();
        group(ui, "SETUP", 3, |ui| {
            let setup = icons::button(
                ui,
                Icon::Setup,
                "Setup",
                "Edit stock, work origin and cutting data",
                true,
            );
            #[cfg(test)]
            self.ui_hits.push(("machine".into(), setup.rect));
            if setup.clicked() {
                self.inspector_tab = InspectorTab::Setup;
                self.setup_section = SetupSection::Machine;
            }
            let configure = icons::button(
                ui,
                Icon::Workspace,
                "Machine",
                "Open detailed machine configuration",
                self.project.machine.is_some(),
            );
            #[cfg(test)]
            self.ui_hits
                .push(("machine:configure".into(), configure.rect));
            if configure.clicked() {
                self.editors.open_machine(&self.project);
            }
            let tool = icons::button(ui, Icon::Tool, "Tool", "Edit the active cutting tool", true);
            #[cfg(test)]
            self.ui_hits.push(("tool".into(), tool.rect));
            if tool.clicked() {
                self.open_library(None);
            }
        });
        if milling {
            group(ui, "STOCK", 1, |ui| {
                let r = icons::button(
                    ui,
                    Icon::Rough3d,
                    "From stock",
                    "Recognize part and generate machining toolpaths",
                    self.project.part.is_some(),
                );
                #[cfg(test)]
                self.ui_hits.push(("machining:open".into(), r.rect));
                if r.clicked() {
                    self.inspector_tab = InspectorTab::Machining;
                }
            });
            group(ui, "2D MILLING", 3, |ui| {
                for (strategy, icon, label, id) in [
                    (Strategy::Pocket, Icon::Pocket, "Pocket", "Pocket"),
                    (
                        Strategy::InsideProfile,
                        Icon::Inside,
                        "Inside",
                        "Inside profile",
                    ),
                    (
                        Strategy::OutsideProfile,
                        Icon::Outside,
                        "Outside",
                        "Outside profile",
                    ),
                ] {
                    let response = icons::button(
                        ui,
                        icon,
                        label,
                        strategy.label(),
                        self.project.operations.len() < 1000,
                    );
                    #[cfg(test)]
                    self.ui_hits.push((format!("add:{id}"), response.rect));
                    let _ = id;
                    if response.clicked() {
                        self.add_operation(strategy);
                    }
                }
            });
            group(ui, "3D MILLING", 2, |ui| {
                if icons::button(
                    ui,
                    Icon::Rough3d,
                    "Rough",
                    "Constant-Z clearing above the imported model",
                    self.project.part.is_some(),
                )
                .clicked()
                {
                    self.add_operation(Strategy::SurfaceRough);
                }
                if icons::button(
                    ui,
                    Icon::Finish3d,
                    "Finish",
                    "Parallel XYZ surface finishing after 3D roughing",
                    self.project.part.is_some(),
                )
                .clicked()
                {
                    self.add_operation(Strategy::ParallelFinish);
                }
            });
            group(ui, "DRILLING", 1, |ui| {
                let drill = icons::button(
                    ui,
                    Icon::Drill,
                    "Drill",
                    "Create a peck-drilling operation",
                    self.project.operations.len() < 1000,
                );
                #[cfg(test)]
                self.ui_hits.push(("add:Drill".into(), drill.rect));
                if drill.clicked() {
                    self.add_operation(Strategy::Drill);
                }
            });
        } else {
            let labels = self.project.process.planned_operations();
            let icon = Icon::process(self.project.process);
            group(ui, "PLANNED STRATEGIES", labels.len(), |ui| {
                for label in labels {
                    let planned = icons::button(ui, icon, label, "Strategy planned", false);
                    #[cfg(test)]
                    self.ui_hits
                        .push((format!("planned:{label}"), planned.rect));
                    #[cfg(not(test))]
                    let _ = planned;
                }
            });
        }
        group(ui, "ACTIONS", 3, |ui| {
            if icons::button(
                ui,
                Icon::Generate,
                "Generate",
                "Regenerate toolpaths from current parameters",
                milling,
            )
            .clicked()
            {
                self.regenerate();
            }
            if icons::button(
                ui,
                Icon::Play,
                "Playback",
                "Play or pause the tool motion preview (not stock-removal simulation)",
                self.path.is_some(),
            )
            .clicked()
            {
                self.code_tab = false;
                self.analysis_open = true;
                if self.progress >= 1.0 {
                    self.progress = 0.0;
                }
                self.playing = !self.playing;
            }
            let post = icons::button(
                ui,
                Icon::Post,
                "Post",
                "Choose a controller and postprocess the program",
                true,
            );
            #[cfg(test)]
            self.ui_hits.push(("post".into(), post.rect));
            if post.clicked() {
                self.inspector_tab = InspectorTab::Post;
            }
        });
        self.manage_group(ui);
        self.inspection_ribbon(ui);
    }

    fn manage_group(&mut self, ui: &mut egui::Ui) {
        group(ui, "MANAGE", 3, |ui| {
            let library = icons::button(
                ui,
                Icon::Tool,
                "Library",
                "Open the local tool library",
                true,
            );
            #[cfg(test)]
            self.ui_hits.push(("library".into(), library.rect));
            if library.clicked() {
                self.open_library(None);
            }
            let selected = self.selected < self.project.operations.len();
            let copy = icons::button(
                ui,
                Icon::Duplicate,
                "Duplicate",
                "Duplicate the selected operation",
                selected && self.project.operations.len() < 1000,
            );
            #[cfg(test)]
            self.ui_hits.push(("duplicate".into(), copy.rect));
            if copy.clicked() {
                self.duplicate_selected();
            }
            let delete = icons::button(
                ui,
                Icon::Delete,
                "Delete",
                "Delete the selected operation",
                selected,
            );
            #[cfg(test)]
            self.ui_hits.push(("delete".into(), delete.rect));
            if delete.clicked() {
                self.delete_selected();
            }
        });
    }

    fn inspection_ribbon(&mut self, ui: &mut egui::Ui) {
        group(ui, "INSPECT", 5, |ui| {
            if icons::button(
                ui,
                Icon::Workspace,
                "Models",
                "Import and position machine, vise and fixture reference models",
                true,
            )
            .clicked()
            {
                self.inspector_tab = InspectorTab::Workspace;
            }
            let features = icons::button(
                ui,
                Icon::Recognize,
                "Features",
                "Review candidate holes and pockets in the imported model",
                self.project.part.is_some(),
            );
            #[cfg(test)]
            self.ui_hits.push(("features".into(), features.rect));
            if features.clicked() {
                self.open_features();
            }
            if icons::button(
                ui,
                Icon::Fit,
                "Fit",
                "Fit the stock and model in the viewport",
                true,
            )
            .clicked()
            {
                self.zoom = 1.0;
                self.pan = Vec2::ZERO;
            }
            if icons::button(ui, Icon::Top, "Top", "Look down the Z axis", true).clicked() {
                self.iso = false;
                self.camera = Camera::top();
            }
            if icons::button(
                ui,
                Icon::Orbit,
                "Isometric",
                "Reset to an isometric view",
                true,
            )
            .clicked()
            {
                self.iso = true;
                self.camera = Camera::default();
            }
        });
    }

    fn utilities_ribbon(&mut self, ui: &mut egui::Ui) {
        group(ui, "PROJECT", 2, |ui| {
            if icons::button(
                ui,
                Icon::Import,
                "Import",
                "Import STL, OBJ, STEP or IGES",
                self.import_receiver.is_none(),
            )
            .clicked()
            {
                self.choose_import();
            }
            if icons::button(ui, Icon::Save, "Save", "Save the current project", true).clicked() {
                self.save(false);
            }
        });
        group(ui, "OPERATION ORDER", 2, |ui| {
            let count = self.project.operations.len();
            if icons::button(
                ui,
                Icon::Up,
                "Move up",
                "Move the selected operation earlier",
                self.selected < count && self.selected > 0,
            )
            .clicked()
            {
                self.move_selected(true);
            }
            if icons::button(
                ui,
                Icon::Down,
                "Move down",
                "Move the selected operation later",
                self.selected + 1 < count,
            )
            .clicked()
            {
                self.move_selected(false);
            }
        });
        self.manage_group(ui);
        group(ui, "OUTPUT", 1, |ui| {
            if icons::button(
                ui,
                Icon::Post,
                "Post",
                "Configure the controller postprocessor",
                true,
            )
            .clicked()
            {
                self.inspector_tab = InspectorTab::Post;
            }
        });
    }
}

fn group(ui: &mut egui::Ui, label: &str, count: usize, body: impl FnOnce(&mut egui::Ui)) {
    let width = count as f32 * 58.0 + count.saturating_sub(1) as f32 * 2.0;
    // Reserve the full group before creating its children. Dynamic child sizing
    // inside the horizontal scroll area can otherwise clip buttons during startup.
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 90.0), egui::Sense::hover());
    let mut contents = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(("ribbon_group", label))
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Center)),
    );
    contents.set_clip_rect(rect.intersect(ui.clip_rect()));
    contents.spacing_mut().item_spacing.y = 4.0;
    contents.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        body(ui);
    });
    contents.label(RichText::new(label).size(10.0).color(MUTED));
    ui.separator();
}
