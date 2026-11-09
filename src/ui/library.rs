use super::*;
use opensourcecam::tool_library::{
    FeedMode, LibraryTool, Parameter, Section, ToolKind, ToolLibrary,
};
pub(super) mod preview;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Tab {
    General,
    Cutter,
    Shaft,
    Holder,
    Cutting,
    Post,
}
pub(super) struct LibraryState {
    pub open: bool,
    pub assign_operation: Option<usize>,
    pub selected: Option<u64>,
    query: String,
    group: Option<String>,
    filter: Option<Process>,
    draft: Option<LibraryTool>,
    picker: bool,
    tab: Tab,
    preset: u64,
    inches: bool,
    error: Option<String>,
    notice: String,
    replace_import: bool,
    camera: Camera,
    zoom: f32,
}
impl Default for LibraryState {
    fn default() -> Self {
        Self {
            open: false,
            assign_operation: None,
            selected: None,
            query: String::new(),
            group: None,
            filter: None,
            draft: None,
            picker: false,
            tab: Tab::General,
            preset: 1,
            inches: false,
            error: None,
            notice: String::new(),
            replace_import: false,
            camera: Camera::default(),
            zoom: 1.0,
        }
    }
}

impl Workbench {
    pub(super) fn open_library(&mut self, operation: Option<usize>) {
        self.project.ensure_tool_library();
        self.library_ui.open = true;
        self.library_ui.assign_operation = operation;
        self.library_ui.selected = operation
            .and_then(|i| self.project.operations.get(i).and_then(|op| op.tool_id))
            .or(self.project.active_tool);
        self.library_ui.filter = if self.project.process == Process::Mixed {
            None
        } else {
            Some(self.project.process)
        };
        if let Some(tool) = self
            .library_ui
            .selected
            .and_then(|id| self.project.tool_library.get(id).ok())
        {
            let requested = operation
                .and_then(|i| self.project.operations.get(i).and_then(|op| op.preset_id))
                .or(if operation.is_none() {
                    self.project.active_preset
                } else {
                    None
                });
            self.library_ui.preset = tool.preset(requested).map_or(tool.default_preset, |p| p.id);
        }
    }
    pub(super) fn library_screen(&mut self, ctx: &egui::Context) {
        let mut state = std::mem::take(&mut self.library_ui);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(PANEL).inner_margin(14.0))
            .show(ctx, |ui| state.show(ui, self));
        self.library_ui = state;
    }
}

