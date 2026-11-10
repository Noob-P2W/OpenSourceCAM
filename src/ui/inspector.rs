use super::*;

impl Workbench {
    pub(super) fn inspector(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("inspector")
            .default_width(322.0)
            .width_range(300.0..=460.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.add_space(12.0);
                section(ui, "PROPERTIES");
                for row in [
                    vec![
                        (InspectorTab::Operation, "Operation"),
                        (InspectorTab::Setup, "Setup"),
                        (InspectorTab::Machining, "From stock"),
                    ],
                    vec![
                        (InspectorTab::Model, "Model"),
                        (InspectorTab::Features, "Features"),
                        (InspectorTab::Workspace, "Workspace"),
                        (InspectorTab::Post, "Post"),
                    ],
                ] {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 3.0;
                        ui.spacing_mut().button_padding = Vec2::new(7.0, 6.0);
                        for (tab, label) in row {
                            ui.selectable_value(&mut self.inspector_tab, tab, label);
                        }
                    });
                }
                ui.separator();
                if self.inspector_tab == InspectorTab::Setup {
                    self.setup_navigation(ui);
                }
                let output = egui::ScrollArea::vertical()
                    .id_salt((
                        "inspector_contents",
                        self.inspector_tab,
                        if self.inspector_tab == InspectorTab::Setup {
                            Some(self.setup_section)
                        } else {
                            None
                        },
                    ))
                    .show(ui, |ui| match self.inspector_tab {
                        InspectorTab::Machining => self.machining_parameters(ui),
                        InspectorTab::Workspace => self.workspace_parameters(ui),
                        InspectorTab::Features => self.feature_parameters(ui),
                        InspectorTab::Operation => self.operation_parameters(ui),
                        InspectorTab::Setup => self.setup_parameters(ui),
                        InspectorTab::Post => self.post_parameters(ui),
                        InspectorTab::Model => {
                            if self.project.part.is_some() {
                                self.model_inspector(ui);
                            } else {
                                ui.label(
                                    RichText::new(
                                        "Import a model to inspect its placement and dimensions.",
                                    )
                                    .color(MUTED),
                                );
                            }
                        }
                    });
                #[cfg(test)]
                {
                    self.inspector_scroll = Some((output.id, output.state.offset));
                }
                #[cfg(not(test))]
                let _ = output;
            });
    }

    fn post_parameters(&mut self, ui: &mut egui::Ui) {
        use opensourcecam::post::{Coolant, PostProcessor, WorkOffset};
        ui.heading("Postprocess");
        section(ui, "CONTROLLER");
        let process = self.project.process;
        egui::ComboBox::from_id_salt("postprocessor")
            .width(236.0)
            .selected_text(self.project.post.processor.label())
            .show_ui(ui, |ui| {
                for post in PostProcessor::ALL
                    .into_iter()
                    .filter(|p| p.compatible(process))
                {
                    ui.selectable_value(&mut self.project.post.processor, post, post.label());
                }
            });
        if !self.project.post.processor.ready(process) {
            ui.add_space(8.0);
            ui.colored_label(MUTED,if self.project.post.processor==PostProcessor::Klipper {
                "Klipper printing output is planned. Slicing, extrusion and printer setup must be implemented first."
            } else { "Output for this process is planned. Reference projects can still be saved." });
            return;
        }
        ui.add_space(12.0);
        section(ui, "WORK COORDINATES");
        egui::ComboBox::from_id_salt("work_offset")
            .selected_text(self.project.post.work_offset.code())
            .show_ui(ui, |ui| {
                for offset in WorkOffset::ALL {
                    ui.selectable_value(&mut self.project.post.work_offset, offset, offset.code());
                }
            });
        ui.label(
            RichText::new(
                "Set this work offset to the mounted tool tip at stock lower-left / top.",
            )
            .size(11.0)
            .color(MUTED),
        );
        ui.add_space(8.0);
        section(ui, "COOLANT");
        if !self.project.tool_library.tools.is_empty() {
            ui.checkbox(
                &mut self.project.post.use_tool_coolant,
                "Use each tool's cutting-preset coolant",
            );
        }
        ui.add_enabled_ui(!self.project.post.use_tool_coolant, |ui| {
            egui::ComboBox::from_id_salt("coolant")
                .selected_text(if self.project.post.coolant == Coolant::Flood {
                    "Flood (M8)"
                } else {
                    "Off"
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.project.post.coolant, Coolant::Off, "Off");
                    ui.selectable_value(
                        &mut self.project.post.coolant,
                        Coolant::Flood,
                        "Flood (M8)",
                    );
                });
        });
        ui.add_space(12.0);
        ui.label(RichText::new(match self.project.post.processor {
            PostProcessor::Mach3=>"Mach3 Mill: exact-stop mode (G61), percent boundaries and M30 program end.",
            PostProcessor::LinuxCnc=>"LinuxCNC: exact-path mode (G61), percent boundaries and M30 program end.",
            PostProcessor::Grbl=>"GRBL 1.1: linear moves and supported modal resets; no tool-change or canned-cycle commands.",
            _=>"Generic metric: absolute XYZ linear motion and M2 program end.",
        }).size(12.0).color(MUTED));
        ui.label(
            RichText::new(
                if self.project.machine.as_ref().is_some_and(|m|m.tool_changer.mode==opensourcecam::machine::ChangeMode::MillingAtc) {"Controller-managed ATC: T/M6 and G43/H. Verify tool table, offsets and changer configuration."} else {"Manual multi-tool export uses a configured change position, operator stop and calibrated H registers; LinuxCNC needs its manual M6 handshake. GRBL remains one tool per file."},
            )
            .size(11.0)
            .color(MUTED),
        );
        ui.separator();
        if ui
            .add_enabled(
                self.path.is_some() && !self.code.is_empty(),
                egui::Button::new("Export G-code").fill(egui::Color32::from_rgb(32, 91, 81)),
            )
            .clicked()
        {
            self.export();
        }
        if ui.button("View NC program").clicked() {
            self.code_tab = true;
            self.analysis_open = true;
        }
    }

    fn operation_parameters(&mut self, ui: &mut egui::Ui) {
        if !self.project.process.supports_toolpaths() {
            ui.heading(self.project.process.label());
            ui.label(self.project.process.status());
            return;
        }
        let Some(op) = self.project.operations.get(self.selected) else {
            ui.add_space(16.0);
            ui.heading("No operation selected");
            ui.label(RichText::new("Select a list item to edit its geometry, tool and cutting data. Use + Add operation to choose an existing strategy.").color(MUTED));
            return;
        };
        ui.add_space(6.0);
        ui.heading(format!("Operation {:02}", self.selected + 1));
        ui.label(RichText::new(op.strategy.label()).color(ACCENT).size(12.0));
        ui.add_space(8.0);
        let edit = ui.add_sized(
            [ui.available_width(), 34.0],
            egui::Button::new(
                RichText::new("Edit all operation settings")
                    .strong()
                    .color(Color32::WHITE),
            )
            .fill(Color32::from_rgb(32, 91, 81)),
        );
        #[cfg(test)]
        self.ui_hits.push(("operation:edit".into(), edit.rect));
        if edit.clicked() {
            self.editors.open_operation(self.selected, &self.project);
        }
        let cutting = self
            .project
            .operation_tool(op)
            .unwrap_or_else(|_| self.project.tool.clone());
        let tool = self
            .project
            .library_tool(Some(op))
            .ok()
            .flatten()
            .map(|t| format!("T{}  {}", t.post.number, t.name))
            .unwrap_or_else(|| self.project.tool.name.clone());
        ui.add_space(8.0);
        egui::Frame::new()
            .fill(SURFACE)
            .corner_radius(8)
            .inner_margin(10.0)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                section(ui, "TOOL & CUTTING DATA");
                ui.add(egui::Label::new(RichText::new(&tool).strong()).truncate())
                    .on_hover_text(&tool);
                ui.label(
                    RichText::new(format!(
                        "Ø {:.3} mm  ·  {} RPM",
                        cutting.diameter, cutting.spindle_rpm
                    ))
                    .color(MUTED),
                );
                ui.label(
                    RichText::new(format!(
                        "Feed {:.0}  ·  Plunge {:.0} mm/min",
                        cutting.feed, cutting.plunge
                    ))
                    .color(MUTED)
                    .size(11.0),
                );
                if ui.button("Choose tool / cutting preset").clicked() {
                    self.open_library(Some(self.selected));
                }
            });
        ui.add_space(10.0);
        let op = &mut self.project.operations[self.selected];
        section(ui, "OPERATION");
        let name = ui.add_sized(
            [ui.available_width(), 28.0],
            egui::TextEdit::singleline(&mut op.name).hint_text("Operation name"),
        );
        #[cfg(test)]
        self.ui_hits.push(("operation:name".into(), name.rect));
        let _ = name;
        ui.checkbox(&mut op.enabled, "Include in NC program");
        let linked = op.model_link.is_some();
        if linked {
            ui.label(
                RichText::new("Linked to target geometry")
                    .color(ACCENT)
                    .size(11.0),
            );
            ui.label(RichText::new("Geometry stays locked to the recognized feature. Cutting and pass settings remain editable.").color(MUTED).size(11.0));
            if !op.strategy.is_surface() && ui.small_button("Unlink for manual editing").clicked() {
                op.model_link = None;
            }
        }
        ui.add_space(10.0);
        section(ui, "GEOMETRY · MILLIMETRES");
        if linked {
            ui.label(format!("Position X {:.3}  Y {:.3}", op.x, op.y));
            if op.strategy == Strategy::CircularPocket {
                ui.label(format!("Diameter {:.3} mm", op.width));
            } else if op.strategy != Strategy::Drill {
                ui.label(format!("Size {:.3} × {:.3} mm", op.width, op.height));
            }
            ui.label(format!("Target depth {:.3} mm", op.depth));
        } else {
            ui.add_enabled_ui(true, |ui| {
                let before = op.strategy;
                egui::ComboBox::from_id_salt("strategy")
                    .width(ui.available_width() - 10.0)
                    .selected_text(op.strategy.label())
                    .show_ui(ui, |ui| {
                        for strategy in Strategy::ALL {
                            ui.selectable_value(&mut op.strategy, strategy, strategy.label());
                        }
                    });
                if op.strategy != before && op.strategy == Strategy::Facing {
                    op.x = 0.0;
                    op.y = 0.0;
                    op.width = self.project.stock.width;
                    op.height = self.project.stock.height;
                    op.settings.radial_allowance = 0.0;
                }
                numeric(
                    ui,
                    if matches!(op.strategy, Strategy::CircularPocket | Strategy::Drill) {
                        "Center X"
                    } else {
                        "X position"
                    },
                    &mut op.x,
                    "mm",
                );
                numeric(
                    ui,
                    if matches!(op.strategy, Strategy::CircularPocket | Strategy::Drill) {
                        "Center Y"
                    } else {
                        "Y position"
                    },
                    &mut op.y,
                    "mm",
                );
                if op.strategy == Strategy::CircularPocket {
                    numeric(ui, "Diameter", &mut op.width, "mm");
                } else if op.strategy != Strategy::Drill {
                    numeric(ui, "Width", &mut op.width, "mm");
                    numeric(ui, "Height", &mut op.height, "mm");
                }
                numeric(ui, "Target depth", &mut op.depth, "mm");
            });
        }
        ui.add_space(10.0);
        egui::Frame::new().fill(SURFACE).corner_radius(8).inner_margin(10.0).show(ui,|ui| {
            ui.set_min_width(ui.available_width());
            section(ui,"PASS SUMMARY");
            ui.label(format!("Stepdown {:.3} mm",cutting.step_down));
            if op.strategy!=Strategy::Drill {ui.label(format!("Stepover {:.0}%",cutting.step_over*100.0));}
            ui.label(RichText::new(format!("Clearance {:.3} mm",op.settings.clearance.unwrap_or(self.project.clearance))).color(MUTED));
            ui.label(RichText::new("Use Edit all operation settings for heights, passes, allowances and linking.").size(11.0).color(MUTED));
        });
    }

    fn setup_navigation(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for (section, label) in [
                (SetupSection::Stock, "Stock"),
                (SetupSection::Machine, "Machine"),
                (SetupSection::Tool, "Tool"),
            ] {
                let response = ui.selectable_value(&mut self.setup_section, section, label);
                #[cfg(test)]
                self.ui_hits.push((format!("setup:{label}"), response.rect));
                #[cfg(not(test))]
                let _ = response;
            }
        });
        ui.separator();
    }

    fn setup_parameters(&mut self, ui: &mut egui::Ui) {
        match self.setup_section {
            SetupSection::Machine => {
                self.machine_parameters(ui);
                self.tool_changer_parameters(ui);
                return;
            }
            SetupSection::Tool => {
                self.tooling_parameters(ui);
                return;
            }
            SetupSection::Stock => {}
        }
        ui.label(
            RichText::new(self.project.process.label())
                .color(ACCENT)
                .strong(),
        );
        egui::CollapsingHeader::new(if self.project.process.supports_toolpaths() {
            "Project & stock"
        } else {
            "Project & reference stock"
        })
        .default_open(true)
        .show(ui, |ui| {
            ui.text_edit_singleline(&mut self.project.name);
            numeric(ui, "Width", &mut self.project.stock.width, "mm");
            numeric(ui, "Height", &mut self.project.stock.height, "mm");
            numeric(ui, "Thickness", &mut self.project.stock.thickness, "mm");
            if self.project.process.supports_toolpaths() {
                numeric(ui, "Clearance Z", &mut self.project.clearance, "mm");
            }
        });
    }
}
