use super::*;
use opensourcecam::workspace::{Frame, ModelKind};
impl Workbench {
    pub(super) fn workspace_parameters(&mut self, ui: &mut egui::Ui) {
        ui.heading("Machine & workholding");
        for (kind, label) in [
            (ModelKind::Machine, "Import machine…"),
            (ModelKind::Vise, "Import vise…"),
            (ModelKind::Fixture, "Import fixture…"),
        ] {
            if ui
                .add_enabled(self.import_receiver.is_none(), egui::Button::new(label))
                .clicked()
            {
                self.import_target = Some(kind);
                self.choose_model_file();
            }
        }
        ui.label(
            RichText::new("STL / OBJ / STEP / IGES · local bundled importer")
                .color(MUTED)
                .size(11.0),
        );
        ui.separator();
        for model in &mut self.project.workspace {
            ui.horizontal(|ui| {
                ui.checkbox(&mut model.part.visible, "");
                ui.selectable_value(
                    &mut self.workspace_selected,
                    Some(model.id),
                    format!("{} · {}", model.kind.label(), model.name),
                );
            });
        }
        let mut remove = None;
        if let Some(model) = self
            .workspace_selected
            .and_then(|id| self.project.workspace.iter_mut().find(|m| m.id == id))
        {
            ui.separator();
            ui.text_edit_singleline(&mut model.name);
            ui.label("Coordinate frame");
            ui.selectable_value(&mut model.frame, Frame::Work, "Work coordinates");
            ui.add_enabled_ui(self.project.machine.is_some(), |ui| {
                ui.selectable_value(&mut model.frame, Frame::Machine, "Machine coordinates");
            });
            ui.label(RichText::new("Machine models follow machine XYZ. Work-frame vises/fixtures stay with the job. Rotation is X → Y → Z about the imported source origin, then translation.").color(MUTED).size(11.0));
            for axis in 0..3 {
                numeric(
                    ui,
                    ["Position X", "Position Y", "Position Z"][axis],
                    &mut model.part.offset[axis],
                    "mm",
                );
            }
            for axis in 0..3 {
                numeric(
                    ui,
                    ["Rotate X", "Rotate Y", "Rotate Z"][axis],
                    &mut model.rotation[axis],
                    "deg",
                );
            }
            if !model.part.geometry.format.is_cad() {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut model.part.units, Units::Millimetres, "Source mm");
                    ui.selectable_value(&mut model.part.units, Units::Inches, "Source inches");
                });
            }
            if let Err(error) = model.validate_placement(self.project.machine.as_ref()) {
                ui.colored_label(Color32::from_rgb(255, 133, 126), error);
            }
            if ui.button("Reset rotation").clicked() {
                model.rotation = [0.0; 3];
            }
            if ui.button("Remove workspace model").clicked() {
                remove = Some(model.id);
            }
        }
        if let Some(id) = remove {
            self.project.workspace.retain(|m| m.id != id);
            self.workspace_selected = None;
        }
        ui.separator();
        ui.label(RichText::new("Reference geometry only. Fixtures and machine bodies are not collision checked or articulated. Keep working clearance appropriate to the real setup.").color(MUTED).size(11.0));
    }
}