impl LibraryState {
    pub(super) fn capture(&mut self, mode: &str, project: &Project) {
        self.open = true;
        self.selected = project.active_tool;
        self.filter = None;
        if mode == "types" {
            self.picker = true;
        }
        if matches!(
            mode,
            "edit"
                | "edit-bottom"
                | "edit-top"
                | "cutting"
                | "post"
                | "general"
                | "shaft"
                | "holder"
        ) {
            self.edit_capture(project);
            self.tab = match mode {
                "cutting" => Tab::Cutting,
                "post" => Tab::Post,
                "general" => Tab::General,
                "shaft" => Tab::Shaft,
                "holder" => Tab::Holder,
                _ => Tab::Cutter,
            };
            if mode == "edit-bottom" {
                self.camera.pitch = -1.15;
            }
            if mode == "edit-top" {
                self.camera.pitch = 1.15;
            }
        }
    }
    fn edit_capture(&mut self, project: &Project) {
        self.draft = self
            .selected
            .and_then(|id| project.tool_library.get(id).ok())
            .cloned();
        if let Some(t) = &self.draft {
            self.preset = t.default_preset;
        }
    }
    fn show(&mut self, ui: &mut egui::Ui, app: &mut Workbench) {
        ui.horizontal(|ui| {
            ui.heading("Tool library");
            ui.label(RichText::new(&app.project.name).color(MUTED));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let close = ui.button(if self.draft.is_some() {
                    "Back to library"
                } else {
                    "Close library"
                });
                #[cfg(test)]
                app.ui_hits.push(("library:close".into(), close.rect));
                if close.clicked() {
                    if self.draft.is_some() || self.picker {
                        self.draft = None;
                        self.picker = false;
                    } else {
                        self.open = false;
                        self.assign_operation = None;
                    }
                }
                ui.label("LOCAL / OFFLINE");
            });
        });
        ui.separator();
        if self.picker {
            self.type_picker(ui, app);
            return;
        }
        if self.draft.is_some() {
            self.editor(ui, app);
            return;
        }
        self.toolbar(ui, app);
        if let Some(error) = &self.error {
            ui.colored_label(Color32::from_rgb(255, 133, 126), error);
        } else if !self.notice.is_empty() {
            ui.label(RichText::new(&self.notice).color(MUTED));
        }
        ui.separator();
        let height = ui.available_height() - 70.0;
        let width = ui.available_width();
        let sidebar = 145.0;
        let info = (width * 0.23).clamp(205.0, 300.0);
        let middle = (width - sidebar - info - 24.0).max(300.0);
        fixed_region(ui, Vec2::new(ui.available_width(), height), |ui| {
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(sidebar, height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| self.navigation(ui, app),
                );
                ui.separator();
                ui.allocate_ui_with_layout(
                    Vec2::new(middle, height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| self.table(ui, app, height),
                );
                ui.separator();
                ui.allocate_ui_with_layout(
                    Vec2::new(info, height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        if let Some(t) = self
                            .selected
                            .and_then(|id| app.project.tool_library.get(id).ok())
                        {
                            ui.heading(&t.name);
                            ui.label(t.kind.label());
                            preview::draw(
                                ui,
                                t,
                                &app.project.tool_library,
                                &mut self.camera,
                                &mut self.zoom,
                                Vec2::new(info - 8.0, (height * 0.48).max(180.0)),
                            );
                            egui::ScrollArea::vertical()
                                .id_salt(("library_information", t.id))
                                .max_height((height * 0.4).max(100.0))
                                .show(ui, |ui| {
                                    for (label, value) in [
                                        ("Vendor", t.vendor.clone()),
                                        ("Product", t.product_id.clone()),
                                        ("Material", t.material.clone()),
                                        ("Tool number", t.post.number.to_string()),
                                        (
                                            "ATC pocket",
                                            t.post
                                                .pocket
                                                .map_or("Unassigned".into(), |n| n.to_string()),
                                        ),
                                    ] {
                                        ui.label(RichText::new(label).color(MUTED).size(11.0));
                                        ui.label(if value.is_empty() { "—" } else { &value });
                                    }
                                    ui.separator();
                                    for p in t.kind.geometry() {
                                        ui.label(format!(
                                            "{}: {:.3} {}",
                                            p.label,
                                            t.dimension(p.key),
                                            p.unit
                                        ));
                                    }
                                });
                        } else {
                            ui.label("Select a tool to inspect its assembly and cutting presets.");
                        }
                    },
                );
            });
        });
        ui.separator();
        ui.horizontal(|ui| {
            let tool = self
                .selected
                .and_then(|id| app.project.tool_library.get(id).ok());
            let compatible = tool.is_some_and(|t| {
                t.uses.contains(&app.project.process) && t.kind != ToolKind::Holder
            });
            let use_tool = ui.add_enabled(
                compatible,
                egui::Button::new(if self.assign_operation.is_some() {
                    "Assign tool to operation"
                } else {
                    "Use as active tool"
                })
                .fill(Color32::from_rgb(32, 91, 81)),
            );
            #[cfg(test)]
            app.ui_hits.push(("library:assign".into(), use_tool.rect));
            if use_tool.clicked() {
                self.assign(app);
            }
            ui.label(
                RichText::new(format!(
                    "{} tools · {}",
                    app.project.tool_library.tools.len(),
                    app.project.process.label()
                ))
                .color(MUTED),
            );
            if self.assign_operation.is_some() {
                ui.label("Select a tool and cutting preset, then assign.");
            }
        });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, app: &mut Workbench) {
        if !app.project.operations.is_empty() {
            self.replace_import = false;
        }
        ui.horizontal_wrapped(|ui| {
            for label in ["New","Edit","Duplicate","Delete"] {
                let response=ui.add_enabled(label=="New"||self.selected.is_some(),egui::Button::new(label));
                #[cfg(test)] app.ui_hits.push((format!("library:{label}"),response.rect));
                if !response.clicked(){continue;}
                match label {
                    "New"=>{self.picker=true;self.query.clear();self.error=None;},
                    "Edit"=>self.edit(app),
                    "Duplicate"=>{
                        if let Some(tool)=self.selected.and_then(|id|app.project.tool_library.get(id).ok()) {
                            let mut copy=tool.clone();copy.id=app.project.tool_library.next_id();copy.name.push_str(" copy");copy.post.number=app.project.tool_library.next_number_for(&copy.uses);
                            copy.post.pocket=None;copy.post.length_offset=None;copy.post.diameter_offset=None;copy.post.turning_offset=None;
                            self.preset=copy.default_preset;self.draft=Some(copy);self.tab=Tab::General;
                        }
                    },
                    "Delete"=>{
                        if let Some(id)=self.selected {
                            if app.project.active_tool==Some(id)||app.project.operations.iter().any(|op|op.tool_id==Some(id))||app.project.tool_library.tools.iter().any(|t|t.holder==Some(id)) {
                                self.error=Some("This tool is used by the project or an assembly. Reassign it before deleting.".into());
                            }else {app.project.tool_library.tools.retain(|t|t.id!=id);self.selected=None;self.error=None;}
                        }
                    },_=>{}
                }
            }
            ui.separator();
            if ui.button("Import tool list").clicked() {
                if let Some(path)=rfd::FileDialog::new().add_filter("OpenSourceCAM tools",&["json"]).pick_file() {
                    match ToolLibrary::load(&path).and_then(|source|import_tools(&mut app.project,source,self.replace_import)) {
                        Ok(message)=>{self.notice=message;self.error=None;self.filter=None;self.group=None;self.query.clear();self.selected=app.project.active_tool;},
                        Err(error)=>self.error=Some(error),
                    }
                }
            }
            ui.add_enabled_ui(app.project.operations.is_empty(),|ui|{ui.checkbox(&mut self.replace_import,"Replace empty-job library").on_hover_text("Replace local definitions and preserve incoming tool numbers, pockets and offsets. Otherwise import adds copies with fresh numbers and cleared device bindings.");});
            if ui.button("Export tool list").clicked() {
                if let Some(path)=rfd::FileDialog::new().add_filter("OpenSourceCAM tools",&["json"]).set_file_name("tools.cam-tools.json").save_file() {
                    let result=app.file.as_ref().map_or(Ok(()),|source|opensourcecam::files::ensure_distinct(source,&path)).and_then(|()|app.project.tool_library.save(&path));
                    self.error=result.err();if self.error.is_none(){self.notice="Tool library exported.".into();}
                }
            }
            ui.separator();ui.label("Search");ui.add(egui::TextEdit::singleline(&mut self.query).desired_width(180.0).hint_text("Name, vendor, type, T number"));
        });
    }

    fn navigation(&mut self, ui: &mut egui::Ui, app: &Workbench) {
        section(ui, "LOCAL LIBRARY");
        ui.label("Current project");
        ui.separator();
        if ui
            .selectable_label(self.group.is_none() && self.filter.is_none(), "All tools")
            .clicked()
        {
            self.group = None;
            self.filter = None;
        }
        egui::ScrollArea::vertical()
            .id_salt("library_groups")
            .show(ui, |ui| {
                section(ui, "TOOL TYPES");
                for group in [
                    "Milling",
                    "Hole making",
                    "Turning",
                    "Cutting",
                    "3D printing",
                    "Probe",
                    "Holders",
                    "Grinding",
                    "EDM",
                ] {
                    if ui
                        .selectable_label(self.group.as_deref() == Some(group), group)
                        .clicked()
                    {
                        self.group = Some(group.into());
                        self.filter = None;
                    }
                }
                ui.separator();
                section(ui, "OPERATIONS");
                for process in Process::ALL {
                    if ui
                        .selectable_label(self.filter == Some(process), process.label())
                        .clicked()
                    {
                        self.filter = Some(process);
                        self.group = None;
                    }
                }
                ui.separator();
                ui.label(
                    RichText::new(format!(
                        "{} local definitions",
                        app.project.tool_library.tools.len()
                    ))
                    .color(MUTED)
                    .size(11.0),
                );
            });
    }

    fn table(&mut self, ui: &mut egui::Ui, app: &mut Workbench, height: f32) {
        let query = self.query.to_lowercase();
        let name_width = (ui.available_width() - 455.0).max(190.0);
        egui::ScrollArea::both()
            .id_salt("library_table")
            .auto_shrink([false, false])
            .max_height((height * 0.65).max(160.0))
            .show(ui, |ui| {
                egui::Grid::new("tool_rows")
                    .striped(true)
                    .min_row_height(28.0)
                    .spacing([10.0, 6.0])
                    .show(ui, |ui| {
                        for (heading, width) in [
                            ("Name", name_width),
                            ("Diameter", 85.0),
                            ("Flute", 85.0),
                            ("Overall", 95.0),
                            ("Type", 150.0),
                        ] {
                            ui.add_sized(
                                [width, 24.0],
                                egui::Label::new(RichText::new(heading).strong()),
                            );
                        }
                        ui.end_row();
                        for t in &app.project.tool_library.tools {
                            if self.filter.is_some_and(|p| !t.uses.contains(&p))
                                || self.group.as_deref().is_some_and(|g| g != t.kind.group())
                                || !format!(
                                    "{} {} {} T{}",
                                    t.name,
                                    t.vendor,
                                    t.kind.label(),
                                    t.post.number
                                )
                                .to_lowercase()
                                .contains(&query)
                            {
                                continue;
                            }
                            let row = ui.add_sized(
                                [name_width, 28.0],
                                egui::Button::new(format!("T{}  {}", t.post.number, t.name))
                                    .selected(self.selected == Some(t.id)),
                            );
                            #[cfg(test)]
                            app.ui_hits
                                .push((format!("library:tool:{}", t.id), row.rect));
                            if row.clicked() {
                                self.selected = Some(t.id);
                                self.preset = t.default_preset;
                                self.error = None;
                            }
                            if row.double_clicked() {
                                self.selected = Some(t.id);
                                self.preset = t.default_preset;
                                self.draft = Some(t.clone());
                                self.tab = Tab::General;
                            }
                            for key in ["diameter", "flute_length", "overall_length"] {
                                ui.label(if t.geometry.contains_key(key) {
                                    format!("{:.2} mm", t.dimension(key))
                                } else {
                                    "—".into()
                                });
                            }
                            ui.label(t.kind.label());
                            ui.end_row();
                        }
                    });
            });
        ui.separator();
        section(ui, "CUTTING PRESETS");
        if let Some(tool) = self
            .selected
            .and_then(|id| app.project.tool_library.get(id).ok())
        {
            egui::ScrollArea::both()
                .id_salt(("library_presets", tool.id))
                .max_height((height * 0.27).max(80.0))
                .show(ui, |ui| {
                    egui::Grid::new("preset_rows").striped(true).show(ui, |ui| {
                        for h in ["Preset", "Material", "Spindle", "Cut feed", "Coolant"] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for preset in &tool.presets {
                            let response =
                                ui.selectable_label(self.preset == preset.id, &preset.name);
                            #[cfg(test)]
                            app.ui_hits
                                .push((format!("library:preset:{}", preset.id), response.rect));
                            if response.clicked() {
                                self.preset = preset.id;
                            }
                            ui.label(&preset.material);
                            ui.label(
                                preset
                                    .values
                                    .get("spindle_rpm")
                                    .map_or("—".into(), |v| format!("{v:.0} rpm")),
                            );
                            ui.label(feed_label(tool, preset));
                            ui.label(if preset.coolant == post::Coolant::Flood {
                                "Flood"
                            } else {
                                "Off"
                            });
                            ui.end_row();
                        }
                    });
                });
        } else {
            ui.label("No tool selected.");
        }
    }

    fn edit(&mut self, app: &Workbench) {
        if let Some(tool) = self
            .selected
            .and_then(|id| app.project.tool_library.get(id).ok())
        {
            self.draft = Some(tool.clone());
            self.preset = tool
                .preset(Some(self.preset))
                .map_or(tool.default_preset, |p| p.id);
            self.tab = Tab::General;
            self.error = None;
        }
    }
    fn assign(&mut self, app: &mut Workbench) {
        if let Some(id) = self.selected {
            if let Err(error) = app
                .project
                .tool_library
                .get(id)
                .and_then(|t| t.preset(Some(self.preset)))
            {
                self.error = Some(error);
                return;
            }
            if let Some(index) = self.assign_operation {
                if let Some(op) = app.project.operations.get_mut(index) {
                    op.tool_id = Some(id);
                    op.preset_id = Some(self.preset);
                } else {
                    self.error = Some("The selected operation no longer exists".into());
                    return;
                }
            } else {
                app.project.active_tool = Some(id);
                app.project.active_preset = Some(self.preset);
            }
            self.open = false;
            self.assign_operation = None;
        }
    }

    fn type_picker(&mut self, ui: &mut egui::Ui, app: &mut Workbench) {
        ui.heading("New tool / head");
        ui.add(egui::TextEdit::singleline(&mut self.query).hint_text("Search tool types"));
        ui.label(
            "Choose a definition type. Toolpaths are enabled only for implemented strategies.",
        );
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt("tool_type_picker")
            .show(ui, |ui| {
                for group in [
                    "Milling",
                    "Hole making",
                    "Turning",
                    "Cutting",
                    "3D printing",
                    "Probe",
                    "Holders",
                    "Grinding",
                    "EDM",
                ] {
                    let matches = ToolKind::ALL
                        .into_iter()
                        .filter(|k| {
                            k.group() == group
                                && format!("{} {}", k.label(), k.group())
                                    .to_lowercase()
                                    .contains(&self.query.to_lowercase())
                        })
                        .collect::<Vec<_>>();
                    if matches.is_empty() {
                        continue;
                    }
                    section(ui, group);
                    ui.horizontal_wrapped(|ui| {
                        for kind in matches {
                            let (rect, button) = ui
                                .allocate_exact_size(Vec2::new(142.0, 78.0), egui::Sense::click());
                            ui.painter().rect_filled(
                                rect,
                                4,
                                if button.hovered() {
                                    Color32::from_rgb(37, 69, 85)
                                } else {
                                    BG
                                },
                            );
                            ui.painter().rect_stroke(
                                rect,
                                4,
                                Stroke::new(1.0_f32, LINE),
                                egui::StrokeKind::Inside,
                            );
                            preview::icon(
                                ui.painter(),
                                kind,
                                Rect::from_center_size(
                                    rect.center_top() + Vec2::new(0.0, 23.0),
                                    Vec2::splat(36.0),
                                ),
                            );
                            let caption = ui.painter().layout(
                                kind.label().into(),
                                egui::FontId::proportional(11.0),
                                Color32::from_rgb(205, 218, 230),
                                138.0,
                            );
                            ui.painter().galley(
                                Pos2::new(
                                    rect.center().x - caption.size().x / 2.0,
                                    rect.bottom() - 5.0 - caption.size().y,
                                ),
                                caption,
                                Color32::WHITE,
                            );
                            #[cfg(test)]
                            app.ui_hits
                                .push((format!("library:new:{}", kind.label()), button.rect));
                            if button.clicked() {
                                let id = self
                                    .draft
                                    .as_ref()
                                    .map_or(app.project.tool_library.next_id(), |t| t.id);
                                let number = self.draft.as_ref().map_or_else(
                                    || app.project.tool_library.next_number(kind.process()),
                                    |t| t.post.number,
                                );
                                self.draft = Some(LibraryTool::new(id, number, kind));
                                if let Some(draft) = &mut self.draft {
                                    if app.project.process == Process::Mixed
                                        && !draft.uses.contains(&Process::Mixed)
                                    {
                                        draft.uses.push(Process::Mixed);
                                    }
                                    if app.project.tool_library.tools.iter().any(|t| {
                                        t.id != id
                                            && t.post.number == draft.post.number
                                            && t.uses.iter().any(|p| draft.uses.contains(p))
                                    }) {
                                        draft.post.number = (1..=9999)
                                            .find(|n| {
                                                !app.project.tool_library.tools.iter().any(|t| {
                                                    t.id != id
                                                        && t.post.number == *n
                                                        && t.uses
                                                            .iter()
                                                            .any(|p| draft.uses.contains(p))
                                                })
                                            })
                                            .unwrap_or(9999);
                                    }
                                }
                                self.preset = 1;
                                self.tab = Tab::General;
                                self.picker = false;
                                self.error = None;
                            }
                        }
                    });
                    ui.add_space(8.0);
                }
            });
    }

    fn editor(&mut self, ui: &mut egui::Ui, app: &mut Workbench) {
        let mut tool = self.draft.take().unwrap();
        ui.horizontal(|ui| {
            ui.label(format!("T{} / {}", tool.post.number, tool.kind.label()));
            if ui.button("Change tool type").clicked() {
                self.picker = true;
                self.query.clear();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.selectable_value(&mut self.inches, true, "Inches");
                ui.selectable_value(&mut self.inches, false, "Millimetres");
            });
        });
        ui.horizontal_wrapped(|ui| {
            for (tab, label) in [
                (Tab::General, "General"),
                (Tab::Cutter, "Cutter / head"),
                (Tab::Shaft, "Shaft"),
                (Tab::Holder, "Holder"),
                (Tab::Cutting, "Cutting data"),
                (Tab::Post, "Post processor"),
            ] {
                let response = ui.selectable_value(&mut self.tab, tab, label);
                #[cfg(not(test))]
                let _ = response;
                #[cfg(test)]
                app.ui_hits
                    .push((format!("library:tab:{label}"), response.rect));
            }
        });
        ui.separator();
        let height = ui.available_height() - 100.0;
        let preview_width = (ui.available_width() * 0.38).clamp(280.0, 520.0);
        let editor_width = ui.available_width() - preview_width - 18.0;
        fixed_region(ui, Vec2::new(ui.available_width(), height), |ui| {
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(editor_width, height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_min_width(editor_width);
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .id_salt(("tool_editor", tool.id, self.tab))
                            .max_height(height)
                            .show(ui, |ui| match self.tab {
                                Tab::General => self.general(ui, &mut tool),
                                Tab::Cutter => self.geometry(ui, &mut tool),
                                Tab::Shaft => Self::sections(ui, &mut tool),
                                Tab::Holder => {
                                    self.holder(ui, &mut tool, &app.project.tool_library)
                                }
                                Tab::Cutting => self.cutting(ui, &mut tool),
                                Tab::Post => Self::post_fields(ui, &mut tool),
                            });
                    },
                );
                ui.separator();
                ui.allocate_ui_with_layout(
                    Vec2::new(preview_width, height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        let preview = preview::draw(
                            ui,
                            &tool,
                            &app.project.tool_library,
                            &mut self.camera,
                            &mut self.zoom,
                            Vec2::new(preview_width - 8.0, height - 45.0),
                        );
                        #[cfg(test)]
                        app.ui_hits.push(("library:preview".into(), preview));
                        #[cfg(not(test))]
                        let _ = preview;
                    },
                );
            });
        });
        ui.separator();
        let mut candidate = app.project.tool_library.clone();
        if let Some(old) = candidate.tools.iter_mut().find(|t| t.id == tool.id) {
            *old = tool.clone();
        } else {
            candidate.tools.push(tool.clone());
        }
        let validation = candidate.validate();
        let validation=validation.and_then(|()|{
            if app.project.active_tool==Some(tool.id)&&app.project.active_preset.is_some_and(|id|tool.presets.iter().all(|p|p.id!=id)){return Err("This cutting preset is active. Choose another preset before deleting it.".into());}
            if app.project.operations.iter().any(|op|op.tool_id==Some(tool.id)&&op.preset_id.is_some_and(|id|tool.presets.iter().all(|p|p.id!=id))){return Err("A deleted cutting preset is assigned to an operation. Reassign it before deleting.".into());}
            Ok(())
        });
        if let Err(error) = &validation {
            ui.colored_label(Color32::from_rgb(255, 133, 126), error);
        }
        let mut saved = false;
        let mut cancelled = false;
        ui.horizontal(|ui| {
            let save = ui.add_enabled(
                validation.is_ok(),
                egui::Button::new("Save tool").fill(Color32::from_rgb(32, 91, 81)),
            );
            #[cfg(test)]
            app.ui_hits.push(("library:save".into(), save.rect));
            if save.clicked() {
                app.project.tool_library = candidate;
                self.filter = None;
                self.group = None;
                self.query.clear();
                self.selected = Some(tool.id);
                self.preset = tool
                    .preset(Some(self.preset))
                    .map_or(tool.default_preset, |p| p.id);
                self.error = None;
                saved = true;
            }
            let cancel = ui.button("Cancel");
            #[cfg(test)]
            app.ui_hits.push(("library:cancel".into(), cancel.rect));
            if cancel.clicked() {
                cancelled = true;
            }
            ui.label("Changes take effect only after Save tool.");
        });
        if !saved && !cancelled {
            self.draft = Some(tool);
        }
    }

    fn general(&self, ui: &mut egui::Ui, t: &mut LibraryTool) {
        for (label, text) in [
            ("Name", &mut t.name),
            ("Description", &mut t.description),
            ("Vendor", &mut t.vendor),
            ("Product ID", &mut t.product_id),
            ("Product link (metadata)", &mut t.product_link),
        ] {
            ui.label(label);
            ui.text_edit_singleline(text);
        }
        ui.separator();
        section(ui, "USE FOR OPERATIONS");
        ui.horizontal_wrapped(|ui| {
            for process in Process::ALL {
                let mut used = t.uses.contains(&process);
                if ui.checkbox(&mut used, process.label()).changed() {
                    if used {
                        t.uses.push(process);
                    } else {
                        t.uses.retain(|p| *p != process);
                    }
                }
            }
        });
        ui.label(RichText::new("Select multiple operation types for shared assemblies or exchangeable heads. Mixed chains remain a future motion engine.").size(11.0).color(MUTED));
    }
    fn geometry(&self, ui: &mut egui::Ui, t: &mut LibraryTool) {
        ui.heading(t.kind.label());
        ui.horizontal(|ui| {
            ui.label("Material");
            ui.text_edit_singleline(&mut t.material);
        });
        if t.kind.rotates() {
            ui.checkbox(&mut t.clockwise, "Clockwise spindle rotation");
            ui.checkbox(
                &mut t.center_cutting,
                "Center-cutting / vertical-entry capable",
            );
            ui.horizontal(|ui| {
                ui.label("Number of flutes / teeth");
                ui.add(egui::DragValue::new(&mut t.flutes).range(1..=1000));
            });
        }
        for p in t.kind.geometry() {
            parameter(ui, p, t.geometry.get_mut(p.key).unwrap(), self.inches);
        }
        if t.kind == ToolKind::BallEndMill {
            ui.label(format!(
                "Ball radius: {:.3} mm",
                t.dimension("diameter") / 2.0
            ));
        }
        if t.kind.group() == "Hole making" {
            ui.label(RichText::new("Programmed depth is measured at the tool tip. Full-diameter drill-point depth compensation is not implemented.").size(11.0).color(MUTED));
        }
    }
    fn sections(ui: &mut egui::Ui, t: &mut LibraryTool) {
        ui.label("Optional revolved shaft / holder sections, from cutting end toward spindle. All dimensions in mm.");
        if ui
            .add_enabled(t.shaft.len() < 64, egui::Button::new("Add section"))
            .clicked()
        {
            t.shaft.push(Section {
                height: 5.0,
                lower_diameter: 6.0,
                upper_diameter: 6.0,
            });
        }
        let mut remove = None;
        for (i, s) in t.shaft.iter_mut().enumerate() {
            ui.push_id(i, |ui| {
                ui.horizontal(|ui| {
                    ui.strong(format!("Section {}", i + 1));
                    if ui.small_button("Remove").clicked() {
                        remove = Some(i);
                    }
                });
                numeric(ui, "Height", &mut s.height, "mm");
                numeric(ui, "Lower diameter", &mut s.lower_diameter, "mm");
                numeric(ui, "Upper diameter", &mut s.upper_diameter, "mm");
                ui.separator();
            });
        }
        if let Some(i) = remove {
            t.shaft.remove(i);
        }
    }
    fn holder(&mut self, ui: &mut egui::Ui, t: &mut LibraryTool, library: &ToolLibrary) {
        ui.heading("Tool assembly");
        if t.kind == ToolKind::Holder {
            Self::sections(ui, t);
            return;
        }
        if ui
            .selectable_label(t.holder.is_none(), "No holder")
            .clicked()
        {
            t.holder = None;
        }
        for holder in library
            .tools
            .iter()
            .filter(|h| h.kind == ToolKind::Holder && h.id != t.id)
        {
            if ui
                .selectable_label(t.holder == Some(holder.id), &holder.name)
                .clicked()
            {
                t.holder = Some(holder.id);
            }
        }
        ui.label(RichText::new("Create holders in the same local library. Holder geometry is shown in the assembly preview; collision simulation is not implemented.").color(MUTED).size(11.0));
    }
    fn cutting(&mut self, ui: &mut egui::Ui, t: &mut LibraryTool) {
        ui.horizontal_wrapped(|ui| {
            for p in &t.presets {
                if ui.selectable_label(self.preset == p.id, &p.name).clicked() {
                    self.preset = p.id;
                }
            }
            if ui
                .add_enabled(t.presets.len() < 64, egui::Button::new("Add preset"))
                .clicked()
            {
                let mut p = t.presets[0].clone();
                p.id = t.presets.iter().map(|p| p.id).max().unwrap_or(0) + 1;
                p.name = format!("Preset {}", p.id);
                self.preset = p.id;
                t.presets.push(p);
            }
            if ui
                .add_enabled(t.presets.len() > 1, egui::Button::new("Delete preset"))
                .clicked()
            {
                t.presets.retain(|p| p.id != self.preset);
                self.preset = t.presets[0].id;
                if t.presets.iter().all(|p| p.id != t.default_preset) {
                    t.default_preset = self.preset;
                }
            }
        });
        if !t.presets.iter().any(|p| p.id == self.preset) {
            self.preset = t.default_preset;
        }
        let specs = t.kind.cutting();
        let diameter = t.dimension("diameter");
        let flutes = t.flutes;
        let kind = t.kind;
        let preset = t.presets.iter_mut().find(|p| p.id == self.preset).unwrap();
        ui.label("Preset name");
        ui.text_edit_singleline(&mut preset.name);
        ui.label("Stock material association");
        ui.text_edit_singleline(&mut preset.material);
        if ui
            .selectable_label(t.default_preset == preset.id, "Default preset")
            .clicked()
        {
            t.default_preset = preset.id;
        }
        if kind.rotates() || kind.group() == "Turning" {
            ui.horizontal_wrapped(|ui| {
                ui.label("Feed source");
                for (mode, label) in [
                    (FeedMode::PerMinute, "mm/min"),
                    (FeedMode::PerTooth, "Per tooth"),
                    (FeedMode::PerRevolution, "Per revolution"),
                ] {
                    if mode != FeedMode::PerTooth || kind.rotates() {
                        ui.selectable_value(&mut preset.feed_mode, mode, label);
                    }
                }
            });
        }
        if kind.group() == "Turning" {
            ui.checkbox(
                &mut preset.constant_surface_speed,
                "Constant surface speed (CSS)",
            );
        }
        if kind.rotates()
            || kind.group() == "Turning"
            || matches!(
                kind,
                ToolKind::GrindingWheel | ToolKind::WireEdm | ToolKind::Electrode
            )
        {
            ui.horizontal(|ui| {
                ui.label("Coolant");
                ui.selectable_value(&mut preset.coolant, post::Coolant::Off, "Off");
                ui.selectable_value(&mut preset.coolant, post::Coolant::Flood, "Flood");
            });
        }
        if kind == ToolKind::PrintingHead {
            ui.checkbox(
                &mut preset.relative_extrusion,
                "Relative extrusion distances",
            );
        }
        if kind == ToolKind::Holder {
            ui.label("Holders have assembly geometry rather than cutting data.");
            return;
        }
        if kind.rotates() {
            ui.label("Strategy association (none = any supported strategy)");
            ui.horizontal_wrapped(|ui| {
                for strategy in Strategy::ALL {
                    let mut selected = preset.strategies.contains(&strategy);
                    if ui.checkbox(&mut selected, strategy.label()).changed() {
                        if selected {
                            preset.strategies.push(strategy);
                        } else {
                            preset.strategies.retain(|s| *s != strategy);
                        }
                    }
                }
            });
        }
        if kind.rotates() {
            let rpm = preset.values["spindle_rpm"];
            preset.values.insert(
                "surface_speed".into(),
                std::f64::consts::PI * diameter * rpm / 1000.0,
            );
            let feed = match preset.feed_mode {
                FeedMode::PerMinute => preset.values.get("cut_feed").copied().unwrap_or(0.0),
                FeedMode::PerTooth => preset.values["feed_per_tooth"] * flutes as f64 * rpm,
                FeedMode::PerRevolution => preset.values["feed_per_revolution"] * rpm,
            };
            ui.label(RichText::new(format!("Effective feed: {feed:.3} mm/min")).color(ACCENT));
        }
        ui.separator();
        for p in specs {
            let derived = (p.key == "cut_feed" && preset.feed_mode != FeedMode::PerMinute)
                || (p.key == "surface_speed" && kind.rotates());
            ui.add_enabled_ui(!derived, |ui| {
                parameter(ui, p, preset.values.get_mut(p.key).unwrap(), self.inches)
            });
        }
        ui.label(RichText::new(if kind.rotates(){"Current 2.5D paths use RPM, cut/plunge feed, step-down/peck and step-over. Lead/ramp data is stored for future strategies."}else{"These process settings are stored with the head. Motion and post output for this process are not implemented yet."}).size(11.0).color(MUTED));
    }
    fn post_fields(ui: &mut egui::Ui, t: &mut LibraryTool) {
        ui.heading("Controller / ATC binding");
        ui.horizontal(|ui| {
            ui.label("Tool number T");
            ui.add(egui::DragValue::new(&mut t.post.number).range(1..=9999));
        });
        for (label, field) in [
            ("Length offset H", &mut t.post.length_offset),
            ("Diameter offset D", &mut t.post.diameter_offset),
            ("Turning offset register", &mut t.post.turning_offset),
            ("ATC pocket / station", &mut t.post.pocket),
        ] {
            ui.horizontal(|ui| {
                let mut enabled = field.is_some();
                if ui.checkbox(&mut enabled, label).changed() {
                    *field = if enabled { Some(t.post.number) } else { None };
                }
                if let Some(value) = field {
                    ui.add(egui::DragValue::new(value).range(1..=9999));
                }
            });
        }
        for (label, field) in [
            ("Turret", &mut t.post.turret),
            ("Head / extruder index", &mut t.post.head_id),
        ] {
            ui.horizontal(|ui| {
                ui.label(label);
                ui.add(egui::DragValue::new(field).range(0..=9999));
            });
        }
        ui.label("Comment");
        ui.text_edit_singleline(&mut t.post.comment);
        ui.checkbox(&mut t.post.manual_change, "Manual tool change");
        ui.checkbox(&mut t.post.live_tool, "Live / driven tool");
        ui.checkbox(&mut t.post.break_control, "Require break control");
        ui.separator();
        section(ui, "HEAD / COUPLER SETTINGS");
        for axis in 0..3 {
            numeric(
                ui,
                ["Mount offset X", "Mount offset Y", "Mount offset Z"][axis],
                &mut t.post.mount_offset[axis],
                "mm",
            );
        }
        for (label, field) in [
            ("Heater channel", &mut t.post.heater_channel),
            ("Fan channel", &mut t.post.fan_channel),
        ] {
            ui.horizontal(|ui| {
                ui.label(label);
                ui.add(egui::DragValue::new(field).range(0..=9999));
            });
        }
        numeric(ui, "Maximum power word S", &mut t.post.power_scale_max, "");
        ui.checkbox(&mut t.post.dynamic_power, "Dynamic laser power mode");
        ui.label("Mount / coupler interface");
        ui.text_edit_singleline(&mut t.post.connector);
        ui.label("Required changer interlocks");
        ui.text_edit_multiline(&mut t.post.interlocks);
        ui.label(RichText::new("Head exchange and interlock execution are future Mixed-engine capabilities. Measured length offsets stay in the controller table; this library selects registers.").size(11.0).color(MUTED));
        ui.label(RichText::new("ATC requires machine capacity, a configured safe controller M6 routine, matching tool-table pockets and calibrated H registers. Current toolpaths bake in cutter radius (G40); D offsets and turret/head data are retained for their respective strategies.").color(MUTED).size(11.0));
    }
}

