use super::*;

#[derive(Clone, Copy)]
enum Action {
    Select,
    Edit,
    Tool,
    Duplicate,
    Delete,
    Up,
    Down,
    Toggle,
}

impl Workbench {
    pub(super) fn tree(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("project").default_width(286.0).width_range(244.0..=400.0).resizable(true).show(ctx,|ui| {
            ui.add_space(10.0);
            ui.add(egui::Label::new(RichText::new(&self.project.name).size(17.0).strong()).truncate()).on_hover_text(&self.project.name);
            ui.horizontal(|ui| {
                ui.label(RichText::new(self.project.process.label()).color(ACCENT).size(11.0));
                ui.label(RichText::new(if self.dirty(){"Unsaved changes"}else{"Saved project"}).color(MUTED).size(11.0));
            });
            ui.add_space(6.0);
            egui::Frame::new().fill(SURFACE).corner_radius(8).inner_margin(10.0).show(ui,|ui| {
                ui.horizontal(|ui| {
                    section(ui,"STOCK & SETUP");
                    if ui.small_button("Edit").clicked() {self.inspector_tab=InspectorTab::Setup;self.setup_section=SetupSection::Stock;}
                });
                ui.add(egui::Label::new(format!("{:.3} × {:.3} × {:.3} mm",self.project.stock.width,self.project.stock.height,self.project.stock.thickness)).truncate())
                    .on_hover_text("Work origin: stock lower-left, top Z0. Positive cut depths extend downwards.");
                if let Some(part)=&mut self.project.part {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut part.visible,"").on_hover_text("Show target part");
                        let response=ui.add_sized([ui.available_width(),22.0],egui::Button::new(&part.geometry.source_name).frame(false).truncate());
                        if response.clicked() {self.inspector_tab=InspectorTab::Model;}
                        response.on_hover_text("Inspect target units, placement and dimensions");
                    });
                }
            });
            ui.add_space(10.0);
            let milling=self.project.process.supports_toolpaths();
            ui.horizontal(|ui| {
                section(ui,"OPERATIONS");
                ui.label(RichText::new(format!("{} / {} enabled",self.project.operations.iter().filter(|o|o.enabled).count(),self.project.operations.len())).size(10.0).color(MUTED));
            });
            let valid=self.selected<self.project.operations.len();
            let mut action=None;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x=5.0;
                for (label,kind,id,enabled,tip) in [
                    ("Edit",Action::Edit,"list:edit",valid,"Edit operation · F2"),
                    ("Duplicate",Action::Duplicate,"list:duplicate",valid&&self.project.operations.len()<1000,"Duplicate · Ctrl+D"),
                    ("Delete",Action::Delete,"list:delete",valid,"Delete operation · Delete · Ctrl+Z to undo"),
                ] {
                    let r=ui.add_enabled(enabled,egui::Button::new(label));
                    #[cfg(test)] self.ui_hits.push((id.into(),r.rect));
                    let _=id;
                    if r.clicked(){action=Some((self.selected,kind));}
                    r.on_hover_text(tip);
                }
            });
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x=5.0;
                for (label,kind,id,enabled) in [
                    ("Up",Action::Up,"list:up",valid&&self.selected>0),
                    ("Down",Action::Down,"list:down",valid&&self.selected+1<self.project.operations.len()),
                ] {
                    let r=ui.add_enabled(enabled,egui::Button::new(label));
                    #[cfg(test)] self.ui_hits.push((id.into(),r.rect));
                    let _=id;
                    if r.clicked(){action=Some((self.selected,kind));}
                    r.on_hover_text("Move operation · Alt+Up / Alt+Down");
                }
                ui.add_enabled_ui(milling&&self.project.operations.len()<1000,|ui| {
                    ui.menu_button("+ Add operation",|ui| {
                        for strategy in Strategy::ALL {
                            let r=ui.button(strategy.label());
                            #[cfg(test)] self.ui_hits.push((format!("list:add:{}",strategy.label()),r.rect));
                            if r.clicked(){self.add_operation(strategy);ui.close_menu();}
                        }
                    });
                });
            });
            ui.add_space(6.0);
            if self.project.operations.is_empty() {
                egui::Frame::new().fill(SURFACE).corner_radius(8).inner_margin(12.0).show(ui,|ui| {
                    ui.label(RichText::new("No operations yet").strong());
                    ui.label(RichText::new(if milling {"Add an existing strategy above, or generate operations from your target part."}else{"Machining strategies for this process are still in development."}).size(12.0).color(MUTED));
                    if milling && self.project.part.is_some() && ui.button("Machine from stock").clicked(){self.inspector_tab=InspectorTab::Machining;}
                });
            }
            let height=(ui.available_height()-76.0).max(80.0);
            let mut scroll=egui::ScrollArea::vertical().id_salt("operation_list").max_height(height).min_scrolled_height(height).auto_shrink([false,false]);
            if self.reveal_selection {scroll=scroll.vertical_scroll_offset((self.selected as f32*64.0-height*0.5).max(0.0));self.reveal_selection=false;}
            scroll.show_rows(ui,58.0,self.project.operations.len(),|ui,range| {
                for index in range {
                    ui.push_id(index,|ui| {
                        let op=&self.project.operations[index];
                        let name=op.name.clone();let strategy=op.strategy.label();let depth=op.depth;
                        let tool=self.project.library_tool(Some(op)).ok().flatten().map(|t|format!("T{}",t.post.number)).unwrap_or_else(||"Tool".into());
                        let mut enabled=op.enabled;
                        let (rect,_)=ui.allocate_exact_size(Vec2::new(ui.available_width(),58.0),egui::Sense::hover());
                        let row=ui.interact(rect,ui.id().with("row"),egui::Sense::click());
                        let selected=self.selected==index;
                        ui.painter().rect_filled(rect.shrink(1.0),7,if selected {Color32::from_rgb(31,75,70)}else{SURFACE});
                        let mut content=ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(Vec2::new(7.0,4.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
                        content.set_clip_rect(rect.intersect(ui.clip_rect()));
                        let check=content.checkbox(&mut enabled,"");
                        #[cfg(test)] self.ui_hits.push((format!("op:enabled:{index}"),check.rect));
                        check.on_hover_text("Include this operation in machining and NC output");
                        let response=content.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y=1.0;
                            let r=ui.add_sized([ui.available_width(),25.0],egui::Button::new(RichText::new(format!("{:02}  {name}",index+1)).strong().size(12.0)).frame(false).truncate());
                            ui.add_sized([ui.available_width(),18.0],egui::Label::new(RichText::new(format!("{strategy} · {tool} · {depth:.3} mm")).color(if enabled {MUTED}else{Color32::GRAY}).size(10.0)).truncate());
                            r
                        }).inner;
                        #[cfg(test)] self.ui_hits.push((format!("op:{index}"),response.rect));
                        let response=row.union(response).on_hover_text(format!("{name}\n{strategy} · {tool} · depth {depth:.3} mm\nDouble-click to edit · Right-click for actions"));
                        if response.clicked() || response.secondary_clicked(){action=Some((index,Action::Select));}
                        if response.double_clicked(){action=Some((index,Action::Edit));}
                        response.context_menu(|ui| {
                            for (label,kind) in [("Edit operation",Action::Edit),("Choose tool / preset",Action::Tool),("Duplicate",Action::Duplicate),(if enabled {"Disable"}else{"Enable"},Action::Toggle),("Move up",Action::Up),("Move down",Action::Down),("Delete operation",Action::Delete)] {
                                let r=ui.button(label);
                                #[cfg(test)] self.ui_hits.push((format!("op:menu:{index}:{label}"),r.rect));
                                if r.clicked(){action=Some((index,kind));ui.close_menu();}
                            }
                        });
                        if enabled!=self.project.operations[index].enabled {self.project.operations[index].enabled=enabled;}
                    });
                }
            });
            if let Some((index,command))=action {
                self.selected=index;
                match command {
                    Action::Select=>{self.inspector_tab=InspectorTab::Operation;self.show_paths=true;self.selected_path_only=true;},
                    Action::Edit=>self.editors.open_operation(index,&self.project),
                    Action::Tool=>self.open_library(Some(index)),
                    Action::Duplicate=>self.duplicate_selected(),
                    Action::Delete=>self.delete_selected(),
                    Action::Up=>self.move_selected(true),Action::Down=>self.move_selected(false),
                    Action::Toggle=>self.project.operations[index].enabled= !self.project.operations[index].enabled,
                }
            }
            ui.separator();
            ui.label(RichText::new(if self.error.is_some(){"Needs attention before export"}else if self.code.is_empty(){"No enabled machining operations"}else{"Program ready for review"}).color(if self.error.is_some(){Color32::LIGHT_RED}else{ACCENT}).size(11.0));
            if let Some(op)=self.project.operations.get(self.selected) {
                let tool=self.project.library_tool(Some(op)).ok().flatten().map(|t|format!("T{}  {}",t.post.number,t.name)).unwrap_or_else(||self.project.tool.name.clone());
                ui.add(egui::Label::new(RichText::new(tool).size(11.0).color(MUTED)).truncate());
            }
        });
    }
}
