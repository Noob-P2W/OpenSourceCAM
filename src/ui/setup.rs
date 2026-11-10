use super::*;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum SetupSection {
    Stock,
    Machine,
    Tool,
}

impl Workbench {
    pub(super) fn machine_parameters(&mut self, ui: &mut egui::Ui) {
        if ui.button("Configure machine…").clicked() {
            self.editors.open_machine(&self.project);
        }
        if ui.button("Machine / vise models…").clicked() {
            self.inspector_tab = InspectorTab::Workspace;
        }
        if self.project.process != Process::Mixed {
            ui.label(RichText::new("Machine profiles store machine settings. Use Tool library to import/export complete toolsets.").size(11.0).color(MUTED));
            ui.horizontal(|ui| {
                if ui.button("Load profile").clicked() {
                    if let Some(file) = rfd::FileDialog::new()
                        .add_filter("Machine/tool profile", &["json"])
                        .pick_file()
                    {
                        match opensourcecam::machine::SetupProfile::load(&file)
                            .and_then(|profile| profile.apply(&mut self.project))
                        {
                            Ok(()) => {
                                self.notice =
                                    "Machine/tool profile applied; review origin and tool settings."
                                        .into()
                            }
                            Err(error) => {
                                self.notice = format!("Profile operation failed: {error}")
                            }
                        }
                    }
                }
                if ui.button("Save profile").clicked() {
                    match opensourcecam::machine::SetupProfile::from_project(&self.project) {
                        Ok(profile) => {
                            if let Some(file) = rfd::FileDialog::new()
                                .add_filter("Machine/tool profile", &["json"])
                                .set_file_name("machine.cam-machine.json")
                                .save_file()
                            {
                                let save = self
                                    .file
                                    .as_ref()
                                    .map_or(Ok(()), |source| {
                                        opensourcecam::files::ensure_distinct(source, &file)
                                    })
                                    .and_then(|()| profile.save(&file));
                                match save {
                                    Ok(()) => self.notice = "Machine/tool profile saved.".into(),
                                    Err(error) => {
                                        self.notice = format!("Profile operation failed: {error}")
                                    }
                                }
                            }
                        }
                        Err(error) => self.notice = format!("Profile operation failed: {error}"),
                    }
                }
            });
        }
        let Some(machine) = &mut self.project.machine else {
            ui.label("Mixed machine setup will be developed last.");
            return;
        };
        ui.heading("Machine definition");
        ui.label(
            RichText::new("Generic editable profile. Enter your actual machine specifications.")
                .color(MUTED)
                .size(11.0),
        );
        ui.text_edit_singleline(&mut machine.name);
        let milling = self.project.process.supports_toolpaths();
        ui.add_space(8.0);
        section(ui, "AXIS TRAVEL / MM");
        egui::Grid::new("machine_axis_limits")
            .num_columns(3)
            .spacing([4.0, 8.0])
            .show(ui, |ui| {
                ui.label("Axis");
                ui.label("Minimum");
                ui.label("Maximum");
                ui.end_row();
                for axis in 0..3 {
                    ui.label(["X", "Y", "Z"][axis]);
                    ui.add(
                        egui::DragValue::new(&mut machine.axis_min[axis])
                            .speed(1)
                            .max_decimals(3),
                    );
                    ui.add(
                        egui::DragValue::new(&mut machine.axis_max[axis])
                            .speed(1)
                            .max_decimals(3),
                    );
                    ui.end_row();
                }
            });
        ui.add_space(8.0);
        section(ui, "WORK ORIGIN IN MACHINE COORDINATES");
        for axis in 0..3 {
            numeric(
                ui,
                ["Origin X", "Origin Y", "Origin Z"][axis],
                &mut machine.work_origin[axis],
                "mm",
            );
        }
        if milling {
            let check = ui.checkbox(&mut machine.enforce_limits, "Check tool-tip travel limits");
            #[cfg(test)]
            self.ui_hits.push(("machine:limits".into(), check.rect));
            #[cfg(not(test))]
            let _ = check;
            ui.label(RichText::new("Machine XYZ = work XYZ + origin. This is a local check; set the same offset on your controller. Initial machine position, holders and fixtures are not checked.").size(11.0).color(MUTED));
        } else {
            ui.label(RichText::new("Reference setup only. Axis conventions, kinematics and travel checks will be connected with this process's motion engine.").size(11.0).color(MUTED));
        }
        ui.separator();
        numeric(ui, "Rapid estimate", &mut machine.rapid_feed, "mm/min");
        numeric(ui, "Max cut feed", &mut machine.max_feed, "mm/min");
        if matches!(
            self.project.process,
            Process::Milling | Process::Turning | Process::Grinding
        ) {
            ui.horizontal(|ui| {
                ui.label("Max spindle");
                ui.add(
                    egui::DragValue::new(&mut machine.max_spindle_rpm)
                        .speed(100)
                        .suffix(" rpm"),
                );
            });
        }
    }