fn parameter(ui: &mut egui::Ui, spec: Parameter, value: &mut f64, inches: bool) {
    let (factor, unit) = if inches {
        match spec.unit {
            "mm" => (25.4, "in"),
            "mm/min" => (25.4, "in/min"),
            "mm/s" => (25.4, "in/s"),
            "mm3/s" => (25.4_f64.powi(3), "in3/s"),
            _ => (1.0, spec.unit),
        }
    } else {
        (1.0, spec.unit)
    };
    let mut shown = *value / factor;
    ui.horizontal(|ui| {
        ui.add_sized([190.0, 20.0], egui::Label::new(spec.label));
        if ui
            .add(
                egui::DragValue::new(&mut shown)
                    .speed(if unit == "rpm" { 100.0 } else { 0.01 })
                    .max_decimals(6)
                    .suffix(format!(" {unit}")),
            )
            .changed()
        {
            *value = shown * factor;
        }
    });
}

fn fixed_region(ui: &mut egui::Ui, size: Vec2, body: impl FnOnce(&mut egui::Ui)) {
    let (rect, _) = ui.allocate_exact_size(size.max(Vec2::splat(100.0)), egui::Sense::hover());
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    body(&mut child);
}

fn import_tools(
    project: &mut Project,
    source: ToolLibrary,
    replace: bool,
) -> Result<String, String> {
    opensourcecam::tool_library::import_into(project, source, replace)
}

