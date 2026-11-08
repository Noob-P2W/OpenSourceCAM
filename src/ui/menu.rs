use super::*;

impl Workbench {
    pub(super) fn main_menu_ui(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(24.0))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().id_salt("process_menu").show(ui, |ui| {
                    let width = (ui.available_width()-2.0).min(1000.0);
                    let padding = ((ui.available_width()-width)/2.0).max(0.0);
                    ui.horizontal(|ui| {
                        ui.add_space(padding);
                        ui.vertical(|ui| {
                            ui.set_width(width);
                            ui.add_space(8.0);
                            section(ui, "NEW PROJECT");
                            ui.label(RichText::new("Choose an operation type").size(32.0).strong());
                            ui.label(RichText::new("Start with the manufacturing process for your machine.").color(MUTED));
                            ui.add_space(10.0);
                            ui.horizontal(|ui| {
                                if ui.button("Open project").clicked() { self.request(Pending::Open, ctx); }
                                if ui.button("Milling demo").clicked() { self.request(Pending::Demo, ctx); }
                                if self.has_project {
                                    let response = ui.button(if self.dirty() { "Resume project *" } else { "Resume project" });
                                    #[cfg(test)]
                                    self.ui_hits.push(("resume".into(),response.rect));
                                    if response.on_hover_text(&self.project.name).clicked() { self.main_menu = false; }
                                }
                            });
                            ui.add_space(16.0);
                            let gap = 14.0;
                            let card_width = (width-gap*2.0)/3.0;
                            for row in Process::ALL.chunks(3) {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = gap;
                                    for process in row {
                                        let response = process_card(ui, *process, Vec2::new(card_width, 112.0));
                                        #[cfg(test)]
                                        self.ui_hits.push((format!("process:{}",process.label()),response.rect));
                                        if response.clicked() { self.request(Pending::New(*process), ctx); }
                                    }
                                });
                                ui.add_space(5.0);
                            }
                            ui.add_space(8.0);
                            ui.label(RichText::new("Milling strategies are available now. Planned workspaces support importing, viewing and saving reference models.").size(12.0).color(MUTED));
                        });
                    });
                });
            });
    }
}

fn process_card(ui: &mut egui::Ui, process: Process, size: Vec2) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), process.label())
    });
    let hovered = response.hovered() || response.has_focus();
    let fill = if hovered {
        Color32::from_rgb(29, 44, 56)
    } else {
        PANEL
    };
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(9), fill);
    ui.painter().rect_stroke(
        rect,
        egui::CornerRadius::same(9),
        Stroke::new(1.0_f32, if hovered { ACCENT } else { LINE }),
        egui::StrokeKind::Inside,
    );
    icons::Icon::process(process).draw(
        ui.painter(),
        Rect::from_min_size(
            Pos2::new(rect.right() - 64.0, rect.top() + 24.0),
            Vec2::splat(48.0),
        ),
        true,
    );
    let mut content_rect = rect.shrink(14.0);
    content_rect.max.x -= 62.0;
    let mut content = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(content_rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    content.spacing_mut().item_spacing.y = 6.0;
    let color = if process.supports_toolpaths() {
        ACCENT
    } else if process == Process::Mixed {
        RAPID
    } else {
        MUTED
    };
    content.label(
        RichText::new(process.status().to_uppercase())
            .size(10.0)
            .color(color)
            .strong(),
    );
    content.label(RichText::new(process.label()).size(21.0).strong());
    content.label(RichText::new(process.description()).size(12.0).color(MUTED));
    response
}