    pub(super) fn tool_changer_parameters(&mut self, ui: &mut egui::Ui) {
        use opensourcecam::machine::ChangeMode;
        if !matches!(self.project.process, Process::Milling | Process::Turning) {
            return;
        }
        if let Some(machine) = &mut self.project.machine {
            ui.separator();
            section(ui, "TOOL CHANGER / TURRET");
            ui.selectable_value(
                &mut machine.tool_changer.mode,
                ChangeMode::MountedTool,
                "Mounted single tool",
            );
            if self.project.process == Process::Milling {
                ui.selectable_value(
                    &mut machine.tool_changer.mode,
                    ChangeMode::Manual,
                    "Manual changes / operator stop",
                );
                if machine.tool_changer.mode == ChangeMode::Manual {
                    let mut configured = machine.tool_changer.manual_position.is_some();
                    if ui
                        .checkbox(
                            &mut configured,
                            "Configure manual change position (work coordinates)",
                        )
                        .changed()
                    {
                        machine.tool_changer.manual_position =
                            configured.then_some([0., 0., self.project.clearance + 20.]);
                    }
                    if let Some(position) = &mut machine.tool_changer.manual_position {
                        for (i, value) in position.iter_mut().enumerate() {
                            numeric(
                                ui,
                                &format!("Change {}", "XYZ".as_bytes()[i] as char),
                                value,
                                "mm",
                            );
                        }
                    }
                }
                ui.selectable_value(
                    &mut machine.tool_changer.mode,
                    ChangeMode::MillingAtc,
                    "Automatic tool changer",
                );
                ui.checkbox(
                    &mut machine.tool_changer.controller_managed,
                    "Controller has a configured safe M6 routine",
                );
                ui.label(RichText::new("M6 positioning and changer interlocks run in the controller. Mach3/LinuxCNC ATC requires calibrated H registers and assigned pockets.").size(11.0).color(MUTED));
            } else {
                ui.selectable_value(
                    &mut machine.tool_changer.mode,
                    ChangeMode::TurningTurret,
                    "Automatic turning turret",
                );
                ui.label(RichText::new("Turret stations and offsets can be configured now. Turning output remains planned.").size(11.0).color(MUTED));
            }
            ui.horizontal(|ui| {
                ui.label("Pockets / stations");
                ui.add(egui::DragValue::new(&mut machine.tool_changer.capacity).range(1..=9999));
            });
        }
    }

    pub(super) fn tooling_parameters(&mut self, ui: &mut egui::Ui) {
        ui.heading("Tool library");
        if let Some(tool) = self
            .project
            .active_tool
            .and_then(|id| self.project.tool_library.get(id).ok())
        {
            ui.label(format!("T{}  {}", tool.post.number, tool.name));
            ui.label(tool.kind.label());
            ui.label(format!("{} cutting presets", tool.presets.len()));
        }
        if ui.button("Open tool library").clicked() {
            self.open_library(None);
        }
        ui.label(RichText::new("Create, edit and assign tools in the shared local library. Tool geometry, holders, cutting presets and post/ATC settings are managed together.").size(11.0).color(MUTED));
    }
}