fn feed_label(tool: &LibraryTool, preset: &opensourcecam::tool_library::CuttingPreset) -> String {
    let value = |key: &str| preset.values.get(key).copied().unwrap_or(0.0);
    match tool.kind {
        ToolKind::PrintingHead => format!("{:.2} mm/s", value("print_speed")),
        ToolKind::Probe => format!("{:.2} mm/min", value("probe_feed")),
        ToolKind::Holder => "—".into(),
        _ if tool.kind.group() == "Turning" && preset.feed_mode == FeedMode::PerRevolution => {
            format!("{:.4} mm/rev", value("feed_per_revolution"))
        }
        _ => format!(
            "{:.2} mm/min",
            tool.cutting_tool(Some(preset.id))
                .map_or(value("cut_feed"), |t| t.feed)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn incoming() -> ToolLibrary {
        let mut cutter = LibraryTool::new(7, 7, ToolKind::FlatEndMill);
        cutter.post.pocket = Some(3);
        cutter.post.length_offset = Some(7);
        cutter.holder = Some(8);
        ToolLibrary {
            tools: vec![cutter, LibraryTool::new(8, 8, ToolKind::Holder)],
        }
    }
    #[test]
    fn adding_tools_remaps_assemblies_and_preserves_existing_job_references() {
        let mut project = Project::default();
        project.ensure_tool_library();
        let old = project.tool_library.tools[0].clone();
        let ops = project.operations.clone();
        import_tools(&mut project, incoming(), false).unwrap();
        assert_eq!(project.tool_library.tools.len(), 3);
        assert_eq!(project.tool_library.tools[0], old);
        assert_eq!(project.operations, ops);
        let cutter = &project.tool_library.tools[1];
        assert_eq!(cutter.holder, Some(project.tool_library.tools[2].id));
        assert_eq!(cutter.post.pocket, None);
        assert_eq!(cutter.post.length_offset, None);
        post::gcode(&project).unwrap();
    }
    #[test]
    fn replacement_is_explicit_preserves_bindings_and_refuses_used_libraries() {
        let mut project = Project::default();
        project.ensure_tool_library();
        let before = project.clone();
        assert!(import_tools(&mut project, incoming(), true).is_err());
        assert_eq!(project, before);
        project.operations.clear();
        let source = incoming();
        import_tools(&mut project, source.clone(), true).unwrap();
        assert_eq!(project.tool_library, source);
        assert_eq!(project.active_tool, Some(7));
        assert_eq!(project.active_preset, Some(1));
    }
}
