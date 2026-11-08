use super::*;
use opensourcecam::{
    machine::MachineSetup,
    operation::{Direction, Raster},
};

#[derive(Default)]
pub(super) struct Editors {
    pub(super) machine: Option<MachineSetup>,
    pub(super) operation: Option<(usize, Operation)>,
    pub(super) tab: usize,
    validation: Option<(Project, Result<(), String>)>,
    source: Option<Project>,
    camera: Camera,
    zoom: f32,
}
impl Editors {
    pub(super) fn open_machine(&mut self, project: &Project) {
        self.source = Some(project.clone());
        self.machine = project.machine.clone();
        self.operation = None;
        self.tab = 0;
        self.validation = None;
    }
    pub(super) fn open_operation(&mut self, index: usize, project: &Project) {
        self.source = Some(project.clone());
        self.operation = project.operations.get(index).cloned().map(|op| (index, op));
        self.machine = None;
        self.tab = 0;
        self.validation = None;
    }
    pub(super) fn is_open(&self) -> bool {
        self.machine.is_some() || self.operation.is_some()
    }
    fn show(&mut self, ui: &mut egui::Ui, app: &mut Workbench) -> bool {
        if self.zoom < 0.25 {
            self.zoom = 1.0;
        }
        let machine = self.machine.is_some();
        ui.heading(if machine {
            "Machine configuration"
        } else {
            "Operation editor"
        });
        ui.label(RichText::new(&app.project.name).color(MUTED));
        ui.separator();
        let tabs = if machine {
            vec![
                "General",
                "Axes & origin",
                "Spindle",
                "Table & models",
                "Tool changer",
            ]
        } else {
            vec!["Tool", "Geometry", "Heights", "Passes", "Linking"]
        };
        ui.horizontal_wrapped(|ui| {
            for (i, label) in tabs.iter().enumerate() {
                let response = ui.selectable_value(&mut self.tab, i, *label);
                #[cfg(test)]
                app.ui_hits
                    .push((format!("editor:tab:{label}"), response.rect));
                #[cfg(not(test))]
                let _ = response;
            }
        });
        ui.separator();
        let available = ui.available_size();
        let height = (available.y - 116.0).max(180.0);
        let right = (available.x * 0.32).clamp(280.0, 420.0);
        let left = available.x - right - 22.0;
        ui.allocate_ui_with_layout(Vec2::new(available.x,height),egui::Layout::left_to_right(egui::Align::Min),|ui|{
            ui.allocate_ui_with_layout(Vec2::new(left,height),egui::Layout::top_down(egui::Align::Min),|ui| {
                ui.set_min_width(left);egui::ScrollArea::vertical().auto_shrink([false,false]).id_salt(("configuration_editor",machine,self.tab)).max_height(height).show(ui,|ui| {
                    egui::Frame::new().fill(SURFACE).corner_radius(8).inner_margin(14.0).show(ui,|ui| {
                        ui.set_min_width(ui.available_width());
                        if let Some(draft)=&mut self.machine {machine_form(ui,draft,self.tab);} else if let Some((_,draft))=&mut self.operation {operation_form(ui,draft,self.tab,&app.project);}
                    });
                });
            });
            ui.separator();
            ui.allocate_ui_with_layout(Vec2::new(right,height),egui::Layout::top_down(egui::Align::Min),|ui| {
                ui.set_min_width(right);
                egui::ScrollArea::vertical().id_salt("configuration_preview").auto_shrink([false,false]).max_height(height).show(ui,|ui| {section(ui,"SETUP PREVIEW");
                if let Some(m)=&self.machine {
                    ui.label(format!("{}\n{} {}",m.name,m.configuration.manufacturer,m.configuration.model));
                    ui.label(format!("XYZ travel: {:.1} × {:.1} × {:.1} mm",m.axis_max[0]-m.axis_min[0],m.axis_max[1]-m.axis_min[1],m.axis_max[2]-m.axis_min[2]));
                    diagram(ui,[m.axis_max[0]-m.axis_min[0],m.axis_max[1]-m.axis_min[1]],"Machine travel / top view");
                    ui.label("Imported machine and vise geometry is displayed in the main workspace. Profiles store configuration; reference models stay in the project.");
                } else if let Some((_,op))=&self.operation {
                    ui.label(op.strategy.label());
                    if self.tab==0 {if let Ok(Some(tool))=app.project.library_tool(Some(op)) {super::library::preview::draw(ui,tool,&app.project.tool_library,&mut self.camera,&mut self.zoom,Vec2::new(right-12.0,250.0));}} else {diagram(ui,[op.width,op.height],"Operation geometry / schematic");}
                    ui.label(format!("Target depth: {:.3} mm\nAxial stock remaining: {:.3} mm\nRadial stock remaining: {:.3} mm",op.depth,op.settings.axial_allowance,op.settings.radial_allowance));
                    if let Ok(tool)=app.project.operation_tool(op) {ui.label(format!("{}\n{} RPM · {:.1} mm/min\nStepdown {:.3} mm · stepover {:.1}%",tool.name,tool.spindle_rpm,tool.feed,tool.step_down,tool.step_over*100.0));}
                }
                });
            });
        });
        ui.separator();
        let mut candidate = app.project.clone();
        if let Some(machine) = &self.machine {
            candidate.machine = Some(machine.clone());
        }
        if let Some((index, operation)) = &self.operation {
            candidate.operations[*index] = operation.clone();
        }
        let validation = if let Some((previous, result)) = &self.validation {
            if previous == &candidate {
                Some(result.clone())
            } else {
                None
            }
        } else {
            None
        };
        let validation = if self.source.as_ref() != Some(&app.project) {
            Err("The project changed while this draft was open. Cancel and reopen the editor before saving.".into())
        } else {
            validation.unwrap_or_else(|| {
                let result = if machine {
                    candidate
                        .machine
                        .as_ref()
                        .unwrap()
                        .validate(candidate.process)
                        .and_then(|()| {
                            opensourcecam::workspace::validate(
                                &candidate.workspace,
                                candidate.machine.as_ref(),
                            )
                        })
                } else {
                    let mut check = candidate.clone();
                    let (_, op) = self.operation.as_ref().unwrap();
                    let mut op = op.clone();
                    op.enabled = true;
                    check.operations = vec![op];
                    cam::generate(&check).map(|_| ())
                };
                self.validation = Some((candidate.clone(), result.clone()));
                result
            })
        };
        if let Err(error) = &validation {
            ui.colored_label(Color32::from_rgb(255, 133, 126), error);
        } else {
            ui.label(
                RichText::new("Draft validated. Changes take effect only after Save.")
                    .color(ACCENT),
            );
        }
        let mut close = false;
        ui.horizontal(|ui| {
            let save = ui.add_enabled(
                validation.is_ok(),
                egui::Button::new("Save").fill(Color32::from_rgb(32, 91, 81)),
            );
            let cancel = ui.button("Cancel");
            #[cfg(test)]
            {
                app.ui_hits.push(("editor:save".into(), save.rect));
                app.ui_hits.push(("editor:cancel".into(), cancel.rect));
            }
            if save.clicked() {
                app.project = candidate;
                close = true;
                app.notice = if machine {
                    "Machine configuration saved; review any job validation errors."
                } else {
                    "Operation settings saved."
                }
                .into();
            }
            if cancel.clicked() {
                close = true;
            }
            if let Some((index, _)) = &self.operation {
                let delete = ui.add_enabled(
                    self.source.as_ref() == Some(&app.project),
                    egui::Button::new("Delete operation"),
                );
                #[cfg(test)]
                app.ui_hits.push(("editor:delete".into(), delete.rect));
                if delete.clicked() {
                    app.selected = *index;
                    app.delete_selected();
                    close = true;
                }
            }
        });
        close
    }
}
impl Workbench {
    pub(super) fn editor_screen(&mut self, ctx: &egui::Context) {
        let mut state = std::mem::take(&mut self.editors);
        let mut close = false;
        egui::CentralPanel::default().show(ctx, |ui| {
            close = state.show(ui, self);
        });
        if !close {
            self.editors = state;
        }
    }
}
fn field(ui: &mut egui::Ui, label: &str, v: &mut f64, unit: &str) {
    ui.horizontal(|ui| {
        ui.add_sized([210.0, 24.0], egui::Label::new(label));
        ui.add(
            egui::DragValue::new(v)
                .speed(0.1)
                .max_decimals(4)
                .suffix(format!(" {unit}")),
        );
    });
}
fn optional(ui: &mut egui::Ui, label: &str, v: &mut Option<f64>, default: f64, unit: &str) {
    let mut enabled = v.is_some();
    if ui.checkbox(&mut enabled, label).changed() {
        *v = enabled.then_some(default);
    }
    if let Some(v) = v {
        field(ui, "Override", v, unit);
    } else {
        ui.label(
            RichText::new(format!("Inherited: {default:.3} {unit}"))
                .size(11.0)
                .color(MUTED),
        );
    }
}
fn machine_form(ui: &mut egui::Ui, m: &mut MachineSetup, tab: usize) {
    let c = &mut m.configuration;
    match tab {
        0 => {
            for (label, v) in [
                ("Name", &mut m.name),
                ("Manufacturer", &mut c.manufacturer),
                ("Model", &mut c.model),
                ("Serial / asset ID", &mut c.serial),
                ("Controller / firmware", &mut c.controller),
            ] {
                ui.label(label);
                ui.text_edit_singleline(v);
            }
            ui.label("Machine data stays local. Select the NC dialect separately in Post.");
        }
        1 => {
            for axis in 0..3 {
                section(ui, ["X AXIS", "Y AXIS", "Z AXIS"][axis]);
                field(ui, "Travel minimum", &mut m.axis_min[axis], "mm");
                field(ui, "Travel maximum", &mut m.axis_max[axis], "mm");
                field(ui, "Maximum feed", &mut c.axis_feed[axis], "mm/min");
                field(
                    ui,
                    "Acceleration (reference)",
                    &mut c.axis_acceleration[axis],
                    "mm/s²",
                );
                optional(
                    ui,
                    "Home position (reference)",
                    &mut c.home[axis],
                    m.axis_max[axis],
                    "mm",
                );
                field(
                    ui,
                    "Work XYZ=0 in machine coordinates",
                    &mut m.work_origin[axis],
                    "mm",
                );
                ui.separator();
            }
            ui.checkbox(&mut m.enforce_limits, "Enforce tool-tip travel limits");
            ui.checkbox(&mut m.check_tool_reach, "Check flute length and stickout");
            field(ui, "Machine maximum cut feed", &mut m.max_feed, "mm/min");
            field(ui, "Rapid time estimate", &mut m.rapid_feed, "mm/min");
            ui.label("Per-axis feed limits are checked on G1 moves. Home/acceleration are reference data, not controller configuration or generated homing commands.");
        }
        2 => {
            ui.horizontal(|ui| {
                ui.label("Minimum spindle RPM");
                ui.add(egui::DragValue::new(&mut c.minimum_rpm));
            });
            ui.horizontal(|ui| {
                ui.label("Maximum spindle RPM");
                ui.add(egui::DragValue::new(&mut m.max_spindle_rpm));
            });
            ui.checkbox(&mut c.reverse_spindle, "Reverse spindle rotation supported");
            optional(
                ui,
                "Rated spindle power (reference)",
                &mut c.spindle_power_kw,
                1.0,
                "kW",
            );
        }
        3 => {
            field(ui, "Table X size", &mut c.table_size[0], "mm");
            field(ui, "Table Y size", &mut c.table_size[1], "mm");
            optional(
                ui,
                "Rated table load (reference)",
                &mut c.maximum_load_kg,
                100.0,
                "kg",
            );
            ui.label("Import the machine, vise and other fixtures through Workspace. Each model has independent translation, rotation, source units and visibility, and a work/machine coordinate frame.");
            ui.label("Table size and load do not constitute a fixture or collision check.");
        }
        _ => {
            use opensourcecam::machine::ChangeMode;
            ui.selectable_value(
                &mut m.tool_changer.mode,
                ChangeMode::MountedTool,
                "Mounted single tool",
            );
            ui.selectable_value(
                &mut m.tool_changer.mode,
                ChangeMode::MillingAtc,
                "Milling ATC",
            );
            ui.selectable_value(
                &mut m.tool_changer.mode,
                ChangeMode::TurningTurret,
                "Turning turret",
            );
            ui.horizontal(|ui| {
                ui.label("Capacity / stations");
                ui.add(egui::DragValue::new(&mut m.tool_changer.capacity));
            });
            ui.checkbox(
                &mut m.tool_changer.controller_managed,
                "Controller has a configured safe M6 routine",
            );
            ui.label("Assign tool numbers, pockets/stations and calibrated offsets in the tool library. M6 motion/interlocks run in the controller. Turning motion remains planned.");
        }
    }
}
fn operation_form(ui: &mut egui::Ui, op: &mut Operation, tab: usize, project: &Project) {
    ui.label("Operation name");
    ui.text_edit_singleline(&mut op.name);
    ui.checkbox(&mut op.enabled, "Enabled in job");
    ui.separator();
    let effective = project
        .operation_tool(op)
        .unwrap_or_else(|_| project.tool.clone());
    match tab {
        0 => {
            section(ui, "LOCAL TOOL LIBRARY · SELECT FOR THIS OPERATION");
            egui::ScrollArea::vertical()
                .id_salt("operation_tools")
                .max_height(180.0)
                .show(ui, |ui| {
                    for tool in project.tool_library.tools.iter().filter(|t| {
                        t.uses.contains(&project.process)
                            && t.kind != opensourcecam::tool_library::ToolKind::Holder
                    }) {
                        if ui
                            .selectable_label(
                                op.tool_id.or(project.active_tool) == Some(tool.id),
                                format!(
                                    "T{}  {}  ·  {}",
                                    tool.post.number,
                                    tool.name,
                                    tool.kind.label()
                                ),
                            )
                            .clicked()
                        {
                            op.tool_id = Some(tool.id);
                            op.preset_id = Some(tool.default_preset);
                        }
                    }
                });
            if let Ok(Some(tool)) = project.library_tool(Some(op)) {
                section(ui, "CUTTING PRESET");
                for p in &tool.presets {
                    if ui
                        .selectable_label(
                            op.preset_id.unwrap_or(tool.default_preset) == p.id,
                            format!("{} · {}", p.name, p.material),
                        )
                        .clicked()
                    {
                        op.preset_id = Some(p.id);
                    }
                }
            }
            optional(
                ui,
                "Override cut feed",
                &mut op.settings.feed,
                effective.feed,
                "mm/min",
            );
            optional(
                ui,
                "Override plunge feed",
                &mut op.settings.plunge,
                effective.plunge,
                "mm/min",
            );
            let mut override_rpm = op.settings.spindle_rpm.is_some();
            if ui
                .checkbox(&mut override_rpm, "Override spindle speed")
                .changed()
            {
                op.settings.spindle_rpm = override_rpm.then_some(effective.spindle_rpm);
            }
            if let Some(rpm) = &mut op.settings.spindle_rpm {
                ui.horizontal(|ui| {
                    ui.label("Spindle speed");
                    ui.add(egui::DragValue::new(rpm).speed(100).suffix(" rpm"));
                });
            } else {
                ui.label(
                    RichText::new(format!("Inherited: {} RPM", effective.spindle_rpm))
                        .size(11.0)
                        .color(MUTED),
                );
            }
            ui.add_space(12.0);
            section(ui, "EFFECTIVE CUTTING DATA");
            egui::Grid::new("effective_cutting_summary")
                .num_columns(2)
                .spacing([28.0, 9.0])
                .show(ui, |ui| {
                    for (label, value) in [
                        ("Tool diameter", format!("{:.3} mm", effective.diameter)),
                        ("Spindle speed", format!("{} RPM", effective.spindle_rpm)),
                        ("Cutting feed", format!("{:.1} mm/min", effective.feed)),
                        ("Plunge feed", format!("{:.1} mm/min", effective.plunge)),
                        ("Maximum stepdown", format!("{:.3} mm", effective.step_down)),
                        ("Stepover", format!("{:.1}%", effective.step_over * 100.0)),
                    ] {
                        ui.label(RichText::new(label).color(MUTED));
                        ui.label(RichText::new(value).strong());
                        ui.end_row();
                    }
                });
            ui.add_space(10.0);
            ui.label("Edit reusable tool geometry and preset definitions in the dedicated Tool library. These overrides affect this operation only.");
        }
        1 => {
            if op.model_link.is_some() {
                ui.colored_label(ACCENT, "Geometry linked to imported model");
                if !op.strategy.is_surface() && ui.button("Unlink for manual programming").clicked()
                {
                    op.model_link = None;
                }
            }
            ui.add_enabled_ui(op.model_link.is_none(),|ui| {
            let before = op.strategy;
            egui::ComboBox::from_id_salt("editor_strategy")
                .selected_text(op.strategy.label())
                .show_ui(ui, |ui| {
                    for s in Strategy::ALL {
                        ui.selectable_value(&mut op.strategy, s, s.label());
                    }
                });
            if before != op.strategy && op.strategy == Strategy::Drill {
                op.settings.radial_allowance = 0.0;
                op.settings.axial_allowance = 0.0;
                op.settings.spring_passes = 0;
            }
            if before != op.strategy && op.strategy == Strategy::Facing {
                op.x = 0.0;
                op.y = 0.0;
                op.width = project.stock.width;
                op.height = project.stock.height;
                op.settings.radial_allowance = 0.0;
            }
            field(ui, "X position", &mut op.x, "mm");
            field(ui, "Y position", &mut op.y, "mm");
            if op.strategy == Strategy::CircularPocket {
                field(ui, "Diameter", &mut op.width, "mm");
            } else if op.strategy != Strategy::Drill {
                field(ui, "Width", &mut op.width, "mm");
                field(ui, "Height", &mut op.height, "mm");
            }
            ui.label("Model-linked rectangles and round holes are generated from recognized features. Facing covers the whole blank. Circular pockets use X/Y as the center.");
            });
        }
        2 => {
            ui.add_enabled_ui(op.model_link.is_none(), |ui| {
                field(ui, "Target depth below stock top", &mut op.depth, "mm");
            });
            optional(
                ui,
                "Override XY clearance height",
                &mut op.settings.clearance,
                project.clearance,
                "mm",
            );
            optional(
                ui,
                "Override peck retract height",
                &mut op.settings.retract,
                op.settings.clearance.unwrap_or(project.clearance),
                "mm",
            );
            ui.label("Stock top / Z zero = 0. Positive depth cuts downward. Inter-operation and ATC clearance uses the highest enabled clearance. Peck retract must not exceed this operation's clearance. No drill-point or breakthrough allowance is inferred.");
        }
        3 => {
            optional(
                ui,
                "Override maximum stepdown / peck",
                &mut op.settings.step_down,
                effective.step_down,
                "mm",
            );
            if op.strategy != Strategy::Drill {
                optional(
                    ui,
                    "Override stepover",
                    &mut op.settings.step_over_percent,
                    effective.step_over * 100.0,
                    "%",
                );
                if op.strategy != Strategy::Facing {
                    field(
                        ui,
                        "Radial stock to leave",
                        &mut op.settings.radial_allowance,
                        "mm",
                    );
                }
                field(
                    ui,
                    "Axial stock to leave",
                    &mut op.settings.axial_allowance,
                    "mm",
                );
                if !op.strategy.is_surface() {
                    ui.horizontal(|ui| {
                        ui.label("Spring passes at final depth");
                        ui.add(egui::DragValue::new(&mut op.settings.spring_passes).range(0..=20));
                    });
                    optional(
                        ui,
                        "Override spring-pass feed",
                        &mut op.settings.finish_feed,
                        effective.feed,
                        "mm/min",
                    );
                }
                if op.strategy.is_surface() {
                    optional(ui, "3D grid step", &mut op.settings.surface_step, 0.5, "mm");
                    ui.label("Smaller cells retain less excess material and require more work. This protects the mesh upper surface, including narrow ridges; it does not machine undercuts.");
                }
            }
            if op.strategy == Strategy::CircularPocket {
                optional(
                    ui,
                    "Chord tolerance",
                    &mut op.settings.path_tolerance,
                    0.01,
                    "mm",
                );
            }
            ui.label("Stock-to-leave remains for a later operation. Spring passes repeat the same offset contour; they do not remove the allowance.");
        }
        _ => {
            if matches!(op.strategy, Strategy::Pocket | Strategy::Facing)
                || op.strategy.is_surface()
            {
                section(ui, "RASTER DIRECTION");
                for (v, label) in [
                    (Raster::Bidirectional, "Bidirectional"),
                    (Raster::PositiveX, "One way +X"),
                    (Raster::NegativeX, "One way −X"),
                ] {
                    ui.selectable_value(&mut op.settings.raster, v, label);
                }
            }
            if !matches!(op.strategy, Strategy::Drill | Strategy::Facing)
                && !op.strategy.is_surface()
            {
                section(ui, "CONTOUR DIRECTION");
                ui.selectable_value(
                    &mut op.settings.direction,
                    Direction::Counterclockwise,
                    "Counterclockwise",
                );
                ui.selectable_value(
                    &mut op.settings.direction,
                    Direction::Clockwise,
                    "Clockwise",
                );
            }
            ui.separator();
            ui.label("Entry: center-cutting plunge for cavities; facing and outside clearing enter in air. One-way rows retract before repositioning. Imported fixture checks use cutter, shaft and defined-holder envelopes. Ramps and helixes are planned.");
        }
    }
}
fn diagram(ui: &mut egui::Ui, size: [f64; 2], label: &str) {
    let (rect, p) =
        ui.allocate_painter(Vec2::new(ui.available_width(), 220.0), egui::Sense::hover());
    let rect = rect.rect;
    let w = size[0].abs().clamp(0.01, 1e6) as f32;
    let h = size[1].abs().clamp(0.01, 1e6) as f32;
    let scale = ((rect.width() - 60.0) / w).min((rect.height() - 60.0) / h);
    let shape = Rect::from_center_size(rect.center(), Vec2::new(w * scale, h * scale));
    p.rect_filled(shape, 3, Color32::from_rgb(35, 64, 77));
    p.rect_stroke(
        shape,
        3,
        Stroke::new(2.0_f32, ACCENT),
        egui::StrokeKind::Inside,
    );
    p.text(
        rect.left_bottom(),
        egui::Align2::LEFT_BOTTOM,
        label,
        egui::FontId::proportional(11.0),
        MUTED,
    );
}
